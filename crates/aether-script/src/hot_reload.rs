//! Hot-reload file watcher for rule DSL files.
//!
//! Watches `.aether` rule files for changes and recompiles them atomically
//! via `ArcSwap`. On compilation error, the previous valid ruleset is preserved.

use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::{ArcSwap, Guard};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::error::ScriptError;
use crate::runtime::CompiledRuleSet;

/// Hot-reloading file watcher for compiled rule sets.
///
/// Watches rule file paths for modifications, recompiles on change with
/// 100ms debounce, and atomically swaps the active ruleset. Compilation
/// errors are logged; the previous valid ruleset remains active.
pub struct HotReloader {
    rules: Arc<ArcSwap<CompiledRuleSet>>,
    watch_paths: Vec<PathBuf>,
}

impl HotReloader {
    /// Create a new hot-reloader with an initial compiled ruleset.
    pub fn new(initial: CompiledRuleSet, watch_paths: Vec<PathBuf>) -> Self {
        Self {
            rules: Arc::new(ArcSwap::from_pointee(initial)),
            watch_paths,
        }
    }

    /// Shared handle to the compiled ruleset (for ScriptEngine and display).
    pub fn rules(&self) -> Arc<ArcSwap<CompiledRuleSet>> {
        Arc::clone(&self.rules)
    }

    /// Get the currently active compiled ruleset.
    pub fn current_rules(&self) -> Guard<Arc<CompiledRuleSet>> {
        self.rules.load()
    }

    /// Watch rule files for changes and recompile on modification.
    ///
    /// Runs until the cancellation token is triggered. File changes are
    /// debounced by 100ms to coalesce rapid saves.
    pub async fn watch(&self, cancel: CancellationToken) {
        let (fs_tx, mut fs_rx) = mpsc::channel::<()>(16);

        let mut watcher = match self.create_watcher(fs_tx) {
            Ok(w) => w,
            Err(e) => {
                warn!("failed to create file watcher: {e}");
                cancel.cancelled().await;
                return;
            }
        };

        for path in &self.watch_paths {
            if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                warn!("failed to watch {}: {e}", path.display());
            }
        }

        info!("hot-reload watcher started for {} paths", self.watch_paths.len());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(()) = fs_rx.recv() => {
                    self.debounce_and_recompile(&mut fs_rx).await;
                }
            }
        }

        drop(watcher);
        info!("hot-reload watcher stopped");
    }

    /// Drain pending events for 100ms then recompile once.
    async fn debounce_and_recompile(&self, rx: &mut mpsc::Receiver<()>) {
        let deadline = Instant::now() + Duration::from_millis(100);

        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                Some(()) = rx.recv() => { /* absorb, keep waiting */ }
            }
        }

        match self.recompile() {
            Ok(()) => info!("rules recompiled successfully"),
            Err(e) => warn!("recompilation failed, keeping old rules: {e}"),
        }
    }

    /// Read all watched files and recompile the ruleset.
    pub(crate) fn recompile(&self) -> Result<(), ScriptError> {
        let mut all_source = String::new();
        for path in &self.watch_paths {
            let content = std::fs::read_to_string(path).map_err(|e| ScriptError::Io {
                path: path.display().to_string(),
                source: e,
            })?;
            all_source.push_str(&content);
            all_source.push('\n');
        }

        let rules = parse_rules(&all_source)?;
        let compiled = CompiledRuleSet::compile(&rules)?;
        self.rules.store(Arc::new(compiled));
        Ok(())
    }

    fn create_watcher(
        &self,
        tx: mpsc::Sender<()>,
    ) -> Result<RecommendedWatcher, notify::Error> {
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.blocking_send(());
                }
            }
        })
    }
}

/// Parse rule source text through the compilation pipeline.
///
/// Runs: tokenize -> parse -> type-check, returning parsed AST rules.
/// Will be connected to the lexer/parser/typechecker modules once implemented.
fn parse_rules(_source: &str) -> Result<Vec<crate::ast::Rule>, ScriptError> {
    // TODO: connect to lexer → parser → type-checker pipeline
    Err(ScriptError::Compile(
        "rule parser not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, CmpOp, Condition, Expr, Literal, Rule, Severity};
    use crate::lexer::Span;

    fn high_cpu_rule() -> Rule {
        Rule {
            name: "high_cpu".to_string(),
            condition: Condition::Comparison {
                left: Expr::FieldAccess {
                    object: "process".to_string(),
                    field: "cpu".to_string(),
                },
                op: CmpOp::Gt,
                right: Expr::Literal(Literal::Float(90.0)),
            },
            actions: vec![Action::Alert {
                message: "high cpu".to_string(),
                severity: Severity::Warning,
            }],
            span: Span { start: 0, end: 0 },
        }
    }

    #[test]
    fn test_current_rules_returns_initial_rules() {
        let compiled = CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
        let reloader = HotReloader::new(compiled, vec![]);

        // Load the ruleset twice and verify they point to the same allocation.
        let first = reloader.rules.load_full();
        let second = reloader.current_rules();
        assert!(
            Arc::ptr_eq(&first, &*second),
            "current_rules should return the initial ruleset"
        );
    }

    #[test]
    fn test_compilation_error_preserves_old_rules() {
        let compiled = CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
        let reloader =
            HotReloader::new(compiled, vec![PathBuf::from("/nonexistent/rules.aether")]);

        let before = reloader.rules.load_full();

        // Attempt recompile — will fail (file doesn't exist)
        let result = reloader.recompile();
        assert!(result.is_err(), "recompile should fail for nonexistent file");

        // Verify old rules are preserved
        let after = reloader.rules.load_full();
        assert!(
            Arc::ptr_eq(&before, &after),
            "failed recompile must not change the active ruleset"
        );
    }
}

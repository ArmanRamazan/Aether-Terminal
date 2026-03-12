//! AnalyzeEngine — orchestrates periodic diagnostic analysis.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use aether_core::metrics::HostId;
use aether_core::models::{Diagnostic, Severity};
use aether_core::WorldGraph;

use crate::analyzers::capacity::CapacityAnalyzer;
use crate::analyzers::trend::TrendAnalyzer;
use crate::recommendations::generator::RecommendationGenerator;
use crate::rules::engine::RuleEngine;
use crate::rules::types::ProcessLimits;
use crate::store::MetricStore;

/// Configuration for the diagnostic engine.
#[derive(Debug, Clone)]
pub struct AnalyzeConfig {
    /// How often to run analysis.
    pub interval: Duration,
    /// Maximum metric history samples per series.
    pub history_capacity: usize,
    /// Host identifier for this machine.
    pub host: HostId,
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            history_capacity: 3600,
            host: HostId::new("local"),
        }
    }
}

/// Runtime statistics for the engine.
#[derive(Debug, Clone, Default)]
pub struct AnalyzeStats {
    pub evaluations: u64,
    pub rules_fired: u64,
    pub active_critical: u32,
    pub active_warning: u32,
    pub active_info: u32,
}

/// Orchestrates metric collection, rule evaluation, and diagnostic generation.
pub struct AnalyzeEngine {
    store: MetricStore,
    rule_engine: RuleEngine,
    trend: TrendAnalyzer,
    capacity: CapacityAnalyzer,
    generator: RecommendationGenerator,
    config: AnalyzeConfig,
    active_diagnostics: Vec<Diagnostic>,
    stats: AnalyzeStats,
}

impl AnalyzeEngine {
    pub fn new(config: AnalyzeConfig) -> Self {
        let store = MetricStore::new(config.history_capacity);
        let mut rule_engine = RuleEngine::new();
        rule_engine.load_builtin();

        Self {
            store,
            rule_engine,
            trend: TrendAnalyzer,
            capacity: CapacityAnalyzer,
            generator: RecommendationGenerator::new(),
            config,
            active_diagnostics: Vec::new(),
            stats: AnalyzeStats::default(),
        }
    }

    /// Run the analysis loop, sending diagnostics on each tick.
    pub async fn run(
        &mut self,
        world: Arc<RwLock<WorldGraph>>,
        diag_tx: mpsc::Sender<Vec<Diagnostic>>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(self.config.interval) => {
                    self.tick(&world);
                    if diag_tx.send(self.active_diagnostics.clone()).await.is_err() {
                        warn!("diagnostic receiver dropped, stopping engine");
                        break;
                    }
                }
            }
        }
    }

    /// Execute a single analysis cycle.
    fn tick(&mut self, world: &Arc<RwLock<WorldGraph>>) {
        let graph = match world.read() {
            Ok(g) => g,
            Err(_) => {
                warn!("WorldGraph lock poisoned, skipping tick");
                return;
            }
        };

        // 1. Ingest current state
        self.store.ingest_world_state(&self.config.host, &graph);

        // 2. Evaluate rules
        let empty_limits: HashMap<u32, ProcessLimits> = HashMap::new();
        let findings = self
            .rule_engine
            .evaluate(&self.store, &self.config.host, &empty_limits);

        // 3. Generate diagnostics from findings
        let new_diags: Vec<Diagnostic> = findings
            .iter()
            .map(|f| {
                self.generator.generate(
                    f,
                    &self.store,
                    &self.trend,
                    &self.capacity,
                    &self.config.host,
                )
            })
            .collect();

        debug!(
            findings = findings.len(),
            new_diags = new_diags.len(),
            "tick complete"
        );

        // 4. Resolve: remove active diagnostics no longer present
        self.active_diagnostics.retain(|active| {
            new_diags
                .iter()
                .any(|new| same_target_category(active, new))
        });

        // 5. Merge: add new diagnostics not already active, update existing
        for new in new_diags {
            if let Some(existing) = self
                .active_diagnostics
                .iter_mut()
                .find(|a| same_target_category(a, &new))
            {
                // Update evidence and severity for existing diagnostic
                existing.evidence = new.evidence;
                existing.severity = new.severity;
                existing.recommendation = new.recommendation;
            } else {
                self.active_diagnostics.push(new);
            }
        }

        // 6. Update stats
        self.stats.evaluations += 1;
        self.stats.rules_fired += findings.len() as u64;
        self.update_severity_counts();
    }

    fn update_severity_counts(&mut self) {
        let mut critical = 0u32;
        let mut warning = 0u32;
        let mut info = 0u32;

        for d in &self.active_diagnostics {
            match d.severity {
                Severity::Critical => critical += 1,
                Severity::Warning => warning += 1,
                Severity::Info => info += 1,
            }
        }

        self.stats.active_critical = critical;
        self.stats.active_warning = warning;
        self.stats.active_info = info;
    }

    /// Current active diagnostics.
    pub fn active_diagnostics(&self) -> &[Diagnostic] {
        &self.active_diagnostics
    }

    /// Current engine statistics.
    pub fn stats(&self) -> &AnalyzeStats {
        &self.stats
    }
}

/// Two diagnostics match the same issue if they share target and category.
fn same_target_category(a: &Diagnostic, b: &Diagnostic) -> bool {
    a.target == b.target && a.category == b.category
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::types::{CompareOp, Rule, RuleCondition};
    use aether_core::models::{DiagCategory, ProcessNode, ProcessState, Severity};
    use glam::Vec3;

    fn make_world(processes: Vec<ProcessNode>) -> Arc<RwLock<WorldGraph>> {
        let mut world = WorldGraph::new();
        for p in processes {
            world.add_process(p);
        }
        Arc::new(RwLock::new(world))
    }

    fn make_process(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: name.to_string(),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    /// Instant-fire rule (no sustained condition) for testing.
    fn cpu_test_rule() -> Rule {
        Rule {
            id: "test_cpu_high",
            name: "Test CPU High",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 90.0,
                sustained: None,
            },
            enabled: true,
        }
    }

    /// Create an engine with builtin rules + an instant-fire CPU test rule.
    fn test_engine() -> AnalyzeEngine {
        let mut engine = AnalyzeEngine::new(AnalyzeConfig::default());
        engine.rule_engine.add_rule(cpu_test_rule());
        engine
    }

    #[test]
    fn test_engine_produces_diagnostics() {
        let mut engine = test_engine();
        let world = make_world(vec![make_process(42, "hot", 99.0, 1024)]);

        engine.tick(&world);

        assert!(
            !engine.active_diagnostics().is_empty(),
            "cpu=99% should trigger at least one diagnostic"
        );
    }

    #[test]
    fn test_engine_resolves_cleared() {
        let mut engine = test_engine();

        // First tick: high CPU triggers diagnostic
        let world_hot = make_world(vec![make_process(42, "hot", 99.0, 1024)]);
        engine.tick(&world_hot);
        let count_after_hot = engine.active_diagnostics().len();
        assert!(count_after_hot > 0, "should have diagnostics for hot CPU");

        // Second tick: CPU back to normal — diagnostics should resolve
        let world_cool = make_world(vec![make_process(42, "hot", 5.0, 1024)]);
        engine.tick(&world_cool);
        assert_eq!(
            engine.active_diagnostics().len(),
            0,
            "diagnostics should resolve when CPU drops"
        );
    }

    #[test]
    fn test_engine_stats_count() {
        let mut engine = test_engine();
        let world = make_world(vec![make_process(1, "idle", 5.0, 1024)]);

        assert_eq!(engine.stats().evaluations, 0);
        engine.tick(&world);
        assert_eq!(
            engine.stats().evaluations,
            1,
            "evaluations should increment per tick"
        );
        engine.tick(&world);
        assert_eq!(engine.stats().evaluations, 2);
    }

    #[test]
    fn test_engine_empty_world_no_crash() {
        let mut engine = test_engine();
        let world = make_world(vec![]);

        engine.tick(&world);

        assert!(
            engine.active_diagnostics().is_empty(),
            "empty world should produce no diagnostics"
        );
        assert_eq!(engine.stats().evaluations, 1);
    }

    #[tokio::test]
    async fn test_run_sends_diagnostics() {
        let config = AnalyzeConfig {
            interval: Duration::from_millis(50),
            ..Default::default()
        };
        let mut engine = AnalyzeEngine::new(config);
        engine.rule_engine.add_rule(cpu_test_rule());

        let world = make_world(vec![make_process(1, "hot", 99.0, 1024)]);
        let (tx, mut rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            engine.run(world, tx, cancel_clone).await;
        });

        let batch = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel should not close");

        assert!(!batch.is_empty(), "should produce at least one diagnostic");
        cancel.cancel();
    }
}

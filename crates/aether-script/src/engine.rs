//! ScriptEngine — tokio task that evaluates compiled rules against system events.
//!
//! Receives `SystemEvent::MetricsUpdate` snapshots, evaluates all active rules
//! against each process, and sends triggered `RuleAction`s to the action channel.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, trace};

use aether_core::events::SystemEvent;
use aether_core::models::{ProcessNode, ProcessState};

use crate::codegen::WorldStateFFI;
use crate::runtime::{CompiledRuleSet, DurationTracker, RuleAction};

/// Statistics tracked by the engine across evaluations.
#[derive(Debug, Default)]
pub struct EngineStats {
    /// Total number of rule evaluations (one per process per snapshot).
    pub evaluations: u64,
    /// Total number of actions triggered.
    pub actions_triggered: u64,
    /// Per-rule match counts (rule_name → count).
    pub rule_match_counts: HashMap<String, u64>,
}

/// JIT rule evaluation engine running as a tokio task.
///
/// Listens for `SystemEvent::MetricsUpdate` on `world_rx`, evaluates all
/// compiled rules against each process, and sends `RuleAction`s to `action_tx`.
pub struct ScriptEngine {
    rules: Arc<ArcSwap<CompiledRuleSet>>,
    duration_tracker: DurationTracker,
    action_tx: mpsc::Sender<RuleAction>,
    stats: EngineStats,
}

impl ScriptEngine {
    /// Create a new engine with a shared ruleset and action output channel.
    pub fn new(
        rules: Arc<ArcSwap<CompiledRuleSet>>,
        action_tx: mpsc::Sender<RuleAction>,
    ) -> Self {
        Self {
            rules,
            duration_tracker: DurationTracker::new(),
            action_tx,
            stats: EngineStats::default(),
        }
    }

    /// Run the engine loop until cancellation.
    ///
    /// Processes `MetricsUpdate` events, evaluates rules, and sends actions.
    pub async fn run(
        &mut self,
        mut world_rx: mpsc::Receiver<SystemEvent>,
        cancel: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(event) = world_rx.recv() => {
                    if let SystemEvent::MetricsUpdate { snapshot } = event {
                        self.evaluate_snapshot(&snapshot.processes).await;
                    }
                }
            }
        }

        debug!("script engine stopped: {} evaluations, {} actions",
            self.stats.evaluations, self.stats.actions_triggered);
    }

    /// Current engine statistics.
    pub fn stats(&self) -> &EngineStats {
        &self.stats
    }

    /// Evaluate all rules against each process in the snapshot.
    async fn evaluate_snapshot(&mut self, processes: &[ProcessNode]) {
        // Collect all actions synchronously (WorldStateFFI has raw pointers, not Send).
        let all_actions = self.collect_actions(processes);

        for action in all_actions {
            *self
                .stats
                .rule_match_counts
                .entry(action.rule_name.clone())
                .or_default() += 1;
            self.stats.actions_triggered += 1;

            if self.action_tx.send(action).await.is_err() {
                trace!("action channel closed, stopping evaluation");
                return;
            }
        }

        self.stats.evaluations += processes.len() as u64;
    }

    /// Evaluate rules against all processes, returning collected actions.
    fn collect_actions(&mut self, processes: &[ProcessNode]) -> Vec<RuleAction> {
        let guard = self.rules.load();
        let mut all_actions = Vec::new();

        for process in processes {
            let name_bytes = process.name.as_bytes();
            let state = WorldStateFFI {
                pid: process.pid,
                cpu_percent: process.cpu_percent,
                mem_bytes: process.mem_bytes,
                mem_growth_percent: 0.0,
                state: process_state_to_u32(process.state),
                hp: process.hp,
                name_ptr: name_bytes.as_ptr(),
                name_len: name_bytes.len() as u32,
                process_count: 0,
                processes_ptr: std::ptr::null(),
            };

            let actions =
                guard.evaluate_with_tracker(&state, &mut self.duration_tracker);
            all_actions.extend(actions);
        }

        all_actions
    }
}

/// Convert `ProcessState` enum to the u32 representation used by JIT rules.
fn process_state_to_u32(state: ProcessState) -> u32 {
    match state {
        ProcessState::Running => 0,
        ProcessState::Sleeping => 1,
        ProcessState::Zombie => 2,
        ProcessState::Stopped => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, CmpOp, Condition, Expr, Literal, Rule, Severity};
    use crate::lexer::Span;
    use aether_core::models::{ProcessNode, SystemSnapshot};

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

    fn make_process(pid: u32, cpu: f32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: "test".to_string(),
            cpu_percent: cpu,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: glam::Vec3::ZERO,
        }
    }

    fn make_metrics_event(processes: Vec<ProcessNode>) -> SystemEvent {
        SystemEvent::MetricsUpdate {
            snapshot: SystemSnapshot {
                processes,
                edges: vec![],
                timestamp: std::time::SystemTime::now(),
            },
        }
    }

    #[tokio::test]
    async fn test_engine_evaluates_rules_and_produces_actions() {
        let compiled =
            CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
        let rules = Arc::new(ArcSwap::from_pointee(compiled));
        let (action_tx, mut action_rx) = mpsc::channel(16);
        let (world_tx, world_rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();

        let mut engine = ScriptEngine::new(rules, action_tx);

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            engine.run(world_rx, cancel_clone).await;
            engine
        });

        // Send a snapshot with one high-cpu and one normal process.
        let event = make_metrics_event(vec![
            make_process(1, 95.0), // should trigger
            make_process(2, 50.0), // should not trigger
        ]);
        world_tx.send(event).await.unwrap();

        // Receive the triggered action.
        let action = action_rx.recv().await.expect("should receive action");
        assert_eq!(action.rule_name, "high_cpu");
        assert_eq!(action.target_pid, 1);
        assert_eq!(action.action, 1, "action_type 1 = alert");

        cancel.cancel();
        let engine = handle.await.unwrap();
        assert_eq!(engine.stats().actions_triggered, 1);
        assert_eq!(engine.stats().evaluations, 2);
    }

    #[tokio::test]
    async fn test_engine_stats_track_counts() {
        let compiled =
            CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
        let rules = Arc::new(ArcSwap::from_pointee(compiled));
        let (action_tx, mut action_rx) = mpsc::channel(16);
        let (world_tx, world_rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();

        let mut engine = ScriptEngine::new(rules, action_tx);

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            engine.run(world_rx, cancel_clone).await;
            engine
        });

        // Two snapshots, each with one matching process.
        for pid in [10, 20] {
            let event = make_metrics_event(vec![make_process(pid, 99.0)]);
            world_tx.send(event).await.unwrap();
            action_rx.recv().await.expect("should receive action");
        }

        cancel.cancel();
        let engine = handle.await.unwrap();

        assert_eq!(engine.stats().actions_triggered, 2);
        assert_eq!(engine.stats().evaluations, 2);
        assert_eq!(
            *engine.stats().rule_match_counts.get("high_cpu").unwrap(),
            2
        );
    }
}

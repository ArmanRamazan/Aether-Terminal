//! Event types for inter-crate communication via tokio channels.
//!
//! Three event families:
//! - [`SystemEvent`] — OS-level process/topology changes
//! - [`GameEvent`] — gamification state changes (HP, XP, achievements)
//! - [`AgentAction`] — commands from AI agents via MCP

use crate::models::{Diagnostic, RecommendedAction, SystemSnapshot};

/// OS-level events produced by ingestion/eBPF layers.
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// A new process appeared in the system.
    ProcessCreated { pid: u32, name: String },
    /// A process exited.
    ProcessExited { pid: u32 },
    /// Periodic metrics refresh with a full system snapshot.
    MetricsUpdate { snapshot: SystemSnapshot },
    /// The process graph topology changed (edges added/removed).
    TopologyChange,
}

/// Gamification events produced by the gamification crate.
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// A process's HP changed (damage or healing).
    HpChanged { pid: u32, delta: f32, new_hp: f32 },
    /// XP was earned by the user.
    XpEarned { amount: u32, reason: String },
    /// An achievement was unlocked.
    AchievementUnlocked { id: String, name: String },
}

/// Diagnostic events from the analysis engine.
#[derive(Debug, Clone)]
pub enum DiagnosticEvent {
    /// A new batch of diagnostics from the analyze engine.
    DiagnosticsUpdated(Vec<Diagnostic>),
    /// A diagnostic was resolved (issue no longer present).
    DiagnosticResolved { id: u64 },
    /// A diagnostic was dismissed by the user.
    DiagnosticDismissed { id: u64 },
    /// User or agent requested an action for a diagnostic.
    DiagnosticActionRequested {
        diagnostic_id: u64,
        action: RecommendedAction,
    },
}

/// Actions requested by AI agents through MCP.
#[derive(Debug, Clone)]
pub enum AgentAction {
    /// Kill a process by PID.
    KillProcess { pid: u32 },
    /// Restart a system service by name.
    RestartService { name: String },
    /// Inspect a process (request detailed info).
    Inspect { pid: u32 },
    /// Execute a custom DSL script command.
    CustomScript { command: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProcessNode, ProcessState};
    use glam::Vec3;

    #[test]
    fn system_event_process_created() {
        let event = SystemEvent::ProcessCreated {
            pid: 42,
            name: "nginx".to_string(),
        };
        let cloned = event.clone();
        match cloned {
            SystemEvent::ProcessCreated { pid, name } => {
                assert_eq!(pid, 42);
                assert_eq!(name, "nginx");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn system_event_process_exited() {
        let event = SystemEvent::ProcessExited { pid: 99 };
        match event.clone() {
            SystemEvent::ProcessExited { pid } => assert_eq!(pid, 99),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn system_event_metrics_update() {
        let snapshot = SystemSnapshot {
            processes: vec![ProcessNode {
                pid: 1,
                ppid: 0,
                name: "init".to_string(),
                cpu_percent: 0.1,
                mem_bytes: 1024,
                state: ProcessState::Running,
                hp: 100.0,
                xp: 0,
                position_3d: Vec3::ZERO,
            }],
            edges: vec![],
            timestamp: std::time::SystemTime::now(),
        };
        let event = SystemEvent::MetricsUpdate { snapshot };
        match event.clone() {
            SystemEvent::MetricsUpdate { snapshot } => {
                assert_eq!(snapshot.processes.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn system_event_topology_change() {
        let event = SystemEvent::TopologyChange;
        let _ = event.clone();
    }

    #[test]
    fn game_event_hp_changed() {
        let event = GameEvent::HpChanged {
            pid: 10,
            delta: -25.0,
            new_hp: 75.0,
        };
        match event.clone() {
            GameEvent::HpChanged { pid, delta, new_hp } => {
                assert_eq!(pid, 10);
                assert_eq!(delta, -25.0);
                assert_eq!(new_hp, 75.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn game_event_xp_earned() {
        let event = GameEvent::XpEarned {
            amount: 50,
            reason: "uptime bonus".to_string(),
        };
        match event.clone() {
            GameEvent::XpEarned { amount, reason } => {
                assert_eq!(amount, 50);
                assert_eq!(reason, "uptime bonus");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn game_event_achievement_unlocked() {
        let event = GameEvent::AchievementUnlocked {
            id: "first_kill".to_string(),
            name: "First Kill".to_string(),
        };
        match event.clone() {
            GameEvent::AchievementUnlocked { id, name } => {
                assert_eq!(id, "first_kill");
                assert_eq!(name, "First Kill");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_action_kill_process() {
        let action = AgentAction::KillProcess { pid: 1337 };
        match action.clone() {
            AgentAction::KillProcess { pid } => assert_eq!(pid, 1337),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_action_restart_service() {
        let action = AgentAction::RestartService {
            name: "nginx".to_string(),
        };
        match action.clone() {
            AgentAction::RestartService { name } => assert_eq!(name, "nginx"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_action_inspect() {
        let action = AgentAction::Inspect { pid: 7 };
        match action.clone() {
            AgentAction::Inspect { pid } => assert_eq!(pid, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn agent_action_custom_script() {
        let action = AgentAction::CustomScript {
            command: "alert cpu > 90".to_string(),
        };
        match action.clone() {
            AgentAction::CustomScript { command } => assert_eq!(command, "alert cpu > 90"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_diagnostic_event_updated_holds_diagnostics() {
        use crate::metrics::HostId;
        use crate::models::{
            DiagCategory, DiagTarget, Evidence, Recommendation, Severity, Urgency,
        };
        use std::time::Instant;

        let diag = Diagnostic {
            id: 1,
            host: HostId::new("localhost"),
            target: DiagTarget::Process {
                pid: 42,
                name: "nginx".to_string(),
            },
            severity: Severity::Warning,
            category: DiagCategory::CpuSaturation,
            summary: "High CPU usage".to_string(),
            evidence: vec![Evidence {
                metric: "cpu_percent".to_string(),
                current: 95.0,
                threshold: 90.0,
                trend: None,
                context: "CPU at 95%".to_string(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::ReduceLoad {
                    suggestion: "Consider renice".to_string(),
                },
                reason: "CPU consistently above threshold".to_string(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        };

        let event = DiagnosticEvent::DiagnosticsUpdated(vec![diag]);
        match &event {
            DiagnosticEvent::DiagnosticsUpdated(diags) => {
                assert_eq!(diags.len(), 1, "should hold one diagnostic");
                assert_eq!(diags[0].id, 1);
                assert_eq!(diags[0].summary, "High CPU usage");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_diagnostic_event_clone() {
        let event = DiagnosticEvent::DiagnosticResolved { id: 42 };
        let cloned = event.clone();
        match cloned {
            DiagnosticEvent::DiagnosticResolved { id } => assert_eq!(id, 42),
            _ => panic!("wrong variant"),
        }

        let event2 = DiagnosticEvent::DiagnosticDismissed { id: 7 };
        let cloned2 = event2.clone();
        match cloned2 {
            DiagnosticEvent::DiagnosticDismissed { id } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }

        let event3 = DiagnosticEvent::DiagnosticActionRequested {
            diagnostic_id: 10,
            action: RecommendedAction::KillProcess {
                pid: 5,
                reason: "runaway process".to_string(),
            },
        };
        let cloned3 = event3.clone();
        match cloned3 {
            DiagnosticEvent::DiagnosticActionRequested {
                diagnostic_id,
                action,
            } => {
                assert_eq!(diagnostic_id, 10);
                match action {
                    RecommendedAction::KillProcess { pid, reason } => {
                        assert_eq!(pid, 5);
                        assert_eq!(reason, "runaway process");
                    }
                    _ => panic!("wrong action variant"),
                }
            }
            _ => panic!("wrong variant"),
        }
    }
}

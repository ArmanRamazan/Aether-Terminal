//! AnalyzeEngine — orchestrates periodic diagnostic analysis.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::metrics::HostId;
use aether_core::models::{
    DiagCategory, DiagTarget, Diagnostic, Evidence, Recommendation, RecommendedAction, Severity,
    Urgency,
};
use aether_core::WorldGraph;

use crate::analyzers::capacity::CapacityAnalyzer;
use crate::analyzers::trend::{TrendAnalyzer, TrendClass};
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

/// Orchestrates metric collection, trend/capacity analysis, and diagnostic generation.
pub struct AnalyzeEngine {
    config: AnalyzeConfig,
    store: MetricStore,
    trend: TrendAnalyzer,
    capacity: CapacityAnalyzer,
    next_diag_id: u64,
}

impl AnalyzeEngine {
    pub fn new(config: AnalyzeConfig) -> Self {
        let store = MetricStore::new(config.history_capacity);
        Self {
            config,
            store,
            trend: TrendAnalyzer,
            capacity: CapacityAnalyzer,
            next_diag_id: 1,
        }
    }

    /// Run the analysis loop, sending batches of diagnostics on each tick.
    pub async fn run(
        mut self,
        world: Arc<RwLock<WorldGraph>>,
        diag_tx: mpsc::Sender<Vec<Diagnostic>>,
        cancel: CancellationToken,
    ) {
        let mut interval = tokio::time::interval(self.config.interval);
        // First tick fires immediately — skip it to let metrics accumulate.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    let diagnostics = self.analyze_once(&world);
                    if !diagnostics.is_empty()
                        && diag_tx.send(diagnostics).await.is_err()
                    {
                        break;
                    }
                }
            }
        }
    }

    fn analyze_once(&mut self, world: &Arc<RwLock<WorldGraph>>) -> Vec<Diagnostic> {
        let graph = match world.read() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };

        self.store.ingest_world_state(&self.config.host, &graph);
        let mut diagnostics = Vec::new();

        // Analyze each process for CPU and memory issues.
        for proc in graph.processes() {
            self.check_cpu(proc.pid, &proc.name, proc.cpu_percent, &mut diagnostics);
            self.check_memory(proc.pid, &proc.name, proc.mem_bytes, &mut diagnostics);
        }

        // Host-level capacity checks.
        self.check_host_cpu(&mut diagnostics);

        diagnostics
    }

    fn check_cpu(&mut self, pid: u32, name: &str, cpu: f32, out: &mut Vec<Diagnostic>) {
        if cpu < 90.0 {
            return;
        }

        let series = self.store.get(&self.config.host, Some(pid), "cpu_percent");
        let trend_info = series.map(|s| self.trend.classify(s, Duration::from_secs(60)));

        let severity = if cpu >= 99.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };

        out.push(Diagnostic {
            id: self.next_id(),
            host: self.config.host.clone(),
            target: DiagTarget::Process {
                pid,
                name: name.to_string(),
            },
            severity,
            category: DiagCategory::CpuSaturation,
            summary: format!("{name} (pid {pid}) CPU at {cpu:.1}%"),
            evidence: vec![Evidence {
                metric: "cpu_percent".into(),
                current: cpu as f64,
                threshold: 90.0,
                trend: match &trend_info {
                    Some(TrendClass::Growing { rate }) => Some(*rate),
                    _ => None,
                },
                context: format!("Process CPU usage: {cpu:.1}%"),
            }],
            recommendation: Recommendation {
                action: if cpu >= 99.0 {
                    RecommendedAction::Investigate {
                        what: format!("Process {name} (pid {pid}) is saturating CPU"),
                    }
                } else {
                    RecommendedAction::NoAction {
                        reason: "Monitor for sustained high CPU".into(),
                    }
                },
                reason: "High CPU usage detected".into(),
                urgency: if cpu >= 99.0 {
                    Urgency::Soon
                } else {
                    Urgency::Informational
                },
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        });
    }

    fn check_memory(&mut self, pid: u32, name: &str, mem_bytes: u64, out: &mut Vec<Diagnostic>) {
        const HIGH_MEM: u64 = 1_073_741_824; // 1 GB

        if mem_bytes < HIGH_MEM {
            return;
        }

        let series = self.store.get(&self.config.host, Some(pid), "mem_bytes");
        let trend_info = series.map(|s| self.trend.classify(s, Duration::from_secs(300)));

        let is_growing = matches!(&trend_info, Some(TrendClass::Growing { .. }));
        let severity = if is_growing {
            Severity::Warning
        } else {
            Severity::Info
        };
        let category = if is_growing {
            DiagCategory::MemoryLeak
        } else {
            DiagCategory::MemoryPressure
        };

        let mb = mem_bytes as f64 / 1_048_576.0;
        out.push(Diagnostic {
            id: self.next_id(),
            host: self.config.host.clone(),
            target: DiagTarget::Process {
                pid,
                name: name.to_string(),
            },
            severity,
            category,
            summary: format!("{name} (pid {pid}) using {mb:.0} MB"),
            evidence: vec![Evidence {
                metric: "mem_bytes".into(),
                current: mem_bytes as f64,
                threshold: HIGH_MEM as f64,
                trend: match &trend_info {
                    Some(TrendClass::Growing { rate }) => Some(*rate),
                    _ => None,
                },
                context: format!("Process memory: {mb:.0} MB"),
            }],
            recommendation: Recommendation {
                action: if is_growing {
                    RecommendedAction::Investigate {
                        what: format!("Possible memory leak in {name} (pid {pid})"),
                    }
                } else {
                    RecommendedAction::NoAction {
                        reason: "High but stable memory usage".into(),
                    }
                },
                reason: "High memory usage detected".into(),
                urgency: if is_growing {
                    Urgency::Soon
                } else {
                    Urgency::Informational
                },
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        });
    }

    fn check_host_cpu(&mut self, out: &mut Vec<Diagnostic>) {
        let series = match self.store.get(&self.config.host, None, "total_cpu") {
            Some(s) => s,
            None => return,
        };

        let current = match series.last() {
            Some(s) => s.value,
            None => return,
        };

        // Host total CPU > 400% (equivalent to 4 fully loaded cores) with growing trend.
        let report = self.capacity.analyze(current, 800.0, &self.trend, series);

        if report.usage_percent > 80.0 {
            out.push(Diagnostic {
                id: self.next_id(),
                host: self.config.host.clone(),
                target: DiagTarget::Host(self.config.host.clone()),
                severity: Severity::Warning,
                category: DiagCategory::CapacityRisk,
                summary: format!("Host CPU load at {current:.0}% aggregate"),
                evidence: vec![Evidence {
                    metric: "total_cpu".into(),
                    current,
                    threshold: 800.0,
                    trend: match &report.trend {
                        TrendClass::Growing { rate } => Some(*rate),
                        _ => None,
                    },
                    context: format!(
                        "Aggregate CPU: {current:.0}%, headroom: {:.0}",
                        report.headroom
                    ),
                }],
                recommendation: Recommendation {
                    action: RecommendedAction::ReduceLoad {
                        suggestion: "Consider reducing concurrent workloads".into(),
                    },
                    reason: "Host approaching CPU capacity".into(),
                    urgency: if report.time_to_exhaustion.is_some() {
                        Urgency::Soon
                    } else {
                        Urgency::Planning
                    },
                    auto_executable: false,
                },
                detected_at: Instant::now(),
                resolved_at: None,
            });
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_diag_id;
        self.next_diag_id += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState};
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

    #[test]
    fn test_analyze_once_no_issues() {
        let config = AnalyzeConfig::default();
        let mut engine = AnalyzeEngine::new(config);
        let world = make_world(vec![make_process(1, "idle", 5.0, 1024)]);
        let diagnostics = engine.analyze_once(&world);
        assert!(
            diagnostics.is_empty(),
            "healthy process should produce no diagnostics"
        );
    }

    #[test]
    fn test_analyze_once_high_cpu() {
        let config = AnalyzeConfig::default();
        let mut engine = AnalyzeEngine::new(config);
        let world = make_world(vec![make_process(42, "busy", 95.0, 1024)]);
        let diagnostics = engine.analyze_once(&world);
        assert_eq!(diagnostics.len(), 1, "should detect high CPU");
        assert!(matches!(
            diagnostics[0].category,
            DiagCategory::CpuSaturation
        ));
    }

    #[test]
    fn test_analyze_once_high_memory() {
        let config = AnalyzeConfig::default();
        let mut engine = AnalyzeEngine::new(config);
        let world = make_world(vec![make_process(7, "hungry", 10.0, 2_000_000_000)]);
        let diagnostics = engine.analyze_once(&world);
        assert_eq!(diagnostics.len(), 1, "should detect high memory");
        assert!(
            matches!(
                diagnostics[0].category,
                DiagCategory::MemoryPressure | DiagCategory::MemoryLeak
            ),
            "category should be memory-related"
        );
    }

    #[test]
    fn test_diag_ids_increment() {
        let config = AnalyzeConfig::default();
        let mut engine = AnalyzeEngine::new(config);
        let world = make_world(vec![
            make_process(1, "a", 95.0, 2_000_000_000),
            make_process(2, "b", 99.0, 1024),
        ]);
        let diagnostics = engine.analyze_once(&world);
        let ids: Vec<u64> = diagnostics.iter().map(|d| d.id).collect();
        for w in ids.windows(2) {
            assert!(w[1] > w[0], "IDs should be monotonically increasing");
        }
    }

    #[tokio::test]
    async fn test_run_sends_diagnostics() {
        let config = AnalyzeConfig {
            interval: Duration::from_millis(50),
            ..Default::default()
        };
        let engine = AnalyzeEngine::new(config);
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

//! AnalyzeEngine — orchestrates periodic diagnostic analysis.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use aether_core::metrics::HostId;
use aether_core::models::{Diagnostic, Severity};
use aether_core::WorldGraph;

use crate::analyzers::capacity::CapacityAnalyzer;
use crate::analyzers::trend::TrendAnalyzer;
use crate::collectors::cgroup::CgroupCollector;
use crate::collectors::procfs::ProcfsCollector;
use crate::collectors::ProcessProfile;
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
    /// Enable procfs/cgroup profiling for top processes.
    pub enable_profiling: bool,
    /// Number of top processes (by CPU) to profile per tick.
    pub top_n_profiled: usize,
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            history_capacity: 3600,
            host: HostId::new("local"),
            enable_profiling: true,
            top_n_profiled: 10,
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
    procfs: ProcfsCollector,
    cgroup: CgroupCollector,
    config: AnalyzeConfig,
    active_diagnostics: Vec<Diagnostic>,
    profiles: HashMap<u32, ProcessProfile>,
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
            procfs: ProcfsCollector::new(),
            cgroup: CgroupCollector::new(),
            config,
            active_diagnostics: Vec::new(),
            profiles: HashMap::new(),
            stats: AnalyzeStats::default(),
        }
    }

    /// Create an engine with custom collector roots (for testing).
    #[cfg(test)]
    fn with_collectors(
        config: AnalyzeConfig,
        procfs: ProcfsCollector,
        cgroup: CgroupCollector,
    ) -> Self {
        let store = MetricStore::new(config.history_capacity);
        let mut rule_engine = RuleEngine::new();
        rule_engine.load_builtin();

        Self {
            store,
            rule_engine,
            trend: TrendAnalyzer,
            capacity: CapacityAnalyzer,
            generator: RecommendationGenerator::new(),
            procfs,
            cgroup,
            config,
            active_diagnostics: Vec::new(),
            profiles: HashMap::new(),
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

    /// Current process profiles from the latest tick.
    pub fn profiles(&self) -> &HashMap<u32, ProcessProfile> {
        &self.profiles
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

        // 1. Ingest current state from WorldGraph.
        self.store.ingest_world_state(&self.config.host, &graph);
        drop(graph);

        // 2. Collect host and process profiles if profiling is enabled.
        let limits_map = if self.config.enable_profiling {
            self.collect_profiles()
        } else {
            self.profiles.clear();
            HashMap::new()
        };

        // 3. Evaluate rules with collected limits.
        let findings = self
            .rule_engine
            .evaluate(&self.store, &self.config.host, &limits_map);

        // 4. Generate diagnostics from findings.
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
            profiles = self.profiles.len(),
            "tick complete"
        );

        // 5. Resolve: remove active diagnostics no longer present.
        self.active_diagnostics.retain(|active| {
            new_diags
                .iter()
                .any(|new| same_target_category(active, new))
        });

        // 6. Merge: add new diagnostics not already active, update existing.
        for new in new_diags {
            if let Some(existing) = self
                .active_diagnostics
                .iter_mut()
                .find(|a| same_target_category(a, &new))
            {
                existing.evidence = new.evidence;
                existing.severity = new.severity;
                existing.recommendation = new.recommendation;
            } else {
                self.active_diagnostics.push(new);
            }
        }

        // 7. Update stats.
        self.stats.evaluations += 1;
        self.stats.rules_fired += findings.len() as u64;
        self.update_severity_counts();
    }

    /// Collect host profile and per-process profiles/limits for top-N processes.
    fn collect_profiles(&mut self) -> HashMap<u32, ProcessLimits> {
        let host = &self.config.host;
        let now = Instant::now();

        // Collect host profile → feed into store.
        match self.procfs.host_profile() {
            Ok(hp) => {
                self.store
                    .push_sample(host, None, "loadavg_1", now, hp.loadavg_1);
                self.store
                    .push_sample(host, None, "loadavg_5", now, hp.loadavg_5);
                self.store
                    .push_sample(host, None, "loadavg_15", now, hp.loadavg_15);
                self.store
                    .push_sample(host, None, "mem_total", now, hp.mem_total as f64);
                self.store
                    .push_sample(host, None, "mem_available", now, hp.mem_available as f64);
                self.store
                    .push_sample(host, None, "swap_total", now, hp.swap_total as f64);
                self.store
                    .push_sample(host, None, "swap_free", now, hp.swap_free as f64);
            }
            Err(e) => {
                warn!("host profile collection failed: {e}");
            }
        }

        // Get top N processes by CPU from the store.
        let top_pids = self.top_pids_by_cpu(self.config.top_n_profiled);

        // Collect process profiles and cgroup limits for top N.
        self.profiles.clear();
        let mut limits_map = HashMap::new();

        for pid in top_pids {
            match self.procfs.process_profile(pid) {
                Ok(profile) => {
                    // Feed process-level metrics into store.
                    self.store
                        .push_sample(host, Some(pid), "threads", now, profile.threads as f64);
                    self.store.push_sample(
                        host,
                        Some(pid),
                        "open_fds",
                        now,
                        profile.open_fds as f64,
                    );
                    self.store.push_sample(
                        host,
                        Some(pid),
                        "io_read_bytes",
                        now,
                        profile.io_read_bytes as f64,
                    );
                    self.store.push_sample(
                        host,
                        Some(pid),
                        "io_write_bytes",
                        now,
                        profile.io_write_bytes as f64,
                    );
                    self.profiles.insert(pid, profile);
                }
                Err(e) => {
                    warn!(pid, "process profile collection failed: {e}");
                }
            }

            match self.cgroup.limits(pid) {
                Ok(limits) => {
                    limits_map.insert(pid, limits);
                }
                Err(e) => {
                    debug!(pid, "cgroup limits collection failed: {e}");
                }
            }
        }

        limits_map
    }

    /// Return PIDs of top N processes by CPU usage from the store.
    fn top_pids_by_cpu(&self, n: usize) -> Vec<u32> {
        let host = &self.config.host;
        let mut pid_cpu: Vec<(u32, f64)> = self
            .store
            .process_pids(host)
            .into_iter()
            .filter_map(|pid| {
                self.store
                    .get(host, Some(pid), "cpu_percent")
                    .and_then(|ts| ts.last())
                    .map(|s| (pid, s.value))
            })
            .collect();

        pid_cpu.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pid_cpu.into_iter().take(n).map(|(pid, _)| pid).collect()
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
            enable_profiling: false,
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

    fn setup_fake_proc(dir: &std::path::Path, pid: u32) {
        let pid_dir = dir.join(pid.to_string());
        std::fs::create_dir_all(pid_dir.join("fd")).unwrap();
        std::fs::write(pid_dir.join("fd/0"), "").unwrap();
        std::fs::write(pid_dir.join("fd/1"), "").unwrap();
        std::fs::write(
            pid_dir.join("status"),
            "Name:\tfake\nThreads:\t8\nvoluntary_ctxt_switches:\t50\nnonvoluntary_ctxt_switches:\t5\n",
        )
        .unwrap();
        std::fs::write(
            pid_dir.join("io"),
            "rchar: 1000\nwchar: 500\nread_bytes: 4096\nwrite_bytes: 2048\n",
        )
        .unwrap();
    }

    fn setup_fake_host(dir: &std::path::Path) {
        std::fs::write(dir.join("loadavg"), "1.50 2.00 1.75 3/200 12345\n").unwrap();
        std::fs::write(
            dir.join("meminfo"),
            "MemTotal:       16000000 kB\nMemFree:         2000000 kB\nMemAvailable:    8000000 kB\nSwapTotal:       4000000 kB\nSwapFree:        3000000 kB\n",
        )
        .unwrap();
    }

    #[test]
    fn test_engine_with_collectors() {
        let tmp = tempfile::tempdir().unwrap();
        setup_fake_host(tmp.path());
        setup_fake_proc(tmp.path(), 42);

        let config = AnalyzeConfig {
            enable_profiling: true,
            top_n_profiled: 10,
            ..Default::default()
        };
        let procfs = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let cgroup = CgroupCollector::with_root(tmp.path().to_path_buf());
        let mut engine = AnalyzeEngine::with_collectors(config, procfs, cgroup);
        engine.rule_engine.add_rule(cpu_test_rule());

        let world = make_world(vec![make_process(42, "hot", 99.0, 1024)]);
        engine.tick(&world);

        // Host metrics should be in the store.
        let host = &engine.config.host;
        assert!(
            engine.store.get(host, None, "loadavg_1").is_some(),
            "host profile should populate loadavg_1"
        );
        assert!(
            engine.store.get(host, None, "mem_total").is_some(),
            "host profile should populate mem_total"
        );

        // Process profile should be collected for the top process.
        assert!(
            engine.profiles().contains_key(&42),
            "process 42 should have a profile"
        );
        let profile = &engine.profiles()[&42];
        assert_eq!(profile.threads, 8, "threads from fake /proc/42/status");
        assert_eq!(profile.open_fds, 2, "fds from fake /proc/42/fd");

        // Process-level metrics should be in the store.
        assert!(
            engine.store.get(host, Some(42), "threads").is_some(),
            "threads metric should be in store"
        );

        assert_eq!(engine.stats().evaluations, 1);
    }

    #[test]
    fn test_engine_without_collectors_fallback() {
        // Point collectors at nonexistent dirs — everything should fail gracefully.
        let tmp = tempfile::tempdir().unwrap();
        let bogus = tmp.path().join("nonexistent");

        let config = AnalyzeConfig {
            enable_profiling: true,
            top_n_profiled: 5,
            ..Default::default()
        };
        let procfs = ProcfsCollector::with_root(bogus.clone());
        let cgroup = CgroupCollector::with_root(bogus);
        let mut engine = AnalyzeEngine::with_collectors(config, procfs, cgroup);
        engine.rule_engine.add_rule(cpu_test_rule());

        let world = make_world(vec![make_process(1, "hot", 99.0, 1024)]);
        engine.tick(&world);

        // Engine should still produce diagnostics from WorldGraph data.
        assert!(
            !engine.active_diagnostics().is_empty(),
            "engine should still work when collectors fail"
        );
        assert!(
            engine.profiles().is_empty(),
            "no profiles should be collected from bogus paths"
        );
        assert_eq!(engine.stats().evaluations, 1);
    }

    #[test]
    fn test_top_n_limits_profiling() {
        let tmp = tempfile::tempdir().unwrap();
        setup_fake_host(tmp.path());

        // Create fake /proc entries for 5 processes.
        for pid in [10, 20, 30, 40, 50] {
            setup_fake_proc(tmp.path(), pid);
        }

        let config = AnalyzeConfig {
            enable_profiling: true,
            top_n_profiled: 3,
            ..Default::default()
        };
        let procfs = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let cgroup = CgroupCollector::with_root(tmp.path().to_path_buf());
        let mut engine = AnalyzeEngine::with_collectors(config, procfs, cgroup);

        // Create processes with varying CPU — top 3 should be pid 50, 40, 30.
        let world = make_world(vec![
            make_process(10, "low1", 10.0, 1024),
            make_process(20, "low2", 20.0, 1024),
            make_process(30, "mid", 50.0, 1024),
            make_process(40, "high", 80.0, 1024),
            make_process(50, "top", 99.0, 1024),
        ]);

        engine.tick(&world);

        // Only top 3 processes should have profiles.
        assert_eq!(
            engine.profiles().len(),
            3,
            "only top_n_profiled=3 processes should be profiled"
        );
        assert!(
            engine.profiles().contains_key(&50),
            "pid 50 (99% cpu) should be profiled"
        );
        assert!(
            engine.profiles().contains_key(&40),
            "pid 40 (80% cpu) should be profiled"
        );
        assert!(
            engine.profiles().contains_key(&30),
            "pid 30 (50% cpu) should be profiled"
        );
        assert!(
            !engine.profiles().contains_key(&10),
            "pid 10 (10% cpu) should NOT be profiled"
        );
    }
}

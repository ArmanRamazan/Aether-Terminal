# Aether-Terminal MVP Evolution: Deterministic Diagnostic Engine

**Date**: 2026-03-10
**Status**: Approved
**Author**: Arman Ramazan
**Supersedes**: 2026-03-08-aether-terminal-design.md (PoC)

---

## Context

PoC phase (MS1-MS8) delivered 9-crate workspace with 18K LOC, 403 tests. All crates compile and pass tests. However, core analysis relies on ML models (tract-onnx) that don't exist yet, and MCP is required for AI interaction. This is a showcase, not a usable tool.

**MVP goal**: Transform Aether-Terminal from portfolio showcase into a production-capable system monitor with real diagnostic value.

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Analysis approach | Deterministic stateful (no ML required) | Predictable, explainable, testable. ML as optional layer later |
| TUI diagnostics | Built-in smart dashboard | No LLM dependency for core UX. Show "what + why + fix" directly |
| MCP role | Optional external API | Core works standalone. MCP for who wants AI agent integration |
| Prometheus | Bidirectional (export + consume) | Exporter for existing infra, consumer for cluster view |
| Stack traces | procfs + perf sampling | Deep "where in code" analysis without source-level mapping |
| Config analysis | cgroup limits, ulimits, sysctl | "Your limit is X, usage is Y, recommendation Z" |
| Scope | Single host MVP, cluster-ready data model | HostId in all models, TimeSeries abstraction over source |

## Architecture Delta (PoC → MVP)

### New Crates

```
aether-analyze (lib)  — deterministic diagnostic engine
  ├── collectors/     — stack traces, /proc, perf, cgroup limits
  ├── analyzers/      — trend, correlation, capacity, anomaly
  ├── rules/          — rule engine + 30+ builtin rules
  └── recommendations/— Diagnostic + Recommendation generation

aether-metrics (lib)  — Prometheus integration
  ├── exporter/       — /metrics endpoint (axum)
  └── consumer/       — PromQL client, remote read
```

### Modified Crates

```
aether-core          — +HostId, +TimeSeries, +Diagnostic, +Recommendation
aether-ingestion     — +stack trace collector via procfs
aether-render        — +Diagnostics tab (F6), +diag indicators in Overview/3D
aether-mcp           — +get_diagnostics tool
aether-gamification  — +XP for resolved diagnostics
aether-terminal      — wire analyze + metrics, new CLI flags
```

### Unchanged Crates

```
aether-ebpf          — stays as is (provides richer data to analyze)
aether-predict       — stays as is (can be wired as analyzer later)
aether-script        — stays as is (user rules complement builtin rules)
```

### Full Crate Graph (11 crates)

```
aether-terminal (bin)
  +-- aether-core           (types, traits, graph, events)
  +-- aether-ingestion      (sysinfo, eBPF bridge, pipeline)
  +-- aether-ebpf           (BPF loader, ring buffer)
  +-- aether-analyze  [NEW] (diagnostic engine)
  +-- aether-metrics  [NEW] (Prometheus exporter + consumer)
  +-- aether-predict        (ML inference, optional)
  +-- aether-script         (JIT DSL, user rules)
  +-- aether-render         (TUI + 3D engine)
  +-- aether-mcp            (MCP server)
  +-- aether-gamification   (HP, XP, achievements)
```

All library crates depend ONLY on aether-core (hexagonal architecture preserved).

---

## Data Model Extensions (aether-core)

### HostId — cluster-ready identifier

```rust
/// Unique host identifier. "local" for current machine.
/// In cluster mode: hostname, IP, or Kubernetes node name.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostId(pub String);

impl Default for HostId {
    fn default() -> Self { Self("local".to_string()) }
}
```

### TimeSeries — universal metric format

```rust
/// A single metric sample with timestamp.
#[derive(Debug, Clone)]
pub struct MetricSample {
    pub timestamp: Instant,
    pub value: f64,
}

/// Named metric with label set and ring buffer of samples.
/// Universal format: local procfs, eBPF, and Prometheus all produce TimeSeries.
pub struct TimeSeries {
    pub name: String,                     // "cpu_percent", "mem_bytes"
    pub labels: BTreeMap<String, String>, // host, pid, process_name, container
    pub samples: VecDeque<MetricSample>,  // ring buffer
    pub capacity: usize,                  // default 3600 (1h at 1Hz)
}

impl TimeSeries {
    pub fn push(&mut self, value: f64);
    pub fn last(&self) -> Option<&MetricSample>;
    pub fn rate(&self, window: Duration) -> Option<f64>;
    pub fn avg(&self, window: Duration) -> f64;
    pub fn min_max(&self, window: Duration) -> (f64, f64);
    pub fn values(&self) -> impl Iterator<Item = f64>;
    pub fn window(&self, duration: Duration) -> &[MetricSample];
}
```

### Diagnostic — analysis output

```rust
/// Target of a diagnostic finding.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum DiagTarget {
    Process { pid: u32, name: String },
    Host(HostId),
    Container { id: String, name: String },
    Disk { mount: String },
    Network { interface: String },
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Category of the diagnostic.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum DiagCategory {
    MemoryLeak,
    MemoryPressure,
    CpuSaturation,
    CpuSpike,
    DiskPressure,
    DiskIoHeavy,
    FdExhaustion,
    ConnectionSurge,
    ZombieAccumulation,
    ThreadExplosion,
    CrashLoop,
    ConfigMismatch,
    CapacityRisk,
    CorrelatedAnomaly,
}

/// Evidence supporting the diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub metric: String,      // "memory_bytes"
    pub current: f64,        // 503316480.0
    pub threshold: f64,      // 536870912.0 (512MB)
    pub trend: Option<f64>,  // +2.3MB/min
    pub context: String,     // "93.7% of cgroup limit"
}

/// What action to take.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum RecommendedAction {
    ScaleUp { resource: String, from: String, to: String },
    Restart { reason: String },
    RaiseLimits { limit_name: String, from: String, to: String },
    ReduceLoad { suggestion: String },
    Investigate { what: String },
    KillProcess { pid: u32, reason: String },
    NoAction { reason: String },
}

/// How urgent is the recommendation.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Urgency {
    Immediate,     // act now or data loss / downtime
    Soon,          // within 1 hour
    Planning,      // within 24 hours
    Informational, // no action needed, FYI
}

/// A complete diagnostic finding.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub id: u64,
    pub host: HostId,
    pub target: DiagTarget,
    pub severity: Severity,
    pub category: DiagCategory,
    pub summary: String,
    pub evidence: Vec<Evidence>,
    pub recommendation: Recommendation,
    pub detected_at: Instant,
    pub resolved_at: Option<Instant>,
}

/// Actionable recommendation.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub action: RecommendedAction,
    pub reason: String,
    pub urgency: Urgency,
    pub auto_executable: bool,
}
```

---

## aether-analyze: Diagnostic Engine

### Module Structure

```
crates/aether-analyze/
├── Cargo.toml
├── CLAUDE.md
└── src/
    ├── lib.rs
    ├── error.rs              — AnalyzeError enum (thiserror)
    ├── engine.rs             — AnalyzeEngine: main async task
    ├── store.rs              — MetricStore: TimeSeries storage per (host, pid, metric)
    ├── collectors/
    │   ├── mod.rs
    │   ├── procfs.rs         — /proc/<pid>/stack, status, smaps, fd, wchan
    │   ├── perf.rs           — perf_event_open() for CPU profiling
    │   └── cgroup.rs         — cgroup v1/v2 limits: memory.max, cpu.max, pids.max
    ├── analyzers/
    │   ├── mod.rs
    │   ├── trend.rs          — linear regression, slope, time-to-threshold
    │   ├── correlation.rs    — cross-metric correlation (Pearson coefficient)
    │   ├── capacity.rs       — capacity planning: usage vs limit vs trend
    │   └── anomaly.rs        — z-score outlier detection, IQR method
    ├── rules/
    │   ├── mod.rs
    │   ├── engine.rs         — RuleEngine: evaluate rules against MetricStore
    │   ├── types.rs          — Rule, Condition, Threshold, Duration requirements
    │   └── builtin.rs        — 30+ built-in rules (memory, cpu, disk, fd, process)
    └── recommendations/
        ├── mod.rs
        └── generator.rs      — RuleFinding + Evidence → Diagnostic + Recommendation
```

### AnalyzeEngine — main async task

```rust
/// Central diagnostic engine. Runs as tokio task.
pub struct AnalyzeEngine {
    store: MetricStore,
    collectors: Vec<Box<dyn Collector>>,
    rule_engine: RuleEngine,
    trend: TrendAnalyzer,
    capacity: CapacityAnalyzer,
    correlation: CorrelationAnalyzer,
    anomaly: AnomalyDetector,
    generator: RecommendationGenerator,
    config: AnalyzeConfig,
}

pub struct AnalyzeConfig {
    pub interval: Duration,            // default 5s
    pub history_capacity: usize,       // default 3600 (1h)
    pub stack_trace_sampling: bool,    // default true on Linux
    pub cgroup_detection: bool,        // default true
}

impl AnalyzeEngine {
    /// Main loop: collect → store → analyze → emit diagnostics
    pub async fn run(
        &mut self,
        world_rx: broadcast::Receiver<WorldState>,
        prometheus_rx: Option<mpsc::Receiver<Vec<TimeSeries>>>,
        diag_tx: mpsc::Sender<Vec<Diagnostic>>,
        cancel: CancellationToken,
    );
}
```

### MetricStore — TimeSeries storage

```rust
/// Stores TimeSeries per (host, pid, metric_name).
/// Ring buffer — old samples automatically evicted.
pub struct MetricStore {
    series: HashMap<MetricKey, TimeSeries>,
    capacity: usize,
}

#[derive(Hash, Eq, PartialEq)]
struct MetricKey {
    host: HostId,
    pid: Option<u32>,       // None for host-level metrics
    metric: String,
}

impl MetricStore {
    /// Ingest a WorldState snapshot into TimeSeries
    pub fn ingest_world_state(&mut self, host: &HostId, world: &WorldGraph);
    /// Ingest remote TimeSeries from Prometheus
    pub fn ingest_remote(&mut self, series: Vec<TimeSeries>);
    /// Get series for a specific process
    pub fn process_series(&self, host: &HostId, pid: u32) -> ProcessMetrics;
    /// Get all series for a host
    pub fn host_series(&self, host: &HostId) -> HostMetrics;
    /// List all known hosts
    pub fn hosts(&self) -> Vec<&HostId>;
}
```

### Collectors

```rust
/// Trait for deep data collectors.
pub(crate) trait Collector: Send + Sync {
    /// Collect data for a specific process.
    fn collect_process(&self, pid: u32) -> Result<ProcessProfile, AnalyzeError>;
    /// Collect host-level data.
    fn collect_host(&self) -> Result<HostProfile, AnalyzeError>;
}

/// Stack trace + resource data from /proc
pub struct ProcfsCollector;

impl ProcfsCollector {
    /// Read /proc/<pid>/stack — kernel stack trace
    pub fn kernel_stack(&self, pid: u32) -> Result<Vec<StackFrame>>;
    /// Read /proc/<pid>/status — detailed process status
    pub fn process_status(&self, pid: u32) -> Result<ProcStatus>;
    /// Read /proc/<pid>/smaps_rollup — memory breakdown
    pub fn memory_map(&self, pid: u32) -> Result<MemoryMap>;
    /// Count /proc/<pid>/fd/* — open file descriptors
    pub fn open_fds(&self, pid: u32) -> Result<FdInfo>;
    /// Read /proc/<pid>/io — I/O statistics
    pub fn io_stats(&self, pid: u32) -> Result<IoStats>;
}

/// CPU profiling via perf_event_open
pub struct PerfCollector;

impl PerfCollector {
    /// Sample CPU call stacks for a process over duration
    pub fn cpu_profile(&self, pid: u32, duration: Duration) -> Result<CpuProfile>;
}

/// cgroup limits reader
pub struct CgroupCollector;

impl CgroupCollector {
    /// Detect cgroup version and read limits
    pub fn limits(&self, pid: u32) -> Result<Option<CgroupLimits>>;
}

pub struct CgroupLimits {
    pub memory_max: Option<u64>,
    pub memory_current: u64,
    pub cpu_quota: Option<u64>,   // microseconds per period
    pub cpu_period: Option<u64>,
    pub pids_max: Option<u64>,
    pub pids_current: u64,
}

pub struct MemoryMap {
    pub rss: u64,
    pub pss: u64,
    pub shared_clean: u64,
    pub shared_dirty: u64,
    pub private_clean: u64,
    pub private_dirty: u64,
    pub swap: u64,
}

pub struct CpuProfile {
    pub frames: Vec<ProfileFrame>,
    pub total_samples: u64,
}

pub struct ProfileFrame {
    pub symbol: String,     // "alloc::vec::Vec<T>::push"
    pub module: String,     // "target" or "libc.so.6"
    pub count: u64,
    pub percentage: f64,
}
```

### Analyzers

```rust
/// Linear regression on TimeSeries for trend detection.
pub struct TrendAnalyzer;

impl TrendAnalyzer {
    /// Compute slope (units per second) using least squares regression.
    pub fn slope(&self, series: &TimeSeries, window: Duration) -> f64;

    /// Predict when value will reach threshold at current rate.
    /// Returns None if trend is flat or decreasing.
    pub fn time_to_threshold(
        &self,
        series: &TimeSeries,
        threshold: f64,
    ) -> Option<Duration>;

    /// Classify the trend pattern.
    pub fn classify(&self, series: &TimeSeries, window: Duration) -> TrendClass;
}

#[derive(Debug, Clone)]
pub enum TrendClass {
    Stable,                    // slope ≈ 0
    Growing { rate: f64 },     // consistent positive slope
    Declining { rate: f64 },   // consistent negative slope
    Spike { magnitude: f64 },  // sudden jump
    Oscillating { period: Duration, amplitude: f64 },
}

/// Capacity planning: current vs limit vs trend.
pub struct CapacityAnalyzer;

impl CapacityAnalyzer {
    /// Analyze resource usage against known limits.
    pub fn analyze(
        &self,
        current: f64,
        limit: f64,
        trend: &TrendAnalyzer,
        series: &TimeSeries,
    ) -> CapacityReport;
}

pub struct CapacityReport {
    pub usage_percent: f64,       // 93.7%
    pub headroom: f64,            // 32MB remaining
    pub trend: TrendClass,        // Growing at +2.3MB/min
    pub time_to_exhaustion: Option<Duration>,  // ~13 minutes
    pub recommended_limit: Option<f64>,        // 1024MB (2x current limit)
}

/// Cross-metric correlation.
pub struct CorrelationAnalyzer;

impl CorrelationAnalyzer {
    /// Pearson correlation coefficient between two series.
    pub fn correlate(
        &self,
        a: &TimeSeries,
        b: &TimeSeries,
        window: Duration,
    ) -> f64;  // -1.0 to 1.0

    /// Find metrics correlated with a given metric (|r| > threshold).
    pub fn find_correlated(
        &self,
        target: &TimeSeries,
        candidates: &[&TimeSeries],
        threshold: f64,
    ) -> Vec<Correlation>;
}

pub struct Correlation {
    pub metric_a: String,
    pub metric_b: String,
    pub coefficient: f64,
    pub interpretation: String,  // "CPU and memory are strongly correlated (r=0.92)"
}

/// Deterministic anomaly detection (no ML).
pub struct AnomalyDetector;

impl AnomalyDetector {
    /// Z-score: how many standard deviations from mean.
    pub fn z_score(&self, series: &TimeSeries, window: Duration) -> f64;
    /// IQR outlier detection.
    pub fn is_outlier_iqr(&self, series: &TimeSeries) -> bool;
    /// Detect sudden change points.
    pub fn change_points(&self, series: &TimeSeries) -> Vec<ChangePoint>;
}
```

### Rules Engine

```rust
/// A diagnostic rule.
pub struct Rule {
    pub id: &'static str,           // "mem_approaching_oom"
    pub name: &'static str,         // "Memory approaching OOM"
    pub category: DiagCategory,
    pub default_severity: Severity,
    pub condition: RuleCondition,
    pub enabled: bool,
}

/// Rule condition — what triggers the rule.
pub enum RuleCondition {
    /// Single metric threshold: "cpu_percent > 95 for 2min"
    Threshold {
        metric: &'static str,
        op: CompareOp,
        value: f64,
        sustained: Option<Duration>,
    },
    /// Rate of change: "mem_bytes growing > 1MB/min for 15min"
    RateOfChange {
        metric: &'static str,
        rate_per_sec: f64,
        sustained: Option<Duration>,
    },
    /// Capacity: "mem_bytes > 90% of cgroup memory.max"
    CapacityPercent {
        metric: &'static str,
        limit_source: LimitSource,
        percent: f64,
    },
    /// Count threshold: "zombie_count > 5"
    Count {
        counter: CounterType,
        op: CompareOp,
        value: u64,
    },
    /// Compound: all conditions must be true
    All(Vec<RuleCondition>),
    /// Compound: any condition must be true
    Any(Vec<RuleCondition>),
}

pub enum LimitSource {
    CgroupMemoryMax,
    CgroupCpuQuota,
    CgroupPidsMax,
    Ulimit(String),     // "nofile", "nproc"
    DiskTotal,
    Custom(f64),
}

/// Evaluates all rules against MetricStore.
pub struct RuleEngine {
    rules: Vec<Rule>,
}

impl RuleEngine {
    pub fn new() -> Self;  // loads builtin rules
    pub fn add_rule(&mut self, rule: Rule);
    pub fn disable_rule(&mut self, id: &str);

    /// Evaluate all enabled rules against current metrics.
    /// Returns findings for rules that matched.
    pub fn evaluate(
        &self,
        store: &MetricStore,
        host: &HostId,
    ) -> Vec<RuleFinding>;
}

pub struct RuleFinding {
    pub rule_id: &'static str,
    pub target: DiagTarget,
    pub severity: Severity,
    pub matched_values: Vec<(String, f64)>,  // what triggered it
}
```

### Built-in Rules (30+)

```rust
pub fn builtin_rules() -> Vec<Rule> {
    vec![
        // === MEMORY ===
        Rule {
            id: "mem_approaching_oom",
            name: "Memory approaching OOM",
            category: DiagCategory::MemoryPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "mem_bytes",
                limit_source: LimitSource::CgroupMemoryMax,
                percent: 90.0,
            },
            enabled: true,
        },
        Rule {
            id: "mem_leak_suspected",
            name: "Memory leak suspected",
            category: DiagCategory::MemoryLeak,
            default_severity: Severity::Warning,
            condition: RuleCondition::RateOfChange {
                metric: "mem_bytes",
                rate_per_sec: 1_048_576.0 / 60.0,  // 1MB/min
                sustained: Some(Duration::from_secs(900)),  // 15 min
            },
            enabled: true,
        },
        Rule {
            id: "mem_doubled",
            name: "Memory doubled since start",
            // ... ratio-based condition
        },

        // === CPU ===
        Rule {
            id: "cpu_saturated",
            name: "CPU saturated",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: Some(Duration::from_secs(120)),  // 2 min
            },
            enabled: true,
        },
        Rule {
            id: "cpu_sustained_high",
            name: "Sustained high CPU",
            category: DiagCategory::CpuSpike,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 80.0,
                sustained: Some(Duration::from_secs(600)),  // 10 min
            },
            enabled: true,
        },
        Rule {
            id: "system_overloaded",
            name: "System load exceeds core count",
            // load_avg > 2 * num_cores
        },

        // === DISK ===
        Rule {
            id: "disk_almost_full",
            name: "Disk almost full",
            category: DiagCategory::DiskPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "disk_usage_bytes",
                limit_source: LimitSource::DiskTotal,
                percent: 90.0,
            },
            enabled: true,
        },
        Rule {
            id: "disk_heavy_write",
            name: "Heavy disk write I/O",
            category: DiagCategory::DiskIoHeavy,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "disk_write_bytes_per_sec",
                op: CompareOp::Gt,
                value: 100_000_000.0,  // 100 MB/s
                sustained: Some(Duration::from_secs(300)),
            },
            enabled: true,
        },

        // === FD / CONNECTIONS ===
        Rule {
            id: "fd_approaching_limit",
            name: "FD count approaching ulimit",
            category: DiagCategory::FdExhaustion,
            default_severity: Severity::Warning,
            condition: RuleCondition::CapacityPercent {
                metric: "open_fds",
                limit_source: LimitSource::Ulimit("nofile".into()),
                percent: 80.0,
            },
            enabled: true,
        },
        Rule {
            id: "connections_growing",
            name: "Connection count growing fast",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Info,
            condition: RuleCondition::RateOfChange {
                metric: "established_connections",
                rate_per_sec: 10.0 / 60.0,  // 10/min
                sustained: Some(Duration::from_secs(300)),
            },
            enabled: true,
        },

        // === PROCESS ===
        Rule {
            id: "zombie_accumulation",
            name: "Zombie process accumulation",
            category: DiagCategory::ZombieAccumulation,
            default_severity: Severity::Warning,
            condition: RuleCondition::Count {
                counter: CounterType::ZombieProcesses,
                op: CompareOp::Gt,
                value: 5,
            },
            enabled: true,
        },
        Rule {
            id: "thread_explosion",
            name: "Thread count explosion",
            category: DiagCategory::ThreadExplosion,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "thread_count",
                op: CompareOp::Gt,
                value: 1000.0,
                sustained: None,
            },
            enabled: true,
        },
        Rule {
            id: "crash_loop",
            name: "Process crash loop",
            category: DiagCategory::CrashLoop,
            default_severity: Severity::Critical,
            condition: RuleCondition::All(vec![
                RuleCondition::Threshold {
                    metric: "uptime_seconds",
                    op: CompareOp::Lt,
                    value: 60.0,
                    sustained: None,
                },
                RuleCondition::Count {
                    counter: CounterType::RestartCount,
                    op: CompareOp::Gt,
                    value: 3,
                },
            ]),
            enabled: true,
        },

        // ... 15+ more rules covering:
        // - swap usage growing
        // - network interface saturation
        // - cgroup cpu throttled
        // - process state stuck (D state)
        // - file system read-only
        // - inode exhaustion
        // - TCP retransmissions high
        // - DNS resolution slow
        // - context switch rate anomaly
    ]
}
```

### Recommendation Generator

```rust
/// Turns RuleFinding + analyzer data into actionable Diagnostic.
pub struct RecommendationGenerator;

impl RecommendationGenerator {
    pub fn generate(
        &self,
        finding: &RuleFinding,
        store: &MetricStore,
        trend: &TrendAnalyzer,
        capacity: &CapacityAnalyzer,
        profile: Option<&ProcessProfile>,
    ) -> Diagnostic;
}

// Example output for "mem_approaching_oom":
// Diagnostic {
//   summary: "nginx (pid 1234): memory at 93.7% of cgroup limit",
//   evidence: [
//     Evidence { metric: "mem_bytes", current: 480MB, threshold: 512MB,
//                trend: +2.3MB/min, context: "93.7% of cgroup memory.max" },
//   ],
//   recommendation: Recommendation {
//     action: ScaleUp { resource: "memory", from: "512MB", to: "1024MB" },
//     reason: "Memory growing at +2.3MB/min for 18 min. At current rate, OOM in ~13min.",
//     urgency: Immediate,
//     auto_executable: true,
//   },
// }
```

---

## aether-metrics: Prometheus Integration

### Module Structure

```
crates/aether-metrics/
├── Cargo.toml
├── CLAUDE.md
└── src/
    ├── lib.rs
    ├── error.rs              — MetricsError enum
    ├── exporter/
    │   ├── mod.rs
    │   ├── server.rs         — axum /metrics endpoint
    │   ├── registry.rs       — MetricRegistry (counter, gauge, histogram)
    │   └── encode.rs         — Prometheus text format (OpenMetrics)
    └── consumer/
        ├── mod.rs
        ├── client.rs         — HTTP client for Prometheus query API
        ├── query.rs          — PromQL query builder helpers
        └── types.rs          — Prometheus response types
```

### Exporter

```rust
pub struct MetricsExporter {
    registry: MetricRegistry,
}

impl MetricsExporter {
    /// Start HTTP server on port. Non-blocking.
    pub async fn serve(self, port: u16, cancel: CancellationToken);

    /// Register standard Aether metrics.
    pub fn register_defaults(&mut self);
}

/// Metrics exported:
///
/// # Per-process (labels: host, pid, name)
/// aether_process_cpu_percent
/// aether_process_memory_bytes
/// aether_process_memory_rss_bytes
/// aether_process_open_fds
/// aether_process_thread_count
/// aether_process_uptime_seconds
/// aether_process_hp
/// aether_process_xp
///
/// # Per-host (labels: host)
/// aether_host_cpu_percent
/// aether_host_memory_total_bytes
/// aether_host_memory_used_bytes
/// aether_host_load_avg_1m
/// aether_host_load_avg_5m
/// aether_host_load_avg_15m
///
/// # Diagnostics (labels: host, severity, category)
/// aether_diagnostics_active
/// aether_diagnostics_total
/// aether_diagnostics_resolved_total
///
/// # Internal (labels: none)
/// aether_ingestion_events_total
/// aether_analyze_evaluations_total
/// aether_analyze_rules_fired_total
/// aether_analyze_latency_seconds (histogram)
```

### Consumer

```rust
pub struct PrometheusConsumer {
    endpoint: Url,
    client: reqwest::Client,
    poll_interval: Duration,
}

impl PrometheusConsumer {
    /// Poll Prometheus and return TimeSeries for all configured queries.
    pub async fn poll(&self) -> Result<Vec<TimeSeries>, MetricsError>;

    /// Execute arbitrary PromQL query.
    pub async fn query(&self, promql: &str) -> Result<Vec<TimeSeries>>;

    /// Preset: get CPU for all nodes
    pub async fn cluster_cpu(&self) -> Result<Vec<TimeSeries>>;

    /// Preset: get memory for all nodes
    pub async fn cluster_memory(&self) -> Result<Vec<TimeSeries>>;

    /// Run polling loop, sending TimeSeries to analyze engine.
    pub async fn run(
        &self,
        tx: mpsc::Sender<Vec<TimeSeries>>,
        cancel: CancellationToken,
    );
}
```

### CLI Flags

```
--metrics [PORT]              Enable Prometheus exporter (default: 9090)
--prometheus <URL>            Connect to Prometheus for cluster metrics
--prometheus-interval <SEC>   Polling interval for Prometheus (default: 15)
--analyze                     Enable diagnostic engine (default: true)
--analyze-interval <SEC>      Diagnostic analysis interval (default: 5)
```

---

## TUI Extensions (aether-render)

### New: Diagnostics Tab (F6)

Two-panel layout: diagnostic list (top) + detail panel (bottom).

```
┌─ Diagnostics ──────────────────────────────────────────────────────┐
│ ■ 2 Critical  ■ 5 Warning  ■ 12 Info          [host: local ▾]    │
├────────────────────────────────────────────────────────────────────┤
│ CRIT │ nginx (1234)    │ OOM in ~13min      │ mem: 480/512MB  ▲  │
│ CRIT │ postgres (892)  │ CPU saturated      │ cpu: 98.2%      │  │
│ WARN │ redis (2201)    │ memory leak        │ +1.1MB/min      │  │
│ WARN │ node (3344)     │ fd limit 82%       │ 819/1024 fds    │  │
│ WARN │ system          │ disk /var 91%      │ 45.5/50GB       │  │
│ INFO │ kafka (5501)    │ connections growing │ +8/min          ▼  │
├────────────────────────────────────────────────────────────────────┤
│ ▶ nginx (pid 1234) — CRITICAL: approaching OOM                    │
│                                                                    │
│ Memory:  480 MB / 512 MB (93.7%) ████████████████████░░ 93.7%     │
│ Trend:   +2.3 MB/min (steady 18 min)  ╱╱╱╱╱╱╱╱╱╱╱╱──────        │
│ ETA OOM: ~13 minutes                                               │
│                                                                    │
│ Top stack frames (CPU):                                            │
│   67%  alloc::vec::Vec<T>::push                                    │
│   22%  serde_json::de::from_str                                    │
│    8%  hyper::proto::h1::decode                                    │
│                                                                    │
│ cgroup limits: memory.max=512MB  cpu.max=200000/100000             │
│                                                                    │
│ Recommendation: SCALE UP memory → 1024 MB                          │
│ [Enter] Execute via Arbiter    [d] Dismiss    [m] Mute rule        │
└────────────────────────────────────────────────────────────────────┘
```

### Overview Tab Extension

Process table adds `Diag` column:

```
PID   │ Name     │ CPU  │ MEM     │ HP  │ Diag
1234  │ nginx    │ 87%  │ 480MB   │ 23  │ ■ OOM ~13m
892   │ postgres │ 98%  │ 1.2GB   │ 15  │ ■ CPU saturated
5501  │ kafka    │ 34%  │ 2.1GB   │ 95  │ ● conn growing
```

### World3D Tab Extension

Visual markers on 3D nodes:
- Critical = red pulsation + exclamation icon
- Warning = yellow outline ring
- Correlation edges highlighted between related diagnostics

### Host Selector (cluster mode)

When Prometheus consumer active, top bar shows host tabs:

```
[local ▾] [node-1] [node-2] [node-3]    Cluster: 4 hosts, 3 critical
```

All tabs filtered by selected host. "All" = aggregate view.

### Tab Layout

```
F1: Overview    — process table + sparklines + diag column
F2: World3D     — 3D graph + diagnostic markers
F3: Network     — connections
F4: Arbiter     — AI action queue
F5: Rules       — JIT rule engine stats
F6: Diagnostics — NEW: diagnostic findings + detail panel
F7: Help        — keybindings + diagnostics legend
```

---

## Channel Architecture (Full MVP)

```
aether-ingestion                    aether-analyze
  ├── SystemEvent ──mpsc──→ Core      ├── collectors (procfs, perf, cgroup)
  │                                    ├── analyzers (trend, capacity, corr.)
  │                                    ├── rules (30+ builtin)
  │                                    └── Vec<Diagnostic> ──mpsc──→ Core
  │                                            ↑
aether-metrics                                  │
  └── consumer ──TimeSeries──mpsc──→ analyze ───┘

                    Core (WorldGraph + DiagnosticStore)
                     │
        ┌────────────┼────────────┬────────────┐
        ▼            ▼            ▼            ▼
    Render       Arbiter      Metrics       MCP
  (all tabs)   (auto-exec)  (/metrics)  (get_diagnostics)
```

### New/Modified Channels

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| diagnostics | `mpsc` | AnalyzeEngine | Core/Render | `Vec<Diagnostic>` |
| prometheus_data | `mpsc` | PrometheusConsumer | AnalyzeEngine | `Vec<TimeSeries>` |
| diag_actions | `mpsc` | Render (Diag tab) | ArbiterQueue | Execute recommendation |

---

## Thread/Task Model (Full MVP)

```
Main Thread:
  +-- tokio runtime
        +-- task: eBPF ring buffer reader (--ebpf, Linux only)
        +-- task: IngestionPipeline (10Hz sysinfo + eBPF hybrid)
        +-- task: WorldGraph updater (SystemEvent → graph sync)
        +-- task: AnalyzeEngine (5s tick: collect → analyze → rules → diagnostics)
        +-- task: PrometheusConsumer (--prometheus, 15s poll)
        +-- task: MetricsExporter (--metrics, axum HTTP)
        +-- task: PredictEngine (--predict, optional ML layer)
        +-- task: ScriptEngine (--rules, user JIT rules)
        +-- task: HotReloader (file watcher for .aether rules)
        +-- task: Arbiter executor (500ms, processes approved actions)
        +-- task: McpServer (--mcp-stdio or --mcp-sse)
        +-- blocking: TUI render loop (60fps)
              +-- F1: Overview (process table + diag column)
              +-- F2: World3D (3D graph + diagnostic markers)
              +-- F3: Network
              +-- F4: Arbiter
              +-- F5: Rules
              +-- F6: Diagnostics (NEW)
              +-- F7: Help
```

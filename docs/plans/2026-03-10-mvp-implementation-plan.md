# Aether-Terminal MVP: Implementation Plan

**Date**: 2026-03-10
**Design doc**: `docs/plans/2026-03-10-mvp-evolution-design.md`
**Architecture**: 11 crates (9 existing + 2 new: aether-analyze, aether-metrics)

---

## Phase Map

```
Phase 1: Foundation         ░░░░░░░░░░░░░░░░░░░░  Core models + analyze scaffolding + basic rules
Phase 2: Deep Analysis      ░░░░░░░░░░░░░░░░░░░░  Collectors + all analyzers + 30+ rules
Phase 3: Prometheus          ░░░░░░░░░░░░░░░░░░░░  Exporter + consumer + cluster view
Phase 4: Integration         ░░░░░░░░░░░░░░░░░░░░  Full TUI + Arbiter + MCP + polish
```

---

# PHASE 1: Foundation

**Goal**: Core data model extensions, aether-analyze crate with basic trend analysis and 10 rules, minimal Diagnostics tab showing results.

**Outcome**: `cargo run` shows live diagnostics in TUI for local host.

## Sprint 9.1: Core Model Extensions

### Task 9.1.1: Add HostId, TimeSeries, Diagnostic types to aether-core

```
Files: crates/aether-core/src/models.rs, crates/aether-core/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-core
Depends: none
```

EXISTING CODE:
- models.rs: ProcessNode (pid, ppid, name, cpu_percent f32, mem_bytes u64, state ProcessState, hp f32, xp u32, position_3d Vec3), NetworkEdge, SystemSnapshot, ProcessState, Protocol, ConnectionState
- events.rs: SystemEvent, GameEvent, AgentAction enums
- error.rs: CoreError with Probe(String), Storage(String) variants
- lib.rs: pub mod models, graph, events, traits, error, arbiter + re-exports

Add to models.rs:
- `HostId(String)` — Default = "local", derives: Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize
- `MetricSample { timestamp: Instant, value: f64 }`
- `TimeSeries { name: String, labels: BTreeMap<String, String>, samples: VecDeque<MetricSample>, capacity: usize }`
  - Methods: new(), push(), last(), rate(window), avg(window), min_max(window), len(), is_empty()
  - Default capacity: 3600
- `DiagTarget` enum: Process { pid, name }, Host(HostId), Container { id, name }, Disk { mount }, Network { interface }
  - #[non_exhaustive]
- `Severity` enum: Info, Warning, Critical — derives Ord for sorting
- `DiagCategory` enum: MemoryLeak, MemoryPressure, CpuSaturation, CpuSpike, DiskPressure, DiskIoHeavy, FdExhaustion, ConnectionSurge, ZombieAccumulation, ThreadExplosion, CrashLoop, ConfigMismatch, CapacityRisk, CorrelatedAnomaly
  - #[non_exhaustive]
- `Evidence { metric: String, current: f64, threshold: f64, trend: Option<f64>, context: String }`
- `RecommendedAction` enum: ScaleUp { resource, from, to }, Restart { reason }, RaiseLimits { limit_name, from, to }, ReduceLoad { suggestion }, Investigate { what }, KillProcess { pid, reason }, NoAction { reason }
  - #[non_exhaustive]
- `Urgency` enum: Immediate, Soon, Planning, Informational
- `Recommendation { action: RecommendedAction, reason: String, urgency: Urgency, auto_executable: bool }`
- `Diagnostic { id: u64, host: HostId, target: DiagTarget, severity: Severity, category: DiagCategory, summary: String, evidence: Vec<Evidence>, recommendation: Recommendation, detected_at: Instant, resolved_at: Option<Instant> }`

Update lib.rs: re-export new types.

Tests:
- test_timeseries_push_and_rate — push 10 samples 1s apart, check rate()
- test_timeseries_capacity_evicts_oldest — push beyond capacity, verify len()
- test_timeseries_avg_window — verify average over time window
- test_diagnostic_severity_ordering — Critical > Warning > Info
- test_host_id_default_is_local

COMMIT: feat(core): add HostId, TimeSeries, Diagnostic types for MVP

### Task 9.1.2: Add DiagnosticEvent to events system

```
Files: crates/aether-core/src/events.rs
Agent: rust-engineer
Test: cargo test -p aether-core
Depends: 9.1.1
```

EXISTING CODE:
- events.rs: SystemEvent (ProcessStarted/Exited/Updated, NetworkConnected/Disconnected, SnapshotCompleted), GameEvent (ProcessHealthChanged, XpGained, AchievementUnlocked, ActionExecuted), AgentAction (Kill, Renice, Suspend, Resume, Investigate)

Add:
- `DiagnosticEvent` enum:
  - DiagnosticsUpdated(Vec<Diagnostic>) — new batch of diagnostics
  - DiagnosticResolved { id: u64 } — a diagnostic was resolved
  - DiagnosticDismissed { id: u64 } — user dismissed
  - DiagnosticActionRequested { diagnostic_id: u64, action: RecommendedAction } — user wants to execute

Tests:
- test_diagnostic_event_clone
- test_diagnostic_action_requested_carries_action

COMMIT: feat(core): add DiagnosticEvent to event system

## Sprint 9.2: aether-analyze Scaffolding

### Task 9.2.1: Initialize aether-analyze crate

```
Files: crates/aether-analyze/Cargo.toml, src/lib.rs, src/error.rs, CLAUDE.md
Agent: rust-engineer
Test: cargo check -p aether-analyze
Depends: 9.1.1
```

Create new crate in workspace:
- Cargo.toml: aether-core dependency, tokio, thiserror, tracing
- error.rs: AnalyzeError enum with thiserror:
  - Collector(String)
  - Analyzer(String)
  - Rule(String)
  - Io(#[from] std::io::Error)
- lib.rs: pub mod error; (more modules added in later tasks)
- CLAUDE.md: crate context doc

Update root Cargo.toml workspace members.
Update aether-terminal Cargo.toml: add aether-analyze dependency.

COMMIT: feat(analyze): initialize aether-analyze crate

### Task 9.2.2: Implement MetricStore

```
Files: crates/aether-analyze/src/store.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.1
```

EXISTING CODE (from 9.1.1):
- aether-core: TimeSeries, MetricSample, HostId

Implement:
- `MetricKey { host: HostId, pid: Option<u32>, metric: String }` — Hash, Eq
- `MetricStore { series: HashMap<MetricKey, TimeSeries>, capacity: usize }`
  - new(capacity) — default 3600
  - ingest_world_state(&mut self, host: &HostId, world: &WorldGraph)
    - For each process: push cpu_percent, mem_bytes, thread_count into TimeSeries
    - For host: push aggregate cpu, memory
  - ingest_remote(&mut self, series: Vec<TimeSeries>)
    - Parse labels to extract host/pid, store into internal HashMap
  - get(&self, host: &HostId, pid: Option<u32>, metric: &str) -> Option<&TimeSeries>
  - process_metrics(&self, host: &HostId, pid: u32) -> Vec<(&str, &TimeSeries)>
  - host_metrics(&self, host: &HostId) -> Vec<(&str, &TimeSeries)>
  - hosts(&self) -> Vec<&HostId>
  - cleanup_dead_processes(&mut self, alive_pids: &HashSet<u32>)

Update lib.rs: pub mod store;

Tests:
- test_ingest_world_state_creates_series — ingest snapshot, verify series exist
- test_ingest_multiple_snapshots_builds_history — 5 ingests, check len=5
- test_get_nonexistent_returns_none
- test_cleanup_removes_dead — ingest pid 1, cleanup with empty set, verify gone
- test_capacity_limits_samples — set capacity=3, push 5, verify len=3

COMMIT: feat(analyze): implement MetricStore for TimeSeries storage

### Task 9.2.3: Implement TrendAnalyzer

```
Files: crates/aether-analyze/src/analyzers/mod.rs, src/analyzers/trend.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.2
```

EXISTING CODE:
- aether-core: TimeSeries with samples VecDeque<MetricSample>

Implement in analyzers/trend.rs:
- `TrendClass` enum: Stable, Growing { rate: f64 }, Declining { rate: f64 }, Spike { magnitude: f64 }, Oscillating { period_secs: f64, amplitude: f64 }
- `TrendAnalyzer` (stateless struct):
  - slope(&self, series: &TimeSeries, window: Duration) -> f64
    - Least squares linear regression on (timestamp_offset, value) pairs
    - Returns slope in units-per-second
  - time_to_threshold(&self, series: &TimeSeries, threshold: f64) -> Option<Duration>
    - If current < threshold and slope > 0: (threshold - current) / slope
    - Returns None if trend is flat/declining or already above threshold
  - classify(&self, series: &TimeSeries, window: Duration) -> TrendClass
    - |slope| < epsilon → Stable
    - slope > 0 with R² > 0.7 → Growing
    - slope < 0 with R² > 0.7 → Declining
    - Last value > mean + 3*stddev → Spike
    - Else: check for oscillation via zero-crossing count

Helper: linear_regression(points: &[(f64, f64)]) -> (slope, intercept, r_squared)

Update lib.rs: pub mod analyzers;
Update analyzers/mod.rs: pub mod trend;

Tests:
- test_slope_constant_series_is_zero — push same value 60 times
- test_slope_linear_growth — push 0,1,2,...,59 → slope ≈ 1.0/s
- test_time_to_threshold_linear — growing 1/s from 50, threshold 100 → ~50s
- test_time_to_threshold_declining_returns_none
- test_classify_stable — constant series → Stable
- test_classify_growing — linear growth → Growing
- test_classify_spike — flat then sudden jump → Spike
- test_linear_regression_perfect_line — R² = 1.0

COMMIT: feat(analyze): implement TrendAnalyzer with linear regression

### Task 9.2.4: Implement CapacityAnalyzer

```
Files: crates/aether-analyze/src/analyzers/capacity.rs, analyzers/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.3
```

EXISTING CODE:
- analyzers/trend.rs: TrendAnalyzer with slope(), time_to_threshold(), classify()

Implement in analyzers/capacity.rs:
- `CapacityReport { usage_percent: f64, headroom: f64, trend: TrendClass, time_to_exhaustion: Option<Duration>, recommended_limit: Option<f64> }`
- `CapacityAnalyzer` (stateless):
  - analyze(&self, current: f64, limit: f64, trend: &TrendAnalyzer, series: &TimeSeries) -> CapacityReport
    - usage_percent = current / limit * 100
    - headroom = limit - current
    - trend = trend.classify(series)
    - time_to_exhaustion = trend.time_to_threshold(series, limit)
    - recommended_limit: if usage > 80%, suggest 2x current limit (round to nice number)
  - format_bytes(bytes: f64) -> String — "480 MB", "1.2 GB"
  - format_duration(dur: Duration) -> String — "~13 minutes", "~2 hours"

Update analyzers/mod.rs: pub mod capacity;

Tests:
- test_analyze_healthy — 50% usage, stable → no exhaustion time
- test_analyze_critical — 93% usage, growing → time_to_exhaustion Some(~13min)
- test_recommended_limit_doubles — at 85% of 512MB → suggest 1024MB
- test_format_bytes_mb_gb — verify human-readable output
- test_zero_limit_no_panic — handle limit=0 gracefully

COMMIT: feat(analyze): implement CapacityAnalyzer with capacity planning

## Sprint 9.3: Rule Engine + First 10 Rules

### Task 9.3.1: Implement Rule types and RuleEngine

```
Files: crates/aether-analyze/src/rules/mod.rs, rules/types.rs, rules/engine.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.4
```

EXISTING CODE:
- store.rs: MetricStore with get(), process_metrics(), host_metrics()
- analyzers/trend.rs: TrendAnalyzer with slope()
- aether-core: Severity, DiagCategory, DiagTarget

Implement rules/types.rs:
- `CompareOp` enum: Gt, Gte, Lt, Lte, Eq
- `LimitSource` enum: CgroupMemoryMax, CgroupCpuQuota, CgroupPidsMax, Ulimit(String), DiskTotal, Custom(f64)
- `CounterType` enum: ZombieProcesses, RestartCount, OpenFds, ThreadCount
- `RuleCondition` enum:
  - Threshold { metric: &'static str, op: CompareOp, value: f64, sustained: Option<Duration> }
  - RateOfChange { metric: &'static str, rate_per_sec: f64, sustained: Option<Duration> }
  - CapacityPercent { metric: &'static str, limit_source: LimitSource, percent: f64 }
  - Count { counter: CounterType, op: CompareOp, value: u64 }
  - All(Vec<RuleCondition>)
  - Any(Vec<RuleCondition>)
- `Rule { id: &'static str, name: &'static str, category: DiagCategory, default_severity: Severity, condition: RuleCondition, enabled: bool }`
- `RuleFinding { rule_id: &'static str, rule_name: &'static str, target: DiagTarget, severity: Severity, category: DiagCategory, matched_values: Vec<(String, f64)> }`

Implement rules/engine.rs:
- `RuleEngine { rules: Vec<Rule>, sustained_state: HashMap<(String, u32), Instant> }`
  - new() → Self (empty rules)
  - load_builtin(&mut self) — calls builtin_rules() and adds them
  - add_rule(&mut self, rule: Rule)
  - disable_rule(&mut self, id: &str) -> bool
  - enable_rule(&mut self, id: &str) -> bool
  - evaluate(&mut self, store: &MetricStore, host: &HostId, limits: &ProcessLimits) -> Vec<RuleFinding>
    - For each process in store: test each rule's condition
    - Threshold: check current value + sustained tracking via HashMap<(rule_id, pid), first_seen>
    - RateOfChange: compute slope, check if exceeds rate for sustained duration
    - CapacityPercent: look up limit from ProcessLimits, compare usage/limit*100
    - Count: lookup counter
    - All/Any: recursive evaluation
- `ProcessLimits { cgroup: Option<CgroupLimits>, ulimits: HashMap<String, u64> }` — passed in from collectors

Update lib.rs: pub mod rules;
Update rules/mod.rs: pub mod types; pub mod engine;

Tests:
- test_threshold_rule_matches_when_above
- test_threshold_rule_no_match_below
- test_sustained_only_fires_after_duration
- test_sustained_resets_when_condition_false
- test_rate_of_change_matches_growing_series
- test_capacity_percent_matches_above_threshold
- test_compound_all_requires_both
- test_compound_any_requires_one
- test_disable_rule_prevents_evaluation

COMMIT: feat(analyze): implement Rule types and RuleEngine

### Task 9.3.2: Implement first 10 builtin rules

```
Files: crates/aether-analyze/src/rules/builtin.rs, rules/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.3.1
```

EXISTING CODE:
- rules/types.rs: Rule, RuleCondition, CompareOp, LimitSource, CounterType, Severity, DiagCategory
- rules/engine.rs: RuleEngine with evaluate()

Implement rules/builtin.rs:
- pub fn builtin_rules() -> Vec<Rule> — returns all built-in rules

First 10 rules:
1. mem_approaching_oom — CapacityPercent mem_bytes > 90% CgroupMemoryMax → Critical
2. mem_leak_suspected — RateOfChange mem_bytes > 1MB/min sustained 15min → Warning
3. cpu_saturated — Threshold cpu_percent > 95% sustained 2min → Critical
4. cpu_sustained_high — Threshold cpu_percent > 80% sustained 10min → Warning
5. disk_almost_full — CapacityPercent disk_usage > 90% DiskTotal → Critical
6. fd_approaching_limit — CapacityPercent open_fds > 80% Ulimit("nofile") → Warning
7. zombie_accumulation — Count ZombieProcesses > 5 → Warning
8. thread_explosion — Threshold thread_count > 1000 → Warning
9. crash_loop — All(uptime < 60s AND restart_count > 3) → Critical
10. connections_growing — RateOfChange connections > 10/min sustained 5min → Info

Update rules/mod.rs: pub mod builtin;

Tests:
- test_builtin_rules_count_is_10
- test_all_builtin_rules_have_unique_ids
- test_builtin_rules_cover_all_severities — at least one per severity level
- test_mem_oom_rule_fires_at_91_percent
- test_cpu_saturated_fires_at_96_percent

COMMIT: feat(analyze): add first 10 builtin diagnostic rules

### Task 9.3.3: Implement RecommendationGenerator

```
Files: crates/aether-analyze/src/recommendations/mod.rs, recommendations/generator.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.3.2, 9.2.4
```

EXISTING CODE:
- rules/types.rs: RuleFinding with rule_id, target, severity, matched_values
- analyzers/trend.rs: TrendAnalyzer
- analyzers/capacity.rs: CapacityAnalyzer, CapacityReport
- aether-core: Diagnostic, Recommendation, RecommendedAction, Urgency, Evidence

Implement recommendations/generator.rs:
- `RecommendationGenerator { next_id: AtomicU64 }`
  - generate(&self, finding: &RuleFinding, store: &MetricStore, trend: &TrendAnalyzer, capacity: &CapacityAnalyzer) -> Diagnostic
    - Match on finding.rule_id to determine:
      - Which evidence to collect (current values, thresholds, trends)
      - Which RecommendedAction to produce
      - What urgency level
      - Whether auto-executable
    - Build summary string: "{process} (pid {pid}): {description}"
    - Build Evidence vec from matched_values + trend data
  - Rule-specific logic:
    - mem_approaching_oom → ScaleUp memory, Immediate urgency
    - mem_leak_suspected → Investigate + Restart suggestion, Soon urgency
    - cpu_saturated → ReduceLoad or ScaleUp CPU, Immediate
    - cpu_sustained_high → Investigate, Soon
    - disk_almost_full → ScaleUp disk or ReduceLoad (cleanup), Immediate
    - fd_approaching_limit → RaiseLimits nofile, Soon
    - zombie_accumulation → KillProcess (parent), Planning
    - thread_explosion → Investigate, Soon
    - crash_loop → Investigate, Immediate
    - connections_growing → NoAction/Investigate, Informational

Update lib.rs: pub mod recommendations;

Tests:
- test_generate_mem_oom_produces_scale_up
- test_generate_cpu_saturated_is_immediate
- test_generate_connections_growing_is_informational
- test_diagnostic_ids_are_unique — generate 3, verify different ids
- test_evidence_contains_current_value

COMMIT: feat(analyze): implement RecommendationGenerator

### Task 9.3.4: Implement AnalyzeEngine main loop

```
Files: crates/aether-analyze/src/engine.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.3.3
```

EXISTING CODE:
- store.rs: MetricStore
- analyzers/trend.rs: TrendAnalyzer
- analyzers/capacity.rs: CapacityAnalyzer
- rules/engine.rs: RuleEngine
- recommendations/generator.rs: RecommendationGenerator
- aether-core: WorldGraph, Diagnostic, HostId

Implement engine.rs:
- `AnalyzeConfig { interval: Duration, history_capacity: usize, host: HostId }`
  - Default: 5s interval, 3600 capacity, "local" host
- `AnalyzeEngine { store: MetricStore, rule_engine: RuleEngine, trend: TrendAnalyzer, capacity: CapacityAnalyzer, generator: RecommendationGenerator, config: AnalyzeConfig, active_diagnostics: Vec<Diagnostic> }`
  - new(config: AnalyzeConfig) -> Self
  - pub async fn run(&mut self, world: Arc<RwLock<WorldGraph>>, prometheus_rx: Option<mpsc::Receiver<Vec<TimeSeries>>>, diag_tx: mpsc::Sender<Vec<Diagnostic>>, cancel: CancellationToken)
    - Loop on interval tick:
      1. Read world graph → ingest into MetricStore
      2. If prometheus_rx: drain and ingest remote TimeSeries
      3. Evaluate rules: rule_engine.evaluate(&store, &host, &limits)
      4. Generate diagnostics: for each finding → generator.generate()
      5. Resolve diagnostics that no longer match
      6. Send active diagnostics via diag_tx
  - pub fn active_diagnostics(&self) -> &[Diagnostic]
  - pub fn stats(&self) -> AnalyzeStats
- `AnalyzeStats { evaluations: u64, rules_fired: u64, active_critical: u32, active_warning: u32, active_info: u32 }`

Update lib.rs: pub mod engine;

Tests:
- test_engine_processes_world_state — create world with high-cpu process, verify diagnostic emitted
- test_engine_resolves_when_condition_clears — inject high CPU then normal CPU, verify resolved
- test_engine_stats_track_counts
- test_engine_runs_with_empty_world — no diagnostics, no crash

COMMIT: feat(analyze): implement AnalyzeEngine main loop

## Sprint 9.4: Basic Diagnostics Tab

### Task 9.4.1: Create Diagnostics tab in aether-render

```
Files: crates/aether-render/src/tui/diagnostics.rs, tui/mod.rs, tui/app.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 9.3.4
```

EXISTING CODE:
- tui/app.rs: App struct with Tab enum (Overview, World3D, Network, Arbiter, Rules), handle_key with F1-F5
- tui/mod.rs: mod declarations for overview, world3d, network, arbiter, rules, help, input, tabs, widgets
- tui/tabs.rs: tab bar rendering
- palette.rs: color constants including PREDICTION (orange)
- aether-core: Diagnostic, Severity, DiagCategory, Evidence, Recommendation

Create tui/diagnostics.rs:
- `DiagnosticsTab { selected: usize, scroll_offset: usize, detail_scroll: usize }`
  - render(&self, area: Rect, buf: &mut Buffer, diagnostics: &[Diagnostic])
    - Split area: 60% list, 40% detail
    - List panel: severity icon (■/●) + color, target name, summary, key metric
    - Detail panel: full diagnostic for selected item
      - Progress bar for capacity metrics
      - Trend indicator (╱ growing, ─ stable, ╲ declining)
      - Stack frames if available
      - Recommendation + keybindings
  - handle_key(&mut self, key: KeyEvent) -> Option<DiagAction>
    - Up/Down/j/k: navigate list
    - Enter: execute recommendation via Arbiter
    - d: dismiss diagnostic
    - m: mute rule
  - severity_color(severity) → Color: Critical=Red, Warning=Yellow, Info=Cyan

Update tui/mod.rs: add pub mod diagnostics;
Update tui/app.rs:
- Add Tab::Diagnostics to enum
- Add F6 keybinding
- Add diagnostics: Vec<Diagnostic> field to App
- Route render/input to DiagnosticsTab when active

COMMIT: feat(render): add Diagnostics tab (F6) with findings list and detail panel

### Task 9.4.2: Add diagnostic indicators to Overview tab

```
Files: crates/aether-render/src/tui/overview.rs, src/palette.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 9.4.1
```

EXISTING CODE:
- tui/overview.rs: OverviewTab with process table (columns: PID, Name, CPU, MEM, State, HP), sparklines, SortColumn enum, detail panel
- palette.rs: PREDICTION Color::Rgb(255, 102, 0), color_for_hp(), color_for_load()
- aether-core: Diagnostic, Severity

Add to palette.rs:
- pub const DIAGNOSTIC_CRITICAL: Color = Color::Rgb(255, 60, 60);
- pub const DIAGNOSTIC_WARNING: Color = Color::Rgb(255, 200, 50);
- pub const DIAGNOSTIC_INFO: Color = Color::Rgb(100, 200, 255);

Modify overview.rs:
- Add "Diag" column to process table (after HP)
- For each process row: lookup diagnostics by pid
  - If Critical: "■ {short summary}" in red
  - If Warning: "■ {short summary}" in yellow
  - If Info: "● {short summary}" in cyan
  - If none: empty
- Add diagnostics: &[Diagnostic] parameter to render()

COMMIT: feat(render): add diagnostic indicators to Overview process table

### Task 9.4.3: Wire AnalyzeEngine into main.rs

```
Files: crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --help
Depends: 9.4.2
```

EXISTING CODE:
- main.rs: Cli struct with --log-level, --mcp-stdio, --mcp-sse, --no-3d, --no-game, --theme, --rules, --predict, --model-path, --ebpf
- Spawns: IngestionPipeline, WorldGraph updater, Arbiter executor, Action forwarder, optional MCP, optional PredictEngine, optional ScriptEngine
- App receives world graph via Arc<RwLock<WorldGraph>>

Add to Cli:
- --no-analyze: disable diagnostic engine (default: enabled)
- --analyze-interval <SEC>: analysis tick interval (default: 5)

Wire in main():
1. Unless --no-analyze: create AnalyzeEngine with config
2. Create mpsc channel for diagnostics
3. Spawn AnalyzeEngine::run() as tokio task
4. Pass diagnostics receiver to App (store in Arc<Mutex<Vec<Diagnostic>>>)
5. App reads diagnostics for Diagnostics tab + Overview indicators

COMMIT: feat(terminal): wire AnalyzeEngine into main with --no-analyze flag

---

# PHASE 2: Deep Analysis

**Goal**: Deep system data collection (procfs, perf, cgroups), all 4 analyzers, 30+ rules, full Diagnostics tab with stack traces.

**Outcome**: Real diagnostic value — detects memory leaks, CPU saturation, capacity issues with evidence and recommendations.

## Sprint 10.1: Collectors

### Task 10.1.1: Implement ProcfsCollector

```
Files: crates/aether-analyze/src/collectors/mod.rs, collectors/procfs.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.3.4
```

EXISTING CODE:
- engine.rs: AnalyzeEngine (expects collectors to provide deep process data)
- aether-core: HostId

Implement collectors/mod.rs:
- `ProcessProfile { status: ProcStatus, memory: MemoryMap, fds: FdInfo, io: IoStats, stack: Vec<StackFrame>, cgroup: Option<CgroupLimits> }`
- `HostProfile { loadavg: (f64, f64, f64), meminfo: HostMemInfo, disk: Vec<DiskInfo> }`
- `StackFrame { symbol: String, address: u64 }`
- `ProcStatus { state: char, threads: u32, vm_rss: u64, vm_size: u64, voluntary_ctxt_switches: u64, nonvoluntary_ctxt_switches: u64 }`
- `MemoryMap { rss: u64, pss: u64, shared_clean: u64, shared_dirty: u64, private_clean: u64, private_dirty: u64, swap: u64 }`
- `FdInfo { count: u32, limit: u64, types: HashMap<String, u32> }` — socket, pipe, file, etc.
- `IoStats { read_bytes: u64, write_bytes: u64, read_ops: u64, write_ops: u64 }`
- `DiskInfo { mount: String, total: u64, used: u64, available: u64 }`
- pub(crate) trait Collector: Send + Sync

Implement collectors/procfs.rs:
- `ProcfsCollector`
  - process_profile(&self, pid: u32) -> Result<ProcessProfile, AnalyzeError>
    - Read /proc/<pid>/status → ProcStatus
    - Read /proc/<pid>/smaps_rollup → MemoryMap
    - Count /proc/<pid>/fd/* → FdInfo (with readlink to classify types)
    - Read /proc/<pid>/io → IoStats
    - Read /proc/<pid>/stack → Vec<StackFrame> (kernel stack)
    - Read /proc/<pid>/cgroup → detect cgroup path → read limits
  - host_profile(&self) -> Result<HostProfile, AnalyzeError>
    - Read /proc/loadavg
    - Read /proc/meminfo
    - Parse `df` output or /proc/mounts + statvfs

Update lib.rs: pub mod collectors;

Tests:
- test_procfs_reads_self_process — read /proc/self/status, verify pid matches
- test_procfs_reads_memory_map — read /proc/self/smaps_rollup, verify rss > 0
- test_procfs_counts_fds — /proc/self/fd count > 0
- test_procfs_reads_loadavg — verify 3 floats
- test_procfs_nonexistent_pid_returns_error

COMMIT: feat(analyze): implement ProcfsCollector for deep process profiling

### Task 10.1.2: Implement CgroupCollector

```
Files: crates/aether-analyze/src/collectors/cgroup.rs, collectors/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 10.1.1
```

EXISTING CODE:
- collectors/mod.rs: CgroupLimits struct
- collectors/procfs.rs: ProcfsCollector reads /proc/<pid>/cgroup

Implement collectors/cgroup.rs:
- `CgroupCollector`
  - detect_version(pid: u32) -> CgroupVersion (V1 or V2)
  - cgroup_path(pid: u32) -> Result<PathBuf> — read /proc/<pid>/cgroup, resolve path
  - limits(&self, pid: u32) -> Result<Option<CgroupLimits>>
    - V2: read memory.max, memory.current, cpu.max, pids.max, pids.current
    - V1: read memory.limit_in_bytes, cpuacct.usage, etc.
    - Return None if not in a cgroup (bare metal)
- `CgroupLimits { memory_max: Option<u64>, memory_current: u64, cpu_quota: Option<u64>, cpu_period: Option<u64>, pids_max: Option<u64>, pids_current: u64 }`

Tests:
- test_detect_cgroup_version — read /proc/self/cgroup
- test_cgroup_path_resolves — non-empty path
- test_limits_returns_some_in_container — (if running in cgroup) or None on bare metal
- test_memory_max_special_values — "max" means unlimited → None

COMMIT: feat(analyze): implement CgroupCollector for container limit detection

### Task 10.1.3: Implement PerfCollector for CPU profiling

```
Files: crates/aether-analyze/src/collectors/perf.rs, collectors/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 10.1.1
```

EXISTING CODE:
- collectors/mod.rs: CpuProfile, ProfileFrame structs

Implement collectors/perf.rs:
- `PerfCollector`
  - cpu_profile(&self, pid: u32, duration: Duration) -> Result<CpuProfile>
    - Sample CPU call stacks via reading /proc/<pid>/stack repeatedly over duration
    - Alternative: use `perf_event_open` syscall for hardware PMU sampling (Linux only)
    - Aggregate stack frames, count occurrences, calculate percentages
    - Resolve symbols via /proc/<pid>/maps + addr2line if available
  - `CpuProfile { frames: Vec<ProfileFrame>, total_samples: u64 }`
  - `ProfileFrame { symbol: String, module: String, count: u64, percentage: f64 }`
  - Falls back gracefully if perf_event_open not permitted (return partial data)

Tests:
- test_cpu_profile_self_process — profile self for 100ms, verify >0 frames
- test_profile_frame_percentages_sum_to_100
- test_nonexistent_pid_returns_error
- test_zero_duration_returns_empty

COMMIT: feat(analyze): implement PerfCollector for CPU stack profiling

## Sprint 10.2: Advanced Analyzers

### Task 10.2.1: Implement CorrelationAnalyzer

```
Files: crates/aether-analyze/src/analyzers/correlation.rs, analyzers/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.3
```

EXISTING CODE:
- aether-core: TimeSeries

Implement analyzers/correlation.rs:
- `Correlation { metric_a: String, metric_b: String, coefficient: f64, interpretation: String }`
- `CorrelationAnalyzer`:
  - correlate(&self, a: &TimeSeries, b: &TimeSeries, window: Duration) -> f64
    - Pearson correlation coefficient on aligned time windows
    - Align samples by timestamp (nearest-neighbor interpolation)
    - Return r in [-1.0, 1.0]
  - find_correlated(&self, target: &TimeSeries, candidates: &[&TimeSeries], threshold: f64) -> Vec<Correlation>
    - Return all pairs with |r| > threshold (default 0.7)
    - Include human-readable interpretation: "CPU and memory strongly correlated (r=0.92)"
  - interpret(r: f64) -> &str
    - |r| > 0.9: "very strongly correlated"
    - |r| > 0.7: "strongly correlated"
    - |r| > 0.5: "moderately correlated"
    - else: "weakly correlated"

Update analyzers/mod.rs: pub mod correlation;

Tests:
- test_perfect_positive_correlation — identical series → r = 1.0
- test_perfect_negative_correlation — inverse series → r = -1.0
- test_uncorrelated_series — random → |r| < 0.3
- test_find_correlated_filters_by_threshold
- test_empty_series_returns_zero

COMMIT: feat(analyze): implement CorrelationAnalyzer with Pearson coefficient

### Task 10.2.2: Implement AnomalyDetector (deterministic)

```
Files: crates/aether-analyze/src/analyzers/anomaly.rs, analyzers/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.2.3
```

Implement analyzers/anomaly.rs:
- `ChangePoint { index: usize, timestamp: Instant, magnitude: f64 }`
- `AnomalyDetector`:
  - z_score(&self, series: &TimeSeries, window: Duration) -> f64
    - (last_value - mean) / stddev
  - is_outlier_iqr(&self, series: &TimeSeries) -> bool
    - Q1, Q3, IQR = Q3-Q1, outlier if value > Q3 + 1.5*IQR or < Q1 - 1.5*IQR
  - is_outlier_zscore(&self, series: &TimeSeries, threshold: f64) -> bool
    - |z_score| > threshold (default 3.0)
  - change_points(&self, series: &TimeSeries, sensitivity: f64) -> Vec<ChangePoint>
    - Sliding window mean comparison: if |mean_before - mean_after| > sensitivity * stddev → change point
  - stddev(&self, series: &TimeSeries, window: Duration) -> f64
  - percentile(&self, series: &TimeSeries, p: f64) -> f64

Update analyzers/mod.rs: pub mod anomaly;

Tests:
- test_z_score_normal_value — value near mean → z ≈ 0
- test_z_score_outlier — value 5 stddev above mean → z ≈ 5
- test_iqr_outlier_detection — known outlier detected
- test_iqr_normal_not_outlier
- test_change_point_detects_level_shift — series jumps from 50 to 100
- test_change_point_ignores_noise — stable noisy series → no change points
- test_percentile_median — 50th percentile of [1..100] ≈ 50

COMMIT: feat(analyze): implement deterministic AnomalyDetector

## Sprint 10.3: Full Rule Set

### Task 10.3.1: Expand to 30+ builtin rules

```
Files: crates/aether-analyze/src/rules/builtin.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 9.3.2, 10.1.2
```

EXISTING CODE:
- rules/builtin.rs: 10 rules (mem_approaching_oom, mem_leak_suspected, cpu_saturated, cpu_sustained_high, disk_almost_full, fd_approaching_limit, zombie_accumulation, thread_explosion, crash_loop, connections_growing)

Add 20+ more rules:
- mem_swap_growing — swap usage increasing > 10MB/min → Warning
- mem_rss_doubled — RSS > 2x value at process start → Warning
- cpu_cgroup_throttled — cpu throttle count growing → Warning
- cpu_context_switches_high — nonvoluntary ctxt switches > 10K/s → Info
- disk_heavy_read — read > 100MB/s sustained 5min → Info
- disk_inode_exhaustion — inode usage > 90% → Critical
- disk_readonly_fs — filesystem mounted read-only → Critical
- net_interface_saturated — bandwidth > 90% of link speed → Warning
- net_tcp_retransmits_high — retransmit rate > 5% → Warning
- net_established_above_limit — connections > 80% somaxconn → Warning
- proc_state_stuck_d — process in D state > 30s → Warning
- proc_nice_abnormal — process re-niced to -20 → Info
- proc_memory_vs_cgroup — mem > 80% of cgroup limit (without cgroup rule overlap) → Warning
- system_load_high — load_avg_1m > 4x cores → Critical
- system_memory_pressure — available memory < 10% → Critical
- system_oom_kills — OOM killer invoked → Critical
- config_cpu_underprovisioned — cpu.max reached consistently → Warning
- config_memory_tight — memory.max - current < 50MB → Warning
- config_pids_approaching_limit — pids.current > 80% pids.max → Warning
- correlated_cpu_memory — cpu and memory both growing > 0.8 correlation → Info

Tests:
- test_builtin_rules_count_gte_30
- test_all_categories_covered — at least one rule per DiagCategory
- test_no_duplicate_rule_ids
- test_all_rules_have_nonempty_names

COMMIT: feat(analyze): expand to 30+ builtin diagnostic rules

### Task 10.3.2: Integrate collectors into AnalyzeEngine

```
Files: crates/aether-analyze/src/engine.rs
Agent: rust-engineer
Test: cargo test -p aether-analyze
Depends: 10.1.1, 10.1.2, 10.1.3, 10.3.1
```

EXISTING CODE:
- engine.rs: AnalyzeEngine with run() loop (ingest → evaluate → generate → send)
- collectors/procfs.rs: ProcfsCollector
- collectors/cgroup.rs: CgroupCollector
- collectors/perf.rs: PerfCollector

Extend AnalyzeEngine:
- Add ProcfsCollector, CgroupCollector, PerfCollector fields
- In run() loop after ingesting metrics:
  1. For top-N CPU processes (configurable, default 10): collect ProcessProfile
  2. Collect host profile (loadavg, meminfo, disk)
  3. Feed cgroup limits to RuleEngine as ProcessLimits
  4. Attach ProfileFrames to diagnostics where available
  5. Run CorrelationAnalyzer on flagged processes
  6. Run AnomalyDetector for z-score based rules
- Store ProcessProfile cache: HashMap<u32, ProcessProfile> refreshed each tick
- Add config options: top_n_profiled: usize, enable_perf_profiling: bool

Tests:
- test_engine_with_collectors_produces_richer_diagnostics
- test_engine_without_collectors_still_works — graceful degradation
- test_top_n_limits_profiling_to_n_processes

COMMIT: feat(analyze): integrate collectors into AnalyzeEngine for deep profiling

## Sprint 10.4: Full Diagnostics Tab

### Task 10.4.1: Diagnostics detail panel with stack traces

```
Files: crates/aether-render/src/tui/diagnostics.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 10.3.2, 9.4.1
```

EXISTING CODE:
- tui/diagnostics.rs: DiagnosticsTab with basic list + detail panel
- aether-core: Diagnostic with evidence, recommendation, ProfileFrame

Extend diagnostics.rs detail panel:
- Progress bar for capacity metrics (████████░░ 87%)
- Trend sparkline (mini chart of last 60 values)
- Stack frames section: top 5 frames with percentage bars
- cgroup limits display: "memory.max=512MB cpu.max=200000/100000"
- Recommendation box with colored action
- Keybindings: [Enter] execute, [d] dismiss, [m] mute, [s] sort, [f] filter
- Sort modes: by severity (default), by time, by category
- Filter: show all / critical only / by category

COMMIT: feat(render): add stack traces and trend charts to Diagnostics detail panel

### Task 10.4.2: Add diagnostic markers to World3D

```
Files: crates/aether-render/src/engine/scene.rs, src/tui/world3d.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 10.4.1
```

EXISTING CODE:
- engine/scene.rs: SceneRenderer with node rendering, prediction pulse at 2Hz (orange)
- tui/world3d.rs: World3dTab with camera controls
- palette.rs: DIAGNOSTIC_CRITICAL (red), DIAGNOSTIC_WARNING (yellow), DIAGNOSTIC_INFO (cyan)

Extend scene.rs:
- Nodes with Critical diagnostic: red pulsation at 1Hz (slower, more alarming than prediction pulse)
- Nodes with Warning: yellow outline ring (static, no pulse)
- Correlation edges: when two processes have correlated diagnostics, draw dashed line between them in orange
- Priority: Critical > Warning > Info > Prediction (if multiple diagnostics on same node)

COMMIT: feat(render): add diagnostic visual markers to 3D scene

---

# PHASE 3: Prometheus

**Goal**: Bidirectional Prometheus integration — export Aether metrics, consume cluster metrics.

**Outcome**: Aether visible in Grafana, cluster metrics visible in Aether TUI.

## Sprint 11.1: Metrics Exporter

### Task 11.1.1: Initialize aether-metrics crate

```
Files: crates/aether-metrics/Cargo.toml, src/lib.rs, src/error.rs, CLAUDE.md
Agent: rust-engineer
Test: cargo check -p aether-metrics
Depends: none
```

Create new crate:
- Cargo.toml: aether-core, axum, tokio, thiserror, tracing, reqwest (for consumer)
- error.rs: MetricsError: Server(String), Export(String), Query(String), Http(#[from] reqwest::Error)
- CLAUDE.md: crate context

Update root Cargo.toml workspace members.
Update aether-terminal Cargo.toml: add aether-metrics dependency.

COMMIT: feat(metrics): initialize aether-metrics crate

### Task 11.1.2: Implement MetricRegistry and encoder

```
Files: crates/aether-metrics/src/exporter/mod.rs, exporter/registry.rs, exporter/encode.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-metrics
Depends: 11.1.1
```

Implement exporter/registry.rs:
- `MetricType` enum: Counter, Gauge, Histogram
- `MetricDesc { name: String, help: String, metric_type: MetricType }`
- `MetricValue` enum: Counter(f64), Gauge(f64), Histogram { sum: f64, count: u64, buckets: Vec<(f64, u64)> }
- `LabelSet = BTreeMap<String, String>`
- `MetricRegistry`:
  - register(&mut self, desc: MetricDesc)
  - set_gauge(&mut self, name: &str, labels: LabelSet, value: f64)
  - inc_counter(&mut self, name: &str, labels: LabelSet, delta: f64)
  - observe_histogram(&mut self, name: &str, labels: LabelSet, value: f64)
  - snapshot(&self) -> Vec<MetricFamily> — thread-safe snapshot for encoding

Implement exporter/encode.rs:
- encode_openmetrics(families: &[MetricFamily]) -> String
  - Prometheus text exposition format (OpenMetrics compatible)
  - # HELP, # TYPE headers
  - metric_name{label="value"} value timestamp

Tests:
- test_gauge_set_and_get
- test_counter_increments
- test_encode_produces_valid_prometheus_format
- test_labels_sorted_in_output
- test_histogram_buckets_ordered

COMMIT: feat(metrics): implement MetricRegistry and Prometheus text encoder

### Task 11.1.3: Implement /metrics HTTP server

```
Files: crates/aether-metrics/src/exporter/server.rs, exporter/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-metrics
Depends: 11.1.2
```

EXISTING CODE:
- exporter/registry.rs: MetricRegistry
- exporter/encode.rs: encode_openmetrics()

Implement exporter/server.rs:
- `MetricsExporter { registry: Arc<RwLock<MetricRegistry>> }`
  - new() -> Self
  - register_defaults(&self) — register all standard Aether metric descriptions
  - update_from_world(&self, world: &WorldGraph, diagnostics: &[Diagnostic])
    - Per-process: aether_process_cpu_percent, _memory_bytes, _open_fds, _thread_count, _hp, _xp
    - Per-host: aether_host_cpu_percent, _memory_total/used_bytes, _load_avg_1m/5m/15m
    - Diagnostics: aether_diagnostics_active{severity, category}
    - Internal: aether_analyze_evaluations_total, _rules_fired_total
  - serve(self, port: u16, cancel: CancellationToken) -> axum server
    - GET /metrics → encode registry → text/plain; version=0.0.4
    - GET /health → 200 OK

Tests:
- test_update_from_world_populates_gauges
- test_serve_responds_with_metrics (use axum test client)
- test_metrics_content_type_header

COMMIT: feat(metrics): implement /metrics HTTP endpoint for Prometheus scraping

## Sprint 11.2: Prometheus Consumer

### Task 11.2.1: Implement PromQL client

```
Files: crates/aether-metrics/src/consumer/mod.rs, consumer/client.rs, consumer/types.rs, consumer/query.rs, src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-metrics
Depends: 11.1.1
```

Implement consumer/types.rs:
- `PromResponse { status: String, data: PromData }`
- `PromData { result_type: String, result: Vec<PromResult> }`
- `PromResult { metric: HashMap<String, String>, values: Vec<(f64, String)> }`
- Serde deserialization for Prometheus JSON response format

Implement consumer/query.rs:
- `QueryBuilder` — helper to build PromQL strings
  - metric(name) → builder
  - label(key, value) → builder
  - rate(interval) → wraps in rate()
  - avg_over_time(interval) → wraps in avg_over_time()
  - build() → String

Implement consumer/client.rs:
- `PrometheusConsumer { endpoint: Url, client: reqwest::Client, poll_interval: Duration }`
  - new(endpoint: &str, poll_interval: Duration) -> Result<Self>
  - query(&self, promql: &str) -> Result<Vec<TimeSeries>>
    - GET /api/v1/query?query=... → parse JSON → convert to TimeSeries
  - query_range(&self, promql: &str, start: Instant, end: Instant, step: Duration) -> Result<Vec<TimeSeries>>
  - cluster_cpu(&self) -> Result<Vec<TimeSeries>> — preset: node_cpu_seconds_total
  - cluster_memory(&self) -> Result<Vec<TimeSeries>> — preset: node_memory_MemAvailable_bytes
  - run(&self, tx: mpsc::Sender<Vec<TimeSeries>>, cancel: CancellationToken)
    - Poll on interval, send TimeSeries batches

Update lib.rs: pub mod consumer;

Tests:
- test_query_builder_simple — metric("cpu_usage").build() == "cpu_usage"
- test_query_builder_with_labels — metric("up").label("job","node").build() == "up{job=\"node\"}"
- test_query_builder_rate — .rate("5m").build() == "rate(cpu_usage[5m])"
- test_prom_response_deserialize — parse sample JSON
- test_prom_result_to_timeseries — converts labels + values correctly

COMMIT: feat(metrics): implement PromQL client and query builder

### Task 11.2.2: Wire Prometheus into main.rs

```
Files: crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --help
Depends: 11.1.3, 11.2.1
```

EXISTING CODE:
- main.rs: Cli with existing flags, AnalyzeEngine wired (from Phase 1)

Add CLI flags:
- --metrics [PORT]: enable Prometheus exporter (default port 9090)
- --prometheus <URL>: connect to Prometheus for cluster metrics
- --prometheus-interval <SEC>: poll interval (default 15)

Wire:
1. If --metrics: create MetricsExporter, spawn serve() task
   - Update registry from WorldGraph on each render tick
2. If --prometheus: create PrometheusConsumer, spawn run() task
   - Connect prometheus_rx to AnalyzeEngine
3. If --prometheus: enable host selector in TUI (pass available hosts to App)

COMMIT: feat(terminal): wire Prometheus exporter and consumer with CLI flags

## Sprint 11.3: Cluster View in TUI

### Task 11.3.1: Add host selector to TUI

```
Files: crates/aether-render/src/tui/app.rs, tui/tabs.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 11.2.2
```

EXISTING CODE:
- tui/app.rs: App with tabs, handle_key
- tui/tabs.rs: tab bar rendering

Add to App:
- hosts: Vec<HostId> — available hosts (always includes "local")
- selected_host: usize — index into hosts vec
- Ctrl+Left/Right or [ / ]: cycle host selection
- Top bar extension: "[local ▾] [node-1] [node-2]    Cluster: 3 hosts, 1 critical"

All tabs filter by selected_host:
- Overview: show processes from selected host
- Diagnostics: filter diagnostics by host
- World3D: show graph for selected host

Special "All" entry: show aggregate across all hosts.

COMMIT: feat(render): add host selector for cluster-wide view

---

# PHASE 4: Integration & Polish

**Goal**: Connect all pieces, improve UX, ensure MCP exposes diagnostics, gamification rewards resolved diagnostics.

**Outcome**: Production-ready MVP. Complete diagnostic workflow: detect → display → act → verify.

## Sprint 12.1: Arbiter + MCP Integration

### Task 12.1.1: Connect diagnostic actions to Arbiter

```
Files: crates/aether-render/src/tui/diagnostics.rs, crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 10.4.1
```

EXISTING CODE:
- tui/diagnostics.rs: DiagnosticsTab with [Enter] to execute
- aether-core/arbiter.rs: ArbiterQueue with add_action()
- main.rs: Arbiter executor processes approved actions

Wire:
- DiagnosticsTab [Enter] → converts Recommendation to AgentAction → sends to ArbiterQueue
- RecommendedAction::ScaleUp → AgentAction with custom metadata
- RecommendedAction::KillProcess → AgentAction::Kill
- RecommendedAction::Restart → AgentAction::Kill + note
- Arbiter tab shows diagnostic-originated actions with "diag:" prefix

COMMIT: feat(terminal): connect diagnostic recommendations to Arbiter queue

### Task 12.1.2: Add get_diagnostics MCP tool

```
Files: crates/aether-mcp/src/tools.rs
Agent: rust-engineer
Test: cargo check -p aether-mcp
Depends: 10.3.2
```

EXISTING CODE:
- tools.rs: get_system_topology, inspect_process, list_anomalies, execute_action, predict_anomalies
- MCP tools read from Arc<RwLock<WorldGraph>> and Arc<Mutex<...>> shared state

Add new MCP tool:
- get_diagnostics(host?: string, severity?: string, category?: string) → JSON
  - Returns active diagnostics filtered by optional params
  - Each diagnostic includes: summary, evidence, recommendation, urgency
  - Response: { diagnostics: [...], stats: { critical: N, warning: N, info: N } }

COMMIT: feat(mcp): add get_diagnostics tool for AI agent access

### Task 12.1.3: Gamification for resolved diagnostics

```
Files: crates/aether-gamification/src/xp.rs, src/achievements.rs
Agent: rust-engineer
Test: cargo test -p aether-gamification
Depends: 9.3.4
```

EXISTING CODE:
- xp.rs: XpTracker with add_xp(), rank system
- achievements.rs: AchievementTracker with check() method

Add:
- XP rewards: +50 XP when a Critical diagnostic is resolved, +20 for Warning, +5 for Info
- New achievements:
  - "First Responder" — resolve first diagnostic
  - "Firefighter" — resolve 10 Critical diagnostics
  - "Stability Master" — run 1 hour with 0 Critical diagnostics
  - "Proactive" — resolve a diagnostic before it escalates from Warning to Critical

COMMIT: feat(gamification): add XP rewards and achievements for resolved diagnostics

## Sprint 12.2: Polish & Wire Existing Systems

### Task 12.2.1: Connect aether-script rules to analyze

```
Files: crates/aether-analyze/src/rules/engine.rs, crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo check -p aether-terminal
Depends: 9.3.1
```

EXISTING CODE:
- aether-script: ScriptEngine evaluates JIT-compiled .aether rules → RuleAction
- aether-analyze: RuleEngine evaluates builtin rules → RuleFinding → Diagnostic
- Both produce actions/diagnostics from different rule sources

Wire in main.rs:
- ScriptEngine RuleActions feed into AnalyzeEngine as additional findings
- User-defined .aether rules can trigger Diagnostics with custom categories
- Builtin rules + user rules coexist, no conflicts

COMMIT: feat(terminal): connect JIT script rules to diagnostic engine

### Task 12.2.2: Fix existing integration gaps

```
Files: multiple
Agent: rust-engineer
Test: cargo test --workspace
Depends: all previous
```

Fix known gaps from PoC audit:
1. aether-script: Connect parser in init_rules_engine() (line 357 TODO in main.rs)
2. aether-predict: Wire as optional analyzer in AnalyzeEngine (ML predictions → Diagnostics)
3. Help tab (F7): Update with new keybindings, diagnostic legend, host selector docs
4. --analyze enabled by default: verify all flags work together (--analyze + --predict + --rules + --ebpf + --metrics)

COMMIT: fix(terminal): resolve integration gaps from PoC audit

### Task 12.2.3: Update architecture docs

```
Files: docs/architecture.md, CLAUDE.md
Agent: rust-engineer
Test: none
Depends: all previous
```

Update docs to reflect MVP state:
- architecture.md: add aether-analyze and aether-metrics crates, update data flow, channel table, thread model
- CLAUDE.md: update crate list (11 crates), add new CLI flags, update build commands
- Crate CLAUDE.md files for aether-analyze and aether-metrics

COMMIT: docs: update architecture for MVP with analyze and metrics crates

---

## Summary

| Phase | Sprints | Tasks | New Code |
|-------|---------|-------|----------|
| Phase 1: Foundation | 4 sprints (9.1-9.4) | 10 tasks | core models, analyze engine, 10 rules, basic Diagnostics tab |
| Phase 2: Deep Analysis | 4 sprints (10.1-10.4) | 8 tasks | collectors (procfs/cgroup/perf), analyzers (correlation/anomaly), 30+ rules, full Diagnostics tab |
| Phase 3: Prometheus | 3 sprints (11.1-11.3) | 6 tasks | metrics exporter, PromQL consumer, cluster view |
| Phase 4: Integration | 2 sprints (12.1-12.2) | 6 tasks | Arbiter wiring, MCP tool, gamification, script integration, polish |
| **Total** | **13 sprints** | **30 tasks** | **2 new crates, ~8-10K LOC estimated** |

### CLI Flags (Final MVP)

```
aether-terminal [OPTIONS]

System:
  --log-level <LEVEL>          Logging level [default: info]
  --ebpf                       Enable eBPF telemetry (Linux, requires CAP_BPF)

Analysis:
  --no-analyze                 Disable diagnostic engine
  --analyze-interval <SEC>     Diagnostic tick interval [default: 5]
  --rules <PATH>               Load .aether rule files (JIT DSL)
  --predict                    Enable ML predictions (optional layer)
  --model-path <PATH>          ONNX model directory

Prometheus:
  --metrics [PORT]             Expose /metrics for Prometheus [default: 9090]
  --prometheus <URL>           Connect to Prometheus for cluster data
  --prometheus-interval <SEC>  Prometheus poll interval [default: 15]

Display:
  --no-3d                      Disable 3D rendering
  --no-game                    Disable gamification
  --theme <NAME>               Color theme [default: cyberpunk]

MCP:
  --mcp-stdio                  MCP stdio mode (no TUI)
  --mcp-sse [PORT]             MCP SSE server alongside TUI
```

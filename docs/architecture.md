# Architecture Overview

> **Implementation Status** (as of 2026-03-13): All 13 milestones complete. 38/38 sprints done.
>
> - **Phase A** (MS1–MS8, Mar 8): Core platform — 9 crates, TUI+3D, eBPF, MCP, JIT DSL, ML prediction, gamification
> - **Phase B** (MS9–MS13, Mar 10–13): MVP evolution — Web UI, deterministic diagnostics, Prometheus, integrations

## System Architecture

```
+--------------------------------------------------------------------------+
|                         aether-terminal (bin)                             |
|                                                                           |
|  +--------+ +--------+ +-------+ +-------+ +------+ +--------+           |
|  |Ingestion| | Render | |  MCP  | | Gamif.| | Pred.| | Script |           |
|  |Pipeline | | Engine | | Server| | Engine| | AI   | | Engine |           |
|  +---+----+ +---+----+ +--+----+ +--+----+ +--+---+ +---+----+           |
|      |          |          |         |         |         |                |
|  +---+----+ +---+----+ +--+----+                                         |
|  |Analyze | |Metrics | |  Web  |                                         |
|  |Engine  | |Exporter| | (axum |                                         |
|  |        | |+Consumer| | +React)|                                        |
|  +---+----+ +---+----+ +--+----+                                         |
|      |          |          |         |         |         |                |
|      +-----+----+-----+---+----+----+---------+---------+                |
|            |          |        |                                          |
|      +-----v----------v-------v------+                                   |
|      |         aether-core           |                                   |
|      |    WorldGraph + Channels      |                                   |
|      +------^------------------------+                                   |
|             |                                                            |
|      +------+--------+                                                   |
|      |  aether-ebpf  |                                                   |
|      | Ring Buffer /  |                                                   |
|      | BPF Programs   |                                                   |
|      +----------------+                                                   |
+--------------------------------------------------------------------------+
```

## Crate Implementation Status

| Crate | Status | Key Modules |
|-------|--------|-------------|
| aether-core | **Done** | models, graph (WorldGraph), events (SystemEvent, GameEvent, AgentAction, DiagnosticEvent), traits (SystemProbe, Storage), error (CoreError), arbiter (ArbiterQueue), HostId, TimeSeries, Diagnostic, Recommendation |
| aether-ingestion | **Done** | sysinfo_probe (SysinfoProbe), pipeline (IngestionPipeline), error |
| aether-render | **Done** | TUI: app, overview, world3d, network, arbiter, diagnostics (F6), help, tabs, input, widgets/sparklines. Engine: camera, layout, projection, rasterizer, scene, shading. Shared: braille, effects, palette |
| aether-mcp | **Done** | server (McpServer), tools (get_system_topology, inspect_process, list_anomalies, predict_anomalies, execute_action, get_diagnostics), transport/stdio, transport/sse, arbiter, error |
| aether-gamification | **Done** | hp (HpEngine), xp (XpTracker), achievements (AchievementTracker + diagnostic achievements), storage (SqliteStorage), error |
| aether-ebpf | **Done** | BPF programs (process/net/syscall monitors), loader (BpfLoader), ring buffer reader, kernel event types, error |
| aether-predict | **Done** | features (FeatureExtractor, FeatureVector), window (SlidingWindow), inference (OnnxModel, AnomalyDetector, CpuForecaster), models (PredictedAnomaly, AnomalyType, ConfidenceScorer), engine (PredictEngine), error |
| aether-script | **Done** | lexer (logos), ast, parser (recursive descent), types (type checker), codegen (Cranelift IR, JitCompiler), runtime (CompiledRuleSet, DurationTracker), hot_reload (HotReloader), engine (ScriptEngine), error. Bridge: JIT rules → diagnostic engine |
| aether-analyze | **Done** | MetricStore, TrendAnalyzer, CapacityAnalyzer, CorrelationAnalyzer, AnomalyDetector (deterministic), RuleEngine (30+ builtin rules), RecommendationGenerator, ProcfsCollector, CgroupCollector, AnalyzeEngine |
| aether-metrics | **Done** | MetricRegistry, Prometheus text encoder, /metrics HTTP exporter, PromQL consumer client |
| aether-web | **Done** | axum backend (REST API + WebSocket), React SPA (Overview, 3D Graph, Network, Arbiter, Diagnostics, Metrics pages), rust-embed static serving, host selector |
| aether-terminal | **Done** | main.rs: CLI (--log-level, --mcp-stdio, --mcp-sse, --ebpf, --predict, --model-path, --rules, --web [PORT]), orchestrates all 11 crates |

## Data Flow

```
Linux Kernel (eBPF probes)          OS (sysinfo fallback)
       |                                    |
       v (100K evt/sec via ring buf)        v (10Hz polling)
  aether-ebpf ------+            aether-ingestion (SysinfoProbe)
                     |                      |
                     +--- Hybrid Pipeline --+
                            |
                     mpsc<SystemEvent>
                            |
                            v
                   Core (WorldGraph updater)
                            |
          Arc<RwLock<WorldGraph>> + broadcast<WorldState>
                            |
    +-------+-------+-------+--------+-----------+-----------+----------+
    v       v       v       v        v           v           v          v
  Render  McpServer Gamif. Predict  Script     Analyze     Metrics    Web
  Engine            Engine Engine   Engine     Engine      Exporter   (axum+
    |       |       |      |        |          |           |          React)
  TUI+3D  Agent   SQLite  Anomaly  JIT Rules  Diagnostics Prometheus WebSocket
    |     (JSON)  (HP/XP)  Alerts  Actions    + Recommend. /metrics   + REST
    |       |                |       |          |           |          |
    +-------+-------+-------+-------+-----------+-----------+----------+
                    |
              ArbiterQueue (approve/deny actions)
```

## Channel Architecture

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| system_events | `mpsc` | IngestionPipeline / eBPF bridge | WorldGraph updater | SystemEvent |
| world_state | `Arc<RwLock<WorldGraph>>` | WorldGraph updater | Render, MCP, Predict, Script, Analyze, Web | shared graph |
| agent_actions | `mpsc` | McpServer | ArbiterQueue | AgentAction |
| game_events | `mpsc` | WorldGraph updater | Gamification | GameEvent |
| arbiter_feedback | `mpsc` | Render (Arbiter tab) / Web UI | ArbiterQueue | Approve/Deny |
| ebpf_events | `mpsc` | eBPF ring buffer reader | Ingestion bridge | RawKernelEvent |
| predictions | `mpsc` | PredictEngine | Core / Render | PredictedAnomaly |
| rule_actions | `mpsc` | ScriptEngine | ArbiterQueue / Core | RuleAction |
| diagnostics | `Arc<Mutex<Vec<Diagnostic>>>` | AnalyzeEngine | Render, MCP, Web, Gamification | Diagnostic findings |

## Thread/Task Model

```
Main Thread:
  +-- tokio runtime
        +-- task: eBPF ring buffer reader (100K evt/sec, Linux only, --ebpf flag)
        +-- task: IngestionPipeline (10Hz sysinfo polling, hybrid with eBPF)
        +-- task: WorldGraph updater (receives SystemEvent via mpsc)
        +-- task: Arbiter executor (processes approved AgentActions)
        +-- task: Action forwarder (sends action results)
        +-- task: McpServer (optional, stdio OR SSE via CLI flags)
        +-- task: PredictEngine (--predict flag, inference every 5s)
        +-- task: ScriptEngine (--rules flag, evaluates JIT rules every tick)
        +-- task: HotReloader (file watcher, recompiles rules on change)
        +-- task: AnalyzeEngine (deterministic diagnostics, 30+ rules, collectors)
        +-- task: MetricsExporter (Prometheus /metrics endpoint)
        +-- task: Web server (--web flag, axum HTTP + WebSocket + React SPA)
        +-- blocking: TUI render loop (crossterm + ratatui, 60fps)
              +-- Tab: Overview (process table, sparklines, prediction + diagnostic indicators)
              +-- Tab: World3D (3D graph with camera, Braille rasterizer, anomaly pulses)
              +-- Tab: Network (connection topology)
              +-- Tab: Arbiter (AI action queue, approve/deny UI)
              +-- Tab: Rules (F5, JIT rule engine stats)
              +-- Tab: Diagnostics (F6, diagnostic findings, severity, recommendations)
              +-- Tab: Help (keybindings)
```

## Crate Dependency Graph

```
aether-terminal
  +-- aether-core
  +-- aether-ebpf         -> aether-core
  +-- aether-ingestion    -> aether-core
  +-- aether-predict      -> aether-core
  +-- aether-script       -> aether-core
  +-- aether-render       -> aether-core
  +-- aether-mcp          -> aether-core
  +-- aether-gamification -> aether-core
  +-- aether-analyze      -> aether-core
  +-- aether-metrics      -> aether-core
  +-- aether-web          -> aether-core
```

Rule: library crates NEVER depend on each other. Only on aether-core. The binary crate wires them together.

## Key Design Patterns

1. **Hexagonal Architecture**: Core defines traits (ports), crates implement (adapters)
2. **Event Sourcing**: All state changes flow through typed events (SystemEvent, GameEvent, AgentAction)
3. **Shared Nothing**: Crates communicate only via channels and `Arc<RwLock<WorldGraph>>`
4. **Graceful Degradation**: sysinfo fallback when eBPF unavailable, hybrid pipeline auto-detects
5. **Feature Gating**: eBPF behind `#[cfg(feature = "ebpf")]`, predict behind `#[cfg(feature = "predict")]`
6. **Custom 3D Rasterizer**: Braille character rendering (2x4 subpixel), software projection pipeline (glam Mat4/Vec3), Phong shading, force-directed graph layout
7. **Dual MCP Transport**: stdio mode (Claude Desktop compat, TUI disabled) and SSE mode (axum HTTP, concurrent with TUI)
8. **Arbiter Pattern**: AI actions queued for human approval before execution; approve/deny via TUI or auto-approve
9. **RPG Gamification**: HP decay on high resource usage, XP gain on stability, ranks, achievements, SQLite persistence
10. **Zero-Copy Telemetry**: eBPF ring buffer reads directly into Rust structs via aya
11. **JIT Hot-Reload**: Rule files recompiled on file watch, swapped atomically via `Arc<ArcSwap<CompiledRuleSet>>`
12. **Streaming Inference**: ML model processes sliding window of features (60 samples = 5 min), ONNX via tract
13. **Deterministic Diagnostics**: Rule-based analysis (30+ rules) with trend/capacity/correlation analyzers — production-grade complement to ML predictions
14. **Prometheus Bidirectional**: Export own metrics + consume external Prometheus data
15. **Web UI**: React SPA embedded in binary, WebSocket realtime updates, concurrent with TUI

## Implemented Subsystem Details

### aether-core

- **WorldGraph** (`graph.rs`): `petgraph::StableGraph<ProcessNode, NetworkEdge>` with pid→NodeIndex HashMap. Methods: add/remove/update/find process, add/remove connection, apply_snapshot, processes/edges iterators.
- **Models** (`models.rs`): ProcessNode (pid, ppid, name, cpu_percent, mem_bytes, state, hp, xp, position_3d), NetworkEdge, SystemSnapshot, ProcessState, Protocol, ConnectionState.
- **Events** (`events.rs`): SystemEvent (ProcessStarted/Exited/Updated, NetworkConnected/Disconnected, SnapshotCompleted), GameEvent, AgentAction (Kill/Renice/Suspend/Resume/Investigate).
- **Traits** (`traits.rs`): `SystemProbe` (async snapshot → Result<SystemSnapshot, CoreError>), `Storage` (async save_session/load_rankings).
- **Arbiter** (`arbiter.rs`): ArbiterQueue with pending actions, auto-approve mode, mpsc-based approval flow.

### aether-render (3D Engine)

- **Projection**: perspective projection via glam, screen-space transform, depth buffer
- **Rasterizer** (`engine/rasterizer.rs`): Bresenham line drawing, Braille 2x4 subpixel encoding, z-buffering
- **Scene** (`engine/scene.rs`): SceneRenderer orchestrates camera → layout → project → rasterize → shade
- **Layout** (`engine/layout.rs`): force-directed graph layout (attraction/repulsion/centering)
- **Camera** (`engine/camera.rs`): orbital camera with azimuth/elevation, zoom, smooth interpolation
- **Shading** (`engine/shading.rs`): Phong lighting model, ambient + diffuse + specular
- **Effects** (`effects.rs`): CPU-load pulsation, HP-based coloring, data flow animation on edges

### aether-mcp (MCP Server)

- **Server** (`server.rs`): McpServer with rmcp, supports 4 tools
- **Tools** (`tools.rs`): get_system_topology, inspect_process, list_anomalies, execute_action, predict_anomalies
- **Transport**: stdio (`transport/stdio.rs`) and SSE/HTTP (`transport/sse.rs` via axum)

### aether-gamification

- **HpEngine** (`hp.rs`): HP decay/recovery based on CPU/memory thresholds
- **XpTracker** (`xp.rs`): XP gain on process stability, rank system
- **AchievementTracker** (`achievements.rs`): milestone-based achievements
- **SqliteStorage** (`storage.rs`): rusqlite persistence for sessions, rankings

### aether-ebpf (eBPF Telemetry)

```
BPF Programs:
  +-- process_monitor: tracepoint/sched_process_fork, tracepoint/sched_process_exit
  +-- net_monitor: kprobe/tcp_connect, kprobe/tcp_close, tracepoint/net_dev_xmit
  +-- syscall_monitor: raw_tracepoint/sys_enter (configurable syscall filter)

Ring Buffer Protocol:
  Kernel -> Ring Buffer (per-CPU, 256KB) -> aya::maps::RingBuf -> tokio mpsc -> Core
```

- **BpfLoader** (`loader.rs`): loads BPF programs, attaches to tracepoints/kprobes
- **RingBufReader** (`ring_buf.rs`): async ring buffer reader, zero-copy event delivery
- **Kernel Events** (`events.rs`): ProcessFork, ProcessExit, TcpConnect, TcpClose, SyscallEvent — all `#[repr(C)]`
- **eBPF Bridge** (`aether-ingestion/ebpf_bridge.rs`): translates kernel events → SystemEvent
- **Hybrid Pipeline** (`aether-ingestion/pipeline.rs`): auto-fallback sysinfo ↔ eBPF
- CLI: `--ebpf` flag enables eBPF telemetry (requires CAP_BPF or root)

### aether-predict (Predictive AI)

```
Pipeline:
  WorldState (every 5s) -> FeatureExtractor -> SlidingWindow (60 samples = 5min)
                                                    |
                                              ONNX Inference (tract)
                                                    |
                                              PredictedAnomaly {
                                                pid, anomaly_type,
                                                confidence, eta_seconds,
                                                recommended_action
                                              }
```

- **FeatureExtractor** (`features.rs`): 9-dimensional feature vector per process, min-max normalized via RunningStats
- **SlidingWindow** (`window.rs`): per-PID VecDeque of FeatureVectors, capacity 60, to_tensor() for inference
- **OnnxModel** (`inference.rs`): tract-onnx model loading and inference
- **AnomalyDetector** (`inference.rs`): autoencoder, MSE reconstruction error = anomaly score
- **CpuForecaster** (`inference.rs`): LSTM, predicts CPU 60s ahead with confidence
- **PredictedAnomaly** (`models.rs`): pid, anomaly_type, confidence, eta_seconds, recommended_action
- **AnomalyType** (`models.rs`): OomRisk, CpuSpike, MemoryLeak, Deadlock, DiskExhaustion
- **ConfidenceScorer** (`models.rs`): score() and classify() methods
- **PredictEngine** (`engine.rs`): async task, select_top_n by variance, configurable interval/threshold
- CLI: `--predict` flag enables engine, `--model-path` for ONNX models
- Visualization: prediction indicators in Overview tab, pulsing orange outline on predicted nodes in 3D

### aether-script (JIT Rule DSL)

```
Compilation Pipeline:
  .aether file -> Lexer (logos) -> Token stream
                                      |
                                  Parser (recursive descent) -> AST
                                      |
                                  Type Checker -> Typed AST
                                      |
                                  Cranelift IR Generator -> CLIF
                                      |
                                  Cranelift Codegen -> Native x86_64/aarch64
                                      |
                                  CompiledRuleSet (function pointers)
```

- **Lexer** (`lexer.rs`): logos-based tokenizer for Aether DSL
- **AST** (`ast.rs`): Rule, Condition, Action, Expression nodes
- **Parser** (`parser.rs`): hand-written recursive descent parser
- **Type Checker** (`types.rs`): validates AST types (Process, System, Duration, Percentage, numeric)
- **CodeGenerator** (`codegen.rs`): Cranelift IR from typed AST, WorldStateFFI/RuleResult `#[repr(C)]`
- **JitCompiler** (`codegen.rs`): compiles Cranelift Function → native code via JITModule
- **CompiledRuleSet** (`runtime.rs`): evaluates rules against WorldState → Vec\<RuleAction\>
- **DurationTracker** (`runtime.rs`): stateful rules fire only after elapsed time
- **HotReloader** (`hot_reload.rs`): file watcher (notify), debounce 100ms, atomic swap via Arc\<ArcSwap\>
- **ScriptEngine** (`engine.rs`): async task, evaluates rules every tick, tracks stats
- CLI: `--rules <PATH>` flag loads rules, spawns engine + hot-reloader
- TUI: Rules tab (F5) shows active rules, stats, details
- Bridge: JIT-compiled rules feed findings into AnalyzeEngine diagnostic pipeline

### aether-analyze (Deterministic Diagnostics)

- **MetricStore** (`metric_store.rs`): TimeSeries storage, sliding window per metric
- **RuleEngine** (`engine.rs`): 30+ builtin rules (CPU, memory, disk, network, zombie, OOM risk, etc.)
- **TrendAnalyzer** (`trend.rs`): linear regression on time-series data
- **CapacityAnalyzer** (`capacity.rs`): capacity planning projections
- **CorrelationAnalyzer** (`correlation.rs`): Pearson coefficient between metrics
- **AnomalyDetector** (`anomaly.rs`): deterministic anomaly detection (threshold + trend based)
- **RecommendationGenerator** (`recommendations/generator.rs`): actionable recommendations per diagnostic
- **Collectors**: ProcfsCollector (/proc profiling), CgroupCollector (container limits)
- **AnalyzeEngine** (`engine.rs`): async task, runs all analyzers + rules → Vec<Diagnostic>
- Diagnostics wired to: Arbiter (auto-execute), MCP (get_diagnostics tool), Gamification (XP rewards), TUI (F6 tab), Web UI

### aether-metrics (Prometheus Integration)

- **MetricRegistry**: collects system metrics for export
- **Prometheus Encoder**: text format encoder for /metrics endpoint
- **HTTP Exporter**: standalone axum server exposing /metrics
- **PromQL Consumer**: client for querying external Prometheus instances

### aether-web (Web UI)

```
Architecture:
  axum HTTP server (--web [PORT] flag)
    +-- REST API: /api/processes, /api/stats, /api/diagnostics, /api/metrics, /api/arbiter
    +-- WebSocket: /ws (500ms push of WorldUpdate)
    +-- Static: React SPA served via rust-embed

  React SPA (Vite + TypeScript):
    +-- Pages: Overview, 3D Graph (react-three-fiber), Network, Arbiter, Diagnostics, Metrics
    +-- Stores: zustand (worldStore, metricsStore)
    +-- Charts: recharts (sparklines, time-series, area charts)
    +-- Host Selector: cluster-ready multi-host filtering
```

- **Backend** (`server.rs`, `api.rs`, `ws.rs`, `state.rs`): axum router, SharedState, REST + WebSocket
- **Frontend**: React 18 + TypeScript, Vite build, embedded in binary via rust-embed
- Runs alongside TUI (concurrent) or standalone via `--web` flag

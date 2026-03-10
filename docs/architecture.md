# Architecture Overview

> **Implementation Status** (as of 2026-03-10): MS1-MS4 complete, MS5 partial (HP/XP/persistence done, polish pending). MS6-MS8 planned.

## System Architecture

```
+-----------------------------------------------------------------+
|                     aether-terminal (bin)                        |
|                                                                  |
|  +--------+ +--------+ +-------+ +-------+ +------+ +--------+  |
|  |Ingestion| | Render | |  MCP  | | Gamif.| | Pred.| | Script |  |
|  |Pipeline | | Engine | | Server| | Engine| | AI   | | Engine |  |
|  +---+----+ +---+----+ +--+----+ +--+----+ +--+---+ +---+----+  |
|   DONE       DONE        DONE      DONE     PLANNED  PLANNED    |
|      |          |          |         |         |         |       |
|      +-----+----+-----+---+----+----+---------+---------+       |
|            |          |        |                                 |
|      +-----v----------v-------v------+                          |
|      |         aether-core           |                          |
|      |    WorldGraph + Channels      |  DONE                    |
|      +------^------------------------+                          |
|             |                                                   |
|      +------+--------+                                          |
|      |  aether-ebpf  |  PLANNED                                 |
|      | Ring Buffer /  |                                          |
|      | BPF Programs   |                                          |
|      +----------------+                                          |
+-----------------------------------------------------------------+
```

## Crate Implementation Status

| Crate | Status | Key Modules |
|-------|--------|-------------|
| aether-core | **Done** | models, graph (WorldGraph), events (SystemEvent, GameEvent, AgentAction), traits (SystemProbe, Storage), error (CoreError), arbiter (ArbiterQueue) |
| aether-ingestion | **Done** | sysinfo_probe (SysinfoProbe), pipeline (IngestionPipeline), error |
| aether-render | **Done** | TUI: app, overview, world3d, network, arbiter, help, tabs, input, widgets/sparklines. Engine: camera, layout, projection, rasterizer, scene, shading. Shared: braille, effects, palette |
| aether-mcp | **Done** | server (McpServer), tools (4 MCP tools), transport/stdio, transport/sse, arbiter, error |
| aether-gamification | **Done** | hp (HpEngine), xp (XpTracker), achievements (AchievementTracker), storage (SqliteStorage), error |
| aether-ebpf | **Stub** | lib.rs only. Planned: BPF programs, ring buffer, loader |
| aether-predict | **Stub** | lib.rs only. Planned: features, window, inference, models, engine |
| aether-script | **Stub** | lib.rs only. Planned: lexer, parser, ast, types, codegen, runtime, hot_reload, engine |
| aether-terminal | **Done** | main.rs: CLI (--log-level, --mcp-stdio, --mcp-sse), orchestrates all implemented crates |

## Data Flow (Current Implementation)

```
OS (sysinfo polling, 10Hz)
       |
  aether-ingestion (SysinfoProbe)
       |
  mpsc<SystemEvent>
       |
       v
  Core (WorldGraph updater)
       |
  Arc<RwLock<WorldGraph>>  ----+---- broadcast<WorldState>
       |                       |
       |            +----------+----------+-----------+
       |            v          v          v           v
       |       RenderEngine McpServer  Gamification  ArbiterQueue
       |            |         |          |            |
       |       Terminal    AI Agent   SQLite      approve/deny
       |       (TUI+3D)   (JSON-RPC)  (HP/XP)      actions
       |            |         |                       ^
       |            |         +---- AgentAction ------+
       |            +---- arbiter_feedback -----------+
       |
  mpsc<GameEvent> --> HpEngine, XpTracker, AchievementTracker
```

### Channels (Implemented)

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| system_events | `mpsc` | IngestionPipeline | WorldGraph updater | SystemEvent |
| world_state | `Arc<RwLock<WorldGraph>>` | WorldGraph updater | Render, MCP | shared graph |
| agent_actions | `mpsc` | McpServer | ArbiterQueue | AgentAction |
| game_events | `mpsc` | WorldGraph updater | Gamification | GameEvent |
| arbiter_feedback | `mpsc` | Render (Arbiter tab) | ArbiterQueue | Approve/Deny |

### Channels (Planned — MS6-MS8)

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| ebpf_events | `mpsc` | eBPF ring buffer reader | Ingestion bridge | RawKernelEvent |
| predictions | `mpsc` | PredictEngine | Core / Render | PredictedAnomaly |
| rule_actions | `mpsc` | ScriptEngine | ArbiterQueue / Core | RuleAction |

## Thread/Task Model (Current)

```
Main Thread:
  +-- tokio runtime
        +-- task: IngestionPipeline (10Hz sysinfo polling)
        +-- task: WorldGraph updater (receives SystemEvent via mpsc)
        +-- task: Arbiter executor (processes approved AgentActions)
        +-- task: Action forwarder (sends action results)
        +-- task: McpServer (optional, stdio OR SSE via CLI flags)
        +-- blocking: TUI render loop (crossterm + ratatui, 60fps)
              +-- Tab: Overview (process table, sparklines, detail panel)
              +-- Tab: World3D (3D graph with camera, Braille rasterizer)
              +-- Tab: Network (connection topology)
              +-- Tab: Arbiter (AI action queue, approve/deny UI)
              +-- Tab: Help (F1-F4 keybindings)
```

### Planned Tasks (MS6-MS8)

```
        +-- task: eBPF ring buffer reader (100K evt/sec, Linux only)
        +-- task: GamificationEngine (receives GameEvent, updates HP/XP)
        +-- task: PredictEngine (receives WorldState, runs inference every 5s)
        +-- task: ScriptEngine (receives WorldState, evaluates JIT rules every tick)
        +-- Tab: Rules (F5, JIT rule engine stats)
```

## Crate Dependency Graph

```
aether-terminal
  +-- aether-core
  +-- aether-ebpf        -> aether-core
  +-- aether-ingestion   -> aether-core
  +-- aether-predict     -> aether-core
  +-- aether-script      -> aether-core
  +-- aether-render      -> aether-core
  +-- aether-mcp         -> aether-core
  +-- aether-gamification -> aether-core
```

Rule: library crates NEVER depend on each other. Only on aether-core. The binary crate wires them together.

## Key Design Patterns

1. **Hexagonal Architecture**: Core defines traits (ports), crates implement (adapters)
2. **Event Sourcing**: All state changes flow through typed events (SystemEvent, GameEvent, AgentAction)
3. **Shared Nothing**: Crates communicate only via channels and `Arc<RwLock<WorldGraph>>`
4. **Graceful Degradation**: sysinfo fallback when eBPF unavailable; stub crates compile but no-op
5. **Feature Gating**: eBPF behind `#[cfg(feature = "ebpf")]`, predict behind `#[cfg(feature = "predict")]`

### Patterns Implemented

6. **Custom 3D Rasterizer**: Braille character rendering (2x4 subpixel), software projection pipeline (glam Mat4/Vec3), Phong shading, force-directed graph layout
7. **Dual MCP Transport**: stdio mode (Claude Desktop compat, TUI disabled) and SSE mode (axum HTTP, concurrent with TUI)
8. **Arbiter Pattern**: AI actions queued for human approval before execution; approve/deny via TUI or auto-approve
9. **RPG Gamification**: HP decay on high resource usage, XP gain on stability, ranks, achievements, SQLite persistence

### Patterns Planned

10. **Zero-Copy Telemetry**: eBPF ring buffer reads directly into Rust structs without allocation
11. **JIT Hot-Reload**: Rule files recompiled on file watch, swapped atomically via `Arc<ArcSwap<CompiledRuleSet>>`
12. **Streaming Inference**: ML model processes sliding window of features (60 samples = 5 min)

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
- **Tools** (`tools.rs`): get_system_topology, inspect_process, list_anomalies, execute_action
- **Transport**: stdio (`transport/stdio.rs`) and SSE/HTTP (`transport/sse.rs` via axum)

### aether-gamification

- **HpEngine** (`hp.rs`): HP decay/recovery based on CPU/memory thresholds
- **XpTracker** (`xp.rs`): XP gain on process stability, rank system
- **AchievementTracker** (`achievements.rs`): milestone-based achievements
- **SqliteStorage** (`storage.rs`): rusqlite persistence for sessions, rankings

## Planned Subsystem: eBPF Telemetry (aether-ebpf) — MS6

```
BPF Programs (bpf/*.bpf.c):
  +-- process_monitor: tracepoint/sched_process_fork, tracepoint/sched_process_exit
  +-- net_monitor: kprobe/tcp_connect, kprobe/tcp_close, tracepoint/net_dev_xmit
  +-- syscall_monitor: raw_tracepoint/sys_enter (configurable syscall filter)

Ring Buffer Protocol:
  Kernel -> Ring Buffer (per-CPU, 256KB) -> aya::maps::RingBuf -> tokio mpsc -> Core

Event Types:
  ProcessFork { parent_pid, child_pid, timestamp_ns }
  ProcessExit { pid, exit_code, timestamp_ns }
  TcpConnect { pid, src, dst, timestamp_ns }
  TcpClose { pid, src, dst, bytes_sent, bytes_recv, duration_ns }
  SyscallEvent { pid, syscall_nr, latency_ns }
```

## Planned Subsystem: Predictive AI (aether-predict) — MS8

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

Models:
  anomaly_detector.onnx  -- Autoencoder, reconstruction error = anomaly score
  cpu_forecast.onnx      -- LSTM/Transformer, predicts CPU 60s ahead

Feature Vector (per process, per tick):
  [cpu_pct, mem_bytes, mem_delta, fd_count, thread_count,
   net_bytes_in, net_bytes_out, syscall_rate, io_wait_pct]
```

## Planned Subsystem: JIT Rule DSL (aether-script) — MS7

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

Runtime:
  WorldState -> CompiledRuleSet.evaluate() -> Vec<RuleAction>

  RuleAction = Alert { message, severity }
             | Kill { pid }
             | Log { message }
             | Metric { name, value }

Hot-Reload:
  File watcher (notify crate) -> recompile -> atomic swap via Arc<ArcSwap<CompiledRuleSet>>
```

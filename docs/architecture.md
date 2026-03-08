# Architecture Overview

## System Architecture

```
+-----------------------------------------------------------------+
|                     aether-terminal (bin)                         |
|                                                                  |
|  +--------+ +--------+ +-------+ +-------+ +------+ +--------+  |
|  |Ingestion| | Render | |  MCP  | | Gamif.| | Pred.| | Script |  |
|  |Pipeline | | Engine | | Server| | Engine| | AI   | | Engine |  |
|  +---+----+ +---+----+ +--+----+ +--+----+ +--+---+ +---+----+  |
|      |          |          |         |         |         |        |
|      +-----+----+-----+---+----+----+---------+---------+        |
|            |          |        |                                  |
|      +-----v----------v-------v------+                           |
|      |         aether-core           |                           |
|      |    WorldGraph + Channels      |                           |
|      +------^------------------------+                           |
|             |                                                    |
|      +------+--------+                                           |
|      |  aether-ebpf  |                                           |
|      | Ring Buffer /  |                                           |
|      | BPF Programs   |                                           |
|      +----------------+                                           |
+-----------------------------------------------------------------+
```

## Data Flow

```
Linux Kernel (eBPF probes)          OS (sysinfo fallback)
       |                                    |
       v (100K evt/sec via ring buf)        v (10Hz polling)
  aether-ebpf ------+            aether-ingestion
                     |                      |
                     +------+-------+-------+
                            |
                     mpsc<SystemEvent>
                            |
                            v
                   Core (WorldGraph updater)
                            |
                   broadcast<WorldState>
                            |
          +---------+-------+--------+-----------+
          v         v       v        v           v
     RenderEngine McpServer Gamif. PredictEngine ScriptEngine
          |         |       |        |           |
     Terminal   AI Agent  SQLite   Anomaly     JIT-compiled
     (TUI+3D)  (JSON-RPC) (HP/XP)  Alerts     Rule Actions
          ^         |                |           |
          |         v                v           v
          +---- ArbiterQueue <------+-----------+
                (approve/deny)
```

## Channel Architecture

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| system_events | `mpsc` | Ingestion / eBPF | Core Updater | SystemEvent |
| world_state | `broadcast` | Core Updater | Render, MCP, Game, Predict, Script | WorldState snapshot |
| agent_actions | `mpsc` | MCP Server | Arbiter Queue | AgentAction |
| game_events | `mpsc` | Core Updater | Gamification Engine | GameEvent |
| arbiter_feedback | `mpsc` | Render (UI) | Arbiter Queue | Approve/Deny |
| predictions | `mpsc` | PredictEngine | Core / Render | PredictedAnomaly |
| rule_actions | `mpsc` | ScriptEngine | Arbiter Queue / Core | RuleAction |
| ebpf_events | `mpsc` | eBPF ring buffer reader | Ingestion bridge | RawKernelEvent |

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

## Thread/Task Model

```
Main Thread:
  +-- tokio runtime
        +-- task: eBPF ring buffer reader (100K evt/sec, Linux only)
        +-- task: IngestionPipeline.fast_tick (10Hz)
        +-- task: IngestionPipeline.slow_tick (1Hz)
        +-- task: Core WorldGraph updater (receives SystemEvent, broadcasts WorldState)
        +-- task: GamificationEngine (receives GameEvent, updates HP/XP)
        +-- task: PredictEngine (receives WorldState, runs inference every 5s)
        +-- task: ScriptEngine (receives WorldState, evaluates JIT rules every tick)
        +-- task: McpServer (stdio OR http, based on CLI flags)
        +-- blocking: TUI render loop (crossterm + ratatui, 60fps)
```

## Key Design Patterns

1. **Hexagonal Architecture**: Core defines traits (ports), crates implement (adapters)
2. **Event Sourcing**: All state changes flow through typed events
3. **Shared Nothing**: Crates communicate only via channels and Arc<RwLock<WorldGraph>>
4. **Graceful Degradation**: --no-3d, --no-game, sysinfo fallback when eBPF unavailable
5. **Feature Gating**: eBPF behind `#[cfg(feature = "ebpf")]`, predict behind `#[cfg(feature = "predict")]`
6. **Zero-Copy Telemetry**: eBPF ring buffer reads directly into Rust structs without allocation
7. **JIT Hot-Reload**: Rule files recompiled on SIGHUP or file watch, swapped atomically
8. **Streaming Inference**: ML model processes sliding window of features, no batch accumulation

## New Subsystem: eBPF Telemetry (aether-ebpf)

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

## New Subsystem: Predictive AI (aether-predict)

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

## New Subsystem: JIT Rule DSL (aether-script)

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
  File watcher (notify crate) or SIGHUP -> recompile -> atomic swap via Arc<RwLock>
```

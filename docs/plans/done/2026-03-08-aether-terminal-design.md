# Aether-Terminal: Full Product Design

**Date**: 2026-03-08
**Status**: Approved
**Author**: Arman Ramazan

---

## Product Vision

Aether-Terminal — кинематографический 3D-визуализированный терминальный интерфейс управления системами, интегрированный с Model Context Protocol (MCP) для автономного взаимодействия с ИИ-агентами.

**Одно предложение**: Командный центр Staff-инженера, где система — живой 3D-мир с eBPF-телеметрией, предиктивным AI, JIT-компилируемыми правилами, а ИИ-агент может исследовать и управлять инфраструктурой через MCP.

### Роль в портфолио

| Проект | Роль | Ключевой навык |
|--------|------|----------------|
| KnowledgeOS | Продуктовый MVP | Fullstack, Product Vision, UX |
| ONNX CLI | Системная библиотека | Rust, Performance, ML Integration |
| **Aether-Terminal** | **Инновационный Showcase (10/10)** | **eBPF, JIT Compiler, ML Inference, 3D TUI, MCP** |

### Ключевые решения

| Вопрос | Решение | Обоснование |
|--------|---------|-------------|
| Телеметрия | eBPF (aya) + sysinfo fallback | eBPF для Linux prod, sysinfo для dev/macOS/Windows |
| 3D-визуализация | Полный software rasterizer | Главный differentiator, showcase Rust-навыков |
| MCP транспорт | Dual mode (stdio + SSE) | Совместимость + realtime Arbiter Mode |
| Мульти-провайдер | Да (Claude, Gemini, OpenAI) | Универсальный агентский хаб через MCP-стандарт |
| Предиктивный AI | tract-onnx (on-device) | Без внешних API, zero latency, train in PyTorch |
| Rule Engine | Cranelift JIT | ms-fast compilation, hot-reload, native performance |
| DSL Parser | Hand-written recursive descent | Portfolio differentiator, demonstrates compiler skills |
| Геймификация | Full (C), поэтапно | B (Light RPG) в первом релизе, C — пост-релиз |
| Структура проекта | Cargo workspace (9 crates) | Hexagonal architecture, параллельная компиляция |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  aether-terminal (bin)               │
│            Event Loop / App Orchestrator             │
└──────┬──────────┬──────────┬──────────┬─────────────┘
       │          │          │          │
┌──────▼──┐ ┌────▼─────┐ ┌─▼────────┐ ┌▼───────────┐ ┌▼─────────┐ ┌▼────────┐
│ aether- │ │ aether-  │ │ aether-  │ │ aether-    │ │ aether-  │ │ aether- │
│ingestion│ │ render   │ │ mcp      │ │gamification│ │ predict  │ │ script  │
│ + ebpf  │ │          │ │          │ │            │ │          │ │         │
│ sysinfo │ │ 3D engine│ │ stdio    │ │ HP/XP/     │ │ ONNX     │ │ Lexer/  │
│ eBPF    │ │ TUI/     │ │ SSE/HTTP │ │ achievmnts │ │ tract    │ │ Parser/ │
│ DPI     │ │ Braille  │ │ multi-AI │ │ SQLite     │ │ forecast │ │ Cranelft│
└──────┬──┘ └────┬─────┘ └─┬────────┘ └┬───────────┘ └┬─────────┘ └┬────────┘
       │         │         │           │              │            │
       └─────────┴────┬────┴───────────┴──────────────┴────────────┘
                 ┌────▼─────┐
                 │ aether-  │
                 │ core     │
                 │          │
                 │ petgraph │
                 │ models   │
                 │ state    │
                 └──────────┘
```

**Принцип**: Все crates зависят от `aether-core`, но НЕ друг от друга. Общение через trait-абстракции и каналы (`tokio::broadcast` / `tokio::mpsc`).

---

## Component 1: aether-core

Сердце системы — графовая модель мира.

### Key Types

```rust
struct ProcessNode {
    pid: u32,
    ppid: u32,
    name: String,
    cpu_percent: f32,
    mem_bytes: u64,
    state: ProcessState,           // Running, Sleeping, Zombie
    connections: Vec<SocketId>,
    hp: f32,                       // gamification
    xp: u32,                       // gamification
    position_3d: Vec3,             // position in 3D scene
}

struct NetworkEdge {
    source_pid: u32,
    dest: SocketAddr,
    protocol: Protocol,            // TCP, UDP, QUIC, DNS
    bytes_per_sec: u64,
    state: ConnectionState,
}

// Graph = petgraph::Graph<ProcessNode, NetworkEdge>
// Updated by ingestion, read by render + mcp + gamification
```

### Trait Abstractions (Hexagonal Ports)

```rust
trait SystemProbe: Send + Sync {
    async fn snapshot(&self) -> SystemSnapshot;
    fn subscribe(&self) -> broadcast::Receiver<SystemEvent>;
}

trait Renderer: Send {
    fn render(&mut self, state: &WorldState, frame: &mut Frame);
}

trait McpTransport: Send + Sync {
    async fn start(&self, handler: Arc<McpHandler>) -> Result<()>;
}

trait Storage: Send + Sync {
    async fn save_session(&self, session: &Session) -> Result<()>;
    async fn load_rankings(&self) -> Result<Vec<Ranking>>;
}
```

---

## Component 2a: aether-ebpf

Ядро телеметрии. Загрузка BPF-программ в ядро Linux, чтение событий через ring buffer.

```
eBPF Architecture:

  BPF Programs (bpf/*.bpf.c):
    ├── process_monitor.bpf.c    tracepoint/sched_process_fork, exit
    ├── net_monitor.bpf.c        kprobe/tcp_connect, tcp_close
    └── syscall_monitor.bpf.c    raw_tracepoint/sys_enter (configurable)

  Loader (aya, pure Rust):
    ├── BpfLoader::load(program_bytes) → attached probes
    ├── RingBuf::poll() → zero-copy event reading
    └── Maps: per-CPU hash maps for aggregation

  Event Types:
    ProcessFork { parent_pid, child_pid, comm, timestamp_ns }
    ProcessExit { pid, exit_code, runtime_ns, timestamp_ns }
    TcpConnect  { pid, src, dst, timestamp_ns }
    TcpClose    { pid, src, dst, bytes_sent, bytes_recv, duration_ns }
    SyscallEvent { pid, syscall_nr, latency_ns, timestamp_ns }
```

**Throughput target**: 100K+ events/sec with zero-copy ring buffer reads.
**Requirement**: Linux kernel 5.8+, `CAP_BPF` or root.
**Feature-gated**: `#[cfg(feature = "ebpf")]`

## Component 2b: aether-ingestion

Сбор системных данных. Мост между eBPF и sysinfo.

```
SystemProbe trait
    ├── SysinfoProbe (crossplatform, no root)
    │     └── sysinfo crate: CPU, RAM, disks, processes, networks
    │     └── etherparse: DPI packet analysis (optional, libpcap)
    │
    └── EbpfBridge (Linux, wraps aether-ebpf events into SystemEvent)
          └── Converts RawKernelEvent → SystemEvent
          └── Merges eBPF streams with sysinfo gap-filling
```

### Update Frequencies

| Tick | Interval | Data |
|------|----------|------|
| `fast_tick` | 16ms (60Hz) | CPU%, memory, connection states, positions |
| `slow_tick` | 1000ms (1Hz) | Disk I/O, network throughput, process tree rebuild |

### Tokio Tasks

- `fast_tick` task: lightweight polling, pushes `SystemEvent::MetricsUpdate`
- `slow_tick` task: full process tree scan, pushes `SystemEvent::TopologyChange`
- Both send events via `mpsc<SystemEvent>` to `aether-core`

---

## Component 3: aether-render

Самый сложный и зрелищный компонент. Два слоя.

### Layer 1: TUI Framework (ratatui + crossterm)

- **Tab system**: `[F1] Overview` / `[F2] 3D World` / `[F3] Network` / `[F4] Arbiter`
- **Vim navigation**: hjkl, /, :cmd
- **Status bar**: system health, uptime, XP, current rank
- **Sparkline widgets**: CPU/RAM/Network in Overview tab

### Layer 2: Software 3D Rasterizer

```
Pipeline:
  World Space (petgraph nodes with Vec3 positions)
    → View Transform (camera: position, yaw, pitch, zoom)
      → Perspective Projection (FOV, aspect ratio, near/far)
        → Screen Space (terminal coordinates)
          → Braille Rasterization (2x4 subpixels per cell)
            → Color mapping (TrueColor ANSI)
```

### Rendering Algorithms

| Algorithm | Purpose |
|-----------|---------|
| **Z-buffer** | Depth testing, array `f32[term_w*2 × term_h*4]` (Braille resolution) |
| **Phong shading** | Ambient + diffuse for node volume. Sphere normals for processes |
| **Bresenham line** | Edge rendering in Braille subpixel space |
| **Force-directed layout** | Fruchterman-Reingold in 3D — repulsion between nodes, attraction along edges |
| **Orbital camera** | Rotation around center of mass, WASD + mouse scroll |

### Render Modes

| Mode | Resolution per cell | Use case |
|------|-------------------|----------|
| Braille | 2x4 pixels | Primary: high-res wireframes and nodes |
| HalfBlock | 1x2 pixels | Secondary: solid fills with TrueColor |
| ASCII | 1x1 pixel | Fallback: terminals without Unicode |

### Visual Effects (tachyonfx)

- **Pulse**: Node size oscillates proportional to CPU load
- **Dissolve**: Symbol scatter animation on process kill
- **Neon trails**: Edge glow on data transfer
- **Bloom**: Critical nodes (HP < 20%) emit glow halo
- **Matrix rain**: Ambient background effect (togglable)

### Color Palette — Cyberpunk

| State | Name | HEX | Usage |
|-------|------|-----|-------|
| Background | Deep Space | `#050A0E` | Scene depth |
| Healthy | Electric Cyan | `#00F0FF` | Normal operation |
| Load 50-75% | Neon Blue | `#0080FF` | Moderate load |
| Warning 75-90% | Neon Yellow | `#FCEE09` | Threshold breach |
| Critical >90% | Cherry Red | `#FF003C` | Danger state |
| Data/Traffic | Pure White | `#FAFAFA` | Packet transfer flashes |
| XP/Rank | Neon Purple | `#BF00FF` | Gamification elements |

---

## Component 4: aether-mcp

Универсальный агентский хаб. Любой ИИ-провайдер через MCP-стандарт.

### Transports

```
McpTransport trait
    ├── StdioTransport    — `aether --mcp-stdio` (Claude Desktop config)
    ├── SseTransport      — `aether --mcp-sse :3000` (HTTP + Server-Sent Events)
    └── (future) WebSocket, gRPC
```

### MCP Tools

```
get_system_topology()          → JSON graph of processes and connections
inspect_process(pid)           → detailed metrics, open files, sockets
list_anomalies()               → processes with HP < 50%, leaks, zombies
recommend_optimization()       → AI-friendly list of recommendations
execute_action(action, pid)    → kill, restart, nice (requires user approve)
get_network_flows()            → current connections with DPI data
```

### MCP Resources (Dynamic)

```
system://topology              → live graph updates
system://process/{pid}         → metrics stream for specific process
system://alerts                → anomaly subscription
```

### Arbiter Mode (Tab F4 in TUI)

1. AI agent sends `execute_action` → UI shows approval prompt
2. User sees: "Claude wants to execute `kill -9 PID 1234 (nginx-worker)`. [Y]es / [N]o / [I]nspect"
3. `[I]nspect` — camera auto-centers on the node in 3D scene
4. All actions logged to SQLite audit trail

### Multi-Provider Support

MCP is a standard supported by Claude, Gemini, OpenAI. One server, any client. Additional CLI flags for testing without IDE: `aether --mcp-test "show topology"`.

---

## Component 5: aether-gamification

### First Release (Light RPG)

**HP System**:
- Each process starts with HP = 100
- Memory leak (growth >5%/min): -1 HP/sec
- CPU spike >90%: -2 HP/sec
- Zombie state: HP = 0 (instant)

**XP System**:
- +1 XP/min of system uptime
- +50 XP per anomaly prevented via Arbiter
- +10 XP per killed zombie process

**Ranks**:

| Rank | XP Required |
|------|-------------|
| Novice | 0 |
| Operator | 100 |
| Engineer | 500 |
| Architect | 2,000 |
| Aether Lord | 10,000 |

**Achievements**:
- "First Blood" — first process kill
- "Uptime Champion" — 24h without anomalies
- "Network Oracle" — 100 DPI analyses
- "Zombie Hunter" — kill 50 zombie processes
- "AI Whisperer" — approve 100 Arbiter actions

**Storage**: SQLite — tables `sessions`, `achievements`, `rankings`

### Target Vision (Full Gamification — post-release)

- Unlockable terminal skins/themes for XP
- "Spells" — remediation scripts purchasable with XP
- Global leaderboard (optional, via HTTP API)

---

## Component 6: aether-predict

On-device ML inference для предсказания аномалий до их возникновения.

```
Prediction Pipeline:

  WorldState (every 5s)
       │
       ▼
  FeatureExtractor
       │  Extracts per-process feature vector:
       │  [cpu_pct, mem_bytes, mem_delta, fd_count, thread_count,
       │   net_bytes_in, net_bytes_out, syscall_rate, io_wait_pct]
       │
       ▼
  SlidingWindow (60 samples = 5 min history)
       │
       ▼
  ONNX Inference (tract-onnx, pure Rust)
       │
       ├── anomaly_detector.onnx   Autoencoder: reconstruction_error > threshold = anomaly
       │
       └── cpu_forecast.onnx       LSTM: predicts CPU load 60 seconds ahead
       │
       ▼
  PredictedAnomaly {
      pid, process_name,
      anomaly_type: OomRisk | CpuSpike | MemoryLeak | Deadlock,
      confidence: f32,        // 0.0-1.0
      eta_seconds: u32,       // predicted time until event
      recommended_action: String
  }
```

**Inference interval**: Every 5 seconds in dedicated tokio task.
**Models**: Pre-trained in PyTorch, exported to ONNX, shipped as assets.
**Feature-gated**: `#[cfg(feature = "predict")]`

---

## Component 7: aether-script

Custom DSL с JIT-компиляцией через Cranelift для реактивных правил мониторинга.

### Language Syntax

```
rule memory_leak {
    when process.mem_growth > 5% for 60s
    then alert "memory leak: {process.name}" severity warning
}

rule cpu_thrashing {
    when process.cpu > 90% for 30s
      and process.parent == "docker"
    then alert "container thrashing" severity critical
    then action kill after 120s unless recovered
}

rule zombie_reaper {
    when process.state == zombie for 10s
    then action kill
    then log "reaped zombie: {process.name}"
}
```

### Compilation Pipeline

```
.aether file
     │
     ▼
  Lexer (logos crate) → Token stream
     │
     ▼
  Parser (hand-written recursive descent) → AST
     │
     ▼
  Type Checker → Typed AST
     │  Types: Process, System, Duration, Percentage,
     │         String, Bool, Numeric
     │
     ▼
  Cranelift IR Generator → CLIF (Cranelift IR)
     │
     ▼
  Cranelift Codegen → Native x86_64 / aarch64
     │
     ▼
  CompiledRuleSet {
      rules: Vec<CompiledRule>,
      evaluate: fn(&WorldState) -> Vec<RuleAction>
  }
```

### Rule Actions

```rust
enum RuleAction {
    Alert { message: String, severity: Severity },
    Kill { pid: u32 },
    Log { message: String },
    Metric { name: String, value: f64 },
    Action { action: AgentAction, delay: Option<Duration>, condition: Option<String> },
}
```

### Hot-Reload

- File watcher via `notify` crate monitors `.aether` files
- On change: recompile → create new CompiledRuleSet → atomic swap via `Arc<ArcSwap>`
- SIGHUP also triggers reload
- Zero downtime: old rules continue executing during compilation

---

## Data Flow

```
[Linux Kernel]                    [OS / sysinfo]
     │                                  │
     ▼ (100K evt/sec)                   ▼ (10Hz polling)
[aether-ebpf] ──────+      [aether-ingestion]
  ring buffer        │              │
                     +------+-------+
                            │
                     mpsc<SystemEvent>
                            │
                            ▼
                   [aether-core: Graph]
                            │
                   broadcast<WorldState>
                            │
     +----------+-----------+----------+-----------+
     ▼          ▼           ▼          ▼           ▼
  [render]   [mcp]    [gamification] [predict]  [script]
     │         │           │          │           │
  Terminal  AI Agent    SQLite    Anomaly     JIT Rule
  (TUI+3D) (JSON-RPC)  (HP/XP)  Alerts      Actions
     ▲         │                    │           │
     │         ▼                    ▼           ▼
     +---- ArbiterQueue ◄──────────+───────────+
           (approve/deny)
```

### Channels

| From → To | Channel Type | Payload |
|-----------|-------------|---------|
| ebpf → ingestion | `mpsc<RawKernelEvent>` | Raw eBPF events from ring buffer |
| ingestion → core | `mpsc<SystemEvent>` | Process events, metrics, network |
| core → render | `broadcast<WorldState>` | Graph snapshot each frame |
| core → mcp | `Arc<RwLock<WorldGraph>>` | Read-only access |
| mcp → core | `mpsc<AgentAction>` | Action requests from AI |
| core → gamification | `mpsc<GameEvent>` | HP changes, XP earnings |
| core → predict | `broadcast<WorldState>` | State for ML inference |
| predict → core/render | `mpsc<PredictedAnomaly>` | Predicted future anomalies |
| core → script | `broadcast<WorldState>` | State for rule evaluation |
| script → arbiter/core | `mpsc<RuleAction>` | JIT-compiled rule results |

---

## File Structure

```
aether-terminal/
├── Cargo.toml                       (workspace definition)
├── crates/
│   ├── aether-terminal/             (bin)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs              (entry, CLI args via clap, orchestrator)
│   ├── aether-core/                 (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── graph.rs             (petgraph WorldGraph)
│   │       ├── models.rs            (ProcessNode, NetworkEdge, Protocol, etc.)
│   │       ├── traits.rs            (SystemProbe, Renderer, McpTransport, Storage)
│   │       └── events.rs            (SystemEvent, GameEvent, AgentAction)
│   ├── aether-ebpf/                 (lib, feature-gated)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── loader.rs            (BPF program loading via aya)
│   │       ├── ring_buffer.rs       (zero-copy ring buffer reader)
│   │       ├── probes.rs            (probe attachment: tracepoints, kprobes)
│   │       └── events.rs            (RawKernelEvent types, C struct mapping)
│   ├── aether-ingestion/            (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sysinfo_probe.rs     (SysinfoProbe: crossplatform fallback)
│   │       ├── ebpf_bridge.rs       (converts eBPF events → SystemEvent)
│   │       ├── pipeline.rs          (dual-tick async pipeline)
│   │       └── dpi.rs               (etherparse packet analysis)
│   ├── aether-render/               (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tui/
│   │       │   ├── mod.rs
│   │       │   ├── app.rs           (main TUI app, tab routing)
│   │       │   ├── overview.rs      (F1: sparklines, process table)
│   │       │   ├── world3d.rs       (F2: 3D viewport widget)
│   │       │   ├── network.rs       (F3: connection map)
│   │       │   └── arbiter.rs       (F4: AI action approval UI)
│   │       ├── engine/
│   │       │   ├── mod.rs
│   │       │   ├── camera.rs        (orbital camera, WASD controls)
│   │       │   ├── projection.rs    (perspective matrix, view transform)
│   │       │   ├── rasterizer.rs    (z-buffer, triangle/line fill)
│   │       │   ├── shading.rs       (Phong: ambient, diffuse, normals)
│   │       │   └── layout.rs        (force-directed 3D graph layout)
│   │       ├── braille.rs           (Braille symbol encoding/mapping)
│   │       ├── effects.rs           (tachyonfx: bloom, dissolve, trails)
│   │       └── palette.rs           (color constants, theme system)
│   ├── aether-predict/              (lib, feature-gated)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── features.rs          (feature extraction from WorldState)
│   │       ├── window.rs            (sliding window buffer, 60 samples)
│   │       ├── inference.rs         (tract-onnx model loading + inference)
│   │       ├── models.rs            (PredictedAnomaly, AnomalyType types)
│   │       └── engine.rs            (PredictEngine: tokio task, 5s interval)
│   ├── aether-script/               (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lexer.rs             (logos-based tokenizer)
│   │       ├── parser.rs            (recursive descent → AST)
│   │       ├── ast.rs               (Rule, Condition, Action nodes)
│   │       ├── types.rs             (type checker, type inference)
│   │       ├── codegen.rs           (AST → Cranelift IR → native code)
│   │       ├── runtime.rs           (CompiledRuleSet, evaluate())
│   │       └── hot_reload.rs        (file watcher, atomic swap)
│   ├── aether-mcp/                  (lib)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs            (JSON-RPC router, method dispatch)
│   │       ├── tools.rs             (MCP tool implementations)
│   │       ├── resources.rs         (MCP dynamic resource handlers)
│   │       ├── transport/
│   │       │   ├── mod.rs
│   │       │   ├── stdio.rs         (stdin/stdout JSON-RPC)
│   │       │   └── sse.rs           (HTTP + Server-Sent Events)
│   │       └── arbiter.rs           (action approval queue, audit log)
│   └── aether-gamification/         (lib)
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── hp.rs                (health point calculation rules)
│           ├── xp.rs                (experience, levels, rank thresholds)
│           ├── achievements.rs      (achievement definitions and tracking)
│           └── storage.rs           (SQLite persistence via rusqlite)
├── bpf/                             (eBPF C programs, compiled to BPF bytecode)
│   ├── process_monitor.bpf.c
│   ├── net_monitor.bpf.c
│   └── syscall_monitor.bpf.c
├── models/                          (pre-trained ONNX models)
│   ├── anomaly_detector.onnx
│   └── cpu_forecast.onnx
├── rules/                           (Aether DSL rule files)
│   ├── default.aether
│   └── docker.aether
├── assets/
│   └── themes/
│       ├── cyberpunk.toml           (default theme)
│       └── matrix.toml              (alt theme)
├── docs/
│   └── plans/
│       └── 2026-03-08-aether-terminal-design.md  (this file)
├── LICENSE
└── README.md
```

---

## Technology Stack

### Core

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1.x | Async runtime |
| `serde` / `serde_json` | 1.x | Serialization |
| `clap` | 4.x | CLI argument parsing |
| `tracing` | 0.1 | Structured logging |

### System Layer

| Crate | Purpose |
|-------|---------|
| `sysinfo` | Cross-platform system metrics (fallback) |
| `aya` | Pure-Rust eBPF loader and ring buffer |
| `petgraph` | Process dependency graph |
| `etherparse` | Deep packet inspection |

### ML & Compiler

| Crate | Purpose |
|-------|---------|
| `tract-onnx` | Pure-Rust ONNX runtime for inference |
| `cranelift-codegen` | Native code generation backend |
| `cranelift-frontend` | IR builder for Cranelift |
| `cranelift-module` | JIT module linking |
| `cranelift-jit` | JIT compilation driver |
| `logos` | Zero-copy lexer generator |
| `notify` | File system watcher for hot-reload |

### Visualization

| Crate | Purpose |
|-------|---------|
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend |
| `glam` | 3D math (Vec3, Mat4, projections) |
| `tachyonfx` | Visual effects |

### MCP & AI

| Crate | Purpose |
|-------|---------|
| `rmcp` | Rust MCP SDK |
| `tower` | Service layer for transports |
| `axum` | HTTP server for SSE transport |

### Storage

| Crate | Purpose |
|-------|---------|
| `rusqlite` | SQLite persistence |

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| eBPF requires kernel 5.8+ and root | Medium | sysinfo fallback, feature-gated, graceful degradation |
| Software 3D rasterizer performance | High | Incremental render, dirty flags, skip unchanged frames |
| Cranelift API instability | Medium | Pin version, wrap in thin abstraction layer |
| DSL security (rule injection) | High | Sandboxed actions only, no arbitrary syscalls, validate all inputs |
| tract-onnx model compatibility | Medium | Test with target ONNX opset, fallback to threshold-based detection |
| `rmcp` crate immature | Medium | Implement raw JSON-RPC if needed, minimal dependency |
| Braille rendering inconsistent across terminals | Low | HalfBlock and ASCII fallback modes |
| Terminal size too small for 3D | Low | Minimum size check, graceful degradation to 2D |
| eBPF ring buffer overflow under high load | Medium | Per-CPU buffers (256KB), backpressure signaling, sample rate limiting |
| JIT memory leaks on hot-reload | Medium | Track compiled modules, deallocate on swap, test with ASAN |

---

## CLI Interface

```
aether [OPTIONS]

Options:
  --mcp-stdio          Start in MCP stdio transport mode
  --mcp-sse <PORT>     Start MCP SSE server on given port (default: 3000)
  --mcp-test <PROMPT>  Send a test prompt to MCP tools and print result
  --theme <NAME>       Color theme (default: cyberpunk)
  --no-3d              Disable 3D rendering, use 2D fallback
  --no-game            Disable gamification layer
  --log-level <LEVEL>  Logging level (default: info)
  -h, --help           Print help
  -V, --version        Print version
```

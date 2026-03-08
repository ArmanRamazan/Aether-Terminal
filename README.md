# Aether-Terminal

**Cinematic 3D system monitor with eBPF telemetry, predictive AI, and a JIT-compiled rule engine**

<!-- TODO: Replace with asciinema GIF after MS3 -->
<!-- ![demo](assets/demo.gif) -->

Aether-Terminal transforms system observability into a spatial experience. Processes are nodes in a 3D force-directed graph rendered in your terminal using Braille characters. A native eBPF engine captures kernel-level events at 100K+ events/sec. An on-device ML model predicts anomalies before they happen. A custom DSL with JIT compilation lets you write reactive rules that compile to machine code. AI agents connect via Model Context Protocol to inspect, analyze, and manage your infrastructure with human-in-the-loop approval.

## Features

- **3D Visualization** -- Software rasterizer projecting process graphs into Braille subpixels (2x4 per cell). Orbital camera, Phong shading, z-buffer depth testing
- **eBPF Telemetry** -- Kernel-level event capture: syscalls, TCP connect/close, fork/exec/exit. Ring buffer with zero-copy reads. 100K+ events/sec throughput
- **Predictive AI** -- On-device ONNX inference engine. Time-series anomaly detection predicts OOM, CPU spikes, and resource exhaustion before they occur
- **JIT Rule Engine** -- Custom DSL compiled to native code via Cranelift. Write reactive rules like `when process.cpu > 90% for 30s -> alert "thrashing"`. Hot-reload without restart
- **Real-time Monitoring** -- CPU, memory, network per process. Dual-tick pipeline: 10Hz metrics, 1Hz topology
- **MCP Integration** -- Built-in MCP server (stdio + SSE). Connect Claude, Gemini, or any MCP-compatible AI agent
- **Arbiter Mode** -- AI proposes actions, you approve/deny from the terminal. Full audit trail
- **RPG Gamification** -- Processes have HP (drops on memory leaks, CPU spikes). Earn XP for uptime. Rank up from Novice to Aether Lord
- **Cyberpunk Aesthetic** -- Neon palette, pulsating nodes, dissolve animations, data flow trails

## Architecture

```
aether-terminal (bin)        -- CLI entry point, orchestration
aether-core (lib)            -- Types, traits, WorldGraph (petgraph)
aether-ebpf (lib)            -- eBPF programs, ring buffer, kernel telemetry
aether-ingestion (lib)       -- System metrics (sysinfo fallback, eBPF bridge)
aether-predict (lib)         -- ONNX inference, time-series models, anomaly prediction
aether-script (lib)          -- DSL lexer/parser, AST, Cranelift JIT compiler
aether-render (lib)          -- TUI (ratatui) + 3D engine (glam, Braille)
aether-mcp (lib)             -- MCP server (stdio + SSE/HTTP)
aether-gamification (lib)    -- HP, XP, achievements, SQLite persistence
```

Hexagonal architecture: all crates depend on `aether-core`, never on each other. The binary crate wires them together via channels and shared state.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2021, MSRV 1.75+) |
| Async | tokio (full features) |
| TUI | ratatui + crossterm |
| 3D Math | glam (Vec3, Mat4, projections) |
| Graph | petgraph (StableGraph) |
| eBPF | aya (pure-Rust eBPF loader, no libbpf-sys) |
| Metrics | sysinfo (crossplatform fallback) |
| ML Inference | tract-onnx (pure-Rust ONNX runtime) |
| JIT Compiler | cranelift-codegen + cranelift-frontend |
| DSL Parser | logos (lexer) + custom recursive-descent parser |
| MCP | rmcp + axum (SSE transport) |
| Storage | rusqlite (bundled SQLite) |

## Quick Start

```bash
# Build (all features)
cargo build --workspace

# Run (with sysinfo fallback, no root required)
cargo run -p aether-terminal

# Run with eBPF (Linux only, requires root/CAP_BPF)
sudo cargo run -p aether-terminal -- --ebpf

# With MCP SSE server (for AI agents)
cargo run -p aether-terminal -- --mcp-sse 3000

# MCP stdio mode (for Claude Desktop)
cargo run -p aether-terminal -- --mcp-stdio

# Load custom rules
cargo run -p aether-terminal -- --rules rules/default.aether
```

## Usage

```
aether [OPTIONS]

Options:
  --ebpf                Enable eBPF telemetry (Linux, requires CAP_BPF)
  --rules <PATH>        Load .aether rule files (JIT-compiled DSL)
  --predict             Enable predictive anomaly detection
  --mcp-stdio           Start in MCP stdio transport mode
  --mcp-sse <PORT>      Start MCP SSE server (default: 3000)
  --theme <NAME>        Color theme: cyberpunk, matrix (default: cyberpunk)
  --no-3d               Disable 3D rendering, use 2D tables
  --no-game             Disable gamification layer
  --log-level <LEVEL>   Logging level (default: info)
  -h, --help            Print help
  -V, --version         Print version
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `F1`-`F5` | Switch tabs (Overview, 3D World, Network, Arbiter, Rules) |
| `h/j/k/l` | Navigate |
| `WASD` | Rotate 3D camera |
| `+/-` | Zoom |
| `Space` | Toggle auto-rotate |
| `/` | Search filter |
| `:` | Command mode |
| `?` | Help overlay |
| `q` | Quit |

### MCP Tools (for AI agents)

| Tool | Description |
|------|-------------|
| `get_system_topology` | Process graph as JSON |
| `inspect_process` | Detailed metrics for a PID |
| `list_anomalies` | Processes with low HP, zombies, CPU spikes |
| `predict_anomalies` | ML-predicted future anomalies (OOM, spikes) |
| `execute_action` | Kill/restart with human approval |
| `get_network_flows` | Active connections with DPI data |
| `eval_rule` | Evaluate an Aether DSL expression on current state |
| `list_rules` | Show active JIT-compiled rules |

### Aether DSL Example

```
# rules/default.aether

rule memory_leak {
  when process.mem_growth > 5% for 60s
  then alert "memory leak detected" severity warning
}

rule cpu_thrashing {
  when process.cpu > 90% for 30s and process.parent == "docker"
  then alert "container thrashing" severity critical
  then action kill after 120s unless recovered
}

rule zombie_reaper {
  when process.state == zombie for 10s
  then action kill
  then log "reaped zombie: {process.name}"
}
```

## Color Palette

| State | Color | Hex |
|-------|-------|-----|
| Background | Deep Space | `#050A0E` |
| Healthy | Electric Cyan | `#00F0FF` |
| Warning | Neon Yellow | `#FCEE09` |
| Critical | Cherry Red | `#FF003C` |
| Predicted | Neon Orange | `#FF6600` |
| XP/Rank | Neon Purple | `#BF00FF` |

## Project Structure

```
aether-terminal/
+-- Cargo.toml                  (workspace)
+-- CLAUDE.md                   (AI agent context)
+-- crates/
|   +-- aether-terminal/        (bin: CLI + orchestration)
|   +-- aether-core/            (lib: types, graph, events, traits)
|   +-- aether-ebpf/            (lib: eBPF loader, ring buffer, probes)
|   +-- aether-ingestion/       (lib: sysinfo + eBPF bridge, pipeline)
|   +-- aether-predict/         (lib: ONNX runtime, feature extraction, models)
|   +-- aether-script/          (lib: DSL lexer, parser, AST, Cranelift JIT)
|   +-- aether-render/          (lib: TUI + 3D engine)
|   +-- aether-mcp/             (lib: MCP server + transports)
|   +-- aether-gamification/    (lib: HP, XP, SQLite)
+-- bpf/                        (eBPF C programs compiled to BPF bytecode)
|   +-- process_monitor.bpf.c
|   +-- net_monitor.bpf.c
|   +-- syscall_monitor.bpf.c
+-- models/                     (pre-trained ONNX models)
|   +-- anomaly_detector.onnx
|   +-- cpu_forecast.onnx
+-- rules/                      (Aether DSL rule files)
|   +-- default.aether
|   +-- docker.aether
+-- docs/
|   +-- architecture.md
|   +-- decisions/              (ADRs)
|   +-- plans/                  (design + implementation plans)
+-- tools/
|   +-- orchestrator/           (automated sprint pipeline)
+-- assets/
    +-- themes/                 (TOML color themes)
```

## Development

```bash
# Check all crates
cargo check --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --check

# Test eBPF programs (Linux, requires root)
sudo cargo test -p aether-ebpf

# Test JIT compiler
cargo test -p aether-script

# Test ML inference
cargo test -p aether-predict
```

### Automated Sprints

The project includes an AI-powered sprint orchestrator for automated development:

```bash
cd tools/orchestrator
python main.py tasks/<sprint>.yaml --dry-run   # preview
python main.py tasks/<sprint>.yaml             # execute
python main.py --status                        # check progress
```

## Roadmap

- [x] Product design and architecture
- [x] CLAUDE.md system for AI agents
- [x] Orchestrator v3 (lean sprint pipeline)
- [ ] MS1: Core types + data ingestion
- [ ] MS2: TUI shell with tabs and sparklines
- [ ] MS3: 3D software rasterizer
- [ ] MS4: MCP server + Arbiter Mode
- [ ] MS5: Gamification, animations, themes
- [ ] MS6: eBPF telemetry engine
- [ ] MS7: JIT-compiled rule DSL
- [ ] MS8: Predictive AI engine
- [ ] Global leaderboard

## Technical Complexity

This project combines several deep systems-programming challenges:

| Subsystem | Complexity Domain |
|-----------|------------------|
| 3D Rasterizer | Software rendering, SIMD, z-buffer, Phong shading in Braille subpixels |
| eBPF Engine | Kernel interaction, BPF bytecode verification, ring buffers, zero-copy I/O |
| JIT Compiler | Lexer, parser, type checker, Cranelift IR generation, hot-reload |
| ML Inference | ONNX runtime, SIMD-optimized tensor ops, streaming feature extraction |
| Async Orchestration | tokio tasks, broadcast/mpsc channels, graceful shutdown |
| MCP Protocol | JSON-RPC 2.0, dual transport, human-in-the-loop approval |

## License

MIT License. See [LICENSE](LICENSE).

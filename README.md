# Aether-Terminal

**Cinematic 3D system monitor with eBPF telemetry, predictive AI, and a JIT-compiled rule engine**

<!-- TODO: Replace with asciinema GIF after MS3 -->
<!-- ![demo](assets/demo.gif) -->

Aether-Terminal transforms system observability into a spatial experience. Processes are nodes in a 3D force-directed graph rendered in your terminal using Braille characters. A native eBPF engine captures kernel-level events at 100K+ events/sec. An on-device ML model predicts anomalies before they happen. A custom DSL with JIT compilation lets you write reactive rules that compile to machine code. AI agents connect via Model Context Protocol to inspect, analyze, and manage your infrastructure with human-in-the-loop approval.

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| Real-time Monitoring | Implemented | CPU, memory, network per process. Dual-tick pipeline: 10Hz metrics, 1Hz topology |
| 3D Visualization | Implemented | Software rasterizer projecting process graphs into Braille subpixels (2x4 per cell). Orbital camera, Phong shading, z-buffer |
| TUI Dashboard | Implemented | 4-tab interface: Overview, 3D World, Network, Arbiter. Sparklines, process tables, help overlay |
| MCP Integration | Implemented | Built-in MCP server (stdio + SSE). Connect Claude, Gemini, or any MCP-compatible AI agent |
| Arbiter Mode | Implemented | AI proposes actions, you approve/deny from the terminal. Full audit trail |
| RPG Gamification | Implemented | Processes have HP (drops on memory leaks, CPU spikes). Earn XP for uptime. Rank up from Novice to Aether Lord |
| Theme System | Implemented | TOML-based color themes. Ships with `cyberpunk` and `matrix` presets |
| Startup Animation | Implemented | Cinematic boot sequence with phased reveals |
| Death Animation | Implemented | Dissolve effect when processes terminate |
| eBPF Telemetry | Planned (MS6) | Kernel-level event capture: syscalls, TCP connect/close, fork/exec/exit. Ring buffer with zero-copy reads |
| JIT Rule Engine | Planned (MS7) | Custom DSL compiled to native code via Cranelift. Hot-reload without restart |
| Predictive AI | Planned (MS8) | On-device ONNX inference. Time-series anomaly detection predicts OOM, CPU spikes, and resource exhaustion |

## Architecture

```
                         +-------------------+
                         | aether-terminal   |  CLI entry point
                         | (bin)             |  orchestrates all crates
                         +--------+----------+
                                  |
                    +-------------+-------------+
                    |                           |
              +-----v------+             +------v-----+
              | aether-core |<-----------+ all crates |
              | (lib)       |  depend on | depend on  |
              +-----+------+  core only | core only  |
                    |                    +------------+
        +-----------+-----------+
        |           |           |
  +-----v---+ +----v----+ +----v--------+
  | ingestion| | render  | | mcp         |
  | sysinfo  | | TUI+3D  | | stdio + SSE |
  +----------+ +---------+ +-------------+
        |           |           |
  +-----v---+ +----v----+ +----v--------+
  | ebpf    | | gamifi- | | predict     |
  | (Linux) | | cation  | | (ONNX)      |
  +----------+ +---------+ +-------------+
                    |
              +-----v------+
              | script      |
              | DSL + JIT   |
              +-----------+
```

Hexagonal architecture: all crates depend on `aether-core`, never on each other. The binary crate wires them together via tokio channels and `Arc<RwLock<T>>` shared state.

### Crate Responsibilities

| Crate | Role |
|-------|------|
| `aether-terminal` | CLI entry point, wires all crates together |
| `aether-core` | Domain types, traits (ports), WorldGraph, events |
| `aether-ingestion` | System metrics via sysinfo, eBPF bridge, dual-tick pipeline |
| `aether-render` | TUI (ratatui) + 3D software rasterizer (glam, Braille) |
| `aether-mcp` | MCP server (stdio + SSE), Arbiter queue |
| `aether-gamification` | HP, XP, achievements, SQLite persistence |
| `aether-ebpf` | eBPF loader (aya), ring buffer, kernel probes |
| `aether-script` | DSL lexer (logos), recursive descent parser, Cranelift JIT |
| `aether-predict` | ONNX inference (tract), feature extraction, anomaly models |

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
# Build
cargo build --workspace

# Run (sysinfo fallback, no root required)
cargo run -p aether-terminal

# With MCP SSE server (for AI agents)
cargo run -p aether-terminal -- --mcp-sse 3000

# MCP stdio mode (for Claude Desktop)
cargo run -p aether-terminal -- --mcp-stdio
```

## Usage

```
aether [OPTIONS]

Options:
  --log-level <LEVEL>   Logging level: trace, debug, info, warn, error (default: info)
  --mcp-stdio           Start in MCP stdio transport mode (no TUI)
  --mcp-sse [PORT]      Start MCP SSE server alongside TUI (default: 3000)
  --no-3d               Disable 3D rendering, use 2D tables
  --no-game             Disable gamification layer
  --theme <NAME>        Color theme name or path to TOML file (default: cyberpunk)
  --rules <PATH>        Load .aether rule files (JIT-compiled DSL) [planned]
  --predict             Enable predictive anomaly detection [planned]
  --ebpf                Enable eBPF telemetry (Linux, requires CAP_BPF) [planned]
  -h, --help            Print help
  -V, --version         Print version
```

### Examples

```bash
# Disable 3D rendering for low-resource environments
cargo run -p aether-terminal -- --no-3d

# Use matrix theme without gamification
cargo run -p aether-terminal -- --theme matrix --no-game

# Full stack: SSE server + custom theme + debug logging
cargo run -p aether-terminal -- --mcp-sse 8080 --theme cyberpunk --log-level debug

# Future: eBPF + predictive AI + custom rules
sudo cargo run -p aether-terminal -- --ebpf --predict --rules rules/default.aether
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
+-- models/                     (pre-trained ONNX models)
+-- rules/                      (Aether DSL rule files)
+-- assets/
|   +-- themes/                 (TOML color themes)
+-- docs/
|   +-- architecture.md
|   +-- decisions/              (ADRs)
|   +-- plans/                  (design + implementation plans)
+-- tools/
    +-- orchestrator/           (automated sprint pipeline)
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
```

## Roadmap

- [x] Product design and architecture
- [x] CLAUDE.md system for AI agents
- [x] Orchestrator v3 (lean sprint pipeline)
- [x] MS1: Core types + data ingestion
- [x] MS2: TUI shell with tabs and sparklines
- [x] MS3: 3D software rasterizer
- [x] MS4: MCP server + Arbiter Mode
- [x] MS5: Gamification, animations, themes
- [ ] MS6: eBPF telemetry engine
- [ ] MS7: JIT-compiled rule DSL
- [ ] MS8: Predictive AI engine
- [ ] Global leaderboard

## Technical Complexity

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

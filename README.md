# Aether-Terminal

**Cinematic 3D system monitor with AI agent integration via MCP**

<!-- TODO: Replace with asciinema GIF after MS3 -->
<!-- ![demo](assets/demo.gif) -->

Aether-Terminal transforms system observability into a spatial experience. Processes are nodes in a 3D force-directed graph rendered in your terminal using Braille characters. AI agents connect via Model Context Protocol to inspect, analyze, and manage your infrastructure with human-in-the-loop approval.

## Features

- **3D Visualization** — Software rasterizer projecting process graphs into Braille subpixels (2x4 per cell). Orbital camera, Phong shading, z-buffer depth testing
- **Real-time Monitoring** — CPU, memory, network per process. Dual-tick pipeline: 10Hz metrics, 1Hz topology
- **MCP Integration** — Built-in MCP server (stdio + SSE). Connect Claude, Gemini, or any MCP-compatible AI agent
- **Arbiter Mode** — AI proposes actions, you approve/deny from the terminal. Full audit trail
- **RPG Gamification** — Processes have HP (drops on memory leaks, CPU spikes). Earn XP for uptime. Rank up from Novice to Aether Lord
- **Cyberpunk Aesthetic** — Neon palette, pulsating nodes, dissolve animations, data flow trails

## Architecture

```
aether-terminal (bin)     — CLI entry point, orchestration
aether-core (lib)         — Types, traits, WorldGraph (petgraph)
aether-ingestion (lib)    — System metrics (sysinfo, future eBPF)
aether-render (lib)       — TUI (ratatui) + 3D engine (glam, Braille)
aether-mcp (lib)          — MCP server (stdio + SSE/HTTP)
aether-gamification (lib) — HP, XP, achievements, SQLite persistence
```

Hexagonal architecture: all crates depend on `aether-core`, never on each other.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2021) |
| Async | tokio |
| TUI | ratatui + crossterm |
| 3D Math | glam (Vec3, Mat4, projections) |
| Graph | petgraph (StableGraph) |
| Metrics | sysinfo (crossplatform) |
| MCP | rmcp + axum (SSE transport) |
| Storage | rusqlite (bundled SQLite) |

## Quick Start

```bash
# Build
cargo build --workspace

# Run
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
| `F1`-`F4` | Switch tabs (Overview, 3D World, Network, Arbiter) |
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
| `execute_action` | Kill/restart with human approval |
| `get_network_flows` | Active connections with DPI data |

## Color Palette

| State | Color | Hex |
|-------|-------|-----|
| Background | Deep Space | `#050A0E` |
| Healthy | Electric Cyan | `#00F0FF` |
| Warning | Neon Yellow | `#FCEE09` |
| Critical | Cherry Red | `#FF003C` |
| XP/Rank | Neon Purple | `#BF00FF` |

## Project Structure

```
aether-terminal/
├── Cargo.toml                  (workspace)
├── CLAUDE.md                   (AI agent context)
├── crates/
│   ├── aether-terminal/        (bin: CLI + orchestration)
│   ├── aether-core/            (lib: types, graph, events, traits)
│   ├── aether-ingestion/       (lib: sysinfo, pipeline)
│   ├── aether-render/          (lib: TUI + 3D engine)
│   ├── aether-mcp/             (lib: MCP server + transports)
│   └── aether-gamification/    (lib: HP, XP, SQLite)
├── docs/
│   ├── architecture.md
│   ├── decisions/              (ADRs)
│   └── plans/                  (design + implementation plans)
├── tools/
│   └── orchestrator/           (automated sprint pipeline)
└── assets/
    └── themes/                 (TOML color themes)
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
- [ ] eBPF probe (Linux, feature-gated)
- [ ] Global leaderboard

## License

MIT License. See [LICENSE](LICENSE).

# aether-terminal

## Purpose
Binary crate. Entry point. Wires all library crates together: creates shared state, spawns async tasks, runs TUI event loop.

## Modules
- `main.rs` — CLI parsing (clap), initialization, orchestration, cleanup

## Initialization Sequence
1. Parse CLI args (clap)
2. Initialize tracing subscriber
3. Create shared state: `Arc<RwLock<WorldGraph>>`
4. Create channels: system_events (mpsc), world_state (broadcast), game_events (mpsc), agent_actions (mpsc)
5. Create SysinfoProbe + IngestionPipeline
6. Create GamificationEngine (HP + XP + Achievements + SqliteStorage)
7. If --mcp-stdio: run MCP stdio transport (no TUI)
8. If --mcp-sse: spawn MCP SSE server task
9. Spawn core updater task (receives SystemEvent → updates WorldGraph → broadcasts)
10. Spawn gamification task
11. Initialize crossterm terminal
12. Run App::run() (TUI event loop)
13. On exit: cancel all tasks, restore terminal, save session

## CLI Flags
```
aether [OPTIONS]
  --mcp-stdio           MCP stdio mode (no TUI)
  --mcp-sse <PORT>      MCP SSE server (default: 3000)
  --mcp-test <PROMPT>   Test MCP tool and exit
  --theme <NAME>        Color theme (default: cyberpunk)
  --no-3d               Disable 3D rendering
  --no-game             Disable gamification
  --log-level <LEVEL>   Logging level (default: info)
```

## Rules
- This is the ONLY crate that knows about all other crates
- Error handling: anyhow for top-level, thiserror in library crates
- Terminal restore MUST happen even on panic (use scopeguard or manual drop)
- Log to file, not terminal (tracing file appender): `~/.aether-terminal/aether.log`

## Testing
```bash
cargo run -p aether-terminal -- --help
cargo run -p aether-terminal
```
Minimal unit tests — this crate is mostly integration wiring.

## Key Dependencies
- All aether-* crates
- clap (derive)
- tokio (full)
- tracing, tracing-subscriber
- anyhow

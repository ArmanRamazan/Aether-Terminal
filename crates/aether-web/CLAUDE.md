# aether-web

## Purpose
Web UI backend. Serves React SPA static files, REST API for system data, and WebSocket for realtime updates. Runs alongside TUI via `--web [PORT]` flag.

## Modules
- `server.rs` — axum Router setup, HTTP server with graceful shutdown
- `state.rs` — SharedState (Arc wrappers for WorldGraph + ArbiterQueue)
- `error.rs` — WebError enum (thiserror)

## Rules
- Depends ONLY on aether-core (hexagonal architecture)
- SharedState uses Arc<RwLock<WorldGraph>> and Arc<Mutex<ArbiterQueue>> — same pattern as aether-mcp
- WebSocket broadcasts system state diffs, REST serves snapshots
- CORS enabled for local dev (localhost origins)
- Static files served via rust-embed or tower-http fs
- All routes return JSON (Content-Type: application/json) except static files
- No .unwrap() in production code

## Testing
```bash
cargo test -p aether-web
```
Test router construction, shared state cloning.

## Key Dependencies
- aether-core (path dependency)
- axum (HTTP + WebSocket)
- tower-http (CORS, static files)
- rust-embed (embed SPA assets)
- serde_json (API serialization)

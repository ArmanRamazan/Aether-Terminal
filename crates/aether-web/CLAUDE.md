# aether-web

## Purpose
Web UI backend + embedded React SPA. Serves REST API, WebSocket realtime updates, and static files. Runs alongside TUI via `--web [PORT]` flag.

## Modules
- `server.rs` — axum Router setup, HTTP server with graceful shutdown
- `api.rs` — REST endpoints: processes, stats, diagnostics, metrics, arbiter
- `ws.rs` — WebSocket handler, pushes WorldUpdate every 500ms
- `state.rs` — SharedState (Arc wrappers for WorldGraph, ArbiterQueue, diagnostics, system metrics)
- `embedded.rs` — rust-embed for serving React SPA static files
- `error.rs` — WebError enum with IntoResponse

## Strict Rules
- Depends ONLY on aether-core (hexagonal architecture)
- **ZERO `.unwrap()` or `.expect()` in ANY handler** — poisoned lock MUST return HTTP 500, not panic
- All handlers return `Result<impl IntoResponse, WebError>` — never raw StatusCode
- Use SharedState helper methods (read_world, read_arbiter) that return WebError
- SharedState uses THE SAME ArbiterQueue as TUI and MCP — never create a separate one
- `format!("{:?}")` FORBIDDEN in API responses — use Display impl or Serialize
- Internal modules (`api`, `ws`, `embedded`) are `pub(crate)` — not part of public API
- Only `server`, `state`, `error` are `pub`
- WebSocket must handle disconnects gracefully — never panic on send failure
- CORS enabled for localhost origins only
- All REST routes return JSON (Content-Type: application/json) except static files

## Frontend
- React 18 + TypeScript + Vite
- Built separately: `cd crates/aether-web/frontend && npm run build`
- Dist files embedded via rust-embed at compile time
- Pages: Overview, 3D Graph, Network, Arbiter, Diagnostics, Metrics
- Stores: zustand (worldStore, metricsStore)

## Testing
```bash
cargo test -p aether-web
```
- Test router construction
- Test SharedState cloning and access
- Test API response formats (no zeros, no Debug format)

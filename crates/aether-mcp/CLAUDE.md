# aether-mcp

## Purpose
MCP (Model Context Protocol) server. Exposes system data as tools for AI agents. Supports multiple transports (stdio for Claude Desktop, SSE/HTTP for realtime).

## Modules
- `server.rs` — JSON-RPC 2.0 router and method dispatch
- `tools.rs` — MCP tool implementations (get_system_topology, inspect_process, etc.)
- `resources.rs` — (future) MCP dynamic resource handlers
- `arbiter.rs` — ArbiterQueue for human-in-the-loop action approval
- `transport/stdio.rs` — stdin/stdout JSON-RPC transport
- `transport/sse.rs` — HTTP + Server-Sent Events via axum

## MCP Tools
| Tool | Input | Output |
|------|-------|--------|
| get_system_topology | none | JSON graph: processes, connections, summary |
| inspect_process | { pid } | ProcessNode details + connections + recommendations |
| list_anomalies | none | Processes with HP<50, zombies, CPU>90% |
| execute_action | { action, pid } | Pushes to ArbiterQueue, returns pending status |
| get_network_flows | none | Active connections with protocol and throughput |

## Rules
- MCP server reads WorldGraph via Arc<RwLock<WorldGraph>> — NEVER mutates it
- Actions go through ArbiterQueue — NEVER execute directly
- Stdio transport: when active, TUI must be disabled (stdin conflict)
- SSE transport: runs on separate tokio task, does not block TUI
- JSON-RPC errors must follow standard error codes (-32600, -32601, -32602, -32603)
- Tool responses: always include `status` field for AI to parse
- Limit topology response to top 50 processes (prevent token overflow in AI context)

## Testing
```bash
cargo test -p aether-mcp
```
Test JSON-RPC parsing, tool dispatch, error handling. Use mock WorldGraph.

## Key Dependencies
- aether-core (path dependency)
- serde_json
- tokio
- axum (for SSE transport)
- rmcp (Rust MCP SDK, if mature enough — else raw JSON-RPC)

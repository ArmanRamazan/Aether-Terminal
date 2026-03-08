# ADR-003: Dual MCP transport (stdio + SSE)

**Status:** Accepted
**Date:** 2026-03-08

## Context
MCP protocol supports multiple transports. Claude Desktop expects stdio. Realtime Arbiter Mode needs concurrent TUI and MCP.

## Decision
Support both stdio and SSE transports, selectable via CLI flags.

## Rationale
- Stdio: required for Claude Desktop integration (standard MCP pattern)
- SSE/HTTP: allows TUI and MCP to run simultaneously (no stdin conflict)
- SSE enables Arbiter Mode where user sees AI actions in realtime
- Multi-provider: MCP standard works with Claude, Gemini, OpenAI

## Consequences
- Stdio mode: TUI disabled, MCP-only operation
- SSE mode: axum HTTP server on separate port (default 3000)
- Must handle port conflicts gracefully
- Two code paths for transport, but shared tool implementations

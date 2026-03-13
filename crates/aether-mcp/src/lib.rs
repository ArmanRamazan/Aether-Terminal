//! MCP (Model Context Protocol) server for AI agent integration.
//!
//! Exposes system topology, process inspection, and action execution as MCP tools.
//! Supports stdio transport (Claude Desktop) and SSE/HTTP (realtime agents).

pub(crate) mod error;
pub(crate) mod server;
pub(crate) mod tools;
pub(crate) mod transport;

pub use error::McpError;
pub use server::McpServer;

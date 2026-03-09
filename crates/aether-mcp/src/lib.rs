//! MCP (Model Context Protocol) server for AI agent integration.
//!
//! Exposes system topology, process inspection, and action execution as MCP tools.
//! Supports stdio transport (Claude Desktop) and SSE/HTTP (realtime agents).

pub mod arbiter;
pub mod error;
pub mod server;
pub mod tools;
pub mod transport;

pub use error::McpError;
pub use server::McpServer;

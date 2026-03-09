//! MCP server with stdio and SSE transport support.

use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::{AgentAction, ArbiterQueue, WorldGraph};

use crate::McpError;

/// MCP server exposing system data as tools for AI agents.
#[allow(dead_code)]
pub struct McpServer {
    world: Arc<RwLock<WorldGraph>>,
    arbiter: Arc<Mutex<ArbiterQueue>>,
    action_tx: mpsc::Sender<AgentAction>,
}

impl McpServer {
    /// Create a new MCP server with shared state.
    pub fn new(
        world: Arc<RwLock<WorldGraph>>,
        arbiter: Arc<Mutex<ArbiterQueue>>,
        action_tx: mpsc::Sender<AgentAction>,
    ) -> Self {
        Self {
            world,
            arbiter,
            action_tx,
        }
    }

    /// Run in stdio transport mode (blocks until cancelled or EOF).
    ///
    /// Used with `--mcp-stdio` flag. Reads JSON-RPC from stdin, writes to stdout.
    /// TUI must NOT be active when using this mode.
    pub async fn run_stdio(self, cancel: CancellationToken) -> Result<(), McpError> {
        tracing::info!("MCP stdio server starting");
        // TODO: implement JSON-RPC stdio transport with rmcp
        cancel.cancelled().await;
        tracing::info!("MCP stdio server shutting down");
        Ok(())
    }

    /// Run SSE/HTTP transport on the given port (blocks until cancelled).
    ///
    /// Used with `--mcp-sse <PORT>` flag. Runs alongside TUI as a background task.
    pub async fn run_sse(self, port: u16, cancel: CancellationToken) -> Result<(), McpError> {
        tracing::info!("MCP SSE server starting on port {port}");
        // TODO: implement axum SSE transport
        cancel.cancelled().await;
        tracing::info!("MCP SSE server shutting down");
        Ok(())
    }
}

//! Error types for the MCP server crate.

/// MCP server errors.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// I/O error during transport communication.
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Server was cancelled via cancellation token.
    #[error("server cancelled")]
    Cancelled,
}

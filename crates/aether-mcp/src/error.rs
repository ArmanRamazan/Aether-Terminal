//! Error types for the MCP server crate.

/// MCP server errors.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// I/O error during transport communication.
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Server initialization failed (handshake or protocol error).
    #[error("server initialization failed: {0}")]
    Init(#[from] Box<rmcp::service::ServerInitializeError>),
    /// Background service task panicked or was cancelled.
    #[error("service task failed: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
    /// Server was cancelled via cancellation token.
    #[error("server cancelled")]
    Cancelled,
}

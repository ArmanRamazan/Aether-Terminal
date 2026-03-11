/// Errors produced by the web server.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("server error: {0}")]
    Server(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("websocket error: {0}")]
    WebSocket(String),
}

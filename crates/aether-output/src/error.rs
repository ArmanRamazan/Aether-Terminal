/// Errors from the output pipeline.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    /// HTTP request to webhook failed.
    #[error("webhook request failed: {0}")]
    Webhook(#[from] reqwest::Error),

    /// File I/O error.
    #[error("file output error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

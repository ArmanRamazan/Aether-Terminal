/// Errors produced by the metrics subsystem.
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("metrics server error: {0}")]
    Server(String),

    #[error("metrics export error: {0}")]
    Export(String),

    #[error("metrics query error: {0}")]
    Query(String),

    #[error("HTTP error: {0}")]
    Http(String),
}

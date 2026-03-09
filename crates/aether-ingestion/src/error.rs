//! Crate-level error types for the ingestion pipeline.

/// Errors that can occur during ingestion pipeline operation.
#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    /// A probe snapshot call failed.
    #[error("probe snapshot failed: {0}")]
    Probe(String),
    /// The event channel receiver was dropped.
    #[error("event channel closed")]
    ChannelClosed,
}

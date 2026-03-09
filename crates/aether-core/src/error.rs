//! Crate-level error types for aether-core.

/// Errors produced by hexagonal port implementations (probes, storage).
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A system probe operation failed.
    #[error("probe error: {0}")]
    Probe(String),
    /// A storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),
}

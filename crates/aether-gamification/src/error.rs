//! Crate-level error types for aether-gamification.

/// Errors from gamification storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// SQLite operation failed.
    #[error("sqlite error: {0}")]
    Sqlite(String),
    /// Filesystem I/O failed.
    #[error("io error: {0}")]
    Io(String),
}

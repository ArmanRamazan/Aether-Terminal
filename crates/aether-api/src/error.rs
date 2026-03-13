//! Error types for the gRPC API crate.

/// Errors that can occur in the gRPC API layer.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Failed to acquire a shared lock.
    #[error("lock poisoned: {0}")]
    LockPoisoned(String),

    /// gRPC transport error.
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    /// Unknown action type in ExecuteAction request.
    #[error("unknown action type: {0}")]
    UnknownAction(String),
}

impl From<ApiError> for tonic::Status {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::LockPoisoned(_) => tonic::Status::internal(err.to_string()),
            ApiError::Transport(_) => tonic::Status::unavailable(err.to_string()),
            ApiError::UnknownAction(_) => tonic::Status::invalid_argument(err.to_string()),
        }
    }
}

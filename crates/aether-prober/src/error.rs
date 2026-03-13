//! Error types for the prober crate.

/// Errors from probe operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProberError {
    /// HTTP request failed.
    #[error("http probe failed: {0}")]
    Http(String),

    /// TCP connection failed.
    #[error("tcp probe failed: {0}")]
    Tcp(String),

    /// DNS resolution failed.
    #[error("dns probe failed: {0}")]
    Dns(String),

    /// TLS handshake failed.
    #[error("tls probe failed: {0}")]
    Tls(String),

    /// Target list lock poisoned.
    #[error("targets lock poisoned")]
    LockPoisoned,
}

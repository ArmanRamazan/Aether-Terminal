/// Discovery-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// TCP connection failed during port scan.
    #[error("scan failed for {host}:{port}: {reason}")]
    ScanFailed {
        host: String,
        port: u16,
        reason: String,
    },

    /// HTTP probe request failed.
    #[error("probe failed for {url}: {reason}")]
    ProbeFailed { url: String, reason: String },

    /// I/O error during network operations.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

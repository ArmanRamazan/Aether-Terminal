//! Crate-level error types for the eBPF telemetry engine.

/// Errors that can occur during eBPF program loading and operation.
#[derive(Debug, thiserror::Error)]
pub enum EbpfError {
    /// Insufficient privileges to load BPF programs (requires root or CAP_BPF).
    #[error("permission denied: BPF loading requires root or CAP_BPF")]
    PermissionDenied,
    /// Kernel version does not support required BPF features.
    #[error("unsupported kernel version: {0}")]
    KernelVersion(String),
    /// BTF (BPF Type Format) data is not available in the kernel.
    #[error("BTF not available in kernel — required for CO-RE")]
    BtfMissing,
    /// BPF program failed to load into the kernel.
    #[error("BPF program load failed: {0}")]
    LoadFailed(String),
    /// A BPF map operation failed.
    #[error("BPF map error: {0}")]
    MapError(String),
    /// Failed to parse raw event data from ring buffer.
    #[error("event parse error: {reason} (type={event_type}, len={data_len})")]
    ParseError {
        /// Event type discriminant from the ring buffer header.
        event_type: u32,
        /// Actual data length received.
        data_len: usize,
        /// Human-readable reason.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_denied_display() {
        let err = EbpfError::PermissionDenied;
        assert!(
            err.to_string().contains("permission denied"),
            "should mention permission denied"
        );
    }

    #[test]
    fn test_kernel_version_display() {
        let err = EbpfError::KernelVersion("5.4 required, got 4.19".to_string());
        assert!(
            err.to_string().contains("5.4 required"),
            "should contain version info"
        );
    }

    #[test]
    fn test_btf_missing_display() {
        let err = EbpfError::BtfMissing;
        assert!(err.to_string().contains("BTF"), "should mention BTF");
    }

    #[test]
    fn test_load_failed_display() {
        let err = EbpfError::LoadFailed("invalid program".to_string());
        assert!(
            err.to_string().contains("invalid program"),
            "should contain failure reason"
        );
    }

    #[test]
    fn test_map_error_display() {
        let err = EbpfError::MapError("ring buffer full".to_string());
        assert!(
            err.to_string().contains("ring buffer full"),
            "should contain map error detail"
        );
    }
}

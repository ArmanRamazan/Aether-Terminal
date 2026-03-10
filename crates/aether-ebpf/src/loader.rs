//! BPF program loader and lifecycle management.
//!
//! Loads pre-compiled BPF bytecode into the kernel via aya, attaches
//! tracepoints, and initializes ring buffer maps.

use aya::programs::TracePoint;
use aya::Ebpf;

use crate::error::EbpfError;

/// Tracepoint definitions for the process monitor BPF program.
const TRACEPOINTS: &[(&str, &str, &str)] = &[
    ("handle_fork", "sched", "sched_process_fork"),
    ("handle_exit", "sched", "sched_process_exit"),
];

/// Loaded BPF program with attached probes. Cleans up on drop.
pub struct LoadedProgram {
    /// The aya BPF instance owning all programs and maps.
    bpf: Ebpf,
}

impl LoadedProgram {
    /// Borrow the inner `Ebpf` instance for map access.
    pub fn bpf_mut(&mut self) -> &mut Ebpf {
        &mut self.bpf
    }
}

/// Load pre-compiled BPF bytecode and attach tracepoints.
///
/// Loads the bytecode into the kernel, then attaches `handle_fork` and
/// `handle_exit` tracepoint programs. Returns a `LoadedProgram` that
/// detaches everything on drop.
pub async fn load_and_attach(program_bytes: &[u8]) -> Result<LoadedProgram, EbpfError> {
    let mut bpf = Ebpf::load(program_bytes).map_err(classify_load_error)?;

    for &(prog_name, category, tracepoint) in TRACEPOINTS {
        let program: &mut TracePoint = bpf
            .program_mut(prog_name)
            .ok_or_else(|| {
                EbpfError::LoadFailed(format!("program '{prog_name}' not found in BPF object"))
            })?
            .try_into()
            .map_err(|e| EbpfError::LoadFailed(format!("'{prog_name}' is not a TracePoint: {e}")))?;

        program
            .load()
            .map_err(|e| EbpfError::LoadFailed(format!("failed to load '{prog_name}': {e}")))?;

        program.attach(category, tracepoint).map_err(|e| {
            EbpfError::LoadFailed(format!(
                "failed to attach '{prog_name}' to {category}/{tracepoint}: {e}"
            ))
        })?;
    }

    Ok(LoadedProgram { bpf })
}

/// Classify aya load errors into domain-specific `EbpfError` variants.
fn classify_load_error(err: aya::EbpfError) -> EbpfError {
    let msg = format!("{err:?}");
    let display = err.to_string();
    let combined = format!("{msg} {display}");
    let lower = combined.to_lowercase();
    if lower.contains("eperm")
        || lower.contains("permission denied")
        || lower.contains("operation not permitted")
        || lower.contains("permissiondenied")
    {
        return EbpfError::PermissionDenied;
    }
    if lower.contains("btf") {
        return EbpfError::BtfMissing;
    }
    EbpfError::LoadFailed(display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_load_error_permission_denied() {
        let err = aya::EbpfError::FileError {
            path: "/sys/fs/bpf".into(),
            error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Operation not permitted"),
        };
        let classified = classify_load_error(err);
        assert!(
            matches!(classified, EbpfError::PermissionDenied),
            "EPERM should map to PermissionDenied"
        );
    }

    #[tokio::test]
    #[ignore] // requires root and valid BPF bytecode
    async fn test_load_and_attach_invalid_bytes_returns_error() {
        let result = load_and_attach(&[0u8; 16]).await;
        assert!(result.is_err(), "invalid bytes should fail to load");
    }
}

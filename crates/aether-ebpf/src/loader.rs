//! BPF program loader and lifecycle management.
//!
//! Loads pre-compiled BPF bytecode into the kernel via aya, attaches
//! tracepoints/kprobes/raw tracepoints, and initializes ring buffer maps.

use aya::programs::{KProbe, RawTracePoint, TracePoint};
use aya::Ebpf;

use crate::error::EbpfError;

/// How a BPF program attaches to the kernel.
#[derive(Debug, Clone, Copy)]
pub enum AttachType {
    /// Tracepoint: category/name (e.g., sched/sched_process_fork).
    TracePoint {
        category: &'static str,
        name: &'static str,
    },
    /// KProbe: attach to a kernel function entry point.
    KProbe { fn_name: &'static str },
    /// Raw tracepoint: attach by tracepoint name.
    RawTracePoint { tp_name: &'static str },
}

/// Definition of a BPF program within a loaded object.
#[derive(Debug, Clone, Copy)]
pub struct ProgramDef {
    /// Name of the program in the BPF ELF object.
    pub name: &'static str,
    /// How to attach the program to the kernel.
    pub attach: AttachType,
}

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

/// Load pre-compiled BPF bytecode and attach programs according to definitions.
///
/// Each `ProgramDef` describes a program name and its attach type. Returns a
/// `LoadedProgram` that detaches everything on drop.
pub async fn load_and_attach(
    program_bytes: &[u8],
    programs: &[ProgramDef],
) -> Result<LoadedProgram, EbpfError> {
    let mut bpf = Ebpf::load(program_bytes).map_err(classify_load_error)?;

    for def in programs {
        match def.attach {
            AttachType::TracePoint { category, name } => {
                attach_tracepoint(&mut bpf, def.name, category, name)?;
            }
            AttachType::KProbe { fn_name } => {
                attach_kprobe(&mut bpf, def.name, fn_name)?;
            }
            AttachType::RawTracePoint { tp_name } => {
                attach_raw_tracepoint(&mut bpf, def.name, tp_name)?;
            }
        }
    }

    Ok(LoadedProgram { bpf })
}

/// Attach a tracepoint program.
fn attach_tracepoint(
    bpf: &mut Ebpf,
    prog_name: &str,
    category: &str,
    tracepoint: &str,
) -> Result<(), EbpfError> {
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

    Ok(())
}

/// Attach a kprobe program to a kernel function.
fn attach_kprobe(bpf: &mut Ebpf, prog_name: &str, fn_name: &str) -> Result<(), EbpfError> {
    let program: &mut KProbe = bpf
        .program_mut(prog_name)
        .ok_or_else(|| {
            EbpfError::LoadFailed(format!("program '{prog_name}' not found in BPF object"))
        })?
        .try_into()
        .map_err(|e| EbpfError::LoadFailed(format!("'{prog_name}' is not a KProbe: {e}")))?;

    program
        .load()
        .map_err(|e| EbpfError::LoadFailed(format!("failed to load '{prog_name}': {e}")))?;

    program.attach(fn_name, 0).map_err(|e| {
        EbpfError::LoadFailed(format!(
            "failed to attach '{prog_name}' to kprobe/{fn_name}: {e}"
        ))
    })?;

    Ok(())
}

/// Attach a raw tracepoint program.
fn attach_raw_tracepoint(bpf: &mut Ebpf, prog_name: &str, tp_name: &str) -> Result<(), EbpfError> {
    let program: &mut RawTracePoint = bpf
        .program_mut(prog_name)
        .ok_or_else(|| {
            EbpfError::LoadFailed(format!("program '{prog_name}' not found in BPF object"))
        })?
        .try_into()
        .map_err(|e| EbpfError::LoadFailed(format!("'{prog_name}' is not a RawTracePoint: {e}")))?;

    program
        .load()
        .map_err(|e| EbpfError::LoadFailed(format!("failed to load '{prog_name}': {e}")))?;

    program.attach(tp_name).map_err(|e| {
        EbpfError::LoadFailed(format!(
            "failed to attach '{prog_name}' to raw_tracepoint/{tp_name}: {e}"
        ))
    })?;

    Ok(())
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
            error: std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Operation not permitted",
            ),
        };
        let classified = classify_load_error(err);
        assert!(
            matches!(classified, EbpfError::PermissionDenied),
            "EPERM should map to PermissionDenied"
        );
    }

    #[test]
    fn test_attach_type_variants() {
        let tp = AttachType::TracePoint {
            category: "sched",
            name: "sched_process_fork",
        };
        assert!(matches!(tp, AttachType::TracePoint { .. }));

        let kp = AttachType::KProbe {
            fn_name: "tcp_v4_connect",
        };
        assert!(matches!(kp, AttachType::KProbe { .. }));

        let rtp = AttachType::RawTracePoint {
            tp_name: "sys_enter",
        };
        assert!(matches!(rtp, AttachType::RawTracePoint { .. }));
    }

    #[test]
    fn test_program_def_construction() {
        let def = ProgramDef {
            name: "handle_fork",
            attach: AttachType::TracePoint {
                category: "sched",
                name: "sched_process_fork",
            },
        };
        assert_eq!(def.name, "handle_fork");
    }

    #[tokio::test]
    #[ignore] // requires root and valid BPF bytecode
    async fn test_load_and_attach_invalid_bytes_returns_error() {
        let defs = [ProgramDef {
            name: "test",
            attach: AttachType::TracePoint {
                category: "sched",
                name: "sched_process_fork",
            },
        }];
        let result = load_and_attach(&[0u8; 16], &defs).await;
        assert!(result.is_err(), "invalid bytes should fail to load");
    }
}

//! Probe management for multiple BPF programs.
//!
//! Coordinates loading and attaching of all BPF probe programs (process,
//! network, syscall) through a single `ProbeManager` interface.

use crate::error::EbpfError;
use crate::loader::{load_and_attach, AttachType, LoadedProgram, ProgramDef};

/// Probe definitions for the process monitor BPF program.
const PROCESS_PROBES: &[ProgramDef] = &[
    ProgramDef {
        name: "handle_fork",
        attach: AttachType::TracePoint {
            category: "sched",
            name: "sched_process_fork",
        },
    },
    ProgramDef {
        name: "handle_exit",
        attach: AttachType::TracePoint {
            category: "sched",
            name: "sched_process_exit",
        },
    },
];

/// Probe definitions for the network monitor BPF program.
const NET_PROBES: &[ProgramDef] = &[
    ProgramDef {
        name: "handle_tcp_connect",
        attach: AttachType::KProbe {
            fn_name: "tcp_v4_connect",
        },
    },
    ProgramDef {
        name: "handle_tcp_close",
        attach: AttachType::KProbe {
            fn_name: "tcp_close",
        },
    },
];

/// Probe definitions for the syscall monitor BPF program.
const SYSCALL_PROBES: &[ProgramDef] = &[ProgramDef {
    name: "handle_sys_enter",
    attach: AttachType::RawTracePoint {
        tp_name: "sys_enter",
    },
}];

/// Manages loading and lifecycle of all BPF probe programs.
///
/// Owns the loaded BPF programs and provides access to their maps for
/// ring buffer readers. Detaches all probes on drop.
pub struct ProbeManager {
    process: Option<LoadedProgram>,
    net: Option<LoadedProgram>,
    syscall: Option<LoadedProgram>,
}

impl ProbeManager {
    /// Load and attach all BPF probe programs.
    ///
    /// Each bytecode slice is the pre-compiled BPF ELF object for the
    /// corresponding monitor. All three are required.
    pub async fn attach_all(
        process_bytes: &[u8],
        net_bytes: &[u8],
        syscall_bytes: &[u8],
    ) -> Result<Self, EbpfError> {
        let process = load_and_attach(process_bytes, PROCESS_PROBES).await?;
        let net = load_and_attach(net_bytes, NET_PROBES).await?;
        let syscall = load_and_attach(syscall_bytes, SYSCALL_PROBES).await?;

        Ok(Self {
            process: Some(process),
            net: Some(net),
            syscall: Some(syscall),
        })
    }

    /// Detach all probes and release BPF resources.
    pub fn detach_all(&mut self) {
        self.process.take();
        self.net.take();
        self.syscall.take();
    }

    /// Mutable reference to the process monitor program (for ring buffer access).
    pub fn process_mut(&mut self) -> Option<&mut LoadedProgram> {
        self.process.as_mut()
    }

    /// Mutable reference to the network monitor program (for ring buffer access).
    pub fn net_mut(&mut self) -> Option<&mut LoadedProgram> {
        self.net.as_mut()
    }

    /// Mutable reference to the syscall monitor program (for ring buffer access).
    pub fn syscall_mut(&mut self) -> Option<&mut LoadedProgram> {
        self.syscall.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_probes_definitions() {
        assert_eq!(PROCESS_PROBES.len(), 2);
        assert_eq!(PROCESS_PROBES[0].name, "handle_fork");
        assert_eq!(PROCESS_PROBES[1].name, "handle_exit");
    }

    #[test]
    fn test_net_probes_definitions() {
        assert_eq!(NET_PROBES.len(), 2);
        assert_eq!(NET_PROBES[0].name, "handle_tcp_connect");
        assert!(matches!(
            NET_PROBES[0].attach,
            AttachType::KProbe {
                fn_name: "tcp_v4_connect"
            }
        ));
        assert_eq!(NET_PROBES[1].name, "handle_tcp_close");
        assert!(matches!(
            NET_PROBES[1].attach,
            AttachType::KProbe {
                fn_name: "tcp_close"
            }
        ));
    }

    #[test]
    fn test_syscall_probes_definitions() {
        assert_eq!(SYSCALL_PROBES.len(), 1);
        assert_eq!(SYSCALL_PROBES[0].name, "handle_sys_enter");
        assert!(matches!(
            SYSCALL_PROBES[0].attach,
            AttachType::RawTracePoint {
                tp_name: "sys_enter"
            }
        ));
    }

    #[tokio::test]
    #[ignore] // requires root and valid BPF bytecode
    async fn test_attach_all_invalid_bytes_returns_error() {
        let result = ProbeManager::attach_all(&[0u8; 16], &[0u8; 16], &[0u8; 16]).await;
        assert!(result.is_err(), "invalid bytes should fail to load");
    }
}

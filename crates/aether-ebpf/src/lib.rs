//! eBPF telemetry engine for high-performance kernel-level process and network monitoring.
//!
//! Uses aya (pure-Rust BPF loader) to attach kernel probes and read events from
//! ring buffers. Feature-gated behind `ebpf` — compiles as no-op without it.

pub mod error;
pub mod events;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod loader;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod probes;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod ring_buffer;

pub use error::EbpfError;
pub use events::{
    comm_to_string, parse_event, ProcessExitEvent, ProcessForkEvent, RawKernelEvent, SyscallEvent,
    TcpCloseEvent, TcpConnectEvent, EVENT_TYPE_EXIT, EVENT_TYPE_FORK, EVENT_TYPE_SYSCALL,
    EVENT_TYPE_TCP_CLOSE, EVENT_TYPE_TCP_CONNECT,
};

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub use loader::{AttachType, ProgramDef};

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub use probes::ProbeManager;

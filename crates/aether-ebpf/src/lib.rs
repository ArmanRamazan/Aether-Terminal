//! eBPF telemetry engine for high-performance kernel-level process and network monitoring.
//!
//! Uses aya (pure-Rust BPF loader) to attach kernel probes and read events from
//! ring buffers. Feature-gated behind `ebpf` — compiles as no-op without it.

pub mod error;
pub mod events;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod loader;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod ring_buffer;

pub use error::EbpfError;
pub use events::{ProcessExitEvent, ProcessForkEvent};

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub use ring_buffer::RawKernelEvent;

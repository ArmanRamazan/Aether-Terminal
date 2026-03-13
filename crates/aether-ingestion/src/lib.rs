//! System data collection via sysinfo (crossplatform) and eBPF (Linux, feature-gated).
//!
//! Implements the `SystemProbe` trait from aether-core, providing process and network
//! telemetry through a dual-tick ingestion pipeline.

pub mod ebpf_bridge;
pub(crate) mod error;
pub mod pipeline;
pub mod sysinfo_probe;

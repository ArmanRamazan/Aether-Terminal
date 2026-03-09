//! eBPF telemetry engine for high-performance kernel-level process and network monitoring.
//!
//! Uses aya (pure-Rust BPF loader) to attach kernel probes and read events from
//! ring buffers. Feature-gated behind `ebpf` — compiles as no-op without it.

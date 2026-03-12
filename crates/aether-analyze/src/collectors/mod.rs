//! System data collectors for /proc, cgroup, and host-level metrics.

pub mod cgroup;
pub mod procfs;

/// Per-process profile collected from /proc/[pid].
#[derive(Debug, Clone, Default)]
pub struct ProcessProfile {
    pub pid: u32,
    pub threads: u32,
    pub open_fds: u32,
    pub voluntary_ctx_switches: u64,
    pub nonvoluntary_ctx_switches: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
}

/// Host-level profile collected from /proc.
#[derive(Debug, Clone, Default)]
pub struct HostProfile {
    pub loadavg_1: f64,
    pub loadavg_5: f64,
    pub loadavg_15: f64,
    pub mem_total: u64,
    pub mem_available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

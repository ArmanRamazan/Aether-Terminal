//! System data collectors for /proc, cgroup, and host-level metrics.

pub mod cgroup;
pub mod procfs;

/// Process state and thread info from /proc/[pid]/status.
#[derive(Debug, Clone, Default)]
pub struct ProcStatus {
    pub state: char,
    pub threads: u32,
    pub vm_rss_kb: u64,
    pub vm_size_kb: u64,
    pub voluntary_ctxt_switches: u64,
    pub nonvoluntary_ctxt_switches: u64,
}

/// Aggregated memory map from /proc/[pid]/smaps_rollup.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryMap {
    pub rss: u64,
    pub pss: u64,
    pub shared_clean: u64,
    pub shared_dirty: u64,
    pub private_clean: u64,
    pub private_dirty: u64,
    pub swap: u64,
}

/// File descriptor info from /proc/[pid]/fd and /proc/[pid]/limits.
#[derive(Debug, Clone, Default)]
pub struct FdInfo {
    pub count: u32,
    pub soft_limit: u64,
}

/// I/O stats from /proc/[pid]/io.
#[derive(Debug, Clone, Default)]
pub struct IoStats {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// Single kernel stack frame from /proc/[pid]/stack.
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub symbol: String,
    pub address: u64,
}

/// Per-process profile collected from /proc/[pid].
#[derive(Debug, Clone, Default)]
pub struct ProcessProfile {
    pub status: ProcStatus,
    pub memory: MemoryMap,
    pub fds: FdInfo,
    pub io: Option<IoStats>,
    pub kernel_stack: Vec<StackFrame>,
}

/// Host memory info from /proc/meminfo.
#[derive(Debug, Clone, Default)]
pub struct HostMemInfo {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// Disk mount info from /proc/mounts + statvfs.
#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

/// Host-level profile collected from /proc.
#[derive(Debug, Clone, Default)]
pub struct HostProfile {
    pub loadavg: (f64, f64, f64),
    pub meminfo: HostMemInfo,
    pub disks: Vec<DiskInfo>,
}

//! ProcfsCollector — reads /proc for deep process and host profiles.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AnalyzeError;

use super::{
    DiskInfo, FdInfo, HostMemInfo, HostProfile, IoStats, MemoryMap, ProcStatus, ProcessProfile,
    StackFrame,
};

/// Collects process and host metrics from /proc.
pub struct ProcfsCollector {
    proc_root: PathBuf,
}

impl ProcfsCollector {
    pub fn new() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
        }
    }

    /// Create a collector reading from a custom root (for testing).
    pub fn with_root(root: PathBuf) -> Self {
        Self { proc_root: root }
    }

    /// Collect per-process profile from /proc/[pid].
    pub fn process_profile(&self, pid: u32) -> Result<ProcessProfile, AnalyzeError> {
        let pid_dir = self.proc_root.join(pid.to_string());
        if !pid_dir.exists() {
            return Err(AnalyzeError::Collector(format!(
                "pid {pid} not found in procfs"
            )));
        }

        let status = self.parse_status(&pid_dir)?;
        let memory = self.parse_smaps_rollup(&pid_dir);
        let fds = self.parse_fds(&pid_dir);
        let io = self.parse_io(&pid_dir);
        let kernel_stack = self.parse_stack(&pid_dir);

        Ok(ProcessProfile {
            status,
            memory,
            fds,
            io,
            kernel_stack,
        })
    }

    /// Collect host-level profile from /proc.
    pub fn host_profile(&self) -> Result<HostProfile, AnalyzeError> {
        let loadavg = self.parse_loadavg()?;
        let meminfo = self.parse_meminfo()?;
        let disks = self.parse_disks();

        Ok(HostProfile {
            loadavg,
            meminfo,
            disks,
        })
    }

    /// Parse /proc/[pid]/status for process state, threads, memory, context switches.
    fn parse_status(&self, pid_dir: &Path) -> Result<ProcStatus, AnalyzeError> {
        let content = fs::read_to_string(pid_dir.join("status")).map_err(|e| {
            AnalyzeError::Collector(format!("failed to read {}/status: {e}", pid_dir.display()))
        })?;

        Ok(ProcStatus {
            state: parse_status_char(&content, "State:"),
            threads: parse_status_field(&content, "Threads:"),
            vm_rss_kb: parse_status_field(&content, "VmRSS:"),
            vm_size_kb: parse_status_field(&content, "VmSize:"),
            voluntary_ctxt_switches: parse_status_field(&content, "voluntary_ctxt_switches:"),
            nonvoluntary_ctxt_switches: parse_status_field(&content, "nonvoluntary_ctxt_switches:"),
        })
    }

    /// Parse /proc/[pid]/smaps_rollup for aggregated memory map. Returns zeros if unavailable.
    fn parse_smaps_rollup(&self, pid_dir: &Path) -> MemoryMap {
        let path = pid_dir.join("smaps_rollup");
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return MemoryMap::default(),
        };

        MemoryMap {
            rss: parse_smaps_kb(&content, "Rss:"),
            pss: parse_smaps_kb(&content, "Pss:"),
            shared_clean: parse_smaps_kb(&content, "Shared_Clean:"),
            shared_dirty: parse_smaps_kb(&content, "Shared_Dirty:"),
            private_clean: parse_smaps_kb(&content, "Private_Clean:"),
            private_dirty: parse_smaps_kb(&content, "Private_Dirty:"),
            swap: parse_smaps_kb(&content, "Swap:"),
        }
    }

    /// Count fds in /proc/[pid]/fd and parse soft limit from /proc/[pid]/limits.
    fn parse_fds(&self, pid_dir: &Path) -> FdInfo {
        let count = fs::read_dir(pid_dir.join("fd"))
            .map(|entries| entries.count() as u32)
            .unwrap_or(0);

        let soft_limit = fs::read_to_string(pid_dir.join("limits"))
            .ok()
            .and_then(|content| parse_limits_max_open_files(&content))
            .unwrap_or(0);

        FdInfo { count, soft_limit }
    }

    /// Parse /proc/[pid]/io for read/write bytes. Returns None if permission denied.
    fn parse_io(&self, pid_dir: &Path) -> Option<IoStats> {
        let content = fs::read_to_string(pid_dir.join("io")).ok()?;
        Some(IoStats {
            read_bytes: parse_io_field(&content, "read_bytes:"),
            write_bytes: parse_io_field(&content, "write_bytes:"),
        })
    }

    /// Parse /proc/[pid]/stack for kernel stack frames. May be empty without CAP_SYS_PTRACE.
    fn parse_stack(&self, pid_dir: &Path) -> Vec<StackFrame> {
        let content = match fs::read_to_string(pid_dir.join("stack")) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        content
            .lines()
            .filter_map(|line| {
                // Format: "[<ffffffff81234567>] symbol_name+0x10/0x20"
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                let addr_start = line.find('<')? + 1;
                let addr_end = line.find('>')?;
                let addr_str = line.get(addr_start..addr_end)?;
                let address = u64::from_str_radix(addr_str, 16).ok()?;
                if address == 0 {
                    return None;
                }
                let rest = line.get(addr_end + 2..)?.trim();
                let symbol = rest.split('+').next().unwrap_or(rest).to_string();
                if symbol.is_empty() || symbol == "0" {
                    return None;
                }
                Some(StackFrame { symbol, address })
            })
            .collect()
    }

    /// Parse /proc/loadavg for 1/5/15 minute load averages.
    fn parse_loadavg(&self) -> Result<(f64, f64, f64), AnalyzeError> {
        let content = fs::read_to_string(self.proc_root.join("loadavg"))
            .map_err(|e| AnalyzeError::Collector(format!("failed to read loadavg: {e}")))?;

        let parts: Vec<&str> = content.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(AnalyzeError::Collector(
                "loadavg: unexpected format".to_string(),
            ));
        }

        let a = parts[0].parse().unwrap_or(0.0);
        let b = parts[1].parse().unwrap_or(0.0);
        let c = parts[2].parse().unwrap_or(0.0);
        Ok((a, b, c))
    }

    /// Parse /proc/meminfo for host memory stats (values in bytes).
    fn parse_meminfo(&self) -> Result<HostMemInfo, AnalyzeError> {
        let content = fs::read_to_string(self.proc_root.join("meminfo"))
            .map_err(|e| AnalyzeError::Collector(format!("failed to read meminfo: {e}")))?;

        let total = parse_meminfo_kb(&content, "MemTotal:");
        let available = parse_meminfo_kb(&content, "MemAvailable:");
        let swap_total = parse_meminfo_kb(&content, "SwapTotal:");
        let swap_free = parse_meminfo_kb(&content, "SwapFree:");

        Ok(HostMemInfo {
            total,
            available,
            used: total.saturating_sub(available),
            swap_total,
            swap_used: swap_total.saturating_sub(swap_free),
        })
    }

    /// Parse /proc/mounts and call statvfs for each real filesystem.
    fn parse_disks(&self) -> Vec<DiskInfo> {
        let content = match fs::read_to_string(self.proc_root.join("mounts")) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        content
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    return None;
                }
                let device = parts[0];
                let mount = parts[1];
                let fstype = parts[2];

                // Filter to real filesystems.
                if !is_real_filesystem(device, fstype) {
                    return None;
                }

                statvfs_disk_info(mount)
            })
            .collect()
    }
}

impl Default for ProcfsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the first character after the label from /proc/[pid]/status State: line.
fn parse_status_char(content: &str, label: &str) -> char {
    content
        .lines()
        .find(|line| line.starts_with(label))
        .and_then(|line| line[label.len()..].trim().chars().next())
        .unwrap_or('?')
}

/// Parse a numeric field from /proc/[pid]/status (format: "Label:\tValue").
fn parse_status_field<T: std::str::FromStr + Default>(content: &str, label: &str) -> T {
    content
        .lines()
        .find(|line| line.starts_with(label))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or_default()
}

/// Parse a kB value from smaps_rollup, converting to bytes.
fn parse_smaps_kb(content: &str, label: &str) -> u64 {
    content
        .lines()
        .find(|line| line.starts_with(label))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

/// Parse a numeric field from /proc/[pid]/io (format: "label: value").
fn parse_io_field(content: &str, label: &str) -> u64 {
    content
        .lines()
        .find(|line| line.starts_with(label))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Parse a kB value from /proc/meminfo, converting to bytes.
fn parse_meminfo_kb(content: &str, label: &str) -> u64 {
    content
        .lines()
        .find(|line| line.starts_with(label))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

/// Parse "Max open files" soft limit from /proc/[pid]/limits.
fn parse_limits_max_open_files(content: &str) -> Option<u64> {
    content
        .lines()
        .find(|line| line.starts_with("Max open files"))
        .and_then(|line| {
            // Format: "Max open files            1024                 1048576              files"
            let rest = line.strip_prefix("Max open files")?;
            rest.split_whitespace().next()?.parse().ok()
        })
}

/// Check if a filesystem type represents a real (disk-backed) filesystem.
fn is_real_filesystem(device: &str, fstype: &str) -> bool {
    // Must be a device path or known real fstype.
    let real_fstypes = [
        "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "ntfs", "vfat", "fat32", "fuseblk",
    ];
    (device.starts_with("/dev/") || real_fstypes.contains(&fstype))
        && !device.starts_with("/dev/loop")
}

/// Call statvfs on a mount point and return DiskInfo.
fn statvfs_disk_info(mount: &str) -> Option<DiskInfo> {
    // Use nix::sys::statvfs if available, otherwise libc directly.
    let c_path = std::ffi::CString::new(mount).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if ret != 0 {
        return None;
    }

    let block_size = stat.f_frsize as u64;
    let total = stat.f_blocks * block_size;
    let available = stat.f_bavail * block_size;
    let used = total.saturating_sub(stat.f_bfree * block_size);

    Some(DiskInfo {
        mount: mount.to_string(),
        total,
        used,
        available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn setup_fake_proc(dir: &Path, pid: u32) {
        let pid_dir = dir.join(pid.to_string());
        fs::create_dir_all(pid_dir.join("fd")).unwrap();

        // Create fake fd entries.
        fs::write(pid_dir.join("fd/0"), "").unwrap();
        fs::write(pid_dir.join("fd/1"), "").unwrap();
        fs::write(pid_dir.join("fd/2"), "").unwrap();

        fs::write(
            pid_dir.join("status"),
            "Name:\tfake\nState:\tS (sleeping)\nThreads:\t4\nVmRSS:\t1234 kB\nVmSize:\t5678 kB\nvoluntary_ctxt_switches:\t100\nnonvoluntary_ctxt_switches:\t10\n",
        )
        .unwrap();

        fs::write(
            pid_dir.join("smaps_rollup"),
            "Rss:                1234 kB\nPss:                 800 kB\nShared_Clean:        200 kB\nShared_Dirty:         50 kB\nPrivate_Clean:       400 kB\nPrivate_Dirty:       100 kB\nSwap:                  0 kB\n",
        )
        .unwrap();

        fs::write(
            pid_dir.join("limits"),
            "Limit                     Soft Limit           Hard Limit           Units\nMax open files            1024                 1048576              files\n",
        )
        .unwrap();

        fs::write(
            pid_dir.join("io"),
            "rchar: 5000\nwchar: 3000\nread_bytes: 4096\nwrite_bytes: 2048\n",
        )
        .unwrap();

        fs::write(
            pid_dir.join("stack"),
            "[<ffffffff81000001>] schedule+0x10/0x20\n[<ffffffff81000002>] do_syscall_64+0x5/0x10\n",
        )
        .unwrap();
    }

    fn setup_fake_host(dir: &Path) {
        fs::write(dir.join("loadavg"), "1.50 2.00 1.75 3/200 12345\n").unwrap();
        fs::write(
            dir.join("meminfo"),
            "MemTotal:       16000000 kB\nMemFree:         2000000 kB\nMemAvailable:    8000000 kB\nSwapTotal:       4000000 kB\nSwapFree:        3000000 kB\n",
        )
        .unwrap();
    }

    #[test]
    fn test_process_profile_from_fake_proc() {
        let tmp = tempfile::tempdir().unwrap();
        setup_fake_proc(tmp.path(), 42);

        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let profile = collector.process_profile(42).unwrap();

        assert_eq!(profile.status.state, 'S');
        assert_eq!(profile.status.threads, 4);
        assert_eq!(profile.status.vm_rss_kb, 1234);
        assert_eq!(profile.status.vm_size_kb, 5678);
        assert_eq!(profile.status.voluntary_ctxt_switches, 100);
        assert_eq!(profile.status.nonvoluntary_ctxt_switches, 10);

        assert_eq!(profile.memory.rss, 1234 * 1024);
        assert_eq!(profile.memory.pss, 800 * 1024);
        assert_eq!(profile.memory.shared_clean, 200 * 1024);
        assert_eq!(profile.memory.shared_dirty, 50 * 1024);
        assert_eq!(profile.memory.private_clean, 400 * 1024);
        assert_eq!(profile.memory.private_dirty, 100 * 1024);
        assert_eq!(profile.memory.swap, 0);

        assert_eq!(profile.fds.count, 3);
        assert_eq!(profile.fds.soft_limit, 1024);

        let io = profile.io.unwrap();
        assert_eq!(io.read_bytes, 4096);
        assert_eq!(io.write_bytes, 2048);

        assert_eq!(profile.kernel_stack.len(), 2);
        assert_eq!(profile.kernel_stack[0].symbol, "schedule");
        assert_eq!(profile.kernel_stack[0].address, 0xffffffff81000001);
        assert_eq!(profile.kernel_stack[1].symbol, "do_syscall_64");
    }

    #[test]
    fn test_host_profile_from_fake_proc() {
        let tmp = tempfile::tempdir().unwrap();
        setup_fake_host(tmp.path());

        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let profile = collector.host_profile().unwrap();

        assert!((profile.loadavg.0 - 1.5).abs() < f64::EPSILON);
        assert!((profile.loadavg.1 - 2.0).abs() < f64::EPSILON);
        assert!((profile.loadavg.2 - 1.75).abs() < f64::EPSILON);

        assert_eq!(profile.meminfo.total, 16_000_000 * 1024);
        assert_eq!(profile.meminfo.available, 8_000_000 * 1024);
        assert_eq!(
            profile.meminfo.used,
            (16_000_000 - 8_000_000) * 1024,
            "used = total - available"
        );
        assert_eq!(profile.meminfo.swap_total, 4_000_000 * 1024);
        assert_eq!(
            profile.meminfo.swap_used,
            (4_000_000 - 3_000_000) * 1024,
            "swap_used = swap_total - swap_free"
        );
    }

    #[test]
    fn test_process_profile_missing_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());

        let result = collector.process_profile(9999);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_profile_missing_optional_files() {
        let tmp = tempfile::tempdir().unwrap();
        let pid_dir = tmp.path().join("1");
        fs::create_dir_all(&pid_dir).unwrap();
        fs::write(
            pid_dir.join("status"),
            "Name:\tminimal\nState:\tR (running)\nThreads:\t1\n",
        )
        .unwrap();

        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let profile = collector.process_profile(1).unwrap();

        assert_eq!(profile.status.state, 'R');
        assert_eq!(profile.status.threads, 1);
        assert_eq!(
            profile.memory,
            MemoryMap::default(),
            "smaps_rollup missing → zeros"
        );
        assert_eq!(profile.fds.count, 0, "fd dir missing → 0");
        assert!(profile.io.is_none(), "io missing → None");
        assert!(profile.kernel_stack.is_empty(), "stack missing → empty");
    }

    #[test]
    fn test_parse_limits_max_open_files() {
        let content = "Limit                     Soft Limit           Hard Limit           Units\nMax cpu time              unlimited            unlimited            seconds\nMax open files            1024                 1048576              files\nMax processes             63304                63304                processes\n";
        assert_eq!(parse_limits_max_open_files(content), Some(1024));
    }

    #[test]
    fn test_parse_stack_frames() {
        let tmp = tempfile::tempdir().unwrap();
        let pid_dir = tmp.path().join("1");
        fs::create_dir_all(&pid_dir).unwrap();
        fs::write(
            pid_dir.join("status"),
            "Name:\ttest\nState:\tS (sleeping)\nThreads:\t1\n",
        )
        .unwrap();
        fs::write(
            pid_dir.join("stack"),
            "[<ffffffff810ab000>] ep_poll+0x1a0/0x250\n[<0>] 0\n",
        )
        .unwrap();

        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let profile = collector.process_profile(1).unwrap();

        assert_eq!(
            profile.kernel_stack.len(),
            1,
            "zero-address frame filtered by parser"
        );
        assert_eq!(profile.kernel_stack[0].symbol, "ep_poll");
        assert_eq!(profile.kernel_stack[0].address, 0xffffffff810ab000);
    }

    // --- Live /proc tests (Linux only) ---

    #[test]
    fn test_procfs_self_status() {
        let collector = ProcfsCollector::new();
        let profile = collector.process_profile(std::process::id()).unwrap();

        assert!(
            profile.status.state == 'R' || profile.status.state == 'S',
            "test process state should be R or S, got '{}'",
            profile.status.state,
        );
        assert!(
            profile.status.threads >= 1,
            "test process should have at least 1 thread"
        );
    }

    #[test]
    fn test_procfs_self_fds() {
        let collector = ProcfsCollector::new();
        let profile = collector.process_profile(std::process::id()).unwrap();

        assert!(profile.fds.count > 0, "test process should have open fds");
    }

    #[test]
    fn test_procfs_host_loadavg() {
        let collector = ProcfsCollector::new();
        let host = collector.host_profile().unwrap();

        assert!(host.loadavg.0 >= 0.0, "loadavg_1 should be non-negative");
        assert!(host.loadavg.1 >= 0.0, "loadavg_5 should be non-negative");
        assert!(host.loadavg.2 >= 0.0, "loadavg_15 should be non-negative");
    }

    #[test]
    fn test_procfs_host_meminfo() {
        let collector = ProcfsCollector::new();
        let host = collector.host_profile().unwrap();

        assert!(host.meminfo.total > 0, "total memory should be > 0");
        assert!(
            host.meminfo.available <= host.meminfo.total,
            "available should be <= total"
        );
    }

    #[test]
    fn test_procfs_nonexistent_pid() {
        let collector = ProcfsCollector::new();
        let result = collector.process_profile(99_999_999);
        assert!(result.is_err(), "nonexistent pid should return error");
    }
}

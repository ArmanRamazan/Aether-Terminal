//! ProcfsCollector — reads /proc for process and host profiles.

use std::fs;
use std::path::PathBuf;

use crate::error::AnalyzeError;

use super::{HostProfile, ProcessProfile};

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

    /// Collect per-process profile from /proc/[pid]/status and /proc/[pid]/io.
    pub fn process_profile(&self, pid: u32) -> Result<ProcessProfile, AnalyzeError> {
        let pid_dir = self.proc_root.join(pid.to_string());
        if !pid_dir.exists() {
            return Err(AnalyzeError::Collector(format!(
                "pid {pid} not found in procfs"
            )));
        }

        let mut profile = ProcessProfile {
            pid,
            ..Default::default()
        };

        // Parse /proc/[pid]/status for threads and context switches.
        if let Ok(status) = fs::read_to_string(pid_dir.join("status")) {
            profile.threads = parse_status_field(&status, "Threads:");
            profile.voluntary_ctx_switches =
                parse_status_field::<u64>(&status, "voluntary_ctxt_switches:");
            profile.nonvoluntary_ctx_switches =
                parse_status_field::<u64>(&status, "nonvoluntary_ctxt_switches:");
        }

        // Count open file descriptors from /proc/[pid]/fd.
        if let Ok(entries) = fs::read_dir(pid_dir.join("fd")) {
            profile.open_fds = entries.count() as u32;
        }

        // Parse /proc/[pid]/io for I/O bytes.
        if let Ok(io) = fs::read_to_string(pid_dir.join("io")) {
            profile.io_read_bytes = parse_io_field(&io, "read_bytes:");
            profile.io_write_bytes = parse_io_field(&io, "write_bytes:");
        }

        Ok(profile)
    }

    /// Collect host-level profile from /proc/loadavg and /proc/meminfo.
    pub fn host_profile(&self) -> Result<HostProfile, AnalyzeError> {
        let mut profile = HostProfile::default();

        // Parse /proc/loadavg.
        let loadavg_path = self.proc_root.join("loadavg");
        if let Ok(content) = fs::read_to_string(&loadavg_path) {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 3 {
                profile.loadavg_1 = parts[0].parse().unwrap_or(0.0);
                profile.loadavg_5 = parts[1].parse().unwrap_or(0.0);
                profile.loadavg_15 = parts[2].parse().unwrap_or(0.0);
            }
        }

        // Parse /proc/meminfo.
        let meminfo_path = self.proc_root.join("meminfo");
        if let Ok(content) = fs::read_to_string(&meminfo_path) {
            profile.mem_total = parse_meminfo_kb(&content, "MemTotal:");
            profile.mem_available = parse_meminfo_kb(&content, "MemAvailable:");
            profile.swap_total = parse_meminfo_kb(&content, "SwapTotal:");
            profile.swap_free = parse_meminfo_kb(&content, "SwapFree:");
        }

        Ok(profile)
    }
}

impl Default for ProcfsCollector {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
            "Name:\tfake\nThreads:\t4\nvoluntary_ctxt_switches:\t100\nnonvoluntary_ctxt_switches:\t10\n",
        )
        .unwrap();

        fs::write(
            pid_dir.join("io"),
            "rchar: 5000\nwchar: 3000\nread_bytes: 4096\nwrite_bytes: 2048\n",
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

        assert_eq!(profile.pid, 42);
        assert_eq!(profile.threads, 4);
        assert_eq!(profile.open_fds, 3);
        assert_eq!(profile.voluntary_ctx_switches, 100);
        assert_eq!(profile.nonvoluntary_ctx_switches, 10);
        assert_eq!(profile.io_read_bytes, 4096);
        assert_eq!(profile.io_write_bytes, 2048);
    }

    #[test]
    fn test_host_profile_from_fake_proc() {
        let tmp = tempfile::tempdir().unwrap();
        setup_fake_host(tmp.path());

        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());
        let profile = collector.host_profile().unwrap();

        assert!((profile.loadavg_1 - 1.5).abs() < f64::EPSILON);
        assert!((profile.loadavg_5 - 2.0).abs() < f64::EPSILON);
        assert_eq!(profile.mem_total, 16_000_000 * 1024);
        assert_eq!(profile.mem_available, 8_000_000 * 1024);
        assert_eq!(profile.swap_total, 4_000_000 * 1024);
        assert_eq!(profile.swap_free, 3_000_000 * 1024);
    }

    #[test]
    fn test_process_profile_missing_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let collector = ProcfsCollector::with_root(tmp.path().to_path_buf());

        let result = collector.process_profile(9999);
        assert!(result.is_err());
    }
}

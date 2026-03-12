//! CgroupCollector — reads cgroup v2 limits for processes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AnalyzeError;
use crate::rules::types::ProcessLimits;

/// Collects cgroup resource limits for processes.
pub struct CgroupCollector {
    cgroup_root: PathBuf,
}

impl CgroupCollector {
    pub fn new() -> Self {
        Self {
            cgroup_root: PathBuf::from("/sys/fs/cgroup"),
        }
    }

    /// Create a collector reading from a custom root (for testing).
    pub fn with_root(root: PathBuf) -> Self {
        Self { cgroup_root: root }
    }

    /// Read cgroup limits for a process by resolving its cgroup path.
    pub fn limits(&self, pid: u32) -> Result<ProcessLimits, AnalyzeError> {
        let cgroup_path = self.resolve_cgroup_path(pid)?;
        let cgroup_dir = self.cgroup_root.join(
            cgroup_path
                .strip_prefix("/")
                .unwrap_or(cgroup_path.as_ref()),
        );

        if !cgroup_dir.exists() {
            return Err(AnalyzeError::Collector(format!(
                "cgroup dir not found for pid {pid}: {}",
                cgroup_dir.display()
            )));
        }

        Ok(ProcessLimits {
            cgroup_memory_max: read_cgroup_u64(&cgroup_dir, "memory.max"),
            cgroup_cpu_quota: read_cpu_quota(&cgroup_dir),
            cgroup_pids_max: read_cgroup_u64(&cgroup_dir, "pids.max"),
            ..Default::default()
        })
    }

    /// Resolve the cgroup path for a pid from /proc/[pid]/cgroup.
    fn resolve_cgroup_path(&self, pid: u32) -> Result<PathBuf, AnalyzeError> {
        let cgroup_file = Path::new("/proc").join(pid.to_string()).join("cgroup");
        let content = fs::read_to_string(&cgroup_file).map_err(|e| {
            AnalyzeError::Collector(format!("failed to read cgroup for pid {pid}: {e}"))
        })?;

        // cgroup v2 format: "0::/path"
        for line in content.lines() {
            if let Some(path) = line.strip_prefix("0::") {
                return Ok(PathBuf::from(path));
            }
        }

        Err(AnalyzeError::Collector(format!(
            "no cgroup v2 entry for pid {pid}"
        )))
    }
}

impl Default for CgroupCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a u64 value from a cgroup file. Returns None for "max" or missing files.
fn read_cgroup_u64(dir: &Path, file: &str) -> Option<u64> {
    let content = fs::read_to_string(dir.join(file)).ok()?;
    let trimmed = content.trim();
    if trimmed == "max" {
        return None;
    }
    trimmed.parse().ok()
}

/// Read CPU quota from cpu.max (format: "quota period", e.g. "100000 100000").
fn read_cpu_quota(dir: &Path) -> Option<u64> {
    let content = fs::read_to_string(dir.join("cpu.max")).ok()?;
    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.is_empty() || parts[0] == "max" {
        return None;
    }
    parts[0].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_fake_cgroup(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("memory.max"), "1073741824\n").unwrap();
        fs::write(dir.join("cpu.max"), "100000 100000\n").unwrap();
        fs::write(dir.join("pids.max"), "4096\n").unwrap();
    }

    #[test]
    fn test_read_cgroup_u64_numeric() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("memory.max"), "1073741824\n").unwrap();

        let val = read_cgroup_u64(tmp.path(), "memory.max");
        assert_eq!(val, Some(1_073_741_824));
    }

    #[test]
    fn test_read_cgroup_u64_max_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("memory.max"), "max\n").unwrap();

        let val = read_cgroup_u64(tmp.path(), "memory.max");
        assert_eq!(val, None);
    }

    #[test]
    fn test_read_cpu_quota() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("cpu.max"), "100000 100000\n").unwrap();

        let val = read_cpu_quota(tmp.path());
        assert_eq!(val, Some(100_000));
    }

    #[test]
    fn test_read_cpu_quota_max() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("cpu.max"), "max 100000\n").unwrap();

        let val = read_cpu_quota(tmp.path());
        assert_eq!(val, None);
    }

    #[test]
    fn test_limits_from_fake_cgroup() {
        let tmp = tempfile::tempdir().unwrap();
        let cgroup_subdir = tmp.path().join("system.slice");
        setup_fake_cgroup(&cgroup_subdir);

        let collector = CgroupCollector::with_root(tmp.path().to_path_buf());

        // Directly test reading from a known cgroup dir (bypass resolve_cgroup_path).
        let dir = &cgroup_subdir;
        let mut limits = ProcessLimits::default();
        limits.cgroup_memory_max = read_cgroup_u64(dir, "memory.max");
        limits.cgroup_cpu_quota = read_cpu_quota(dir);
        limits.cgroup_pids_max = read_cgroup_u64(dir, "pids.max");

        assert_eq!(limits.cgroup_memory_max, Some(1_073_741_824));
        assert_eq!(limits.cgroup_cpu_quota, Some(100_000));
        assert_eq!(limits.cgroup_pids_max, Some(4096));

        // Test that collector with invalid pid returns error.
        let result = collector.limits(99999);
        assert!(result.is_err());
    }
}

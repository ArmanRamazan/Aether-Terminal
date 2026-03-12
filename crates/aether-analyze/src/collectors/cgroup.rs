//! CgroupCollector — reads cgroup v1/v2 limits for container resource detection.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AnalyzeError;
use crate::rules::types::ProcessLimits;

use super::FdInfo;

/// Cgroup hierarchy version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupVersion {
    V1,
    V2,
}

/// Resource limits and usage from cgroup.
#[derive(Debug, Clone, Default)]
pub struct CgroupLimits {
    pub memory_max: Option<u64>,
    pub memory_current: u64,
    pub cpu_quota: Option<u64>,
    pub cpu_period: Option<u64>,
    pub pids_max: Option<u64>,
    pub pids_current: u64,
}

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

    /// Detect whether a process uses cgroup v1 or v2.
    pub fn detect_version(&self, pid: u32) -> Result<CgroupVersion, AnalyzeError> {
        let content = read_proc_cgroup(pid)?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();

        if lines.len() == 1 && lines[0].starts_with("0::") {
            Ok(CgroupVersion::V2)
        } else {
            Ok(CgroupVersion::V1)
        }
    }

    /// Resolve the cgroup filesystem path for a process. Returns None if path doesn't exist.
    pub fn cgroup_path(&self, pid: u32) -> Result<Option<PathBuf>, AnalyzeError> {
        let content = read_proc_cgroup(pid)?;
        let version = self.detect_version(pid)?;

        let relative = match version {
            CgroupVersion::V2 => {
                content
                    .lines()
                    .find_map(|l| l.strip_prefix("0::"))
                    .unwrap_or("/")
            }
            CgroupVersion::V1 => {
                // Find memory controller: "<id>:memory:<path>" or "<id>:...,memory,...:<path>"
                content
                    .lines()
                    .find_map(|line| {
                        let parts: Vec<&str> = line.splitn(3, ':').collect();
                        if parts.len() == 3 && parts[1].split(',').any(|c| c == "memory") {
                            Some(parts[2])
                        } else {
                            None
                        }
                    })
                    .unwrap_or("/")
            }
        };

        let base = match version {
            CgroupVersion::V2 => self.cgroup_root.clone(),
            CgroupVersion::V1 => self.cgroup_root.join("memory"),
        };
        let path = base.join(relative.trim_start_matches('/'));

        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    /// Read cgroup limits and current usage for a process. Returns None on bare metal.
    pub fn limits(&self, pid: u32) -> Result<Option<CgroupLimits>, AnalyzeError> {
        let cg_path = match self.cgroup_path(pid)? {
            Some(p) => p,
            None => return Ok(None),
        };

        let version = self.detect_version(pid)?;
        let limits = match version {
            CgroupVersion::V2 => read_v2_limits(&cg_path),
            CgroupVersion::V1 => read_v1_limits(&cg_path),
        };

        Ok(Some(limits))
    }

    /// Map CgroupLimits + FdInfo into ProcessLimits for the rule engine.
    pub fn to_process_limits(&self, cg: &CgroupLimits, fd_info: &FdInfo) -> ProcessLimits {
        ProcessLimits {
            cgroup_memory_max: cg.memory_max,
            cgroup_cpu_quota: cg.cpu_quota,
            cgroup_pids_max: cg.pids_max,
            ulimit_nofile: if fd_info.soft_limit > 0 {
                Some(fd_info.soft_limit)
            } else {
                None
            },
            disk_total: None,
        }
    }
}

impl Default for CgroupCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Read /proc/{pid}/cgroup content.
fn read_proc_cgroup(pid: u32) -> Result<String, AnalyzeError> {
    fs::read_to_string(format!("/proc/{pid}/cgroup")).map_err(|e| {
        AnalyzeError::Collector(format!("failed to read /proc/{pid}/cgroup: {e}"))
    })
}

/// Read cgroup v2 control files.
fn read_v2_limits(path: &Path) -> CgroupLimits {
    let memory_max =
        read_file_trimmed(&path.join("memory.max")).and_then(|s| parse_max_value(&s));
    let memory_current = read_file_trimmed(&path.join("memory.current"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let (cpu_quota, cpu_period) = read_file_trimmed(&path.join("cpu.max"))
        .map(|s| parse_cpu_max(&s))
        .unwrap_or((None, None));

    let pids_max = read_file_trimmed(&path.join("pids.max")).and_then(|s| parse_max_value(&s));
    let pids_current = read_file_trimmed(&path.join("pids.current"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    CgroupLimits {
        memory_max,
        memory_current,
        cpu_quota,
        cpu_period,
        pids_max,
        pids_current,
    }
}

/// Read cgroup v1 control files from the memory controller path.
fn read_v1_limits(path: &Path) -> CgroupLimits {
    let memory_max = read_file_trimmed(&path.join("memory.limit_in_bytes")).and_then(|s| {
        let val: u64 = s.parse().ok()?;
        // V1 uses a huge sentinel for "no limit" (PAGE_COUNTER_MAX * PAGE_SIZE).
        if val >= u64::MAX / 2 {
            None
        } else {
            Some(val)
        }
    });
    let memory_current = read_file_trimmed(&path.join("memory.usage_in_bytes"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // CPU is in a separate hierarchy for v1; try sibling path.
    let (cpu_quota, cpu_period) = resolve_v1_sibling(path, "memory", "cpu")
        .map(|cpu_path| {
            let quota = read_file_trimmed(&cpu_path.join("cpu.cfs_quota_us")).and_then(|s| {
                let val: i64 = s.parse().ok()?;
                if val < 0 {
                    None
                } else {
                    Some(val as u64)
                }
            });
            let period = read_file_trimmed(&cpu_path.join("cpu.cfs_period_us"))
                .and_then(|s| s.parse().ok());
            (quota, period)
        })
        .unwrap_or((None, None));

    // PIDs is in a separate hierarchy for v1.
    let (pids_max, pids_current) = resolve_v1_sibling(path, "memory", "pids")
        .map(|pids_path| {
            let max =
                read_file_trimmed(&pids_path.join("pids.max")).and_then(|s| parse_max_value(&s));
            let current = read_file_trimmed(&pids_path.join("pids.current"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            (max, current)
        })
        .unwrap_or((None, 0));

    CgroupLimits {
        memory_max,
        memory_current,
        cpu_quota,
        cpu_period,
        pids_max,
        pids_current,
    }
}

/// Resolve a v1 sibling controller path by swapping the controller name.
fn resolve_v1_sibling(path: &Path, from: &str, to: &str) -> Option<PathBuf> {
    let s = path.to_str()?;
    let prefix = format!("/sys/fs/cgroup/{from}");
    let relative = s.strip_prefix(&prefix)?;
    Some(PathBuf::from(format!("/sys/fs/cgroup/{to}")).join(relative.trim_start_matches('/')))
}

/// Read a cgroup file, returning trimmed content.
fn read_file_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Parse a value that can be "max" (no limit) or a number.
fn parse_max_value(s: &str) -> Option<u64> {
    if s == "max" {
        None
    } else {
        s.parse().ok()
    }
}

/// Parse cgroup v2 cpu.max: "quota period" or "max period".
fn parse_cpu_max(s: &str) -> (Option<u64>, Option<u64>) {
    let mut parts = s.split_whitespace();
    let quota = parts.next().and_then(parse_max_value);
    let period = parts.next().and_then(|v| v.parse().ok());
    (quota, period)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_max_value() {
        assert_eq!(parse_max_value("max"), None);
        assert_eq!(parse_max_value("1048576"), Some(1_048_576));
        assert_eq!(parse_max_value("0"), Some(0));
        assert_eq!(parse_max_value("garbage"), None);
    }

    #[test]
    fn test_parse_cpu_max() {
        assert_eq!(parse_cpu_max("max 100000"), (None, Some(100_000)));
        assert_eq!(parse_cpu_max("50000 100000"), (Some(50_000), Some(100_000)));
        assert_eq!(parse_cpu_max("max"), (None, None));
    }

    #[test]
    fn test_detect_cgroup_version_self() {
        let collector = CgroupCollector::new();
        let version = collector.detect_version(std::process::id());
        assert!(version.is_ok(), "should read /proc/self/cgroup");
        let v = version.unwrap();
        assert!(
            v == CgroupVersion::V1 || v == CgroupVersion::V2,
            "should detect a valid cgroup version"
        );
    }

    #[test]
    fn test_cgroup_path_self() {
        let collector = CgroupCollector::new();
        let result = collector.cgroup_path(std::process::id());
        assert!(
            result.is_ok(),
            "cgroup_path should not error for self process"
        );
        // Path may be None on bare metal or unsupported environments.
    }

    #[test]
    fn test_limits_self() {
        let collector = CgroupCollector::new();
        let result = collector.limits(std::process::id());
        assert!(result.is_ok(), "limits should not error for self process");
        // May be None if no cgroup path exists.
    }

    #[test]
    fn test_to_process_limits_mapping() {
        let collector = CgroupCollector::new();
        let cg = CgroupLimits {
            memory_max: Some(512 * 1024 * 1024),
            memory_current: 100 * 1024 * 1024,
            cpu_quota: Some(50_000),
            cpu_period: Some(100_000),
            pids_max: Some(1000),
            pids_current: 42,
        };
        let fd_info = FdInfo {
            count: 15,
            soft_limit: 1024,
        };

        let limits = collector.to_process_limits(&cg, &fd_info);

        assert_eq!(
            limits.cgroup_memory_max,
            Some(512 * 1024 * 1024),
            "memory_max should map directly"
        );
        assert_eq!(
            limits.cgroup_cpu_quota,
            Some(50_000),
            "cpu_quota should map directly"
        );
        assert_eq!(
            limits.cgroup_pids_max,
            Some(1000),
            "pids_max should map directly"
        );
        assert_eq!(
            limits.ulimit_nofile,
            Some(1024),
            "soft_limit > 0 should map to Some"
        );
        assert_eq!(limits.disk_total, None, "disk_total always None from cgroup");
    }

    #[test]
    fn test_to_process_limits_no_limits() {
        let collector = CgroupCollector::new();
        let cg = CgroupLimits::default();
        let fd_info = FdInfo::default();

        let limits = collector.to_process_limits(&cg, &fd_info);

        assert_eq!(limits.cgroup_memory_max, None);
        assert_eq!(limits.cgroup_cpu_quota, None);
        assert_eq!(limits.cgroup_pids_max, None);
        assert_eq!(limits.ulimit_nofile, None, "soft_limit 0 → None");
    }

    #[test]
    fn test_read_v2_limits_from_fake() {
        let tmp = tempfile::tempdir().unwrap();
        let cg = tmp.path();
        fs::write(cg.join("memory.max"), "1073741824\n").unwrap();
        fs::write(cg.join("memory.current"), "524288\n").unwrap();
        fs::write(cg.join("cpu.max"), "50000 100000\n").unwrap();
        fs::write(cg.join("pids.max"), "4096\n").unwrap();
        fs::write(cg.join("pids.current"), "42\n").unwrap();

        let limits = read_v2_limits(cg);
        assert_eq!(limits.memory_max, Some(1_073_741_824));
        assert_eq!(limits.memory_current, 524_288);
        assert_eq!(limits.cpu_quota, Some(50_000));
        assert_eq!(limits.cpu_period, Some(100_000));
        assert_eq!(limits.pids_max, Some(4096));
        assert_eq!(limits.pids_current, 42);
    }

    #[test]
    fn test_read_v2_limits_no_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let cg = tmp.path();
        fs::write(cg.join("memory.max"), "max\n").unwrap();
        fs::write(cg.join("memory.current"), "1024\n").unwrap();
        fs::write(cg.join("cpu.max"), "max 100000\n").unwrap();
        fs::write(cg.join("pids.max"), "max\n").unwrap();
        fs::write(cg.join("pids.current"), "1\n").unwrap();

        let limits = read_v2_limits(cg);
        assert_eq!(limits.memory_max, None, "\"max\" → None");
        assert_eq!(limits.cpu_quota, None, "cpu \"max\" → None");
        assert_eq!(limits.cpu_period, Some(100_000));
        assert_eq!(limits.pids_max, None, "pids \"max\" → None");
    }

    #[test]
    fn test_read_v2_limits_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let limits = read_v2_limits(tmp.path());

        assert_eq!(limits.memory_max, None, "missing file → None");
        assert_eq!(limits.memory_current, 0, "missing file → 0");
        assert_eq!(limits.cpu_quota, None);
        assert_eq!(limits.cpu_period, None);
        assert_eq!(limits.pids_max, None);
        assert_eq!(limits.pids_current, 0);
    }
}

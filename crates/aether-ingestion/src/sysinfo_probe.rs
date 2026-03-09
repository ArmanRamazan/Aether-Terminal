//! Cross-platform system metrics probe backed by the `sysinfo` crate.

use std::sync::Mutex;
use std::time::SystemTime;

use aether_core::models::{ProcessNode, ProcessState, SystemSnapshot};
use aether_core::traits::SystemProbe;
use glam::Vec3;
use sysinfo::{ProcessStatus, ProcessesToUpdate, System};

/// Cross-platform [`SystemProbe`] implementation using the `sysinfo` crate.
///
/// Refreshes process data on every [`snapshot()`](SystemProbe::snapshot) call.
/// Network edges are left empty — per-process connection tracking requires
/// eBPF or `/proc/net` parsing, which is out of scope for this probe.
pub struct SysinfoProbe {
    system: Mutex<System>,
}

impl SysinfoProbe {
    /// Create a new probe. Does NOT refresh on construction (per crate rules).
    pub fn new() -> Self {
        Self {
            system: Mutex::new(System::new()),
        }
    }
}

impl Default for SysinfoProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbe for SysinfoProbe {
    async fn snapshot(&self) -> Result<SystemSnapshot, Box<dyn std::error::Error + Send + Sync>> {
        let processes = {
            let mut sys = self.system.lock().map_err(|e| e.to_string())?;
            sys.refresh_processes(ProcessesToUpdate::All, true);

            sys.processes()
                .values()
                .map(|proc| ProcessNode {
                    pid: proc.pid().as_u32(),
                    ppid: proc.parent().map_or(0, |p| p.as_u32()),
                    name: proc.name().to_string_lossy().to_string(),
                    cpu_percent: proc.cpu_usage(),
                    mem_bytes: proc.memory(),
                    state: map_process_status(proc.status()),
                    hp: 100.0,
                    xp: 0,
                    position_3d: Vec3::ZERO,
                })
                .collect()
        };

        Ok(SystemSnapshot {
            processes,
            edges: Vec::new(),
            timestamp: SystemTime::now(),
        })
    }
}

/// Map sysinfo's [`ProcessStatus`] to our [`ProcessState`].
fn map_process_status(status: ProcessStatus) -> ProcessState {
    match status {
        ProcessStatus::Run => ProcessState::Running,
        ProcessStatus::Sleep
        | ProcessStatus::Idle
        | ProcessStatus::UninterruptibleDiskSleep
        | ProcessStatus::Waking
        | ProcessStatus::Wakekill
        | ProcessStatus::Parked
        | ProcessStatus::LockBlocked => ProcessState::Sleeping,
        ProcessStatus::Zombie | ProcessStatus::Dead => ProcessState::Zombie,
        ProcessStatus::Stop | ProcessStatus::Tracing => ProcessState::Stopped,
        ProcessStatus::Unknown(_) => ProcessState::Sleeping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshot_returns_processes() {
        let probe = SysinfoProbe::new();
        let snap = probe.snapshot().await.expect("snapshot should succeed");
        assert!(
            !snap.processes.is_empty(),
            "at least one system process must exist"
        );
    }

    #[tokio::test]
    async fn process_fields_are_populated() {
        let probe = SysinfoProbe::new();
        let snap = probe.snapshot().await.expect("snapshot should succeed");
        for proc in &snap.processes {
            assert!(proc.pid > 0, "pid should be > 0");
            assert!(!proc.name.is_empty(), "name should not be empty");
        }
    }

    #[test]
    fn map_status_run() {
        assert_eq!(
            map_process_status(ProcessStatus::Run),
            ProcessState::Running
        );
    }

    #[test]
    fn map_status_sleep() {
        assert_eq!(
            map_process_status(ProcessStatus::Sleep),
            ProcessState::Sleeping
        );
    }

    #[test]
    fn map_status_zombie() {
        assert_eq!(
            map_process_status(ProcessStatus::Zombie),
            ProcessState::Zombie
        );
    }

    #[test]
    fn map_status_stop() {
        assert_eq!(
            map_process_status(ProcessStatus::Stop),
            ProcessState::Stopped
        );
    }

    #[test]
    fn map_status_unknown() {
        assert_eq!(
            map_process_status(ProcessStatus::Unknown(99)),
            ProcessState::Sleeping
        );
    }
}

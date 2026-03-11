//! Core data models for process nodes, network edges, and system snapshots.

use std::time::Instant;

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::metrics::HostId;

/// A process represented as a node in the 3D world graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessNode {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    pub state: ProcessState,
    pub hp: f32,
    pub xp: u32,
    pub position_3d: Vec3,
}

/// OS-level process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    Running,
    Sleeping,
    Zombie,
    Stopped,
}

/// A network connection represented as an edge in the world graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEdge {
    pub source_pid: u32,
    pub dest: SocketAddr,
    pub protocol: Protocol,
    pub bytes_per_sec: u64,
    pub state: ConnectionState,
}

/// Network protocol classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    TCP,
    UDP,
    DNS,
    QUIC,
    HTTP,
    HTTPS,
    Unknown,
}

/// TCP connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Established,
    Listen,
    TimeWait,
    CloseWait,
}

/// A point-in-time snapshot of the entire system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub processes: Vec<ProcessNode>,
    pub edges: Vec<NetworkEdge>,
    pub timestamp: std::time::SystemTime,
}

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Target of a diagnostic finding.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum DiagTarget {
    /// A specific OS process.
    Process { pid: u32, name: String },
    /// An entire host machine.
    Host(HostId),
    /// A container (Docker, podman, etc.).
    Container { id: String, name: String },
    /// A disk / mount point.
    Disk { mount: String },
    /// A network interface.
    Network { interface: String },
}

/// Severity of a diagnostic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// Informational, no action needed.
    Info,
    /// Potential issue, investigate soon.
    Warning,
    /// Immediate attention required.
    Critical,
}

/// Category describing the root cause of a diagnostic.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum DiagCategory {
    MemoryLeak,
    MemoryPressure,
    CpuSaturation,
    CpuSpike,
    DiskPressure,
    DiskIoHeavy,
    FdExhaustion,
    ConnectionSurge,
    ZombieAccumulation,
    ThreadExplosion,
    CrashLoop,
    ConfigMismatch,
    CapacityRisk,
    CorrelatedAnomaly,
}

/// A piece of evidence supporting a diagnostic finding.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    /// Name of the metric (e.g. "cpu_percent", "mem_rss").
    pub metric: String,
    /// Current observed value.
    pub current: f64,
    /// Threshold that was breached.
    pub threshold: f64,
    /// Optional trend (rate of change).
    pub trend: Option<f64>,
    /// Human-readable context.
    pub context: String,
}

/// A concrete action recommendation.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum RecommendedAction {
    ScaleUp { resource: String, from: String, to: String },
    Restart { reason: String },
    RaiseLimits { limit_name: String, from: String, to: String },
    ReduceLoad { suggestion: String },
    Investigate { what: String },
    KillProcess { pid: u32, reason: String },
    NoAction { reason: String },
}

/// How urgently a recommendation should be acted upon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    /// Act now.
    Immediate,
    /// Act within minutes.
    Soon,
    /// Schedule for later.
    Planning,
    /// FYI only.
    Informational,
}

/// A recommendation attached to a diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    /// What to do.
    pub action: RecommendedAction,
    /// Why this action is recommended.
    pub reason: String,
    /// How urgently to act.
    pub urgency: Urgency,
    /// Whether this can be executed automatically.
    pub auto_executable: bool,
}

/// A complete diagnostic finding.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Unique identifier.
    pub id: u64,
    /// Host where the issue was detected.
    pub host: HostId,
    /// What is affected.
    pub target: DiagTarget,
    /// How severe the issue is.
    pub severity: Severity,
    /// Root-cause category.
    pub category: DiagCategory,
    /// One-line human-readable summary.
    pub summary: String,
    /// Supporting evidence.
    pub evidence: Vec<Evidence>,
    /// What to do about it.
    pub recommendation: Recommendation,
    /// When the diagnostic was first detected.
    pub detected_at: Instant,
    /// When it was resolved, if ever.
    pub resolved_at: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn sample_process() -> ProcessNode {
        ProcessNode {
            pid: 1234,
            ppid: 1,
            name: "test-proc".to_string(),
            cpu_percent: 25.5,
            mem_bytes: 1024 * 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::new(1.0, 2.0, 3.0),
        }
    }

    fn sample_edge() -> NetworkEdge {
        NetworkEdge {
            source_pid: 1234,
            dest: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            protocol: Protocol::TCP,
            bytes_per_sec: 4096,
            state: ConnectionState::Established,
        }
    }

    #[test]
    fn process_node_construction() {
        let p = sample_process();
        assert_eq!(p.pid, 1234);
        assert_eq!(p.ppid, 1);
        assert_eq!(p.name, "test-proc");
        assert_eq!(p.state, ProcessState::Running);
        assert_eq!(p.position_3d, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn network_edge_construction() {
        let e = sample_edge();
        assert_eq!(e.source_pid, 1234);
        assert_eq!(e.protocol, Protocol::TCP);
        assert_eq!(e.state, ConnectionState::Established);
        assert_eq!(e.bytes_per_sec, 4096);
    }

    #[test]
    fn process_node_serde_roundtrip() {
        let original = sample_process();
        let json = serde_json::to_string(&original).unwrap();
        let restored: ProcessNode = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pid, original.pid);
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.state, original.state);
        assert_eq!(restored.position_3d, original.position_3d);
    }

    #[test]
    fn network_edge_serde_roundtrip() {
        let original = sample_edge();
        let json = serde_json::to_string(&original).unwrap();
        let restored: NetworkEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.source_pid, original.source_pid);
        assert_eq!(restored.dest, original.dest);
        assert_eq!(restored.protocol, original.protocol);
        assert_eq!(restored.state, original.state);
    }

    #[test]
    fn system_snapshot_serde_roundtrip() {
        let snapshot = SystemSnapshot {
            processes: vec![sample_process()],
            edges: vec![sample_edge()],
            timestamp: std::time::SystemTime::now(),
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: SystemSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.processes.len(), 1);
        assert_eq!(restored.edges.len(), 1);
        assert_eq!(restored.processes[0].pid, 1234);
    }

    #[test]
    fn process_state_all_variants() {
        for state in [
            ProcessState::Running,
            ProcessState::Sleeping,
            ProcessState::Zombie,
            ProcessState::Stopped,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let restored: ProcessState = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, state);
        }
    }

    #[test]
    fn protocol_all_variants() {
        for proto in [
            Protocol::TCP,
            Protocol::UDP,
            Protocol::DNS,
            Protocol::QUIC,
            Protocol::HTTP,
            Protocol::HTTPS,
            Protocol::Unknown,
        ] {
            let json = serde_json::to_string(&proto).unwrap();
            let restored: Protocol = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, proto);
        }
    }

    #[test]
    fn connection_state_all_variants() {
        for state in [
            ConnectionState::Established,
            ConnectionState::Listen,
            ConnectionState::TimeWait,
            ConnectionState::CloseWait,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let restored: ConnectionState = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, state);
        }
    }

    #[test]
    fn test_diagnostic_severity_ordering() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Critical > Severity::Info);
    }

    #[test]
    fn test_diagnostic_construction() {
        let diag = Diagnostic {
            id: 1,
            host: HostId::default(),
            target: DiagTarget::Process {
                pid: 42,
                name: "nginx".to_string(),
            },
            severity: Severity::Warning,
            category: DiagCategory::MemoryLeak,
            summary: "RSS growing steadily".to_string(),
            evidence: vec![Evidence {
                metric: "mem_rss".to_string(),
                current: 512.0,
                threshold: 400.0,
                trend: Some(10.0),
                context: "grew 10 MB/min over last 30 min".to_string(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Restart {
                    reason: "memory leak detected".to_string(),
                },
                reason: "RSS exceeds threshold with positive trend".to_string(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: std::time::Instant::now(),
            resolved_at: None,
        };
        assert_eq!(diag.id, 1);
        assert_eq!(diag.evidence.len(), 1);
    }
}

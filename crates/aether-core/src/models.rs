//! Core data models for process nodes, network edges, and system snapshots.

use std::fmt;
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

impl fmt::Display for ProcessState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => f.write_str("running"),
            Self::Sleeping => f.write_str("sleeping"),
            Self::Zombie => f.write_str("zombie"),
            Self::Stopped => f.write_str("stopped"),
        }
    }
}

/// A network connection represented as an edge in the world graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEdge {
    pub source_pid: u32,
    pub dest_pid: Option<u32>,
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

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TCP => f.write_str("tcp"),
            Self::UDP => f.write_str("udp"),
            Self::DNS => f.write_str("dns"),
            Self::QUIC => f.write_str("quic"),
            Self::HTTP => f.write_str("http"),
            Self::HTTPS => f.write_str("https"),
            Self::Unknown => f.write_str("unknown"),
        }
    }
}

/// TCP connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Established,
    Listen,
    TimeWait,
    CloseWait,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Established => f.write_str("established"),
            Self::Listen => f.write_str("listen"),
            Self::TimeWait => f.write_str("time_wait"),
            Self::CloseWait => f.write_str("close_wait"),
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Critical => f.write_str("critical"),
        }
    }
}

/// Category describing the root cause of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
    ScriptRule,
}

impl fmt::Display for DiagCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MemoryLeak => f.write_str("memory_leak"),
            Self::MemoryPressure => f.write_str("memory_pressure"),
            Self::CpuSaturation => f.write_str("cpu_saturation"),
            Self::CpuSpike => f.write_str("cpu_spike"),
            Self::DiskPressure => f.write_str("disk_pressure"),
            Self::DiskIoHeavy => f.write_str("disk_io_heavy"),
            Self::FdExhaustion => f.write_str("fd_exhaustion"),
            Self::ConnectionSurge => f.write_str("connection_surge"),
            Self::ZombieAccumulation => f.write_str("zombie_accumulation"),
            Self::ThreadExplosion => f.write_str("thread_explosion"),
            Self::CrashLoop => f.write_str("crash_loop"),
            Self::ConfigMismatch => f.write_str("config_mismatch"),
            Self::CapacityRisk => f.write_str("capacity_risk"),
            Self::CorrelatedAnomaly => f.write_str("correlated_anomaly"),
            Self::ScriptRule => f.write_str("script_rule"),
        }
    }
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

impl fmt::Display for RecommendedAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScaleUp { resource, from, to } => {
                write!(f, "scale_up({resource}: {from} -> {to})")
            }
            Self::Restart { reason } => write!(f, "restart({reason})"),
            Self::RaiseLimits { limit_name, from, to } => {
                write!(f, "raise_limits({limit_name}: {from} -> {to})")
            }
            Self::ReduceLoad { suggestion } => write!(f, "reduce_load({suggestion})"),
            Self::Investigate { what } => write!(f, "investigate({what})"),
            Self::KillProcess { pid, reason } => write!(f, "kill_process(pid={pid}: {reason})"),
            Self::NoAction { reason } => write!(f, "no_action({reason})"),
        }
    }
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

impl fmt::Display for Urgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Immediate => f.write_str("immediate"),
            Self::Soon => f.write_str("soon"),
            Self::Planning => f.write_str("planning"),
            Self::Informational => f.write_str("informational"),
        }
    }
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
            dest_pid: None,
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
    fn test_process_state_display() {
        assert_eq!(ProcessState::Running.to_string(), "running");
        assert_eq!(ProcessState::Sleeping.to_string(), "sleeping");
        assert_eq!(ProcessState::Zombie.to_string(), "zombie");
        assert_eq!(ProcessState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_protocol_display() {
        assert_eq!(Protocol::TCP.to_string(), "tcp");
        assert_eq!(Protocol::UDP.to_string(), "udp");
        assert_eq!(Protocol::HTTPS.to_string(), "https");
        assert_eq!(Protocol::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Info.to_string(), "info");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Critical.to_string(), "critical");
    }

    #[test]
    fn test_diag_category_display() {
        assert_eq!(DiagCategory::MemoryLeak.to_string(), "memory_leak");
        assert_eq!(DiagCategory::CpuSpike.to_string(), "cpu_spike");
        assert_eq!(DiagCategory::ScriptRule.to_string(), "script_rule");
    }

    #[test]
    fn test_urgency_display() {
        assert_eq!(Urgency::Immediate.to_string(), "immediate");
        assert_eq!(Urgency::Soon.to_string(), "soon");
        assert_eq!(Urgency::Planning.to_string(), "planning");
        assert_eq!(Urgency::Informational.to_string(), "informational");
    }

    #[test]
    fn test_recommended_action_display() {
        let action = RecommendedAction::Restart {
            reason: "memory leak".to_string(),
        };
        assert_eq!(action.to_string(), "restart(memory leak)");
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

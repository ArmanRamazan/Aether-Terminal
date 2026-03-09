//! Core data models for process nodes, network edges, and system snapshots.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
}

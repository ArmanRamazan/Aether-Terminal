//! World graph — the central data structure mapping processes and their connections.
//!
//! Wraps `petgraph::StableGraph` with a `HashMap<u32, NodeIndex>` for O(1) pid lookups.
//! `StableGraph` is used because node indices remain valid after removal.

use std::collections::HashMap;

use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableGraph};

use crate::models::{NetworkEdge, ProcessNode, SystemSnapshot};

/// The 3D world graph of processes and their network connections.
#[derive(Debug)]
pub struct WorldGraph {
    graph: StableGraph<ProcessNode, NetworkEdge>,
    pid_index: HashMap<u32, NodeIndex>,
}

impl WorldGraph {
    /// Creates an empty world graph.
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            pid_index: HashMap::new(),
        }
    }

    /// Inserts a process node. If a process with the same pid already exists, it is replaced.
    pub fn add_process(&mut self, node: ProcessNode) -> NodeIndex {
        let pid = node.pid;
        if let Some(&idx) = self.pid_index.get(&pid) {
            self.graph[idx] = node;
            return idx;
        }
        let idx = self.graph.add_node(node);
        self.pid_index.insert(pid, idx);
        idx
    }

    /// Removes a process and all its incident edges. Returns `true` if the process existed.
    pub fn remove_process(&mut self, pid: u32) -> bool {
        if let Some(idx) = self.pid_index.remove(&pid) {
            self.graph.remove_node(idx);
            true
        } else {
            false
        }
    }

    /// Mutates a process node in-place via a closure.
    pub fn update_process(&mut self, pid: u32, f: impl FnOnce(&mut ProcessNode)) {
        if let Some(&idx) = self.pid_index.get(&pid) {
            f(&mut self.graph[idx]);
        }
    }

    /// Adds a directed edge between two processes identified by pid.
    /// Returns `None` if either pid is not in the graph.
    pub fn add_connection(
        &mut self,
        from_pid: u32,
        to_pid: u32,
        edge: NetworkEdge,
    ) -> Option<EdgeIndex> {
        let &from = self.pid_index.get(&from_pid)?;
        let &to = self.pid_index.get(&to_pid)?;
        Some(self.graph.add_edge(from, to, edge))
    }

    /// Finds a process by pid (shared reference).
    pub fn find_by_pid(&self, pid: u32) -> Option<&ProcessNode> {
        self.pid_index.get(&pid).map(|&idx| &self.graph[idx])
    }

    /// Finds a process by pid (mutable reference).
    pub fn find_by_pid_mut(&mut self, pid: u32) -> Option<&mut ProcessNode> {
        self.pid_index
            .get(&pid)
            .copied()
            .map(move |idx| &mut self.graph[idx])
    }

    /// Iterates over all process nodes.
    pub fn processes(&self) -> impl Iterator<Item = &ProcessNode> {
        self.graph.node_weights()
    }

    /// Iterates over all network edges.
    pub fn edges(&self) -> impl Iterator<Item = &NetworkEdge> {
        self.graph.edge_weights()
    }

    /// Returns the number of processes in the graph.
    pub fn process_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Returns all process pids in the graph.
    pub fn pids(&self) -> Vec<u32> {
        self.pid_index.keys().copied().collect()
    }

    /// Returns endpoint pids for all edges as (source_pid, target_pid) pairs.
    pub fn edge_pairs(&self) -> Vec<(u32, u32)> {
        self.graph
            .edge_indices()
            .filter_map(|idx| {
                let (a, b) = self.graph.edge_endpoints(idx)?;
                Some((self.graph[a].pid, self.graph[b].pid))
            })
            .collect()
    }

    /// Returns endpoint pids and edge data for all edges.
    pub fn edge_pairs_with_data(&self) -> Vec<(u32, u32, &NetworkEdge)> {
        self.graph
            .edge_indices()
            .filter_map(|idx| {
                let (a, b) = self.graph.edge_endpoints(idx)?;
                let weight = &self.graph[idx];
                Some((self.graph[a].pid, self.graph[b].pid, weight))
            })
            .collect()
    }

    /// Synchronises the graph with a new system snapshot.
    ///
    /// - Adds new processes (by pid) and updates existing ones.
    /// - Removes processes not present in the snapshot.
    /// - Replaces all edges (network connections are ephemeral).
    pub fn apply_snapshot(&mut self, snapshot: &SystemSnapshot) {
        let new_pids: HashMap<u32, &ProcessNode> =
            snapshot.processes.iter().map(|p| (p.pid, p)).collect();

        // Remove processes that are no longer present.
        let stale: Vec<u32> = self
            .pid_index
            .keys()
            .filter(|pid| !new_pids.contains_key(pid))
            .copied()
            .collect();
        for pid in stale {
            self.remove_process(pid);
        }

        // Add or update processes.
        for proc in &snapshot.processes {
            if let Some(&idx) = self.pid_index.get(&proc.pid) {
                self.graph[idx] = proc.clone();
            } else {
                self.add_process(proc.clone());
            }
        }

        // Clear all edges and re-add from the snapshot.
        let edge_indices: Vec<EdgeIndex> = self.graph.edge_indices().collect();
        for idx in edge_indices {
            self.graph.remove_edge(idx);
        }
        for edge in &snapshot.edges {
            if let Some(dest) = edge.dest_pid {
                self.add_connection(edge.source_pid, dest, edge.clone());
            }
        }
    }
}

impl Default for WorldGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConnectionState, NetworkEdge, ProcessNode, ProcessState, Protocol, SystemSnapshot,
    };
    use glam::Vec3;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn make_process(pid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: 10.0,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    fn make_edge(source_pid: u32) -> NetworkEdge {
        NetworkEdge {
            source_pid,
            dest_pid: None,
            dest: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
            protocol: Protocol::TCP,
            bytes_per_sec: 1024,
            state: ConnectionState::Established,
        }
    }

    #[test]
    fn test_add_and_find_process() {
        let mut g = WorldGraph::new();
        let idx = g.add_process(make_process(42));
        assert!(idx.index() < usize::MAX);

        let found = g.find_by_pid(42).unwrap();
        assert_eq!(found.pid, 42);
        assert_eq!(found.name, "proc-42");

        assert!(g.find_by_pid(999).is_none());
    }

    #[test]
    fn test_remove_process() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(10));
        g.add_process(make_process(20));

        assert!(g.remove_process(10));
        assert!(!g.remove_process(10)); // already removed
        assert!(g.find_by_pid(10).is_none());
        assert_eq!(g.process_count(), 1);

        // Remaining process is still accessible.
        assert!(g.find_by_pid(20).is_some());
    }

    #[test]
    fn test_update_process() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(5));

        g.update_process(5, |p| {
            p.cpu_percent = 99.0;
            p.name = "updated".to_string();
        });

        let p = g.find_by_pid(5).unwrap();
        assert_eq!(p.cpu_percent, 99.0);
        assert_eq!(p.name, "updated");

        // Updating a non-existent pid is a no-op.
        g.update_process(999, |p| p.cpu_percent = 0.0);
    }

    #[test]
    fn test_add_connection() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1));
        g.add_process(make_process(2));

        let edge_idx = g.add_connection(1, 2, make_edge(1));
        assert!(edge_idx.is_some());
        assert_eq!(g.edge_count(), 1);

        // Connection with missing pid returns None.
        assert!(g.add_connection(1, 999, make_edge(1)).is_none());
        assert!(g.add_connection(999, 2, make_edge(999)).is_none());
    }

    #[test]
    fn test_process_count() {
        let mut g = WorldGraph::new();
        assert_eq!(g.process_count(), 0);
        assert_eq!(g.edge_count(), 0);

        g.add_process(make_process(1));
        g.add_process(make_process(2));
        g.add_process(make_process(3));
        assert_eq!(g.process_count(), 3);

        g.remove_process(2);
        assert_eq!(g.process_count(), 2);

        // Iterators agree with counts.
        assert_eq!(g.processes().count(), 2);
    }

    #[test]
    fn test_pids_returns_all() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1));
        g.add_process(make_process(2));
        g.add_process(make_process(3));

        let mut pids = g.pids();
        pids.sort();
        assert_eq!(pids, vec![1, 2, 3]);
    }

    #[test]
    fn test_edge_pairs_with_data_returns_edge() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1));
        g.add_process(make_process(2));
        g.add_connection(1, 2, make_edge(1));

        let data = g.edge_pairs_with_data();
        assert_eq!(data.len(), 1, "should have one edge");
        assert_eq!(data[0].0, 1, "source pid");
        assert_eq!(data[0].1, 2, "dest pid");
        assert_eq!(data[0].2.bytes_per_sec, 1024, "edge bytes_per_sec");
    }

    #[test]
    fn test_apply_snapshot() {
        let mut g = WorldGraph::new();
        g.add_process(make_process(1));
        g.add_process(make_process(2));
        g.add_process(make_process(3));

        // Snapshot: pid 2 updated, pid 4 new, pid 1 & 3 gone.
        let mut updated = make_process(2);
        updated.cpu_percent = 77.0;

        let snapshot = SystemSnapshot {
            processes: vec![updated, make_process(4)],
            edges: vec![],
            timestamp: std::time::SystemTime::now(),
        };

        g.apply_snapshot(&snapshot);

        assert_eq!(g.process_count(), 2);
        assert!(g.find_by_pid(1).is_none());
        assert!(g.find_by_pid(3).is_none());
        assert_eq!(g.find_by_pid(2).unwrap().cpu_percent, 77.0);
        assert!(g.find_by_pid(4).is_some());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_apply_snapshot_with_edges() {
        let mut g = WorldGraph::new();

        let mut edge = make_edge(1);
        edge.dest_pid = Some(2);

        let snapshot = SystemSnapshot {
            processes: vec![make_process(1), make_process(2)],
            edges: vec![edge],
            timestamp: std::time::SystemTime::now(),
        };

        g.apply_snapshot(&snapshot);

        assert_eq!(g.process_count(), 2);
        assert_eq!(g.edge_count(), 1, "snapshot edge should be added");

        let pairs = g.edge_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (1, 2), "edge should connect pid 1 → 2, not self-loop");
    }
}

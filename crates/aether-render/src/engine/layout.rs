//! Force-directed graph layout using Fruchterman-Reingold algorithm in 3D.
//!
//! Connected nodes attract, all nodes repel. Temperature-based cooling
//! drives convergence to a stable configuration.

use std::collections::HashMap;

use aether_core::graph::WorldGraph;
use glam::Vec3;

/// 3D force-directed graph layout with Fruchterman-Reingold forces.
pub struct ForceLayout {
    positions: HashMap<u32, Vec3>,
    velocities: HashMap<u32, Vec3>,
    temperature: f32,
    k: f32,
}

impl ForceLayout {
    /// Creates an empty layout with default parameters.
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            velocities: HashMap::new(),
            temperature: 1.0,
            k: 1.0,
        }
    }

    /// Distributes pids randomly on a sphere surface.
    pub fn initial_placement(&mut self, pids: &[u32]) {
        let radius = (pids.len() as f32).sqrt();
        self.k = compute_k(pids.len());

        for (i, &pid) in pids.iter().enumerate() {
            let pos = sphere_point(i, pids.len(), radius);
            self.positions.insert(pid, pos);
            self.velocities.insert(pid, Vec3::ZERO);
        }
        self.temperature = 1.0;
    }

    /// Runs one iteration of the force-directed algorithm.
    pub fn step(&mut self, graph: &WorldGraph) {
        let pids: Vec<u32> = self.positions.keys().copied().collect();
        if pids.len() < 2 {
            return;
        }

        let mut displacements: HashMap<u32, Vec3> =
            pids.iter().map(|&pid| (pid, Vec3::ZERO)).collect();

        self.apply_repulsive_forces(&pids, &mut displacements);
        self.apply_attractive_forces(graph, &mut displacements);
        self.apply_displacements(&pids, &displacements);

        self.temperature *= 0.95;
    }

    /// Returns the position for a given pid.
    pub fn get_position(&self, pid: u32) -> Option<Vec3> {
        self.positions.get(&pid).copied()
    }

    /// Synchronises layout state with the graph: adds new pids, removes dead ones.
    pub fn sync_with_graph(&mut self, graph: &WorldGraph) {
        let live_pids: Vec<u32> = graph.processes().map(|p| p.pid).collect();
        let live_set: std::collections::HashSet<u32> = live_pids.iter().copied().collect();

        // Remove dead pids.
        self.positions.retain(|pid, _| live_set.contains(pid));
        self.velocities.retain(|pid, _| live_set.contains(pid));

        // Add new pids near their parent.
        for proc in graph.processes() {
            if self.positions.contains_key(&proc.pid) {
                continue;
            }
            let parent_pos = self
                .positions
                .get(&proc.ppid)
                .copied()
                .unwrap_or(Vec3::ZERO);
            let jitter = pseudo_random_jitter(proc.pid);
            self.positions.insert(proc.pid, parent_pos + jitter);
            self.velocities.insert(proc.pid, Vec3::ZERO);
        }

        self.k = compute_k(self.positions.len());
    }

    /// Writes layout positions back into each ProcessNode's `position_3d` field.
    pub fn update_graph_positions(&self, graph: &mut WorldGraph) {
        for (&pid, &pos) in &self.positions {
            graph.update_process(pid, |node| {
                node.position_3d = pos;
            });
        }
    }

    /// Runs `step()` for the given number of iterations.
    pub fn converge(&mut self, graph: &WorldGraph, iterations: usize) {
        for _ in 0..iterations {
            self.step(graph);
        }
    }

    fn apply_repulsive_forces(&self, pids: &[u32], displacements: &mut HashMap<u32, Vec3>) {
        let k_sq = self.k * self.k;
        for i in 0..pids.len() {
            for j in (i + 1)..pids.len() {
                let pi = self.positions[&pids[i]];
                let pj = self.positions[&pids[j]];
                let delta = pi - pj;
                let dist = delta.length().max(0.01);
                let force_mag = k_sq / dist;
                let force = delta / dist * force_mag;

                *displacements.get_mut(&pids[i]).expect("pid in map") += force;
                *displacements.get_mut(&pids[j]).expect("pid in map") -= force;
            }
        }
    }

    fn apply_attractive_forces(
        &self,
        graph: &WorldGraph,
        displacements: &mut HashMap<u32, Vec3>,
    ) {
        for (a, b) in graph.edge_pairs() {
            let (Some(&pa), Some(&pb)) = (self.positions.get(&a), self.positions.get(&b)) else {
                continue;
            };
            let delta = pa - pb;
            let dist = delta.length().max(0.01);
            let force_mag = dist * dist / self.k;
            let force = delta / dist * force_mag;

            if let Some(d) = displacements.get_mut(&a) {
                *d -= force;
            }
            if let Some(d) = displacements.get_mut(&b) {
                *d += force;
            }
        }
    }

    fn apply_displacements(&mut self, pids: &[u32], displacements: &HashMap<u32, Vec3>) {
        let damping = 0.9;
        for &pid in pids {
            let disp = displacements[&pid];
            let vel = self.velocities.get_mut(&pid).expect("pid in map");
            *vel = (*vel + disp) * damping;

            let speed = vel.length();
            if speed > self.temperature {
                *vel = *vel / speed * self.temperature;
            }

            let pos = self.positions.get_mut(&pid).expect("pid in map");
            *pos += *vel;
        }
    }
}

impl Default for ForceLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute optimal distance from node count, assuming a unit-volume cube.
fn compute_k(node_count: usize) -> f32 {
    let volume = (node_count as f32).max(1.0);
    (volume / node_count.max(1) as f32).cbrt()
}

/// Deterministic point on a sphere using golden-ratio spiral.
fn sphere_point(index: usize, total: usize, radius: f32) -> Vec3 {
    let golden = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let theta = 2.0 * std::f32::consts::PI * index as f32 / golden;
    let phi = (1.0 - 2.0 * (index as f32 + 0.5) / total.max(1) as f32).acos();
    Vec3::new(
        radius * phi.sin() * theta.cos(),
        radius * phi.sin() * theta.sin(),
        radius * phi.cos(),
    )
}

/// Deterministic jitter based on pid for reproducible placement.
fn pseudo_random_jitter(pid: u32) -> Vec3 {
    let h = pid.wrapping_mul(2654435761);
    let x = ((h & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
    let y = (((h >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
    let z = (((h >> 16) & 0xFF) as f32 / 255.0 - 0.5) * 0.2;
    Vec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState};

    fn make_process(pid: u32, ppid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid,
            name: format!("proc-{pid}"),
            cpu_percent: 10.0,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    fn make_edge(source_pid: u32) -> aether_core::models::NetworkEdge {
        aether_core::models::NetworkEdge {
            source_pid,
            dest: std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                8080,
            ),
            protocol: aether_core::models::Protocol::TCP,
            bytes_per_sec: 1024,
            state: aether_core::models::ConnectionState::Established,
        }
    }

    #[test]
    fn test_connected_nodes_converge() {
        let mut graph = WorldGraph::new();
        graph.add_process(make_process(1, 0));
        graph.add_process(make_process(2, 1));
        graph.add_connection(1, 2, make_edge(1));

        let mut layout = ForceLayout::new();
        layout.initial_placement(&[1, 2]);

        let initial_dist = layout.get_position(1).expect("pid 1").distance(
            layout.get_position(2).expect("pid 2"),
        );

        layout.converge(&graph, 50);

        let final_dist = layout.get_position(1).expect("pid 1").distance(
            layout.get_position(2).expect("pid 2"),
        );

        assert!(
            final_dist < initial_dist,
            "connected nodes should converge: {initial_dist} -> {final_dist}"
        );
    }

    #[test]
    fn test_disconnected_nodes_repel() {
        let mut graph = WorldGraph::new();
        graph.add_process(make_process(1, 0));
        graph.add_process(make_process(2, 0));
        // No edge between them.

        let mut layout = ForceLayout::new();
        // Place close together.
        layout.positions.insert(1, Vec3::new(0.0, 0.0, 0.0));
        layout.positions.insert(2, Vec3::new(0.1, 0.0, 0.0));
        layout.velocities.insert(1, Vec3::ZERO);
        layout.velocities.insert(2, Vec3::ZERO);
        layout.k = compute_k(2);
        layout.temperature = 1.0;

        let initial_dist = layout.get_position(1).expect("1").distance(
            layout.get_position(2).expect("2"),
        );

        layout.converge(&graph, 50);

        let final_dist = layout.get_position(1).expect("1").distance(
            layout.get_position(2).expect("2"),
        );

        assert!(
            final_dist > initial_dist,
            "disconnected nodes should repel: {initial_dist} -> {final_dist}"
        );
    }

    #[test]
    fn test_sync_adds_new_and_removes_dead() {
        let mut graph = WorldGraph::new();
        graph.add_process(make_process(1, 0));
        graph.add_process(make_process(2, 1));

        let mut layout = ForceLayout::new();
        layout.initial_placement(&[1, 2]);

        // Add pid 3, remove pid 2 from graph.
        graph.add_process(make_process(3, 1));
        graph.remove_process(2);

        layout.sync_with_graph(&graph);

        assert!(layout.get_position(1).is_some(), "pid 1 should remain");
        assert!(layout.get_position(2).is_none(), "pid 2 should be removed");
        assert!(layout.get_position(3).is_some(), "pid 3 should be added");
    }

    #[test]
    fn test_update_graph_positions_writes_back() {
        let mut graph = WorldGraph::new();
        graph.add_process(make_process(1, 0));
        graph.add_process(make_process(2, 1));

        let mut layout = ForceLayout::new();
        layout.initial_placement(&[1, 2]);
        layout.converge(&graph, 10);

        layout.update_graph_positions(&mut graph);

        let p1 = graph.find_by_pid(1).expect("pid 1 exists");
        let p2 = graph.find_by_pid(2).expect("pid 2 exists");

        assert_eq!(p1.position_3d, layout.get_position(1).expect("layout has pid 1"));
        assert_eq!(p2.position_3d, layout.get_position(2).expect("layout has pid 2"));
    }

    #[test]
    fn test_new_process_placed_near_parent() {
        let mut graph = WorldGraph::new();
        graph.add_process(make_process(1, 0));

        let mut layout = ForceLayout::new();
        layout.initial_placement(&[1]);

        // Add child process.
        graph.add_process(make_process(10, 1));
        layout.sync_with_graph(&graph);

        let parent_pos = layout.get_position(1).expect("parent");
        let child_pos = layout.get_position(10).expect("child");

        assert!(
            parent_pos.distance(child_pos) < 0.5,
            "child should be placed near parent: dist = {}",
            parent_pos.distance(child_pos)
        );
    }

    #[test]
    fn test_initial_placement_distributes_in_3d() {
        let mut layout = ForceLayout::new();
        layout.initial_placement(&[1, 2, 3, 4, 5]);

        let all_positions: Vec<Vec3> = (1..=5)
            .map(|pid| layout.get_position(pid).expect("placed"))
            .collect();

        // No position should be at the origin.
        for pos in &all_positions {
            assert!(pos.length() > 0.1, "node should not be at origin: {pos}");
        }

        // Not all at the same point.
        let first = all_positions[0];
        let all_same = all_positions.iter().all(|p| p.distance(first) < 0.01);
        assert!(!all_same, "nodes should not all be at the same position");
    }
}

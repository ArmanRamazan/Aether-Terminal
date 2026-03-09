//! HP calculation engine — determines health point changes based on process metrics.

use aether_core::graph::WorldGraph;
use aether_core::models::{ProcessNode, ProcessState};

/// Stateless calculator for process HP deltas.
pub struct HpEngine;

impl HpEngine {
    /// Calculate HP change for a process given its current and previous state.
    ///
    /// Rules applied in order:
    /// - Zombie → instant death (HP to 0)
    /// - CPU > 90% → -2.0/s
    /// - Memory growth > 5%/min → -1.0/s
    /// - Healthy (no anomalies) → +0.5/s regeneration
    ///
    /// Result is clamped so that `current.hp + delta` stays in `0.0..=100.0`.
    #[must_use]
    pub fn calculate_hp_delta(
        current: &ProcessNode,
        previous: &ProcessNode,
        dt_secs: f32,
    ) -> f32 {
        if current.state == ProcessState::Zombie {
            return -current.hp;
        }

        let mut delta = 0.0_f32;
        let mut has_anomaly = false;

        if current.cpu_percent > 90.0 {
            delta -= 2.0 * dt_secs;
            has_anomaly = true;
        }

        if previous.mem_bytes > 0 {
            let growth = (current.mem_bytes as f64 - previous.mem_bytes as f64)
                / previous.mem_bytes as f64;
            // Convert per-tick growth to per-minute rate.
            let growth_per_min = if dt_secs > 0.0 {
                growth * (60.0 / dt_secs as f64)
            } else {
                0.0
            };
            if growth_per_min > 0.05 {
                delta -= 1.0 * dt_secs;
                has_anomaly = true;
            }
        }

        if !has_anomaly {
            delta += 0.5 * dt_secs;
        }

        // Clamp so HP stays in 0.0..=100.0.
        let new_hp = current.hp + delta;
        if new_hp > 100.0 {
            100.0 - current.hp
        } else if new_hp < 0.0 {
            -current.hp
        } else {
            delta
        }
    }

    /// Apply HP deltas to all processes in the graph.
    ///
    /// Processes not present in `prev_graph` are treated as new (HP stays at 100.0).
    pub fn apply_to_graph(graph: &mut WorldGraph, prev_graph: &WorldGraph, dt_secs: f32) {
        let pids = graph.pids();
        for pid in pids {
            let Some(current) = graph.find_by_pid(pid) else {
                continue;
            };
            let Some(previous) = prev_graph.find_by_pid(pid) else {
                // New process — HP stays at 100.0.
                continue;
            };
            let delta = Self::calculate_hp_delta(current, previous, dt_secs);
            graph.update_process(pid, |node| {
                node.hp = (node.hp + delta).clamp(0.0, 100.0);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn make_process(pid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: 10.0,
            mem_bytes: 1_000_000,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_high_cpu_loses_hp() {
        let mut current = make_process(1);
        current.cpu_percent = 95.0;
        let previous = make_process(1);

        let delta = HpEngine::calculate_hp_delta(&current, &previous, 1.0);
        assert!(delta < 0.0, "high CPU should reduce HP, got {delta}");
        assert_eq!(delta, -2.0, "CPU > 90% should deal -2.0/s");
    }

    #[test]
    fn test_memory_leak_loses_hp() {
        let previous = make_process(1);
        let mut current = make_process(1);
        // 10% growth in 1 second = 600%/min, well above 5%/min threshold.
        current.mem_bytes = 1_100_000;

        let delta = HpEngine::calculate_hp_delta(&current, &previous, 1.0);
        assert!(delta < 0.0, "memory leak should reduce HP, got {delta}");
        assert_eq!(delta, -1.0, "memory growth > 5%/min should deal -1.0/s");
    }

    #[test]
    fn test_zombie_goes_to_zero() {
        let mut current = make_process(1);
        current.state = ProcessState::Zombie;
        current.hp = 75.0;
        let previous = make_process(1);

        let delta = HpEngine::calculate_hp_delta(&current, &previous, 1.0);
        assert_eq!(delta, -75.0, "zombie should lose all HP");
        assert_eq!(current.hp + delta, 0.0, "zombie HP should become 0");
    }

    #[test]
    fn test_healthy_regenerates_capped() {
        let current = make_process(1);
        let previous = make_process(1);

        let delta = HpEngine::calculate_hp_delta(&current, &previous, 1.0);
        assert_eq!(delta, 0.0, "already at 100 HP, regen should be clamped to 0");

        let mut damaged = make_process(1);
        damaged.hp = 80.0;
        let delta = HpEngine::calculate_hp_delta(&damaged, &previous, 1.0);
        assert_eq!(delta, 0.5, "healthy process should regen +0.5/s");
    }

    #[test]
    fn test_new_process_stays_at_100() {
        let mut graph = WorldGraph::new();
        let prev_graph = WorldGraph::new();
        graph.add_process(make_process(1));

        HpEngine::apply_to_graph(&mut graph, &prev_graph, 1.0);

        let node = graph.find_by_pid(1).expect("process should exist");
        assert_eq!(node.hp, 100.0, "new process HP should stay at 100");
    }

    #[test]
    fn test_apply_to_graph_updates_hp() {
        let mut prev_graph = WorldGraph::new();
        prev_graph.add_process(make_process(1));

        let mut graph = WorldGraph::new();
        let mut high_cpu = make_process(1);
        high_cpu.cpu_percent = 95.0;
        graph.add_process(high_cpu);

        HpEngine::apply_to_graph(&mut graph, &prev_graph, 1.0);

        let node = graph.find_by_pid(1).expect("process should exist");
        assert_eq!(node.hp, 98.0, "HP should be 100 - 2.0");
    }

    #[test]
    fn test_hp_never_below_zero() {
        let previous = make_process(1);
        let mut current = make_process(1);
        current.cpu_percent = 95.0;
        current.hp = 0.5;

        let delta = HpEngine::calculate_hp_delta(&current, &previous, 1.0);
        assert_eq!(
            current.hp + delta,
            0.0,
            "HP should clamp to 0, not go negative"
        );
    }
}

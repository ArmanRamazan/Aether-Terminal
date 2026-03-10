//! Per-process feature extraction and min-max normalization.
//!
//! Extracts a 9-dimensional feature vector from each process in the world graph,
//! then normalizes all values to \[0, 1\] using min-max scaling across the batch.

use std::collections::HashMap;

use aether_core::graph::WorldGraph;

/// 9-dimensional feature vector per process.
///
/// Indices: \[cpu_pct, mem_bytes, mem_delta, fd_count, thread_count,
///           net_bytes_in, net_bytes_out, syscall_rate, io_wait_pct\]
pub type FeatureVector = [f32; 9];

/// Tracks per-dimension min/max for normalization.
#[derive(Debug, Clone)]
pub(crate) struct RunningStats {
    min: [f32; 9],
    max: [f32; 9],
}

impl RunningStats {
    /// Creates stats initialized to extreme values that will be overwritten on first observation.
    fn new() -> Self {
        Self {
            min: [f32::MAX; 9],
            max: [f32::MIN; 9],
        }
    }

    /// Updates min/max from a single raw feature vector.
    fn observe(&mut self, features: &FeatureVector) {
        for (i, &val) in features.iter().enumerate() {
            if val < self.min[i] {
                self.min[i] = val;
            }
            if val > self.max[i] {
                self.max[i] = val;
            }
        }
    }

    /// Normalizes a raw feature vector to \[0, 1\] using observed min/max.
    /// If min == max for a dimension, that dimension is set to 0.0.
    fn normalize(&self, features: &FeatureVector) -> FeatureVector {
        let mut out = [0.0_f32; 9];
        for i in 0..9 {
            let range = self.max[i] - self.min[i];
            if range > f32::EPSILON {
                out[i] = (features[i] - self.min[i]) / range;
            }
        }
        out
    }
}

/// Extracts and normalizes feature vectors from a [`WorldGraph`].
///
/// Tracks previous memory values per-pid to compute `mem_delta`.
#[derive(Debug)]
pub struct FeatureExtractor {
    prev_mem: HashMap<u32, u64>,
}

impl FeatureExtractor {
    /// Creates a new extractor with no history.
    pub fn new() -> Self {
        Self {
            prev_mem: HashMap::new(),
        }
    }

    /// Extracts a normalized feature vector for every process in the graph.
    ///
    /// Features not yet available on `ProcessNode` (fd_count, thread_count,
    /// net_in, net_out, syscall_rate, io_wait) default to 0.0 and will be
    /// populated once the ingestion pipeline provides them.
    pub fn extract(&mut self, world: &WorldGraph) -> HashMap<u32, FeatureVector> {
        if world.process_count() == 0 {
            self.prev_mem.clear();
            return HashMap::new();
        }

        // Phase 1: extract raw features and collect min/max.
        let mut stats = RunningStats::new();
        let mut raw: Vec<(u32, FeatureVector)> =
            Vec::with_capacity(world.process_count());

        for proc in world.processes() {
            let mem_delta = self
                .prev_mem
                .get(&proc.pid)
                .map(|&prev| proc.mem_bytes as f32 - prev as f32)
                .unwrap_or(0.0);

            let fv: FeatureVector = [
                proc.cpu_percent,
                proc.mem_bytes as f32,
                mem_delta,
                0.0, // fd_count — not yet in ProcessNode
                0.0, // thread_count
                0.0, // net_bytes_in
                0.0, // net_bytes_out
                0.0, // syscall_rate
                0.0, // io_wait_pct
            ];

            stats.observe(&fv);
            raw.push((proc.pid, fv));
        }

        // Update previous memory snapshot.
        self.prev_mem.clear();
        for &(pid, ref fv) in &raw {
            self.prev_mem.insert(pid, fv[1] as u64);
        }

        // Phase 2: normalize.
        let mut result = HashMap::with_capacity(raw.len());
        for (pid, fv) in &raw {
            result.insert(*pid, stats.normalize(fv));
        }

        result
    }
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState};
    use glam::Vec3;

    fn make_process(pid: u32, cpu: f32, mem: u64) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_extract_features_from_sample_world() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 50.0, 1024));
        world.add_process(make_process(2, 80.0, 2048));

        let mut extractor = FeatureExtractor::new();
        let features = extractor.extract(&world);

        assert_eq!(features.len(), 2, "should have one vector per process");
        assert!(features.contains_key(&1));
        assert!(features.contains_key(&2));

        // Each vector has 9 elements.
        assert_eq!(features[&1].len(), 9);
        assert_eq!(features[&2].len(), 9);
    }

    #[test]
    fn test_features_in_zero_one_range() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 10.0, 500));
        world.add_process(make_process(2, 90.0, 4000));
        world.add_process(make_process(3, 50.0, 2000));

        let mut extractor = FeatureExtractor::new();
        let features = extractor.extract(&world);

        for (_, fv) in &features {
            for (i, &val) in fv.iter().enumerate() {
                assert!(
                    (0.0..=1.0).contains(&val),
                    "feature[{i}] = {val} out of [0, 1]"
                );
            }
        }

        // The process with min cpu (10.0) should normalize to 0.0 on dimension 0.
        assert!(
            (features[&1][0]).abs() < f32::EPSILON,
            "min cpu should normalize to 0"
        );
        // The process with max cpu (90.0) should normalize to 1.0 on dimension 0.
        assert!(
            (features[&2][0] - 1.0).abs() < f32::EPSILON,
            "max cpu should normalize to 1"
        );
    }

    #[test]
    fn test_delta_computed_between_snapshots() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 50.0, 1000));

        let mut extractor = FeatureExtractor::new();

        // First extraction: no previous data, mem_delta = 0.
        let first = extractor.extract(&world);
        // With a single process, all features normalize to 0 (min == max).
        assert!(
            first[&1][2].abs() < f32::EPSILON,
            "first snapshot mem_delta should be 0"
        );

        // Update memory and extract again.
        world.update_process(1, |p| p.mem_bytes = 3000);
        world.add_process(make_process(2, 50.0, 1000));

        let second = extractor.extract(&world);

        // pid 1 had mem go from 1000 → 3000, delta = 2000.
        // pid 2 has no history, delta = 0.
        // After normalization, pid 1 should have higher mem_delta than pid 2.
        assert!(
            second[&1][2] > second[&2][2],
            "pid 1 mem_delta ({}) should exceed pid 2 ({})",
            second[&1][2],
            second[&2][2]
        );
    }

    #[test]
    fn test_empty_world_returns_empty_map() {
        let world = WorldGraph::new();
        let mut extractor = FeatureExtractor::new();
        let features = extractor.extract(&world);
        assert!(features.is_empty(), "empty world should yield empty map");
    }
}

//! In-memory time-series metric storage with per-host, per-process granularity.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use aether_core::graph::WorldGraph;
use aether_core::metrics::{HostId, MetricSample, TimeSeries};

/// Composite key for looking up a specific metric series.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(crate) struct MetricKey {
    pub(crate) host: HostId,
    pub(crate) pid: Option<u32>,
    pub(crate) metric: String,
}

/// Bounded in-memory store for time-series metrics, indexed by host/pid/metric name.
#[derive(Debug)]
pub struct MetricStore {
    series: HashMap<MetricKey, TimeSeries>,
    capacity: usize,
}

impl MetricStore {
    /// Creates a new store where each series holds up to `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            series: HashMap::new(),
            capacity,
        }
    }

    /// Ingests current state from a WorldGraph, creating per-process and host-level series.
    pub fn ingest_world_state(&mut self, host: &HostId, world: &WorldGraph) {
        let now = Instant::now();
        let mut total_cpu: f64 = 0.0;
        let mut total_mem: u64 = 0;

        for proc in world.processes() {
            let cpu = proc.cpu_percent as f64;
            let mem = proc.mem_bytes;
            total_cpu += cpu;
            total_mem += mem;

            self.push_sample(host, Some(proc.pid), "cpu_percent", now, cpu);
            self.push_sample(host, Some(proc.pid), "mem_bytes", now, mem as f64);
        }

        self.push_sample(host, None, "total_cpu", now, total_cpu);
        self.push_sample(host, None, "total_memory", now, total_mem as f64);
        self.push_sample(
            host,
            None,
            "process_count",
            now,
            world.process_count() as f64,
        );
    }

    /// Merges externally-produced time series by parsing their labels into MetricKeys.
    pub fn ingest_remote(&mut self, series: Vec<TimeSeries>) {
        for ts in series {
            let host = ts
                .labels
                .get("host")
                .map(|h| HostId::new(h.as_str()))
                .unwrap_or_default();
            let pid = ts.labels.get("pid").and_then(|p| p.parse::<u32>().ok());
            let metric = ts
                .labels
                .get("__name__")
                .cloned()
                .unwrap_or_else(|| ts.name.clone());

            let key = MetricKey { host, pid, metric };
            let dest = self
                .series
                .entry(key)
                .or_insert_with(|| TimeSeries::new(&ts.name, self.capacity));
            for sample in &ts.samples {
                dest.push_sample(*sample);
            }
        }
    }

    /// Looks up a specific series by host, optional pid, and metric name.
    pub fn get(&self, host: &HostId, pid: Option<u32>, metric: &str) -> Option<&TimeSeries> {
        let key = MetricKey {
            host: host.clone(),
            pid,
            metric: metric.to_string(),
        };
        self.series.get(&key)
    }

    /// Returns all metric series for a specific process on a host.
    pub fn process_metrics(&self, host: &HostId, pid: u32) -> Vec<(&str, &TimeSeries)> {
        self.series
            .iter()
            .filter(|(k, _)| &k.host == host && k.pid == Some(pid))
            .map(|(k, v)| (k.metric.as_str(), v))
            .collect()
    }

    /// Returns all host-level (pid=None) metric series for a host.
    pub fn host_metrics(&self, host: &HostId) -> Vec<(&str, &TimeSeries)> {
        self.series
            .iter()
            .filter(|(k, _)| &k.host == host && k.pid.is_none())
            .map(|(k, v)| (k.metric.as_str(), v))
            .collect()
    }

    /// Returns all distinct process IDs for a given host.
    pub fn process_pids(&self, host: &HostId) -> HashSet<u32> {
        self.series
            .keys()
            .filter(|k| &k.host == host)
            .filter_map(|k| k.pid)
            .collect()
    }

    /// Returns all distinct host IDs present in the store.
    pub fn hosts(&self) -> Vec<&HostId> {
        let mut seen = HashSet::new();
        self.series
            .keys()
            .filter_map(|k| {
                if seen.insert(&k.host) {
                    Some(&k.host)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Removes all per-process series for pids NOT in `alive_pids`.
    pub fn cleanup_dead_processes(&mut self, host: &HostId, alive_pids: &HashSet<u32>) {
        self.series.retain(|k, _| {
            if &k.host != host {
                return true;
            }
            match k.pid {
                Some(pid) => alive_pids.contains(&pid),
                None => true,
            }
        });
    }

    pub(crate) fn push_sample(
        &mut self,
        host: &HostId,
        pid: Option<u32>,
        metric: &str,
        timestamp: Instant,
        value: f64,
    ) {
        let key = MetricKey {
            host: host.clone(),
            pid,
            metric: metric.to_string(),
        };
        let ts = self
            .series
            .entry(key)
            .or_insert_with(|| TimeSeries::new(metric, self.capacity));
        ts.push_sample(MetricSample { timestamp, value });
    }
}

impl Default for MetricStore {
    fn default() -> Self {
        Self::new(3600)
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

    fn make_world(processes: Vec<ProcessNode>) -> WorldGraph {
        let mut world = WorldGraph::new();
        for p in processes {
            world.add_process(p);
        }
        world
    }

    #[test]
    fn test_ingest_world_state_creates_series() {
        let host = HostId::new("local");
        let world = make_world(vec![make_process(100, 25.0, 1024)]);

        let mut store = MetricStore::new(100);
        store.ingest_world_state(&host, &world);

        assert!(store.get(&host, Some(100), "cpu_percent").is_some());
        assert!(store.get(&host, Some(100), "mem_bytes").is_some());
        assert!(store.get(&host, None, "total_cpu").is_some());
        assert!(store.get(&host, None, "total_memory").is_some());
        assert!(store.get(&host, None, "process_count").is_some());

        let cpu = store.get(&host, Some(100), "cpu_percent").unwrap();
        assert_eq!(cpu.last().map(|s| s.value), Some(25.0));
    }

    #[test]
    fn test_ingest_multiple_builds_history() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);

        for _ in 0..5 {
            let world = make_world(vec![make_process(1, 10.0, 512)]);
            store.ingest_world_state(&host, &world);
        }

        let ts = store.get(&host, Some(1), "cpu_percent").unwrap();
        assert_eq!(ts.len(), 5);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = MetricStore::new(100);
        let host = HostId::new("ghost");
        assert!(store.get(&host, Some(999), "cpu_percent").is_none());
    }

    #[test]
    fn test_cleanup_removes_dead() {
        let host = HostId::new("local");
        let world = make_world(vec![make_process(100, 10.0, 256)]);

        let mut store = MetricStore::new(100);
        store.ingest_world_state(&host, &world);
        assert!(store.get(&host, Some(100), "cpu_percent").is_some());

        store.cleanup_dead_processes(&host, &HashSet::new());
        assert!(store.get(&host, Some(100), "cpu_percent").is_none());
        // Host-level metrics survive cleanup.
        assert!(store.get(&host, None, "total_cpu").is_some());
    }

    #[test]
    fn test_host_metrics_returns_aggregates() {
        let host = HostId::new("local");
        let world = make_world(vec![make_process(1, 10.0, 100), make_process(2, 20.0, 200)]);

        let mut store = MetricStore::new(100);
        store.ingest_world_state(&host, &world);

        let metrics = store.host_metrics(&host);
        let names: HashSet<&str> = metrics.iter().map(|(n, _)| *n).collect();
        assert!(names.contains("total_cpu"));
        assert!(names.contains("total_memory"));
        assert!(names.contains("process_count"));

        let total_cpu = store.get(&host, None, "total_cpu").unwrap();
        assert_eq!(total_cpu.last().map(|s| s.value), Some(30.0));

        let total_mem = store.get(&host, None, "total_memory").unwrap();
        assert_eq!(total_mem.last().map(|s| s.value), Some(300.0));

        let count = store.get(&host, None, "process_count").unwrap();
        assert_eq!(count.last().map(|s| s.value), Some(2.0));
    }
}

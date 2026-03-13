use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

use aether_core::models::Diagnostic;
use aether_core::{ArbiterQueue, WorldGraph};

/// A single time-stamped metric observation for the web API.
#[derive(Debug, Clone, Copy)]
pub struct MetricSample {
    pub timestamp_ms: u64,
    pub value: f64,
}

/// Stores recent time-series data for named metrics.
#[derive(Debug)]
pub struct MetricStore {
    series: HashMap<String, VecDeque<MetricSample>>,
    capacity: usize,
}

impl MetricStore {
    /// Create a store that keeps up to `capacity` samples per metric.
    pub fn new(capacity: usize) -> Self {
        Self {
            series: HashMap::new(),
            capacity,
        }
    }

    /// Append a sample for a named metric.
    pub fn push(&mut self, metric: &str, timestamp_ms: u64, value: f64) {
        let series = self
            .series
            .entry(metric.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.capacity.min(256)));
        if series.len() >= self.capacity {
            series.pop_front();
        }
        series.push_back(MetricSample {
            timestamp_ms,
            value,
        });
    }

    /// Get samples for a metric within the last `duration_secs` seconds.
    pub fn history(&self, metric: &str, duration_secs: u64) -> Vec<MetricSample> {
        let Some(series) = self.series.get(metric) else {
            return Vec::new();
        };
        if duration_secs == 0 {
            return series.iter().copied().collect();
        }
        let Some(latest) = series.back() else {
            return Vec::new();
        };
        let cutoff = latest.timestamp_ms.saturating_sub(duration_secs * 1000);
        series
            .iter()
            .filter(|s| s.timestamp_ms >= cutoff)
            .copied()
            .collect()
    }
}

impl Default for MetricStore {
    fn default() -> Self {
        Self::new(3600)
    }
}

/// System-level metrics not tied to individual processes.
#[derive(Debug, Clone, Copy)]
pub struct SystemMetrics {
    pub memory_total_bytes: u64,
    pub load_avg: [f64; 3],
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            memory_total_bytes: 0,
            load_avg: [0.0; 3],
        }
    }
}

/// Shared application state passed to axum handlers.
///
/// All fields are `Arc`-wrapped, so cloning is cheap.
#[derive(Clone)]
pub struct SharedState {
    pub(crate) world: Arc<RwLock<WorldGraph>>,
    pub(crate) arbiter: Arc<Mutex<ArbiterQueue>>,
    pub(crate) diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    pub(crate) metrics: Arc<Mutex<MetricStore>>,
    pub(crate) system_metrics: Arc<RwLock<SystemMetrics>>,
}

impl SharedState {
    /// Create shared state from pre-existing Arc handles.
    pub fn new(
        world: Arc<RwLock<WorldGraph>>,
        arbiter: Arc<Mutex<ArbiterQueue>>,
        diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    ) -> Self {
        Self {
            world,
            arbiter,
            diagnostics,
            metrics: Arc::new(Mutex::new(MetricStore::default())),
            system_metrics: Arc::new(RwLock::new(SystemMetrics::default())),
        }
    }

    /// Update system-level metrics (memory total, load average).
    pub fn update_system_metrics(&self, memory_total_bytes: u64, load_avg: [f64; 3]) {
        if let Ok(mut m) = self.system_metrics.write() {
            m.memory_total_bytes = memory_total_bytes;
            m.load_avg = load_avg;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_state_clone_is_same_arc() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let state = SharedState::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            Arc::clone(&diagnostics),
        );

        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.world, &cloned.world), "clone shares same world Arc");
        assert!(Arc::ptr_eq(&state.arbiter, &cloned.arbiter), "clone shares same arbiter Arc");
        assert!(
            Arc::ptr_eq(&state.diagnostics, &cloned.diagnostics),
            "clone shares same diagnostics Arc"
        );
        assert!(
            Arc::ptr_eq(&state.metrics, &cloned.metrics),
            "clone shares same metrics Arc"
        );
        assert!(
            Arc::ptr_eq(&state.system_metrics, &cloned.system_metrics),
            "clone shares same system_metrics Arc"
        );
    }

    #[test]
    fn test_metric_store_push_and_history() {
        let mut store = MetricStore::new(100);
        store.push("cpu", 1000, 50.0);
        store.push("cpu", 2000, 60.0);
        store.push("cpu", 3000, 70.0);

        let history = store.history("cpu", 5);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].value, 50.0);
        assert_eq!(history[2].value, 70.0);
    }

    #[test]
    fn test_metric_store_capacity_eviction() {
        let mut store = MetricStore::new(3);
        for i in 0..5 {
            store.push("mem", i * 1000, i as f64);
        }
        let history = store.history("mem", 0);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].value, 2.0);
    }

    #[test]
    fn test_metric_store_history_duration_filter() {
        let mut store = MetricStore::new(100);
        store.push("cpu", 1000, 10.0);
        store.push("cpu", 3000, 20.0);
        store.push("cpu", 5000, 30.0);

        // Duration 3s = 3000ms window from latest (5000). Cutoff = 2000.
        let history = store.history("cpu", 3);
        assert_eq!(history.len(), 2, "should include samples at 3000 and 5000");
    }

    #[test]
    fn test_metric_store_unknown_metric_returns_empty() {
        let store = MetricStore::new(100);
        assert!(store.history("unknown", 60).is_empty());
    }
}

//! Time-series metric types for system telemetry.

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Identifies a host in a multi-node topology.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostId(String);

impl HostId {
    /// Creates a new host identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for HostId {
    fn default() -> Self {
        Self("local".to_string())
    }
}

/// A single metric observation at a point in time.
#[derive(Debug, Clone, Copy)]
pub struct MetricSample {
    pub timestamp: Instant,
    pub value: f64,
}

/// A named series of metric samples with bounded capacity.
#[derive(Debug, Clone)]
pub struct TimeSeries {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub samples: VecDeque<MetricSample>,
    pub capacity: usize,
}

impl TimeSeries {
    /// Creates an empty time series with the given name and capacity.
    pub fn new(name: impl Into<String>, capacity: usize) -> Self {
        Self {
            name: name.into(),
            labels: BTreeMap::new(),
            samples: VecDeque::with_capacity(capacity.min(256)),
            capacity,
        }
    }

    /// Appends a sample, evicting the oldest if at capacity.
    pub fn push(&mut self, sample: MetricSample) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Returns the most recent sample value, if any.
    pub fn last_value(&self) -> Option<f64> {
        self.samples.back().map(|s| s.value)
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns true if no samples have been recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_id_default_is_local() {
        let host = HostId::default();
        assert_eq!(host.as_str(), "local");
    }

    #[test]
    fn test_host_id_equality() {
        let a = HostId::new("node-1");
        let b = HostId::new("node-1");
        assert_eq!(a, b);
    }

    #[test]
    fn test_time_series_push_and_len() {
        let mut ts = TimeSeries::new("cpu", 3);
        assert!(ts.is_empty());

        for i in 0..3 {
            ts.push(MetricSample {
                timestamp: Instant::now(),
                value: i as f64,
            });
        }
        assert_eq!(ts.len(), 3);
        assert_eq!(ts.last_value(), Some(2.0));
    }

    #[test]
    fn test_time_series_evicts_oldest() {
        let mut ts = TimeSeries::new("mem", 2);
        for i in 0..5 {
            ts.push(MetricSample {
                timestamp: Instant::now(),
                value: i as f64,
            });
        }
        assert_eq!(ts.len(), 2);
        assert_eq!(ts.samples[0].value, 3.0);
        assert_eq!(ts.samples[1].value, 4.0);
    }
}

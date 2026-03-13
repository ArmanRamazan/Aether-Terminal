//! Time-series metric types for system telemetry.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Identifies a host in a multi-node topology.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostId(pub String);

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

impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A single metric observation at a point in time.
#[derive(Debug, Clone, Copy)]
pub struct MetricSample {
    pub timestamp: Instant,
    pub value: f64,
}

/// Default capacity for a new time series (1 hour at 1 sample/sec).
const DEFAULT_CAPACITY: usize = 3600;

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
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Appends a sample with the current timestamp, evicting the oldest if at capacity.
    pub fn push(&mut self, value: f64) {
        self.push_sample(MetricSample {
            timestamp: Instant::now(),
            value,
        });
    }

    /// Appends a pre-built sample, evicting the oldest if at capacity.
    pub fn push_sample(&mut self, sample: MetricSample) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Returns the most recent sample, if any.
    pub fn last(&self) -> Option<&MetricSample> {
        self.samples.back()
    }

    /// Computes the rate of change over the given window: (last - first) / elapsed.
    pub fn rate(&self, window: Duration) -> Option<f64> {
        let samples = self.samples_in_window(window);
        if samples.len() < 2 {
            return None;
        }
        let first = samples[0];
        let last = samples[samples.len() - 1];
        let elapsed = last.timestamp.duration_since(first.timestamp).as_secs_f64();
        if elapsed <= 0.0 {
            return None;
        }
        Some((last.value - first.value) / elapsed)
    }

    /// Average value of samples within the given window.
    pub fn avg(&self, window: Duration) -> f64 {
        let samples = self.samples_in_window(window);
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples.iter().map(|s| s.value).sum();
        sum / samples.len() as f64
    }

    /// Returns (min, max) of samples within the given window.
    pub fn min_max(&self, window: Duration) -> (f64, f64) {
        let samples = self.samples_in_window(window);
        if samples.is_empty() {
            return (0.0, 0.0);
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for s in &samples {
            if s.value < min {
                min = s.value;
            }
            if s.value > max {
                max = s.value;
            }
        }
        (min, max)
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns true if no samples have been recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    fn samples_in_window(&self, window: Duration) -> Vec<&MetricSample> {
        let Some(latest) = self.samples.back() else {
            return Vec::new();
        };
        let cutoff = latest.timestamp - window;
        self.samples
            .iter()
            .filter(|s| s.timestamp >= cutoff)
            .collect()
    }
}

impl Default for TimeSeries {
    fn default() -> Self {
        Self::new("", DEFAULT_CAPACITY)
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
    fn test_host_id_display() {
        let host = HostId::new("node-42");
        assert_eq!(host.to_string(), "node-42");
    }

    #[test]
    fn test_host_id_equality() {
        let a = HostId::new("node-1");
        let b = HostId::new("node-1");
        assert_eq!(a, b);
    }

    #[test]
    fn test_timeseries_push_and_rate() {
        let mut ts = TimeSeries::new("cpu", 100);
        let start = Instant::now();
        for i in 0..10 {
            ts.samples.push_back(MetricSample {
                timestamp: start + Duration::from_secs(i),
                value: i as f64 * 10.0,
            });
        }
        assert_eq!(ts.len(), 10);
        let rate = ts.rate(Duration::from_secs(60)).expect("should have rate");
        assert!((rate - 10.0).abs() < 0.01, "rate should be ~10.0, got {rate}");
    }

    #[test]
    fn test_timeseries_capacity_evicts_oldest() {
        let mut ts = TimeSeries::new("mem", 3);
        for i in 0..5 {
            ts.push(i as f64);
        }
        assert_eq!(ts.len(), 3, "should evict oldest to stay at capacity");
    }

    #[test]
    fn test_timeseries_avg_window() {
        let mut ts = TimeSeries::new("test", 100);
        let start = Instant::now();
        for (i, val) in [10.0, 20.0, 30.0, 40.0, 50.0].iter().enumerate() {
            ts.samples.push_back(MetricSample {
                timestamp: start + Duration::from_secs(i as u64),
                value: *val,
            });
        }
        let avg = ts.avg(Duration::from_secs(60));
        assert!((avg - 30.0).abs() < 0.01, "avg should be ~30.0, got {avg}");
    }

    #[test]
    fn test_timeseries_push_and_len() {
        let mut ts = TimeSeries::new("cpu", 3);
        assert!(ts.is_empty());

        for i in 0..3 {
            ts.push(i as f64);
        }
        assert_eq!(ts.len(), 3);
        assert!(ts.last().is_some());
        assert_eq!(ts.last().expect("has samples").value, 2.0);
    }

    #[test]
    fn test_timeseries_evicts_oldest() {
        let mut ts = TimeSeries::new("mem", 2);
        for i in 0..5 {
            ts.push(i as f64);
        }
        assert_eq!(ts.len(), 2);
        assert_eq!(ts.samples[0].value, 3.0);
        assert_eq!(ts.samples[1].value, 4.0);
    }
}

//! Deterministic anomaly detection for metric time series.

use std::time::Duration;

use aether_core::metrics::TimeSeries;

/// A detected change point in a time series.
#[derive(Debug, Clone)]
pub struct ChangePoint {
    pub index: usize,
    pub magnitude: f64,
}

/// Stateless anomaly detector operating on `TimeSeries` data.
#[derive(Debug)]
pub struct AnomalyDetector;

impl AnomalyDetector {
    /// Z-score of the last value relative to the windowed mean and stddev.
    ///
    /// Returns 0.0 if stddev is near zero or the series is empty.
    pub fn z_score(&self, series: &TimeSeries, window: Duration) -> f64 {
        let samples = windowed_values(series, window);
        if samples.is_empty() {
            return 0.0;
        }

        let last_value = samples[samples.len() - 1];
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let stddev = compute_stddev(&samples);

        if stddev < 1e-12 {
            return 0.0;
        }

        (last_value - mean) / stddev
    }

    /// Returns true if the absolute z-score exceeds the given threshold.
    pub fn is_outlier_zscore(&self, series: &TimeSeries, threshold: f64) -> bool {
        let z = self.z_score(series, Duration::from_secs(300));
        z.abs() > threshold
    }

    /// Detects outliers using the interquartile range method.
    ///
    /// The last value is an outlier if it falls outside Q1 - 1.5*IQR .. Q3 + 1.5*IQR.
    pub fn is_outlier_iqr(&self, series: &TimeSeries) -> bool {
        if series.samples.len() < 4 {
            return false;
        }

        let mut values: Vec<f64> = series.samples.iter().map(|s| s.value).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let q1 = interpolated_percentile(&values, 0.25);
        let q3 = interpolated_percentile(&values, 0.75);
        let iqr = q3 - q1;

        let last_value = series.samples.back().unwrap().value;
        last_value > q3 + 1.5 * iqr || last_value < q1 - 1.5 * iqr
    }

    /// Detects change points using a sliding window comparison.
    ///
    /// Compares the mean of a left window (size=10) against a right window.
    /// Reports a change point when the difference exceeds `sensitivity * global_stddev`.
    pub fn change_points(&self, series: &TimeSeries, sensitivity: f64) -> Vec<ChangePoint> {
        let values: Vec<f64> = series.samples.iter().map(|s| s.value).collect();
        let window_size = 10;

        if values.len() < window_size * 2 {
            return Vec::new();
        }

        let global_stddev = compute_stddev(&values);
        if global_stddev < 1e-12 {
            return Vec::new();
        }

        let threshold = sensitivity * global_stddev;
        let mut points = Vec::new();

        for i in window_size..values.len().saturating_sub(window_size) {
            let left_mean =
                values[i - window_size..i].iter().sum::<f64>() / window_size as f64;
            let right_mean =
                values[i..i + window_size].iter().sum::<f64>() / window_size as f64;
            let diff = (right_mean - left_mean).abs();

            if diff > threshold {
                points.push(ChangePoint {
                    index: i,
                    magnitude: diff,
                });
            }
        }

        points
    }

    /// Standard deviation of values within the given window.
    pub fn stddev(&self, series: &TimeSeries, window: Duration) -> f64 {
        let samples = windowed_values(series, window);
        compute_stddev(&samples)
    }

    /// Computes the p-th percentile (0.0..=1.0) using linear interpolation.
    pub fn percentile(&self, series: &TimeSeries, p: f64) -> f64 {
        let mut values: Vec<f64> = series.samples.iter().map(|s| s.value).collect();
        if values.is_empty() {
            return 0.0;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        interpolated_percentile(&values, p)
    }
}

/// Extracts values from samples within the window measured back from the latest sample.
fn windowed_values(series: &TimeSeries, window: Duration) -> Vec<f64> {
    if series.is_empty() {
        return Vec::new();
    }

    let latest = series.samples.back().unwrap().timestamp;
    let cutoff = latest - window;

    series
        .samples
        .iter()
        .filter(|s| s.timestamp >= cutoff)
        .map(|s| s.value)
        .collect()
}

/// Population standard deviation of a slice.
fn compute_stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Linear interpolation percentile on a **sorted** slice. `p` must be in 0.0..=1.0.
fn interpolated_percentile(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    let p = p.clamp(0.0, 1.0);
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use aether_core::metrics::MetricSample;

    use super::*;

    fn make_series(values: &[(f64, f64)]) -> TimeSeries {
        let mut ts = TimeSeries::new("test", values.len() + 10);
        let base = Instant::now();
        for &(secs, value) in values {
            ts.push_sample(MetricSample {
                timestamp: base + Duration::from_secs_f64(secs),
                value,
            });
        }
        ts
    }

    #[test]
    fn test_z_score_normal() {
        // 50 points at value 50.0, last value also 50.0 → z ≈ 0
        let points: Vec<(f64, f64)> = (0..50).map(|i| (i as f64, 50.0)).collect();
        let series = make_series(&points);
        let detector = AnomalyDetector;
        let z = detector.z_score(&series, Duration::from_secs(120));
        assert!(z.abs() < 0.01, "expected z ≈ 0, got {z}");
    }

    #[test]
    fn test_z_score_outlier() {
        // 50 points with mean=50, stddev=10, then last value = 100 → z ≈ 5
        let mut points: Vec<(f64, f64)> = Vec::new();
        for i in 0..49 {
            // Alternating 40 and 60 gives mean=50, stddev=10
            let value = if i % 2 == 0 { 40.0 } else { 60.0 };
            points.push((i as f64, value));
        }
        points.push((49.0, 100.0)); // outlier: 5 stddev above mean
        let series = make_series(&points);
        let detector = AnomalyDetector;
        let z = detector.z_score(&series, Duration::from_secs(120));
        assert!(z > 4.0, "expected z > 4, got {z}");
    }

    #[test]
    fn test_iqr_detects_outlier() {
        // Normal range [10..20], then add extreme value
        let mut points: Vec<(f64, f64)> = (0..20)
            .map(|i| (i as f64, 10.0 + (i as f64 % 10.0)))
            .collect();
        points.push((20.0, 100.0)); // extreme outlier
        let series = make_series(&points);
        let detector = AnomalyDetector;
        assert!(detector.is_outlier_iqr(&series), "expected outlier detection");
    }

    #[test]
    fn test_iqr_normal_not_outlier() {
        let points: Vec<(f64, f64)> = (0..20)
            .map(|i| (i as f64, 50.0 + (i as f64 % 5.0)))
            .collect();
        let series = make_series(&points);
        let detector = AnomalyDetector;
        assert!(
            !detector.is_outlier_iqr(&series),
            "expected no outlier for normal data"
        );
    }

    #[test]
    fn test_change_point_level_shift() {
        // 20 points at 50, then 20 points at 100 → change around index 20
        let mut points: Vec<(f64, f64)> = Vec::new();
        for i in 0..20 {
            points.push((i as f64, 50.0));
        }
        for i in 20..40 {
            points.push((i as f64, 100.0));
        }
        let series = make_series(&points);
        let detector = AnomalyDetector;
        let cps = detector.change_points(&series, 1.0);
        assert!(!cps.is_empty(), "expected at least one change point");
        // Change point should be near index 20
        let near_20 = cps.iter().any(|cp| (10..=30).contains(&cp.index));
        assert!(near_20, "expected change point near index 20, got {cps:?}");
    }

    #[test]
    fn test_change_point_stable_no_detection() {
        let points: Vec<(f64, f64)> = (0..40).map(|i| (i as f64, 50.0)).collect();
        let series = make_series(&points);
        let detector = AnomalyDetector;
        let cps = detector.change_points(&series, 1.0);
        assert!(cps.is_empty(), "expected no change points for stable data");
    }

    #[test]
    fn test_percentile_median() {
        // Values 1..=100, 50th percentile ≈ 50.5
        let points: Vec<(f64, f64)> = (1..=100).map(|i| (i as f64, i as f64)).collect();
        let series = make_series(&points);
        let detector = AnomalyDetector;
        let median = detector.percentile(&series, 0.5);
        assert!(
            (median - 50.5).abs() < 1.0,
            "expected median ≈ 50.5, got {median}"
        );
    }
}

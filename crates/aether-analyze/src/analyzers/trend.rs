//! Trend detection and classification for metric time series.

use std::time::Duration;

use aether_core::metrics::TimeSeries;

/// Classification of a metric trend within a time window.
#[derive(Debug, Clone)]
pub enum TrendClass {
    Stable,
    Growing { rate: f64 },
    Declining { rate: f64 },
    Spike { magnitude: f64 },
    Oscillating { period_secs: f64, amplitude: f64 },
}

/// Stateless trend analyzer operating on `TimeSeries` data.
#[derive(Debug)]
pub struct TrendAnalyzer;

impl TrendAnalyzer {
    /// Computes the slope (value-units per second) over the given window.
    pub fn slope(&self, series: &TimeSeries, window: Duration) -> f64 {
        let points = windowed_points(series, window);
        let (slope, _, _) = linear_regression(&points);
        slope
    }

    /// Estimates time until the series reaches `threshold` based on recent trend.
    ///
    /// Returns `None` if the slope is non-positive or the current value already
    /// exceeds the threshold.
    pub fn time_to_threshold(&self, series: &TimeSeries, threshold: f64) -> Option<Duration> {
        let current = series.last_value()?;
        let slope = self.slope(series, Duration::from_secs(300));

        if slope <= 0.0 || current >= threshold {
            return None;
        }

        Some(Duration::from_secs_f64((threshold - current) / slope))
    }

    /// Classifies the trend within the given window.
    pub fn classify(&self, series: &TimeSeries, window: Duration) -> TrendClass {
        let points = windowed_points(series, window);
        if points.len() < 2 {
            return TrendClass::Stable;
        }

        let (slope, _, r_squared) = linear_regression(&points);

        let mean = points.iter().map(|(_, y)| y).sum::<f64>() / points.len() as f64;
        let variance =
            points.iter().map(|(_, y)| (y - mean).powi(2)).sum::<f64>() / points.len() as f64;
        let stddev = variance.sqrt();

        let last_value = points.last().unwrap().1;

        if last_value > mean + 3.0 * stddev {
            return TrendClass::Spike {
                magnitude: last_value - mean,
            };
        }

        let threshold = 0.001 * mean.abs();
        if slope.abs() < threshold {
            return TrendClass::Stable;
        }

        if slope > 0.0 && r_squared > 0.5 {
            return TrendClass::Growing { rate: slope };
        }

        if slope < 0.0 && r_squared > 0.5 {
            return TrendClass::Declining {
                rate: slope.abs(),
            };
        }

        TrendClass::Stable
    }
}

/// Extracts (seconds_since_first, value) points within the window from the latest sample.
fn windowed_points(series: &TimeSeries, window: Duration) -> Vec<(f64, f64)> {
    if series.is_empty() {
        return Vec::new();
    }

    let latest = series.samples.back().unwrap().timestamp;
    let cutoff = latest - window;

    let filtered: Vec<_> = series
        .samples
        .iter()
        .filter(|s| s.timestamp >= cutoff)
        .collect();

    if filtered.is_empty() {
        return Vec::new();
    }

    let first_ts = filtered[0].timestamp;
    filtered
        .iter()
        .map(|s| (s.timestamp.duration_since(first_ts).as_secs_f64(), s.value))
        .collect()
}

/// Standard least-squares linear regression.
///
/// Returns `(slope, intercept, r_squared)`. Returns `(0.0, 0.0, 0.0)` for fewer
/// than 2 points.
pub(crate) fn linear_regression(points: &[(f64, f64)]) -> (f64, f64, f64) {
    if points.len() < 2 {
        return (0.0, 0.0, 0.0);
    }

    let n = points.len() as f64;
    let x_mean = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let y_mean = points.iter().map(|(_, y)| y).sum::<f64>() / n;

    let mut ss_xy = 0.0;
    let mut ss_xx = 0.0;
    let mut ss_yy = 0.0;

    for &(x, y) in points {
        let dx = x - x_mean;
        let dy = y - y_mean;
        ss_xy += dx * dy;
        ss_xx += dx * dx;
        ss_yy += dy * dy;
    }

    if ss_xx == 0.0 {
        return (0.0, y_mean, 0.0);
    }

    let slope = ss_xy / ss_xx;
    let intercept = y_mean - slope * x_mean;

    let r_squared = if ss_yy == 0.0 {
        1.0
    } else {
        (ss_xy * ss_xy) / (ss_xx * ss_yy)
    };

    (slope, intercept, r_squared)
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
            ts.push(MetricSample {
                timestamp: base + Duration::from_secs_f64(secs),
                value,
            });
        }
        ts
    }

    #[test]
    fn test_slope_constant_series_is_zero() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 50.0)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        let slope = analyzer.slope(&series, Duration::from_secs(120));
        assert!(slope.abs() < 1e-10, "expected ~0, got {slope}");
    }

    #[test]
    fn test_slope_linear_growth() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        let slope = analyzer.slope(&series, Duration::from_secs(120));
        assert!(
            (slope - 1.0).abs() < 1e-10,
            "expected ~1.0, got {slope}"
        );
    }

    #[test]
    fn test_time_to_threshold_growing() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 50.0 + i as f64)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        let ttl = analyzer.time_to_threshold(&series, 160.0).unwrap();
        let secs = ttl.as_secs_f64();
        assert!(
            (secs - 51.0).abs() < 1.0,
            "expected ~51s, got {secs}"
        );
    }

    #[test]
    fn test_time_to_threshold_declining_returns_none() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 100.0 - i as f64)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        assert!(analyzer.time_to_threshold(&series, 200.0).is_none());
    }

    #[test]
    fn test_classify_stable() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 50.0)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        assert!(matches!(
            analyzer.classify(&series, Duration::from_secs(120)),
            TrendClass::Stable
        ));
    }

    #[test]
    fn test_classify_growing() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64 * 10.0)).collect();
        let series = make_series(&points);
        let analyzer = TrendAnalyzer;
        assert!(matches!(
            analyzer.classify(&series, Duration::from_secs(120)),
            TrendClass::Growing { .. }
        ));
    }

    #[test]
    fn test_linear_regression_perfect_line() {
        let points: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 2.0 * i as f64 + 1.0)).collect();
        let (slope, intercept, r_squared) = linear_regression(&points);
        assert!(
            (slope - 2.0).abs() < 1e-10,
            "expected slope=2.0, got {slope}"
        );
        assert!(
            (intercept - 1.0).abs() < 1e-10,
            "expected intercept=1.0, got {intercept}"
        );
        assert!(
            (r_squared - 1.0).abs() < 1e-10,
            "expected r²=1.0, got {r_squared}"
        );
    }

    #[test]
    fn test_linear_regression_single_point() {
        let (slope, intercept, r_squared) = linear_regression(&[(5.0, 3.0)]);
        assert_eq!(slope, 0.0);
        assert_eq!(intercept, 0.0);
        assert_eq!(r_squared, 0.0);
    }
}

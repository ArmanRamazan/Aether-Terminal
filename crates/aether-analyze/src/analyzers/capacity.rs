//! Capacity planning analysis for resource metrics.

use std::time::Duration;

use aether_core::metrics::TimeSeries;

use super::trend::{TrendAnalyzer, TrendClass};

/// Capacity report for a single resource metric.
#[derive(Debug, Clone)]
pub struct CapacityReport {
    pub usage_percent: f64,
    pub headroom: f64,
    pub trend: TrendClass,
    pub time_to_exhaustion: Option<Duration>,
    pub recommended_limit: Option<f64>,
}

/// Stateless capacity analyzer producing resource planning reports.
#[derive(Debug)]
pub struct CapacityAnalyzer;

impl CapacityAnalyzer {
    /// Analyze current resource usage against a limit, incorporating trend data.
    pub fn analyze(
        &self,
        current: f64,
        limit: f64,
        trend: &TrendAnalyzer,
        series: &TimeSeries,
    ) -> CapacityReport {
        let usage_percent = if limit > 0.0 {
            current / limit * 100.0
        } else {
            0.0
        };
        let headroom = (limit - current).max(0.0);
        let trend_class = trend.classify(series, Duration::from_secs(300));
        let time_to_exhaustion = trend.time_to_threshold(series, limit);
        let recommended_limit = if usage_percent > 80.0 {
            Some(round_up_nice(limit * 2.0))
        } else {
            None
        };

        CapacityReport {
            usage_percent,
            headroom,
            trend: trend_class,
            time_to_exhaustion,
            recommended_limit,
        }
    }
}

/// Format a byte count into a human-readable string.
#[allow(dead_code)]
pub(crate) fn format_bytes(bytes: f64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;

    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{:.0} B", bytes)
    }
}

/// Format a duration into an approximate human-readable string.
#[allow(dead_code)]
pub(crate) fn format_duration(dur: Duration) -> String {
    let secs = dur.as_secs();
    if secs >= 3600 {
        format!("~{} hours", secs / 3600)
    } else if secs >= 60 {
        format!("~{} minutes", secs / 60)
    } else {
        format!("~{} seconds", secs)
    }
}

/// Round a value up to the nearest "nice" number (power of 2 or 1.5x step).
fn round_up_nice(value: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }

    // Find the nearest power of 2 >= value
    let log2 = value.log2();
    let power_of_2 = 2.0_f64.powf(log2.ceil());

    // Find the nearest 1.5x step: 1.5 * 2^n
    let log_1_5 = (value / 1.5).log2();
    let step_1_5 = 1.5 * 2.0_f64.powf(log_1_5.ceil());

    // Return whichever is closer to value but still >= value
    if power_of_2 <= step_1_5 {
        power_of_2
    } else {
        step_1_5
    }
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
    fn test_analyze_healthy_usage() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 50.0)).collect();
        let series = make_series(&points);
        let analyzer = CapacityAnalyzer;
        let trend = TrendAnalyzer;

        let report = analyzer.analyze(50.0, 100.0, &trend, &series);

        assert!((report.usage_percent - 50.0).abs() < 1e-10, "usage should be 50%");
        assert!((report.headroom - 50.0).abs() < 1e-10, "headroom should be 50");
        assert!(matches!(report.trend, TrendClass::Stable));
        assert!(report.time_to_exhaustion.is_none(), "stable trend should not exhaust");
        assert!(report.recommended_limit.is_none(), "healthy usage needs no recommendation");
    }

    #[test]
    fn test_analyze_critical_usage() {
        // 93% usage with growing trend — series values stay below limit
        let points: Vec<(f64, f64)> =
            (0..60).map(|i| (i as f64, 80.0 + i as f64 * 0.2)).collect();
        let series = make_series(&points);
        let analyzer = CapacityAnalyzer;
        let trend = TrendAnalyzer;

        let current = 93.0;
        let limit = 100.0;
        let report = analyzer.analyze(current, limit, &trend, &series);

        assert!(report.usage_percent > 80.0, "usage should be critical");
        assert!(report.time_to_exhaustion.is_some(), "growing trend should have exhaustion time");
        assert!(report.recommended_limit.is_some(), "critical usage should recommend higher limit");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(503_316_480.0), "480.0 MB");
    }

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(1_073_741_824.0), "1.0 GB");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(780)), "~13 minutes");
    }

    #[test]
    fn test_zero_limit_no_panic() {
        let points: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, 100.0)).collect();
        let series = make_series(&points);
        let analyzer = CapacityAnalyzer;
        let trend = TrendAnalyzer;

        let report = analyzer.analyze(100.0, 0.0, &trend, &series);

        assert!((report.usage_percent - 0.0).abs() < 1e-10, "zero limit should yield 0% usage");
        assert!((report.headroom - 0.0).abs() < 1e-10, "zero limit should yield 0 headroom");
    }
}

//! Correlation analysis between metric time series using Pearson coefficient.

use std::time::Duration;

use aether_core::metrics::TimeSeries;

/// A correlation finding between two metrics.
#[derive(Debug, Clone)]
pub struct Correlation {
    pub metric_a: String,
    pub metric_b: String,
    pub coefficient: f64,
    pub interpretation: String,
}

/// Stateless correlation analyzer using Pearson coefficient.
#[derive(Debug)]
pub struct CorrelationAnalyzer;

impl CorrelationAnalyzer {
    /// Computes Pearson correlation between two series, aligning samples by nearest timestamp.
    ///
    /// Returns 0.0 if fewer than 2 aligned points exist.
    pub fn correlate(&self, a: &TimeSeries, b: &TimeSeries, window: Duration) -> f64 {
        let pairs = align_samples(a, b, window);
        if pairs.len() < 2 {
            return 0.0;
        }

        let n = pairs.len() as f64;
        let mean_a = pairs.iter().map(|(x, _)| x).sum::<f64>() / n;
        let mean_b = pairs.iter().map(|(_, y)| y).sum::<f64>() / n;

        let mut ss_ab = 0.0;
        let mut ss_aa = 0.0;
        let mut ss_bb = 0.0;

        for &(va, vb) in &pairs {
            let da = va - mean_a;
            let db = vb - mean_b;
            ss_ab += da * db;
            ss_aa += da * da;
            ss_bb += db * db;
        }

        let denom = (ss_aa * ss_bb).sqrt();
        if denom == 0.0 {
            return 0.0;
        }

        ss_ab / denom
    }

    /// Finds all candidates correlated with `target` above the given threshold.
    pub fn find_correlated(
        &self,
        target: &TimeSeries,
        candidates: &[&TimeSeries],
        threshold: f64,
    ) -> Vec<Correlation> {
        let window = Duration::from_secs(300);
        candidates
            .iter()
            .filter_map(|candidate| {
                let r = self.correlate(target, candidate, window);
                if r.abs() > threshold {
                    Some(Correlation {
                        metric_a: target.name.clone(),
                        metric_b: candidate.name.clone(),
                        coefficient: r,
                        interpretation: interpret(r).to_owned(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Human-readable interpretation of a Pearson coefficient.
    pub fn interpret(r: f64) -> &'static str {
        interpret(r)
    }
}

fn interpret(r: f64) -> &'static str {
    let abs_r = r.abs();
    if abs_r > 0.9 {
        "very strongly"
    } else if abs_r > 0.7 {
        "strongly"
    } else if abs_r > 0.5 {
        "moderately"
    } else {
        "weakly"
    }
}

/// Aligns two series by matching each sample in `a` to the nearest sample in `b`
/// within the given window. Returns paired values.
fn align_samples(a: &TimeSeries, b: &TimeSeries, window: Duration) -> Vec<(f64, f64)> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }

    let b_samples: Vec<_> = b.samples.iter().collect();
    let mut pairs = Vec::new();

    for sa in &a.samples {
        let mut best_dist = window;
        let mut best_val = None;

        for sb in &b_samples {
            let dist = if sa.timestamp >= sb.timestamp {
                sa.timestamp.duration_since(sb.timestamp)
            } else {
                sb.timestamp.duration_since(sa.timestamp)
            };

            if dist <= best_dist {
                best_dist = dist;
                best_val = Some(sb.value);
            }
        }

        if let Some(vb) = best_val {
            pairs.push((sa.value, vb));
        }
    }

    pairs
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use aether_core::metrics::MetricSample;

    use super::*;

    fn make_series(name: &str, values: &[(f64, f64)]) -> TimeSeries {
        let mut ts = TimeSeries::new(name, values.len() + 10);
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
    fn test_perfect_positive() {
        let points: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64)).collect();
        let a = make_series("a", &points);
        let b = make_series("b", &points);
        let analyzer = CorrelationAnalyzer;
        let r = analyzer.correlate(&a, &b, Duration::from_secs(1));
        assert!((r - 1.0).abs() < 1e-10, "expected r ≈ 1.0, got {r}");
    }

    #[test]
    fn test_perfect_negative() {
        let a_pts: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64)).collect();
        let b_pts: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, 60.0 - i as f64)).collect();
        let a = make_series("a", &a_pts);
        let b = make_series("b", &b_pts);
        let analyzer = CorrelationAnalyzer;
        let r = analyzer.correlate(&a, &b, Duration::from_secs(1));
        assert!((r + 1.0).abs() < 1e-10, "expected r ≈ -1.0, got {r}");
    }

    #[test]
    fn test_uncorrelated() {
        // Alternating pattern vs linear — low correlation
        let a_pts: Vec<(f64, f64)> = (0..60)
            .map(|i| (i as f64, if i % 2 == 0 { 100.0 } else { 0.0 }))
            .collect();
        let b_pts: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64)).collect();
        let a = make_series("a", &a_pts);
        let b = make_series("b", &b_pts);
        let analyzer = CorrelationAnalyzer;
        let r = analyzer.correlate(&a, &b, Duration::from_secs(1));
        assert!(r.abs() < 0.3, "expected |r| < 0.3, got {r}");
    }

    #[test]
    fn test_find_correlated_filters() {
        let target_pts: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64)).collect();
        let correlated_pts: Vec<(f64, f64)> = (0..60).map(|i| (i as f64, i as f64 * 2.0)).collect();
        let uncorrelated_pts: Vec<(f64, f64)> = (0..60)
            .map(|i| (i as f64, if i % 2 == 0 { 100.0 } else { 0.0 }))
            .collect();

        let target = make_series("target", &target_pts);
        let correlated = make_series("correlated", &correlated_pts);
        let uncorrelated = make_series("uncorrelated", &uncorrelated_pts);

        let analyzer = CorrelationAnalyzer;
        let results = analyzer.find_correlated(&target, &[&correlated, &uncorrelated], 0.5);

        assert_eq!(results.len(), 1, "expected 1 correlated pair");
        assert_eq!(results[0].metric_b, "correlated");
    }

    #[test]
    fn test_empty_series_zero() {
        let a = TimeSeries::new("a", 10);
        let b = TimeSeries::new("b", 10);
        let analyzer = CorrelationAnalyzer;
        let r = analyzer.correlate(&a, &b, Duration::from_secs(1));
        assert_eq!(r, 0.0, "empty series should give r=0.0");
    }
}

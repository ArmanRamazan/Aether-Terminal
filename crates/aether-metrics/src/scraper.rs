//! Prometheus scraper — fetches `/metrics` endpoints from discovered targets.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::{debug, warn};

use aether_core::models::{CollectedMetric, EndpointType, Target};
use aether_core::traits::DataSource;
use aether_core::{MetricSample, TimeSeries};

use crate::error::MetricsError;

/// Scrapes Prometheus text exposition endpoints from discovered targets.
///
/// Unlike [`PrometheusConsumer`](crate::consumer::PrometheusConsumer) which
/// queries a Prometheus server API, this scrapes individual service `/metrics`
/// endpoints directly.
pub struct PrometheusScraper {
    targets: Arc<RwLock<Vec<Target>>>,
    client: reqwest::Client,
}

impl PrometheusScraper {
    /// Create a scraper that reads targets from the shared discovery list.
    pub fn new(targets: Arc<RwLock<Vec<Target>>>, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self { targets, client }
    }

    /// Scrape all Prometheus endpoints from the current target list.
    /// Returns parsed time series ready for MetricStore ingestion.
    pub async fn scrape(&self) -> Result<Vec<TimeSeries>, MetricsError> {
        let endpoints: Vec<(String, String)> = {
            let targets = self
                .targets
                .read()
                .map_err(|_| MetricsError::Query("targets lock poisoned".into()))?;
            targets
                .iter()
                .flat_map(|t| {
                    t.endpoints
                        .iter()
                        .filter(|e| e.endpoint_type == EndpointType::Prometheus)
                        .map(|e| (t.id.clone(), e.url.clone()))
                })
                .collect()
        };

        if endpoints.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_series = Vec::new();

        for (target_id, url) in &endpoints {
            match self.scrape_endpoint(target_id, url).await {
                Ok(series) => {
                    debug!(target_id, url, count = series.len(), "scraped metrics");
                    all_series.extend(series);
                }
                Err(e) => {
                    warn!(target_id, url, "scrape failed: {e}");
                }
            }
        }

        Ok(all_series)
    }

    /// Scrape a single endpoint and parse the Prometheus text exposition format.
    async fn scrape_endpoint(
        &self,
        target_id: &str,
        url: &str,
    ) -> Result<Vec<TimeSeries>, MetricsError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| MetricsError::Http(format!("{url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(MetricsError::Http(format!(
                "{url}: HTTP {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| MetricsError::Http(format!("{url}: body read failed: {e}")))?;

        Ok(parse_text_exposition(&body, target_id))
    }
}

#[async_trait]
impl DataSource for PrometheusScraper {
    async fn collect(
        &self,
    ) -> Result<Vec<CollectedMetric>, Box<dyn std::error::Error + Send + Sync>> {
        let series = self.scrape().await?;
        let now = std::time::SystemTime::now();
        let metrics = series
            .into_iter()
            .filter_map(|ts| {
                let value = ts.last().map(|s| s.value)?;
                Some(CollectedMetric {
                    name: ts.name,
                    value,
                    labels: ts.labels.into_iter().collect(),
                    timestamp: now,
                })
            })
            .collect();
        Ok(metrics)
    }

    fn name(&self) -> &str {
        "prometheus-scraper"
    }
}

/// Parse Prometheus text exposition format into TimeSeries.
///
/// Handles lines like:
/// ```text
/// metric_name{label1="val1",label2="val2"} 123.45
/// metric_name 42
/// ```
/// Ignores `# HELP` and `# TYPE` comment lines.
fn parse_text_exposition(body: &str, target_id: &str) -> Vec<TimeSeries> {
    let now = Instant::now();
    let mut result = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((name, labels, value)) = parse_metric_line(line) {
            let mut ts = TimeSeries::new(&name, 3600);
            let mut label_map: BTreeMap<String, String> = labels.into_iter().collect();
            label_map.insert("target".to_owned(), target_id.to_owned());
            ts.labels = label_map;
            ts.push_sample(MetricSample {
                timestamp: now,
                value,
            });
            result.push(ts);
        }
    }

    result
}

/// Parsed components of a single Prometheus metric line.
type ParsedMetric = (String, Vec<(String, String)>, f64);

/// Parse a single Prometheus metric line.
/// Returns (metric_name, labels, value) or None if unparseable.
fn parse_metric_line(line: &str) -> Option<ParsedMetric> {
    // Split into metric part and value
    // Format: metric_name{labels} value [timestamp]
    // or:     metric_name value [timestamp]

    let (metric_part, rest) = if let Some(brace_start) = line.find('{') {
        let brace_end = line.find('}')?;
        let metric_name = line[..brace_start].to_owned();
        let labels_str = &line[brace_start + 1..brace_end];
        let labels = parse_labels(labels_str);
        let rest = line[brace_end + 1..].trim();
        ((metric_name, labels), rest)
    } else {
        // No labels
        let mut parts = line.splitn(2, char::is_whitespace);
        let name = parts.next()?.to_owned();
        let rest = parts.next()?.trim();
        ((name, Vec::new()), rest)
    };

    // Parse value (first token, ignore optional timestamp)
    let value_str = rest.split_whitespace().next()?;
    let value = value_str.parse::<f64>().ok()?;

    Some((metric_part.0, metric_part.1, value))
}

/// Parse label pairs from inside braces: `key1="val1",key2="val2"`.
fn parse_labels(s: &str) -> Vec<(String, String)> {
    let mut labels = Vec::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some(eq_pos) = pair.find('=') {
            let key = pair[..eq_pos].trim().to_owned();
            let val = pair[eq_pos + 1..]
                .trim()
                .trim_matches('"')
                .to_owned();
            if !key.is_empty() {
                labels.push((key, val));
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metric_line_with_labels() {
        let line = r#"http_requests_total{method="GET",status="200"} 1234"#;
        let (name, labels, value) = parse_metric_line(line).expect("should parse");
        assert_eq!(name, "http_requests_total");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], ("method".into(), "GET".into()));
        assert_eq!(labels[1], ("status".into(), "200".into()));
        assert!((value - 1234.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_metric_line_without_labels() {
        let line = "go_goroutines 42";
        let (name, labels, value) = parse_metric_line(line).expect("should parse");
        assert_eq!(name, "go_goroutines");
        assert!(labels.is_empty());
        assert!((value - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_metric_line_with_timestamp() {
        let line = "process_cpu_seconds_total 0.5 1625847621000";
        let (name, _, value) = parse_metric_line(line).expect("should parse");
        assert_eq!(name, "process_cpu_seconds_total");
        assert!((value - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_text_exposition() {
        let body = r#"
# HELP http_requests_total Total requests
# TYPE http_requests_total counter
http_requests_total{method="GET"} 100
http_requests_total{method="POST"} 50
go_goroutines 42
"#;
        let series = parse_text_exposition(body, "target-1");
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].name, "http_requests_total");
        assert_eq!(
            series[0].labels.get("target"),
            Some(&"target-1".to_owned())
        );
        assert_eq!(series[2].name, "go_goroutines");
    }

    #[test]
    fn test_parse_empty_body() {
        let series = parse_text_exposition("", "t1");
        assert!(series.is_empty());
    }

    #[test]
    fn test_parse_comments_only() {
        let body = "# HELP foo\n# TYPE foo gauge\n";
        let series = parse_text_exposition(body, "t1");
        assert!(series.is_empty());
    }
}

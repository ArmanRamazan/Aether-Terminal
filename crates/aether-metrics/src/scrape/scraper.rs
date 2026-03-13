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

use super::parser::{parse_prometheus_text, ScrapedSample};

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

        Ok(samples_to_timeseries(&body, target_id))
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

/// Convert parsed Prometheus samples into TimeSeries for MetricStore.
fn samples_to_timeseries(body: &str, target_id: &str) -> Vec<TimeSeries> {
    let now = Instant::now();
    let samples = parse_prometheus_text(body);

    samples
        .into_iter()
        .map(|s| to_timeseries(s, target_id, now))
        .collect()
}

/// Convert a single scraped sample into a TimeSeries.
fn to_timeseries(sample: ScrapedSample, target_id: &str, now: Instant) -> TimeSeries {
    let mut ts = TimeSeries::new(&sample.name, 3600);
    let mut labels: BTreeMap<String, String> = sample.labels;
    labels.insert("target".to_owned(), target_id.to_owned());
    ts.labels = labels;
    ts.push_sample(MetricSample {
        timestamp: now,
        value: sample.value,
    });
    ts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scraper_construction() {
        let targets = Arc::new(RwLock::new(Vec::new()));
        let scraper = PrometheusScraper::new(targets, Duration::from_secs(5));
        assert_eq!(scraper.name(), "prometheus-scraper");
    }

    #[test]
    fn test_samples_to_timeseries_with_labels() {
        let body = r#"http_requests_total{method="GET",status="200"} 1234"#;
        let series = samples_to_timeseries(body, "target-1");
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].name, "http_requests_total");
        assert_eq!(
            series[0].labels.get("target"),
            Some(&"target-1".to_owned())
        );
        assert_eq!(
            series[0].labels.get("method"),
            Some(&"GET".to_owned())
        );
    }

    #[test]
    fn test_samples_to_timeseries_multiline() {
        let body = "\
# HELP http_requests_total Total requests
# TYPE http_requests_total counter
http_requests_total{method=\"GET\"} 100
http_requests_total{method=\"POST\"} 50
go_goroutines 42
";
        let series = samples_to_timeseries(body, "target-1");
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].name, "http_requests_total");
        assert_eq!(series[2].name, "go_goroutines");
    }

    #[test]
    fn test_samples_to_timeseries_empty() {
        let series = samples_to_timeseries("", "t1");
        assert!(series.is_empty());
    }

    #[test]
    fn test_samples_to_timeseries_comments_only() {
        let body = "# HELP foo\n# TYPE foo gauge\n";
        let series = samples_to_timeseries(body, "t1");
        assert!(series.is_empty());
    }

    #[tokio::test]
    async fn test_scrape_empty_targets() {
        let targets = Arc::new(RwLock::new(Vec::new()));
        let scraper = PrometheusScraper::new(targets, Duration::from_secs(5));
        let result = scraper.scrape().await.expect("should succeed");
        assert!(result.is_empty());
    }
}

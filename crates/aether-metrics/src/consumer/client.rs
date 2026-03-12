//! Prometheus HTTP client for querying metrics.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use reqwest::Url;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use aether_core::TimeSeries;

use crate::consumer::types::PromResponse;
use crate::consumer::QueryBuilder;
use crate::error::MetricsError;

/// Client that polls a Prometheus-compatible HTTP API.
pub struct PrometheusConsumer {
    endpoint: Url,
    client: reqwest::Client,
    poll_interval: Duration,
}

impl PrometheusConsumer {
    /// Creates a consumer targeting the given Prometheus endpoint.
    pub fn new(endpoint: &str, poll_interval: Duration) -> Result<Self, MetricsError> {
        let endpoint =
            Url::parse(endpoint).map_err(|e| MetricsError::Query(format!("invalid URL: {e}")))?;
        Ok(Self {
            endpoint,
            client: reqwest::Client::new(),
            poll_interval,
        })
    }

    /// Executes an instant PromQL query and returns parsed time series.
    pub async fn query(&self, promql: &str) -> Result<Vec<TimeSeries>, MetricsError> {
        let url = self
            .endpoint
            .join("/api/v1/query")
            .map_err(|e| MetricsError::Query(format!("URL join failed: {e}")))?;

        let resp = self
            .client
            .get(url)
            .query(&[("query", promql)])
            .send()
            .await
            .map_err(|e| MetricsError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(MetricsError::Http(format!(
                "Prometheus returned {}",
                resp.status()
            )));
        }

        let prom: PromResponse = resp
            .json()
            .await
            .map_err(|e| MetricsError::Query(format!("failed to parse response: {e}")))?;

        if prom.status != "success" {
            return Err(MetricsError::Query(format!(
                "Prometheus status: {}",
                prom.status
            )));
        }

        let now = Instant::now();
        let series = prom
            .data
            .result
            .into_iter()
            .filter_map(|r| {
                let name = r
                    .metric
                    .get("__name__")
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());

                let labels: BTreeMap<String, String> = r
                    .metric
                    .into_iter()
                    .filter(|(k, _)| k != "__name__")
                    .collect();

                let mut ts = TimeSeries::new(&name, 3600);
                ts.labels = labels;

                if let Some(values) = r.values {
                    // Range query: compute relative offsets from latest timestamp.
                    let max_ts = values
                        .iter()
                        .map(|(t, _)| *t)
                        .fold(f64::NEG_INFINITY, f64::max);
                    for (epoch, val_str) in &values {
                        if let Ok(val) = val_str.parse::<f64>() {
                            let offset = Duration::from_secs_f64((max_ts - epoch).max(0.0));
                            ts.push_sample(aether_core::MetricSample {
                                timestamp: now - offset,
                                value: val,
                            });
                        }
                    }
                } else if let Some((_, val_str)) = r.value {
                    if let Ok(val) = val_str.parse::<f64>() {
                        ts.push_sample(aether_core::MetricSample {
                            timestamp: now,
                            value: val,
                        });
                    }
                }

                if ts.is_empty() {
                    None
                } else {
                    Some(ts)
                }
            })
            .collect();

        Ok(series)
    }

    /// Preset query: cluster CPU usage percentage.
    pub async fn cluster_cpu(&self) -> Result<Vec<TimeSeries>, MetricsError> {
        let promql = QueryBuilder::default()
            .metric("aether_host_cpu_percent")
            .build();
        self.query(&promql).await
    }

    /// Preset query: cluster memory usage in bytes.
    pub async fn cluster_memory(&self) -> Result<Vec<TimeSeries>, MetricsError> {
        let promql = QueryBuilder::default()
            .metric("aether_host_memory_used_bytes")
            .build();
        self.query(&promql).await
    }

    /// Polls Prometheus on an interval, sending batches to the channel.
    pub async fn run(&self, tx: mpsc::Sender<Vec<TimeSeries>>, cancel: CancellationToken) {
        let promql = QueryBuilder::default()
            .metric("aether_host_cpu_percent")
            .build();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("PrometheusConsumer shutting down");
                    return;
                }
                _ = tokio::time::sleep(self.poll_interval) => {
                    match self.query(&promql).await {
                        Ok(batch) if !batch.is_empty() => {
                            if tx.send(batch).await.is_err() {
                                debug!("PrometheusConsumer channel closed");
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!("PrometheusConsumer query failed: {e}");
                        }
                    }
                }
            }
        }
    }
}

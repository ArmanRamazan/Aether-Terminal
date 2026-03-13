//! ProberEngine — performs HTTP health checks and TCP probes on discovered targets.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use tokio::net::TcpStream;
use tracing::{debug, warn};

use aether_core::models::{CollectedMetric, EndpointType, Target};
use aether_core::traits::DataSource;

use crate::error::ProberError;

/// Probes HTTP health and TCP connectivity for discovered targets.
pub struct ProberEngine {
    targets: Arc<RwLock<Vec<Target>>>,
    client: reqwest::Client,
    timeout: Duration,
}

impl ProberEngine {
    /// Create a prober that reads endpoints from the shared target list.
    pub fn new(targets: Arc<RwLock<Vec<Target>>>, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self {
            targets,
            client,
            timeout,
        }
    }

    /// Probe all health and TCP endpoints from the current target list.
    pub async fn probe(&self) -> Result<Vec<CollectedMetric>, ProberError> {
        let endpoints: Vec<(String, String, EndpointType)> = {
            let targets = self
                .targets
                .read()
                .map_err(|_| ProberError::LockPoisoned)?;
            targets
                .iter()
                .flat_map(|t| {
                    t.endpoints
                        .iter()
                        .filter(|e| {
                            matches!(
                                e.endpoint_type,
                                EndpointType::Health | EndpointType::TcpProbe
                            )
                        })
                        .map(|e| (t.id.clone(), e.url.clone(), e.endpoint_type))
                })
                .collect()
        };

        if endpoints.is_empty() {
            return Ok(Vec::new());
        }

        let mut metrics = Vec::new();

        for (target_id, url, ep_type) in &endpoints {
            match ep_type {
                EndpointType::Health => {
                    let result = self.probe_http(target_id, url).await;
                    metrics.extend(result);
                }
                EndpointType::TcpProbe => {
                    // tcp:// prefix → strip for connection
                    let addr = url.strip_prefix("tcp://").unwrap_or(url);
                    let result = self.probe_tcp(target_id, addr).await;
                    metrics.extend(result);
                }
                _ => {}
            }
        }

        Ok(metrics)
    }

    /// Probe an HTTP health endpoint. Returns latency and status metrics.
    async fn probe_http(&self, target_id: &str, url: &str) -> Vec<CollectedMetric> {
        let now = SystemTime::now();
        let start = Instant::now();
        let mut metrics = Vec::new();

        let mut labels = HashMap::new();
        labels.insert("target".to_owned(), target_id.to_owned());
        labels.insert("endpoint".to_owned(), url.to_owned());
        labels.insert("probe_type".to_owned(), "http".to_owned());

        match self.client.get(url).send().await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                let status_up = if resp.status().is_success() {
                    1.0
                } else {
                    0.0
                };

                debug!(target_id, url, latency_ms, status = %resp.status(), "http probe");

                metrics.push(CollectedMetric {
                    name: "probe_http_latency_ms".to_owned(),
                    value: latency_ms,
                    labels: labels.clone(),
                    timestamp: now,
                });
                metrics.push(CollectedMetric {
                    name: "probe_http_up".to_owned(),
                    value: status_up,
                    labels,
                    timestamp: now,
                });
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                warn!(target_id, url, "http probe failed: {e}");

                metrics.push(CollectedMetric {
                    name: "probe_http_latency_ms".to_owned(),
                    value: latency_ms,
                    labels: labels.clone(),
                    timestamp: now,
                });
                metrics.push(CollectedMetric {
                    name: "probe_http_up".to_owned(),
                    value: 0.0,
                    labels,
                    timestamp: now,
                });
            }
        }

        metrics
    }

    /// Probe a TCP endpoint. Returns latency and status metrics.
    async fn probe_tcp(&self, target_id: &str, addr: &str) -> Vec<CollectedMetric> {
        let now = SystemTime::now();
        let start = Instant::now();
        let mut metrics = Vec::new();

        let mut labels = HashMap::new();
        labels.insert("target".to_owned(), target_id.to_owned());
        labels.insert("endpoint".to_owned(), addr.to_owned());
        labels.insert("probe_type".to_owned(), "tcp".to_owned());

        let result =
            tokio::time::timeout(self.timeout, TcpStream::connect(addr)).await;

        match result {
            Ok(Ok(_stream)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                debug!(target_id, addr, latency_ms, "tcp probe ok");

                metrics.push(CollectedMetric {
                    name: "probe_tcp_latency_ms".to_owned(),
                    value: latency_ms,
                    labels: labels.clone(),
                    timestamp: now,
                });
                metrics.push(CollectedMetric {
                    name: "probe_tcp_up".to_owned(),
                    value: 1.0,
                    labels,
                    timestamp: now,
                });
            }
            Ok(Err(e)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tcp probe failed: {e}");

                metrics.push(CollectedMetric {
                    name: "probe_tcp_latency_ms".to_owned(),
                    value: latency_ms,
                    labels: labels.clone(),
                    timestamp: now,
                });
                metrics.push(CollectedMetric {
                    name: "probe_tcp_up".to_owned(),
                    value: 0.0,
                    labels,
                    timestamp: now,
                });
            }
            Err(_) => {
                let latency_ms = self.timeout.as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tcp probe timed out");

                metrics.push(CollectedMetric {
                    name: "probe_tcp_latency_ms".to_owned(),
                    value: latency_ms,
                    labels: labels.clone(),
                    timestamp: now,
                });
                metrics.push(CollectedMetric {
                    name: "probe_tcp_up".to_owned(),
                    value: 0.0,
                    labels,
                    timestamp: now,
                });
            }
        }

        metrics
    }
}

#[async_trait]
impl DataSource for ProberEngine {
    async fn collect(
        &self,
    ) -> Result<Vec<CollectedMetric>, Box<dyn std::error::Error + Send + Sync>> {
        self.probe().await.map_err(Into::into)
    }

    fn name(&self) -> &str {
        "network-prober"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{Endpoint, TargetKind};

    fn make_targets(endpoints: Vec<Endpoint>) -> Arc<RwLock<Vec<Target>>> {
        Arc::new(RwLock::new(vec![Target {
            id: "test-1".into(),
            name: "test-service".into(),
            kind: TargetKind::Service,
            endpoints,
            labels: HashMap::new(),
            discovered_at: SystemTime::now(),
        }]))
    }

    #[test]
    fn test_prober_creation() {
        let targets = make_targets(vec![]);
        let prober = ProberEngine::new(targets, Duration::from_secs(5));
        assert_eq!(prober.name(), "network-prober");
    }

    #[tokio::test]
    async fn test_probe_empty_targets() {
        let targets = Arc::new(RwLock::new(Vec::new()));
        let prober = ProberEngine::new(targets, Duration::from_secs(5));
        let result = prober.probe().await;
        assert!(result.is_ok());
        assert!(result.as_ref().map(|v| v.is_empty()).unwrap_or(false));
    }

    #[tokio::test]
    async fn test_probe_no_matching_endpoints() {
        let targets = make_targets(vec![Endpoint {
            url: "http://localhost:9090/metrics".into(),
            endpoint_type: EndpointType::Prometheus,
        }]);
        let prober = ProberEngine::new(targets, Duration::from_secs(5));
        let result = prober.probe().await;
        assert!(result.is_ok());
        assert!(result.as_ref().map(|v| v.is_empty()).unwrap_or(false));
    }

    #[tokio::test]
    async fn test_probe_tcp_unreachable() {
        let targets = make_targets(vec![Endpoint {
            url: "tcp://127.0.0.1:1".into(),
            endpoint_type: EndpointType::TcpProbe,
        }]);
        let prober = ProberEngine::new(targets, Duration::from_millis(100));
        let result = prober.probe().await;
        assert!(result.is_ok());
        let metrics = result.as_ref().map(|v| v.len()).unwrap_or(0);
        assert_eq!(metrics, 2, "should produce latency + up metrics");

        let metrics = result.as_ref().expect("should succeed");
        let up = metrics.iter().find(|m| m.name == "probe_tcp_up");
        assert!(up.is_some(), "should have probe_tcp_up metric");
        assert!((up.map(|m| m.value).unwrap_or(1.0) - 0.0).abs() < f64::EPSILON);
    }
}

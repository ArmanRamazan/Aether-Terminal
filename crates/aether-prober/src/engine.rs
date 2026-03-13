//! ProberEngine — orchestrates HTTP, TCP, DNS, and TLS probes on discovered targets.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use tracing::debug;

use aether_core::models::{CollectedMetric, EndpointType, ProbeResult, Target};
use aether_core::traits::DataSource;

use crate::dns::DnsProber;
use crate::error::ProberError;
use crate::http::HttpProber;
use crate::tcp::TcpProber;
use crate::tls::TlsProber;

/// Orchestrates all probe types against discovered targets.
///
/// Implements [`DataSource`] to feed collected metrics into the diagnostic pipeline.
pub struct ProberEngine {
    targets: Arc<RwLock<Vec<Target>>>,
    http: HttpProber,
    tcp: TcpProber,
    dns: DnsProber,
    tls: TlsProber,
}

impl ProberEngine {
    /// Create a prober engine that reads endpoints from the shared target list.
    pub fn new(targets: Arc<RwLock<Vec<Target>>>, timeout: Duration) -> Self {
        Self {
            targets,
            http: HttpProber::new(timeout),
            tcp: TcpProber::new(timeout),
            dns: DnsProber::new(timeout),
            tls: TlsProber::new(timeout),
        }
    }

    /// Probe all endpoints from the current target list.
    pub async fn probe(&self) -> Result<Vec<CollectedMetric>, ProberError> {
        let endpoints: Vec<(String, String, EndpointType)> = {
            let targets = self
                .targets
                .read()
                .map_err(|_| ProberError::LockPoisoned)?;
            targets
                .iter()
                .flat_map(|t| {
                    t.endpoints.iter().map(|e| {
                        (t.id.clone(), e.url.clone(), e.endpoint_type)
                    })
                })
                .collect()
        };

        if endpoints.is_empty() {
            return Ok(Vec::new());
        }

        let mut metrics = Vec::new();

        for (target_id, url, ep_type) in &endpoints {
            let result = match ep_type {
                EndpointType::Health => {
                    self.http.check(target_id, url).await
                }
                EndpointType::TcpProbe => {
                    let addr = url.strip_prefix("tcp://").unwrap_or(url);
                    self.tcp.check(target_id, addr).await
                }
                EndpointType::DnsProbe => {
                    self.dns.check(target_id, url).await
                }
                EndpointType::TlsProbe => {
                    let addr = url.strip_prefix("tls://").unwrap_or(url);
                    self.tls.check(target_id, addr).await
                }
                _ => continue,
            };

            debug!(
                target_id,
                check_type = %result.check_type,
                status = %result.status,
                latency_ms = result.latency_ms,
                "probe complete"
            );

            metrics.extend(probe_result_to_metrics(&result));
        }

        Ok(metrics)
    }
}

/// Convert a [`ProbeResult`] into [`CollectedMetric`] samples.
///
/// Produces two metrics per probe:
/// - `probe_latency_ms` — response time in milliseconds
/// - `probe_up` — 1.0 if Ok, 0.0 otherwise
pub fn probe_result_to_metrics(result: &ProbeResult) -> Vec<CollectedMetric> {
    let mut labels = HashMap::new();
    labels.insert("target".to_owned(), result.target_id.clone());
    labels.insert("check_type".to_owned(), result.check_type.to_string());
    if let Some(ref details) = result.details {
        labels.insert("details".to_owned(), details.clone());
    }

    let up_value = match result.status {
        aether_core::models::ProbeStatus::Ok => 1.0,
        _ => 0.0,
    };

    vec![
        CollectedMetric {
            name: "probe_latency_ms".to_owned(),
            value: result.latency_ms,
            labels: labels.clone(),
            timestamp: result.timestamp,
        },
        CollectedMetric {
            name: "probe_up".to_owned(),
            value: up_value,
            labels,
            timestamp: result.timestamp,
        },
    ]
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
    use std::time::SystemTime;

    use super::*;
    use aether_core::models::{CheckType, Endpoint, ProbeStatus, TargetKind};

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

        let metrics = result.expect("should succeed");
        assert_eq!(metrics.len(), 2, "should produce latency + up metrics");

        let up = metrics.iter().find(|m| m.name == "probe_up");
        assert!(up.is_some(), "should have probe_up metric");
        assert!((up.map(|m| m.value).unwrap_or(1.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_probe_result_to_metric_sample() {
        let result = ProbeResult {
            target_id: "svc-1".to_owned(),
            check_type: CheckType::HttpHealth,
            status: ProbeStatus::Ok,
            latency_ms: 42.5,
            details: Some("status_code=200".to_owned()),
            timestamp: SystemTime::now(),
        };

        let metrics = probe_result_to_metrics(&result);
        assert_eq!(metrics.len(), 2);

        let latency = &metrics[0];
        assert_eq!(latency.name, "probe_latency_ms");
        assert!((latency.value - 42.5).abs() < f64::EPSILON);
        assert_eq!(latency.labels["target"], "svc-1");
        assert_eq!(latency.labels["check_type"], "http_health");

        let up = &metrics[1];
        assert_eq!(up.name, "probe_up");
        assert!((up.value - 1.0).abs() < f64::EPSILON);
    }
}

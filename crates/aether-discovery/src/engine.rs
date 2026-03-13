use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use tracing::info;

use aether_core::models::{Endpoint, EndpointType, Target, TargetKind};
use aether_core::traits::ServiceDiscovery;

use crate::probe::MetricsProbe;
use crate::scanner::PortScanner;

/// Known pattern for a service on a specific port.
#[derive(Debug, Clone)]
struct KnownPattern {
    service_name: String,
    kind: TargetKind,
    metrics_path: Option<String>,
}

/// Orchestrates port scanning and metrics probing to discover services.
pub struct DiscoveryEngine {
    scanner: PortScanner,
    probe: MetricsProbe,
    known_patterns: HashMap<u16, KnownPattern>,
    host: String,
    interval: Duration,
}

impl DiscoveryEngine {
    /// Create an engine that scans `host` with the given port list and interval.
    pub fn new(host: String, ports: Vec<u16>, interval: Duration) -> Self {
        let scanner = PortScanner::new(ports);
        let probe = MetricsProbe::new();
        let known_patterns = build_known_patterns();

        Self {
            scanner,
            probe,
            known_patterns,
            host,
            interval,
        }
    }

    /// Discovery interval for periodic scanning.
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

#[async_trait]
impl ServiceDiscovery for DiscoveryEngine {
    async fn discover(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>> {
        info!(host = %self.host, "starting service discovery");

        let open_ports = self.scanner.scan(&self.host).await;
        let mut targets = Vec::with_capacity(open_ports.len());

        for open in &open_ports {
            let pattern = self.known_patterns.get(&open.port);

            let name = pattern
                .map(|p| p.service_name.clone())
                .or_else(|| open.service_hint.clone())
                .unwrap_or_else(|| format!("unknown-{}", open.port));

            let kind = pattern
                .map(|p| p.kind)
                .unwrap_or(TargetKind::Service);

            let mut endpoints = vec![Endpoint {
                url: format!("tcp://{}:{}", self.host, open.port),
                endpoint_type: EndpointType::TcpProbe,
            }];

            // Try metrics probe if pattern suggests a metrics path.
            if let Some(path) = pattern.and_then(|p| p.metrics_path.as_deref()) {
                let metrics_url = format!("http://{}:{}{}", self.host, open.port, path);
                if let Some(me) = self.probe.check(&metrics_url).await {
                    endpoints.push(Endpoint {
                        url: me.url,
                        endpoint_type: EndpointType::Prometheus,
                    });
                }
            }

            // For generic HTTP ports without a known pattern, try /metrics.
            if pattern.is_none() && matches!(open.port, 8080 | 9090 | 3000 | 4000) {
                let metrics_url =
                    format!("http://{}:{}/metrics", self.host, open.port);
                if let Some(me) = self.probe.check(&metrics_url).await {
                    endpoints.push(Endpoint {
                        url: me.url,
                        endpoint_type: EndpointType::Prometheus,
                    });
                }
            }

            let id = format!("disc-{}-{}", self.host, open.port);

            targets.push(Target {
                id,
                name,
                kind,
                endpoints,
                labels: HashMap::new(),
                discovered_at: SystemTime::now(),
            });
        }

        info!(count = targets.len(), "discovery complete");
        Ok(targets)
    }

    fn name(&self) -> &str {
        "port-scanner"
    }
}

/// Build the map of well-known port → service patterns.
fn build_known_patterns() -> HashMap<u16, KnownPattern> {
    let entries: Vec<(u16, &str, TargetKind, Option<&str>)> = vec![
        (5432, "postgresql", TargetKind::Service, None),
        (6379, "redis", TargetKind::Service, None),
        (3306, "mysql", TargetKind::Service, None),
        (27017, "mongodb", TargetKind::Service, None),
        (9200, "elasticsearch", TargetKind::Service, Some("/_prometheus/metrics")),
        (80, "nginx", TargetKind::Service, None),
        (443, "nginx-tls", TargetKind::Service, None),
        (9090, "prometheus", TargetKind::Service, Some("/metrics")),
        (8080, "http-service", TargetKind::Service, Some("/metrics")),
    ];

    entries
        .into_iter()
        .map(|(port, name, kind, path)| {
            (
                port,
                KnownPattern {
                    service_name: name.to_owned(),
                    kind,
                    metrics_path: path.map(String::from),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_engine_construction() {
        let engine = DiscoveryEngine::new(
            "127.0.0.1".to_owned(),
            vec![80, 443, 5432, 6379],
            Duration::from_secs(60),
        );
        assert_eq!(engine.name(), "port-scanner");
        assert_eq!(engine.interval(), Duration::from_secs(60));
    }

    #[test]
    fn test_known_patterns_populated() {
        let patterns = build_known_patterns();
        assert_eq!(patterns.get(&5432).map(|p| p.service_name.as_str()), Some("postgresql"));
        assert_eq!(patterns.get(&6379).map(|p| p.service_name.as_str()), Some("redis"));
        assert_eq!(patterns.get(&9200).map(|p| p.service_name.as_str()), Some("elasticsearch"));
        assert!(patterns.get(&9200).and_then(|p| p.metrics_path.as_deref()).is_some());
        assert!(patterns.get(&5432).and_then(|p| p.metrics_path.as_deref()).is_none());
    }
}

//! DNS resolution prober.

use std::time::{Duration, Instant, SystemTime};

use tokio::net::lookup_host;
use tracing::{debug, warn};

use aether_core::models::{CheckType, ProbeResult, ProbeStatus};

/// Performs DNS resolution probes to measure name resolution time.
pub struct DnsProber {
    timeout: Duration,
}

impl DnsProber {
    /// Create a DNS prober with the given resolution timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Resolve the given hostname and measure resolution time.
    ///
    /// The hostname must include a port (e.g. "example.com:443") as required
    /// by `tokio::net::lookup_host`.
    pub async fn check(&self, target_id: &str, hostname: &str) -> ProbeResult {
        let start = Instant::now();

        // lookup_host requires host:port format
        let lookup_addr = if hostname.contains(':') {
            hostname.to_owned()
        } else {
            format!("{hostname}:0")
        };

        let result = tokio::time::timeout(self.timeout, lookup_host(&lookup_addr)).await;

        match result {
            Ok(Ok(addrs)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
                let ip_count = ips.len();

                debug!(target_id, hostname, latency_ms, ip_count, "dns probe ok");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::DnsResolve,
                    status: if ips.is_empty() {
                        ProbeStatus::Failed
                    } else {
                        ProbeStatus::Ok
                    },
                    latency_ms,
                    details: Some(format!("resolved={}", ips.join(","))),
                    timestamp: SystemTime::now(),
                }
            }
            Ok(Err(e)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                warn!(target_id, hostname, "dns probe failed: {e}");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::DnsResolve,
                    status: ProbeStatus::Failed,
                    latency_ms,
                    details: Some(format!("error={e}")),
                    timestamp: SystemTime::now(),
                }
            }
            Err(_) => {
                let latency_ms = self.timeout.as_secs_f64() * 1000.0;
                warn!(target_id, hostname, "dns probe timed out");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::DnsResolve,
                    status: ProbeStatus::Timeout,
                    latency_ms,
                    details: Some("timeout".to_owned()),
                    timestamp: SystemTime::now(),
                }
            }
        }
    }

    /// Returns the configured timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_prober_creation() {
        let prober = DnsProber::new(Duration::from_secs(5));
        assert_eq!(prober.timeout().as_secs(), 5);
    }

    #[tokio::test]
    async fn test_dns_resolve_localhost() {
        let prober = DnsProber::new(Duration::from_secs(2));
        let result = prober.check("test", "localhost").await;
        assert_eq!(result.check_type, CheckType::DnsResolve);
        assert_eq!(result.status, ProbeStatus::Ok, "localhost should resolve");
        assert!(result.latency_ms >= 0.0);
        assert!(result.details.is_some());
    }
}

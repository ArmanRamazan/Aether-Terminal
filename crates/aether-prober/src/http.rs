//! HTTP health-check prober.

use std::time::{Instant, SystemTime};

use tracing::{debug, warn};

use aether_core::models::{CheckType, ProbeResult, ProbeStatus};

/// Performs HTTP GET health checks against URLs.
pub struct HttpProber {
    client: reqwest::Client,
    timeout: std::time::Duration,
}

impl HttpProber {
    /// Create an HTTP prober with the given timeout.
    pub fn new(timeout: std::time::Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();
        Self { client, timeout }
    }

    /// Probe the given URL with an HTTP GET request.
    ///
    /// Returns Ok if 2xx, Degraded if 3xx/4xx, Failed if 5xx or error.
    pub async fn check(&self, target_id: &str, url: &str) -> ProbeResult {
        let start = Instant::now();

        match self.client.get(url).send().await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                let status_code = resp.status().as_u16();

                let status = if resp.status().is_success() {
                    ProbeStatus::Ok
                } else if resp.status().is_server_error() {
                    ProbeStatus::Failed
                } else {
                    ProbeStatus::Degraded
                };

                debug!(target_id, url, latency_ms, status_code, "http probe");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::HttpHealth,
                    status,
                    latency_ms,
                    details: Some(format!("status_code={status_code}")),
                    timestamp: SystemTime::now(),
                }
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                let status = if e.is_timeout() {
                    ProbeStatus::Timeout
                } else {
                    ProbeStatus::Failed
                };

                warn!(target_id, url, "http probe failed: {e}");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::HttpHealth,
                    status,
                    latency_ms,
                    details: Some(format!("error={e}")),
                    timestamp: SystemTime::now(),
                }
            }
        }
    }

    /// Returns the configured timeout.
    pub fn timeout(&self) -> std::time::Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_prober_creation() {
        let prober = HttpProber::new(std::time::Duration::from_secs(5));
        assert_eq!(prober.timeout().as_secs(), 5);
    }

    #[tokio::test]
    async fn test_http_probe_localhost() {
        let prober = HttpProber::new(std::time::Duration::from_millis(500));
        // localhost likely refuses or 404s, but should not panic
        let result = prober.check("test", "http://127.0.0.1:1").await;
        assert_eq!(result.check_type, CheckType::HttpHealth);
        assert!(
            matches!(result.status, ProbeStatus::Failed | ProbeStatus::Timeout),
            "unreachable port should fail or timeout"
        );
        assert!(result.latency_ms >= 0.0);
    }
}

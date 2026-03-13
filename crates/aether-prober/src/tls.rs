//! TLS certificate prober.

use std::time::{Duration, Instant, SystemTime};

use tokio::net::TcpStream;
use tracing::{debug, warn};

use aether_core::models::{CheckType, ProbeResult, ProbeStatus};

/// Performs TLS handshake probes to check certificate validity and expiry.
pub struct TlsProber {
    timeout: Duration,
}

impl TlsProber {
    /// Create a TLS prober with the given handshake timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Perform a TLS handshake and check certificate status.
    ///
    /// The address should be "host:port" (e.g. "example.com:443").
    /// Uses `rustls` via `reqwest` for the TLS connection.
    ///
    /// Status: Ok if handshake succeeds, Failed if refused/error, Timeout if timed out.
    /// Certificate expiry details require HTTPS probe — this checks connectivity only.
    pub async fn check(&self, target_id: &str, addr: &str) -> ProbeResult {
        let start = Instant::now();

        // First establish TCP, then attempt TLS via an HTTPS GET
        let tcp_result = tokio::time::timeout(self.timeout, TcpStream::connect(addr)).await;

        match tcp_result {
            Ok(Ok(_stream)) => {
                let tcp_latency = start.elapsed();
                // TCP connected — now try HTTPS to validate TLS
                let (host, _port) = split_host_port(addr);
                let url = format!("https://{addr}");
                let remaining = self.timeout.saturating_sub(tcp_latency);

                let client = reqwest::Client::builder()
                    .timeout(remaining)
                    .build()
                    .unwrap_or_default();

                match client.head(&url).send().await {
                    Ok(resp) => {
                        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                        debug!(target_id, addr, latency_ms, "tls probe ok");

                        // We can't extract cert expiry from reqwest directly,
                        // but a successful HTTPS means valid TLS handshake
                        let details = format!(
                            "tls_ok=true, host={host}, status_code={}",
                            resp.status().as_u16()
                        );

                        ProbeResult {
                            target_id: target_id.to_owned(),
                            check_type: CheckType::TlsCertificate,
                            status: ProbeStatus::Ok,
                            latency_ms,
                            details: Some(details),
                            timestamp: SystemTime::now(),
                        }
                    }
                    Err(e) => {
                        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                        // Distinguish TLS errors from other failures
                        let is_tls_error = e.to_string().to_lowercase().contains("tls")
                            || e.to_string().to_lowercase().contains("certificate")
                            || e.to_string().to_lowercase().contains("ssl");

                        let status = if e.is_timeout() {
                            ProbeStatus::Timeout
                        } else if is_tls_error {
                            ProbeStatus::Failed
                        } else {
                            ProbeStatus::Degraded
                        };

                        warn!(target_id, addr, "tls probe failed: {e}");

                        ProbeResult {
                            target_id: target_id.to_owned(),
                            check_type: CheckType::TlsCertificate,
                            status,
                            latency_ms,
                            details: Some(format!("error={e}")),
                            timestamp: SystemTime::now(),
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tls probe tcp connect failed: {e}");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::TlsCertificate,
                    status: ProbeStatus::Failed,
                    latency_ms,
                    details: Some(format!("tcp_error={e}")),
                    timestamp: SystemTime::now(),
                }
            }
            Err(_) => {
                let latency_ms = self.timeout.as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tls probe timed out");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::TlsCertificate,
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

/// Split "host:port" into (host, port). Returns (addr, "") if no colon.
fn split_host_port(addr: &str) -> (&str, &str) {
    match addr.rsplit_once(':') {
        Some((host, port)) => (host, port),
        None => (addr, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_prober_creation() {
        let prober = TlsProber::new(Duration::from_secs(5));
        assert_eq!(prober.timeout().as_secs(), 5);
    }

    #[test]
    fn test_split_host_port() {
        assert_eq!(split_host_port("example.com:443"), ("example.com", "443"));
        assert_eq!(split_host_port("localhost"), ("localhost", ""));
    }

    #[tokio::test]
    async fn test_tls_probe_unreachable() {
        let prober = TlsProber::new(Duration::from_millis(100));
        let result = prober.check("test", "127.0.0.1:1").await;
        assert_eq!(result.check_type, CheckType::TlsCertificate);
        assert!(
            matches!(result.status, ProbeStatus::Failed | ProbeStatus::Timeout),
            "unreachable port should fail or timeout"
        );
    }
}

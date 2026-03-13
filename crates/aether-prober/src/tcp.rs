//! TCP connectivity prober.

use std::time::{Duration, Instant, SystemTime};

use tokio::net::TcpStream;
use tracing::{debug, warn};

use aether_core::models::{CheckType, ProbeResult, ProbeStatus};

/// Performs TCP connect probes to measure port reachability and latency.
pub struct TcpProber {
    timeout: Duration,
}

impl TcpProber {
    /// Create a TCP prober with the given connection timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Probe the given address (host:port) with a TCP connect.
    ///
    /// Returns Ok if connected, Timeout if timed out, Failed if refused.
    pub async fn check(&self, target_id: &str, addr: &str) -> ProbeResult {
        let start = Instant::now();

        let result = tokio::time::timeout(self.timeout, TcpStream::connect(addr)).await;

        match result {
            Ok(Ok(_stream)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                debug!(target_id, addr, latency_ms, "tcp probe ok");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::TcpConnect,
                    status: ProbeStatus::Ok,
                    latency_ms,
                    details: Some("connected".to_owned()),
                    timestamp: SystemTime::now(),
                }
            }
            Ok(Err(e)) => {
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tcp probe failed: {e}");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::TcpConnect,
                    status: ProbeStatus::Failed,
                    latency_ms,
                    details: Some(format!("error={e}")),
                    timestamp: SystemTime::now(),
                }
            }
            Err(_) => {
                let latency_ms = self.timeout.as_secs_f64() * 1000.0;
                warn!(target_id, addr, "tcp probe timed out");

                ProbeResult {
                    target_id: target_id.to_owned(),
                    check_type: CheckType::TcpConnect,
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
    fn test_tcp_prober_creation() {
        let prober = TcpProber::new(Duration::from_secs(3));
        assert_eq!(prober.timeout().as_secs(), 3);
    }

    #[tokio::test]
    async fn test_tcp_probe_connect_refused() {
        let prober = TcpProber::new(Duration::from_millis(100));
        let result = prober.check("test", "127.0.0.1:1").await;
        assert_eq!(result.check_type, CheckType::TcpConnect);
        assert!(
            matches!(result.status, ProbeStatus::Failed | ProbeStatus::Timeout),
            "port 1 should be refused or timeout"
        );
    }
}

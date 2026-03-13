use std::time::Duration;

use tokio::net::TcpStream;
use tracing::{debug, trace};

/// A discovered open TCP port with an optional service hint.
#[derive(Debug, Clone)]
pub struct OpenPort {
    pub port: u16,
    pub service_hint: Option<String>,
}

/// Scans TCP ports on a given host.
pub struct PortScanner {
    ports: Vec<u16>,
    timeout: Duration,
}

impl PortScanner {
    /// Create a scanner for the given port list with a 2-second default timeout.
    pub fn new(ports: Vec<u16>) -> Self {
        Self {
            ports,
            timeout: Duration::from_secs(2),
        }
    }

    /// Scan all configured ports on `host`, returning those that accept TCP connections.
    pub async fn scan(&self, host: &str) -> Vec<OpenPort> {
        let mut results = Vec::new();

        for &port in &self.ports {
            let addr = format!("{host}:{port}");
            trace!(addr, "probing port");

            let connected =
                tokio::time::timeout(self.timeout, TcpStream::connect(&addr)).await;

            match connected {
                Ok(Ok(_stream)) => {
                    let hint = known_service(port);
                    debug!(port, service = ?hint, "port open");
                    results.push(OpenPort {
                        port,
                        service_hint: hint,
                    });
                }
                Ok(Err(_)) | Err(_) => {
                    // Connection refused or timeout — port closed/filtered.
                }
            }
        }

        results
    }
}

/// Map well-known ports to service names.
pub(crate) fn known_service(port: u16) -> Option<String> {
    let name = match port {
        80 => "nginx",
        443 => "nginx-tls",
        3306 => "mysql",
        5432 => "postgresql",
        6379 => "redis",
        8080 => "http-alt",
        9090 => "prometheus",
        9200 => "elasticsearch",
        27017 => "mongodb",
        _ => return None,
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_port_mapping() {
        assert_eq!(known_service(5432).as_deref(), Some("postgresql"));
        assert_eq!(known_service(6379).as_deref(), Some("redis"));
        assert_eq!(known_service(3306).as_deref(), Some("mysql"));
        assert_eq!(known_service(27017).as_deref(), Some("mongodb"));
        assert_eq!(known_service(9200).as_deref(), Some("elasticsearch"));
        assert_eq!(known_service(11111), None, "unknown port returns None");
    }

    #[tokio::test]
    async fn test_scan_localhost() {
        // Scan a few common ports on localhost.
        // In CI/dev at least one port is usually open (e.g. SSH on 22, or nothing).
        // We just verify the scan completes without panicking.
        let scanner = PortScanner::new(vec![22, 80, 443, 8080]);
        let results = scanner.scan("127.0.0.1").await;
        // Results may be empty — that's fine, we're testing the scan logic.
        for open in &results {
            assert!(open.port > 0);
        }
    }
}

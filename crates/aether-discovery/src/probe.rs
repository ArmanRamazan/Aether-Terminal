use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, trace};

/// A confirmed Prometheus/OpenMetrics endpoint.
#[derive(Debug, Clone)]
pub struct MetricsEndpoint {
    pub url: String,
    pub metric_count: usize,
}

/// Probes HTTP endpoints for Prometheus text-format metrics.
pub struct MetricsProbe {
    timeout: Duration,
}

impl MetricsProbe {
    /// Create a probe with a 3-second default timeout.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(3),
        }
    }

    /// Try to fetch `url` and check if it returns Prometheus text format.
    ///
    /// Returns `Some(MetricsEndpoint)` if the response looks like metrics,
    /// `None` otherwise (connection refused, timeout, non-metrics response).
    pub async fn check(&self, url: &str) -> Option<MetricsEndpoint> {
        trace!(url, "probing metrics endpoint");

        let (host, port, path) = parse_url(url)?;
        let addr = format!("{host}:{port}");

        let mut stream = tokio::time::timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .ok()?
            .ok()?;

        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.ok()?;

        let mut buf = Vec::with_capacity(8192);
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut buf))
            .await
            .ok()?
            .ok()?;

        let body = String::from_utf8_lossy(&buf);

        // Find the body after the HTTP headers.
        let body = body.split("\r\n\r\n").nth(1).unwrap_or("");

        if is_prometheus_format(body) {
            let metric_count = count_metrics(body);
            debug!(url, metric_count, "metrics endpoint confirmed");
            Some(MetricsEndpoint {
                url: url.to_owned(),
                metric_count,
            })
        } else {
            None
        }
    }
}

impl Default for MetricsProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal URL parser for http://host:port/path.
fn parse_url(url: &str) -> Option<(String, u16, String)> {
    let url = url.strip_prefix("http://")?;
    let (host_port, path) = match url.find('/') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, "/"),
    };
    let (host, port) = match host_port.find(':') {
        Some(i) => (&host_port[..i], host_port[i + 1..].parse().ok()?),
        None => (host_port, 80),
    };
    Some((host.to_owned(), port, path.to_owned()))
}

/// Check if text looks like Prometheus exposition format.
/// Lines matching `metric_name value` or starting with `# HELP`/`# TYPE` qualify.
fn is_prometheus_format(body: &str) -> bool {
    body.lines().any(|line| {
        line.starts_with("# HELP ") || line.starts_with("# TYPE ")
    })
}

/// Count metric lines (non-comment, non-empty).
fn count_metrics(body: &str) -> usize {
    body.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_full() {
        let (host, port, path) = parse_url("http://localhost:9090/metrics").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 9090);
        assert_eq!(path, "/metrics");
    }

    #[test]
    fn test_parse_url_default_port() {
        let (host, port, path) = parse_url("http://example.com/metrics").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/metrics");
    }

    #[test]
    fn test_parse_url_no_path() {
        let (host, port, path) = parse_url("http://localhost:8080").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 8080);
        assert_eq!(path, "/");
    }

    #[test]
    fn test_parse_url_invalid() {
        assert!(parse_url("ftp://example.com").is_none());
        assert!(parse_url("not-a-url").is_none());
    }

    #[test]
    fn test_is_prometheus_format() {
        let body = "# HELP http_requests Total requests\n# TYPE http_requests counter\nhttp_requests 42\n";
        assert!(is_prometheus_format(body));
    }

    #[test]
    fn test_is_not_prometheus_format() {
        assert!(!is_prometheus_format("<html>hello</html>"));
        assert!(!is_prometheus_format(""));
    }

    #[test]
    fn test_count_metrics() {
        let body = "# HELP foo help\n# TYPE foo gauge\nfoo 1\nbar 2\n";
        assert_eq!(count_metrics(body), 2);
    }
}

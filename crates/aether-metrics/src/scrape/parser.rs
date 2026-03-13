//! Prometheus text exposition format parser.
//!
//! Parses the standard Prometheus text format into structured samples,
//! tracking `# TYPE` and `# HELP` metadata for each metric family.

use std::collections::BTreeMap;

/// Prometheus metric type as declared by `# TYPE` comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
    Untyped,
}

/// A single parsed sample from a Prometheus text exposition body.
#[derive(Debug, Clone)]
pub struct ScrapedSample {
    /// Metric name (e.g. `http_requests_total`, `request_duration_bucket`).
    pub name: String,
    /// Label key-value pairs, sorted by key.
    pub labels: BTreeMap<String, String>,
    /// Observed numeric value.
    pub value: f64,
    /// Optional timestamp in milliseconds (from the exposition line).
    pub timestamp_ms: Option<i64>,
    /// Metric type from the most recent `# TYPE` declaration, if any.
    pub metric_type: MetricType,
    /// Help text from the most recent `# HELP` declaration, if any.
    pub help: Option<String>,
}

/// Parse a complete Prometheus text exposition body into samples.
///
/// Tracks `# HELP` and `# TYPE` metadata and attaches them to
/// subsequent metric lines belonging to the same family.
pub fn parse_prometheus_text(body: &str) -> Vec<ScrapedSample> {
    let mut samples = Vec::new();
    let mut current_help: Option<(String, String)> = None;
    let mut current_type: Option<(String, MetricType)> = None;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("# HELP ") {
            if let Some((name, help)) = split_first_word(rest) {
                current_help = Some((name.to_owned(), help.to_owned()));
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("# TYPE ") {
            if let Some((name, type_str)) = split_first_word(rest) {
                let metric_type = parse_metric_type(type_str);
                current_type = Some((name.to_owned(), metric_type));
            }
            continue;
        }

        // Skip other comments
        if line.starts_with('#') {
            continue;
        }

        if let Some(sample) = parse_sample_line(line, &current_type, &current_help) {
            samples.push(sample);
        }
    }

    samples
}

/// Parse a metric type string into the enum.
fn parse_metric_type(s: &str) -> MetricType {
    match s.trim() {
        "counter" => MetricType::Counter,
        "gauge" => MetricType::Gauge,
        "histogram" => MetricType::Histogram,
        "summary" => MetricType::Summary,
        _ => MetricType::Untyped,
    }
}

/// Parse a single metric line into a `ScrapedSample`.
///
/// Format: `metric_name{label="val",...} value [timestamp_ms]`
/// or:     `metric_name value [timestamp_ms]`
fn parse_sample_line(
    line: &str,
    current_type: &Option<(String, MetricType)>,
    current_help: &Option<(String, String)>,
) -> Option<ScrapedSample> {
    let (name, labels, rest) = if let Some(brace_start) = line.find('{') {
        let brace_end = line.find('}')?;
        let name = &line[..brace_start];
        let labels_str = &line[brace_start + 1..brace_end];
        let rest = line[brace_end + 1..].trim();
        (name, parse_labels(labels_str), rest)
    } else {
        let mut parts = line.splitn(2, char::is_whitespace);
        let name = parts.next()?;
        let rest = parts.next()?.trim();
        (name, BTreeMap::new(), rest)
    };

    if name.is_empty() {
        return None;
    }

    // Parse value and optional timestamp
    let mut tokens = rest.split_whitespace();
    let value = parse_value(tokens.next()?)?;
    let timestamp_ms = tokens.next().and_then(|t| t.parse::<i64>().ok());

    // Match against TYPE/HELP: try exact name first, then strip suffixes
    let metric_type = lookup_type(name, current_type);
    let help = lookup_help(name, current_help);

    Some(ScrapedSample {
        name: name.to_owned(),
        labels,
        value,
        timestamp_ms,
        metric_type,
        help,
    })
}

/// Parse a value token, handling special Prometheus values.
fn parse_value(s: &str) -> Option<f64> {
    match s {
        "+Inf" => Some(f64::INFINITY),
        "-Inf" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        _ => s.parse::<f64>().ok(),
    }
}

/// Look up the metric type, trying exact match then suffix-stripped match.
fn lookup_type(name: &str, current_type: &Option<(String, MetricType)>) -> MetricType {
    let Some((type_name, mt)) = current_type else {
        return MetricType::Untyped;
    };
    if name == type_name {
        return *mt;
    }
    let family = strip_metric_suffixes(name);
    if family == type_name {
        return *mt;
    }
    MetricType::Untyped
}

/// Look up the help text, trying exact match then suffix-stripped match.
fn lookup_help(name: &str, current_help: &Option<(String, String)>) -> Option<String> {
    let (help_name, text) = current_help.as_ref()?;
    if name == help_name {
        return Some(text.clone());
    }
    let family = strip_metric_suffixes(name);
    if family == help_name {
        return Some(text.clone());
    }
    None
}

/// Extract the base metric family name from a sample name.
///
/// Strips histogram suffixes (`_bucket`, `_sum`, `_count`, `_total`)
/// and summary suffixes (`_sum`, `_count`) so the name matches the
/// `# TYPE` declaration.
fn strip_metric_suffixes(name: &str) -> &str {
    for suffix in &["_bucket", "_sum", "_count", "_total", "_created"] {
        if let Some(base) = name.strip_suffix(suffix) {
            return base;
        }
    }
    name
}

/// Parse label pairs from inside braces: `key1="val1",key2="val2"`.
fn parse_labels(s: &str) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let mut rest = s;

    while !rest.is_empty() {
        // Find key=
        let Some(eq_pos) = rest.find('=') else {
            break;
        };
        let key = rest[..eq_pos].trim().trim_start_matches(',').trim();
        rest = &rest[eq_pos + 1..];

        // Value must start with "
        rest = rest.trim_start();
        if !rest.starts_with('"') {
            break;
        }
        rest = &rest[1..]; // skip opening quote

        // Find closing quote, handling escaped quotes
        let mut value = String::new();
        let mut chars = rest.chars();
        let mut found_end = false;
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        match escaped {
                            'n' => value.push('\n'),
                            '\\' => value.push('\\'),
                            '"' => value.push('"'),
                            other => {
                                value.push('\\');
                                value.push(other);
                            }
                        }
                    }
                }
                '"' => {
                    found_end = true;
                    break;
                }
                other => value.push(other),
            }
        }

        if !found_end {
            break;
        }

        rest = chars.as_str().trim_start();
        // Skip comma separator
        rest = rest.trim_start_matches(',').trim_start();

        if !key.is_empty() {
            labels.insert(key.to_owned(), value);
        }
    }

    labels
}

/// Split a string into the first whitespace-delimited word and the rest.
fn split_first_word(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let idx = s.find(char::is_whitespace)?;
    Some((&s[..idx], s[idx..].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_counter() {
        let body = "\
# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method=\"GET\",status=\"200\"} 1234 1706000000000
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 1);
        let s = &samples[0];
        assert_eq!(s.name, "http_requests_total");
        assert_eq!(s.metric_type, MetricType::Counter);
        assert_eq!(s.help.as_deref(), Some("Total HTTP requests"));
        assert!((s.value - 1234.0).abs() < f64::EPSILON);
        assert_eq!(s.timestamp_ms, Some(1706000000000));
        assert_eq!(s.labels.get("method").map(String::as_str), Some("GET"));
        assert_eq!(s.labels.get("status").map(String::as_str), Some("200"));
    }

    #[test]
    fn test_parse_gauge() {
        let body = "\
# TYPE temperature gauge
temperature 36.6
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].metric_type, MetricType::Gauge);
        assert!((samples[0].value - 36.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_histogram() {
        let body = "\
# HELP request_duration Duration of requests
# TYPE request_duration histogram
request_duration_bucket{le=\"0.1\"} 10
request_duration_bucket{le=\"0.5\"} 25
request_duration_bucket{le=\"1.0\"} 30
request_duration_bucket{le=\"+Inf\"} 35
request_duration_sum 18.7
request_duration_count 35
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 6, "4 buckets + sum + count");

        // All should be Histogram type
        for s in &samples {
            assert_eq!(s.metric_type, MetricType::Histogram);
            assert_eq!(s.help.as_deref(), Some("Duration of requests"));
        }

        // Check bucket labels
        assert_eq!(
            samples[0].labels.get("le").map(String::as_str),
            Some("0.1")
        );
        assert!((samples[0].value - 10.0).abs() < f64::EPSILON);

        // +Inf bucket
        assert_eq!(
            samples[3].labels.get("le").map(String::as_str),
            Some("+Inf")
        );
        assert!((samples[3].value - 35.0).abs() < f64::EPSILON);

        // sum and count
        assert_eq!(samples[4].name, "request_duration_sum");
        assert!((samples[4].value - 18.7).abs() < f64::EPSILON);
        assert_eq!(samples[5].name, "request_duration_count");
        assert!((samples[5].value - 35.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_with_labels() {
        let body = "api_errors{service=\"auth\",env=\"prod\",code=\"500\"} 42\n";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 1);
        let s = &samples[0];
        assert_eq!(s.labels.len(), 3);
        assert_eq!(s.labels.get("service").map(String::as_str), Some("auth"));
        assert_eq!(s.labels.get("env").map(String::as_str), Some("prod"));
        assert_eq!(s.labels.get("code").map(String::as_str), Some("500"));
    }

    #[test]
    fn test_parse_multiline() {
        let body = "\
# HELP go_goroutines Number of goroutines
# TYPE go_goroutines gauge
go_goroutines 42
# HELP process_cpu_seconds_total Total user and system CPU time
# TYPE process_cpu_seconds_total counter
process_cpu_seconds_total 0.57
# HELP http_request_duration_seconds Duration histogram
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{le=\"0.01\"} 500
http_request_duration_seconds_bucket{le=\"0.1\"} 800
http_request_duration_seconds_bucket{le=\"+Inf\"} 1000
http_request_duration_seconds_sum 45.2
http_request_duration_seconds_count 1000
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 7, "1 gauge + 1 counter + 3 buckets + sum + count");

        assert_eq!(samples[0].name, "go_goroutines");
        assert_eq!(samples[0].metric_type, MetricType::Gauge);

        assert_eq!(samples[1].name, "process_cpu_seconds_total");
        assert_eq!(samples[1].metric_type, MetricType::Counter);

        assert_eq!(samples[2].metric_type, MetricType::Histogram);
        assert_eq!(samples[6].name, "http_request_duration_seconds_count");
    }

    #[test]
    fn test_parse_special_values() {
        let body = "\
some_metric 0
nan_metric NaN
inf_metric +Inf
neg_inf_metric -Inf
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 4);
        assert!((samples[0].value - 0.0).abs() < f64::EPSILON);
        assert!(samples[1].value.is_nan());
        assert!(samples[2].value.is_infinite() && samples[2].value > 0.0);
        assert!(samples[3].value.is_infinite() && samples[3].value < 0.0);
    }

    #[test]
    fn test_parse_empty_body() {
        assert!(parse_prometheus_text("").is_empty());
    }

    #[test]
    fn test_parse_comments_only() {
        let body = "# HELP foo help\n# TYPE foo gauge\n";
        assert!(parse_prometheus_text(body).is_empty());
    }

    #[test]
    fn test_parse_escaped_label_value() {
        let body = "metric{path=\"/foo\\\"bar\"} 1\n";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 1);
        assert_eq!(
            samples[0].labels.get("path").map(String::as_str),
            Some("/foo\"bar")
        );
    }

    #[test]
    fn test_parse_no_timestamp() {
        let body = "up 1\n";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 1);
        assert!(samples[0].timestamp_ms.is_none());
    }

    #[test]
    fn test_strip_metric_suffixes() {
        assert_eq!(strip_metric_suffixes("http_requests_total"), "http_requests");
        assert_eq!(strip_metric_suffixes("req_duration_bucket"), "req_duration");
        assert_eq!(strip_metric_suffixes("req_duration_sum"), "req_duration");
        assert_eq!(strip_metric_suffixes("req_duration_count"), "req_duration");
        assert_eq!(strip_metric_suffixes("plain_gauge"), "plain_gauge");
    }

    #[test]
    fn test_parse_summary() {
        let body = "\
# HELP rpc_duration RPC duration summary
# TYPE rpc_duration summary
rpc_duration{quantile=\"0.5\"} 0.042
rpc_duration{quantile=\"0.9\"} 0.105
rpc_duration{quantile=\"0.99\"} 0.230
rpc_duration_sum 17.0
rpc_duration_count 200
";
        let samples = parse_prometheus_text(body);
        assert_eq!(samples.len(), 5, "3 quantiles + sum + count");
        for s in &samples {
            assert_eq!(s.metric_type, MetricType::Summary);
        }
        assert_eq!(
            samples[0].labels.get("quantile").map(String::as_str),
            Some("0.5")
        );
    }
}

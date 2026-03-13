//! Application-level diagnostic rules for production monitoring.
//!
//! These rules detect issues visible through Prometheus-scraped metrics and
//! probe results: throughput drops, latency growth, error rate spikes,
//! connection pool saturation, health-check failures, TLS expiry, etc.

use std::time::Duration;

use aether_core::models::{DiagCategory, Severity};

use super::types::{CompareOp, Rule, RuleCondition};

/// Returns all application-level diagnostic rules.
pub fn application_rules() -> Vec<Rule> {
    vec![
        // 1. HTTP throughput dropped >30% in 5-minute window.
        Rule {
            id: "app_throughput_drop",
            name: "HTTP throughput drop",
            category: DiagCategory::ThroughputDrop,
            default_severity: Severity::Warning,
            condition: RuleCondition::RateChange {
                metric: "http_requests_total",
                window_secs: 300,
                threshold_percent: -30.0,
            },
            enabled: true,
        },
        // 2. P99 latency exceeds 500ms.
        Rule {
            id: "app_latency_p99_high",
            name: "P99 latency high",
            category: DiagCategory::LatencyHigh,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "http_latency_p99_ms",
                op: CompareOp::Gt,
                value: 500.0,
                sustained: None,
            },
            enabled: true,
        },
        // 3. P99 latency trending upward for 10 minutes.
        Rule {
            id: "app_latency_growing",
            name: "P99 latency growing",
            category: DiagCategory::LatencyHigh,
            default_severity: Severity::Warning,
            condition: RuleCondition::GrowthTrend {
                metric: "http_latency_p99_ms",
                window_secs: 600,
                slope_threshold: 0.0,
                sustained: Some(Duration::from_secs(10 * 60)),
            },
            enabled: true,
        },
        // 4. Connection pool utilization >80%.
        Rule {
            id: "app_connpool_saturated",
            name: "Connection pool saturated",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "connection_pool_usage_percent",
                op: CompareOp::Gt,
                value: 80.0,
                sustained: None,
            },
            enabled: true,
        },
        // 5. Error rate (5xx / total) exceeds 5%.
        Rule {
            id: "app_error_rate_spike",
            name: "Error rate spike",
            category: DiagCategory::ErrorRateHigh,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "error_rate_percent",
                op: CompareOp::Gt,
                value: 5.0,
                sustained: None,
            },
            enabled: true,
        },
        // 6. Health-check probe reports failure.
        Rule {
            id: "app_health_check_failed",
            name: "Health check failed",
            category: DiagCategory::HealthCheckFailed,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "probe_success",
                op: CompareOp::Lt,
                value: 1.0,
                sustained: None,
            },
            enabled: true,
        },
        // 7. TCP probe latency exceeds 200ms (degraded).
        Rule {
            id: "app_tcp_latency_degraded",
            name: "TCP latency degraded",
            category: DiagCategory::NetworkDegradation,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "probe_latency_ms",
                op: CompareOp::Gt,
                value: 200.0,
                sustained: None,
            },
            enabled: true,
        },
        // 8. TLS certificate expires within 30 days.
        Rule {
            id: "app_tls_expiry_warning",
            name: "TLS certificate expiring soon",
            category: DiagCategory::CertificateExpiry,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "tls_days_remaining",
                op: CompareOp::Lt,
                value: 30.0,
                sustained: None,
            },
            enabled: true,
        },
        // 9. DNS resolution latency exceeds 100ms.
        Rule {
            id: "app_dns_resolution_slow",
            name: "DNS resolution slow",
            category: DiagCategory::NetworkDegradation,
            default_severity: Severity::Info,
            condition: RuleCondition::Threshold {
                metric: "dns_latency_ms",
                op: CompareOp::Gt,
                value: 100.0,
                sustained: None,
            },
            enabled: true,
        },
        // 10. Disk I/O time saturated (>90%).
        Rule {
            id: "app_disk_io_saturated",
            name: "Disk I/O saturated",
            category: DiagCategory::DiskIoHeavy,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "disk_io_time_percent",
                op: CompareOp::Gt,
                value: 90.0,
                sustained: None,
            },
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_application_rules_count() {
        assert_eq!(
            application_rules().len(),
            10,
            "expected exactly 10 application rules"
        );
    }

    #[test]
    fn test_no_duplicate_app_rule_ids() {
        let rules = application_rules();
        let ids: HashSet<&str> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), rules.len(), "duplicate application rule IDs");
    }

    #[test]
    fn test_all_app_rules_enabled() {
        for rule in application_rules() {
            assert!(
                rule.enabled,
                "rule '{}' should be enabled by default",
                rule.id
            );
        }
    }

    #[test]
    fn test_all_app_rule_ids_prefixed() {
        for rule in application_rules() {
            assert!(
                rule.id.starts_with("app_"),
                "rule '{}' should have 'app_' prefix",
                rule.id
            );
        }
    }
}

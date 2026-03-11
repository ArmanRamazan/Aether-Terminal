use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use aether_core::metrics::HostId;
use aether_core::models::{
    DiagTarget, Diagnostic, Evidence, Recommendation, RecommendedAction, Urgency,
};

use crate::analyzers::capacity::CapacityAnalyzer;
use crate::analyzers::trend::TrendAnalyzer;
use crate::rules::types::RuleFinding;
use crate::store::MetricStore;

/// Converts rule findings into actionable diagnostics with evidence and recommendations.
pub struct RecommendationGenerator {
    next_id: AtomicU64,
}

impl RecommendationGenerator {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }

    /// Generate a diagnostic from a rule finding with trend/capacity context.
    pub fn generate(
        &self,
        finding: &RuleFinding,
        store: &MetricStore,
        trend: &TrendAnalyzer,
        capacity: &CapacityAnalyzer,
        host: &HostId,
    ) -> Diagnostic {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let evidence = self.build_evidence(finding, store, trend, host);
        let (action, urgency, auto_executable) = self.rule_action(finding, store, capacity, trend, host);

        let target_name = target_display(&finding.target);
        let summary = format!("{target_name}: {}", finding.rule_name);

        Diagnostic {
            id,
            host: host.clone(),
            target: finding.target.clone(),
            severity: finding.severity,
            category: finding.category.clone(),
            summary,
            evidence,
            recommendation: Recommendation {
                action,
                reason: self.build_reason(finding),
                urgency,
                auto_executable,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    fn build_evidence(
        &self,
        finding: &RuleFinding,
        store: &MetricStore,
        trend: &TrendAnalyzer,
        host: &HostId,
    ) -> Vec<Evidence> {
        finding
            .matched_values
            .iter()
            .map(|(metric, value)| {
                let trend_val = self.metric_trend(store, trend, host, &finding.target, metric);
                Evidence {
                    metric: metric.clone(),
                    current: *value,
                    threshold: 0.0, // threshold info not available from finding
                    trend: trend_val,
                    context: format!("{metric} = {value:.1}"),
                }
            })
            .collect()
    }

    fn metric_trend(
        &self,
        store: &MetricStore,
        trend: &TrendAnalyzer,
        host: &HostId,
        target: &DiagTarget,
        metric: &str,
    ) -> Option<f64> {
        let pid = match target {
            DiagTarget::Process { pid, .. } => Some(*pid),
            _ => None,
        };
        let series = store.get(host, pid, metric)?;
        let slope = trend.slope(series, Duration::from_secs(300));
        if slope.abs() > f64::EPSILON {
            Some(slope)
        } else {
            None
        }
    }

    fn rule_action(
        &self,
        finding: &RuleFinding,
        store: &MetricStore,
        _capacity: &CapacityAnalyzer,
        trend: &TrendAnalyzer,
        host: &HostId,
    ) -> (RecommendedAction, Urgency, bool) {
        match finding.rule_id {
            "mem_approaching_oom" => {
                let current = finding_value(finding, "mem_bytes").unwrap_or(0.0);
                let limit = (current / 0.9).ceil();
                let new_limit = limit * 2.0;
                (
                    RecommendedAction::ScaleUp {
                        resource: "memory".into(),
                        from: format_bytes(limit),
                        to: format_bytes(new_limit),
                    },
                    Urgency::Immediate,
                    true,
                )
            }
            "mem_leak_suspected" => {
                let pid = extract_pid(&finding.target);
                let context = if let Some(series) = store.get(host, pid, "mem_bytes") {
                    let slope = trend.slope(series, Duration::from_secs(900));
                    format!("memory growing at {:.0} bytes/sec", slope)
                } else {
                    "memory growth pattern detected".into()
                };
                (
                    RecommendedAction::Investigate { what: context },
                    Urgency::Soon,
                    false,
                )
            }
            "cpu_saturated" => (
                RecommendedAction::Investigate {
                    what: "CPU at saturation, check for busy loops or unbounded computation".into(),
                },
                Urgency::Immediate,
                false,
            ),
            "cpu_sustained_high" => (
                RecommendedAction::Investigate {
                    what: "sustained high CPU usage, consider profiling".into(),
                },
                Urgency::Soon,
                false,
            ),
            "disk_almost_full" => (
                RecommendedAction::ReduceLoad {
                    suggestion: "clean up old logs, temp files, or expand disk".into(),
                },
                Urgency::Immediate,
                false,
            ),
            "fd_approaching_limit" => {
                let current = finding_value(finding, "open_fds").unwrap_or(0.0) as u64;
                let limit = (current as f64 / 0.8).ceil() as u64;
                let new_limit = limit * 2;
                (
                    RecommendedAction::RaiseLimits {
                        limit_name: "nofile".into(),
                        from: limit.to_string(),
                        to: new_limit.to_string(),
                    },
                    Urgency::Soon,
                    false,
                )
            }
            "zombie_accumulation" => {
                let pid = extract_pid(&finding.target).unwrap_or(0);
                (
                    RecommendedAction::KillProcess {
                        pid,
                        reason: "parent accumulating zombie children".into(),
                    },
                    Urgency::Planning,
                    true,
                )
            }
            "thread_explosion" => (
                RecommendedAction::Investigate {
                    what: "thread count exceeds 1000, check for thread pool misconfiguration".into(),
                },
                Urgency::Soon,
                false,
            ),
            "crash_loop" => (
                RecommendedAction::Investigate {
                    what: "process restarting repeatedly, check logs for root cause".into(),
                },
                Urgency::Immediate,
                false,
            ),
            "connections_growing" => (
                RecommendedAction::NoAction {
                    reason: "connections growing but may be normal traffic".into(),
                },
                Urgency::Informational,
                false,
            ),
            _ => (
                RecommendedAction::Investigate {
                    what: format!("unknown rule '{}' triggered", finding.rule_id),
                },
                Urgency::Soon,
                false,
            ),
        }
    }

    fn build_reason(&self, finding: &RuleFinding) -> String {
        match finding.rule_id {
            "mem_approaching_oom" => "Memory usage above 90% of limit, OOM kill imminent".into(),
            "mem_leak_suspected" => "Sustained memory growth suggests a leak".into(),
            "cpu_saturated" => "CPU at >95% for extended period".into(),
            "cpu_sustained_high" => "CPU above 80% for extended period".into(),
            "disk_almost_full" => "Disk usage above 90%, writes may fail".into(),
            "fd_approaching_limit" => "File descriptors nearing ulimit".into(),
            "zombie_accumulation" => "Zombie processes accumulating, parent not reaping".into(),
            "thread_explosion" => "Thread count abnormally high".into(),
            "crash_loop" => "Process restarting in rapid succession".into(),
            "connections_growing" => "Network connections increasing steadily".into(),
            _ => format!("Rule '{}' matched", finding.rule_id),
        }
    }
}

impl Default for RecommendationGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn target_display(target: &DiagTarget) -> String {
    match target {
        DiagTarget::Process { name, pid, .. } => format!("{name} (pid {pid})"),
        DiagTarget::Host(id) => format!("host:{}", id.as_str()),
        DiagTarget::Container { name, .. } => format!("container:{name}"),
        DiagTarget::Disk { mount } => format!("disk:{mount}"),
        DiagTarget::Network { interface } => format!("net:{interface}"),
        _ => "unknown".into(),
    }
}

fn extract_pid(target: &DiagTarget) -> Option<u32> {
    match target {
        DiagTarget::Process { pid, .. } => Some(*pid),
        _ => None,
    }
}

fn finding_value(finding: &RuleFinding, metric: &str) -> Option<f64> {
    finding
        .matched_values
        .iter()
        .find(|(m, _)| m == metric)
        .map(|(_, v)| *v)
}

fn format_bytes(bytes: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{:.0} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{DiagCategory, Severity};

    fn make_finding(rule_id: &'static str, severity: Severity) -> RuleFinding {
        RuleFinding {
            rule_id,
            rule_name: match rule_id {
                "mem_approaching_oom" => "Memory approaching OOM",
                "cpu_saturated" => "CPU saturated",
                "connections_growing" => "Connections growing rapidly",
                _ => "Test rule",
            },
            target: DiagTarget::Process {
                pid: 1234,
                name: "test-app".into(),
            },
            severity,
            category: match rule_id {
                "mem_approaching_oom" => DiagCategory::MemoryPressure,
                "cpu_saturated" => DiagCategory::CpuSaturation,
                "connections_growing" => DiagCategory::ConnectionSurge,
                _ => DiagCategory::CapacityRisk,
            },
            matched_values: vec![("mem_bytes".into(), 900_000_000.0)],
        }
    }

    fn test_deps() -> (MetricStore, TrendAnalyzer, CapacityAnalyzer, HostId) {
        (
            MetricStore::new(64),
            TrendAnalyzer,
            CapacityAnalyzer,
            HostId::new("test"),
        )
    }

    #[test]
    fn test_generate_mem_oom_produces_scale_up() {
        let gen = RecommendationGenerator::new();
        let (store, trend, cap, host) = test_deps();
        let finding = make_finding("mem_approaching_oom", Severity::Critical);

        let diag = gen.generate(&finding, &store, &trend, &cap, &host);

        assert!(
            matches!(diag.recommendation.action, RecommendedAction::ScaleUp { .. }),
            "expected ScaleUp action"
        );
        assert_eq!(diag.recommendation.urgency, Urgency::Immediate);
        assert!(diag.recommendation.auto_executable);
    }

    #[test]
    fn test_generate_cpu_saturated_is_immediate() {
        let gen = RecommendationGenerator::new();
        let (store, trend, cap, host) = test_deps();
        let finding = make_finding("cpu_saturated", Severity::Critical);

        let diag = gen.generate(&finding, &store, &trend, &cap, &host);

        assert_eq!(diag.recommendation.urgency, Urgency::Immediate);
        assert_eq!(diag.severity, Severity::Critical);
    }

    #[test]
    fn test_generate_connections_growing_is_informational() {
        let gen = RecommendationGenerator::new();
        let (store, trend, cap, host) = test_deps();
        let finding = make_finding("connections_growing", Severity::Info);

        let diag = gen.generate(&finding, &store, &trend, &cap, &host);

        assert_eq!(diag.recommendation.urgency, Urgency::Informational);
        assert!(
            matches!(diag.recommendation.action, RecommendedAction::NoAction { .. }),
            "expected NoAction"
        );
    }

    #[test]
    fn test_ids_are_unique() {
        let gen = RecommendationGenerator::new();
        let (store, trend, cap, host) = test_deps();
        let finding = make_finding("mem_approaching_oom", Severity::Critical);

        let d1 = gen.generate(&finding, &store, &trend, &cap, &host);
        let d2 = gen.generate(&finding, &store, &trend, &cap, &host);
        let d3 = gen.generate(&finding, &store, &trend, &cap, &host);

        assert_ne!(d1.id, d2.id, "d1 and d2 should differ");
        assert_ne!(d2.id, d3.id, "d2 and d3 should differ");
        assert_ne!(d1.id, d3.id, "d1 and d3 should differ");
    }

    #[test]
    fn test_evidence_contains_matched_values() {
        let gen = RecommendationGenerator::new();
        let (store, trend, cap, host) = test_deps();
        let mut finding = make_finding("cpu_saturated", Severity::Critical);
        finding.matched_values = vec![
            ("cpu_percent".into(), 97.5),
            ("thread_count".into(), 42.0),
        ];

        let diag = gen.generate(&finding, &store, &trend, &cap, &host);

        assert_eq!(diag.evidence.len(), 2, "should have 2 evidence entries");
        assert_eq!(diag.evidence[0].metric, "cpu_percent");
        assert!((diag.evidence[0].current - 97.5).abs() < f64::EPSILON);
        assert_eq!(diag.evidence[1].metric, "thread_count");
        assert!((diag.evidence[1].current - 42.0).abs() < f64::EPSILON);
    }
}

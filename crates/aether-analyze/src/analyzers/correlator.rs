//! Cross-source diagnostic correlation with causal pattern detection.
//!
//! Groups diagnostics that likely share a root cause based on:
//! - Temporal proximity (within 60s window)
//! - Same target with different categories (compound issues)
//! - Known causal patterns between diagnostic categories

use std::time::Duration;

use aether_core::models::{DiagCategory, Diagnostic};

/// Maximum time gap between diagnostics to consider them temporally related.
const TIME_WINDOW: Duration = Duration::from_secs(60);

/// Minimum confidence to emit a correlated group.
const MIN_CONFIDENCE: f64 = 0.3;

/// A group of diagnostics that likely share a root cause.
#[derive(Debug, Clone)]
pub struct CorrelatedGroup {
    /// Indices into the original diagnostics slice.
    pub diagnostics: Vec<usize>,
    /// Identified root cause description, if a causal pattern matched.
    pub root_cause: Option<String>,
    /// Confidence that these diagnostics are truly related (0.0–1.0).
    pub confidence: f64,
}

/// Correlates diagnostics from different sources to find related issues.
#[derive(Debug)]
pub struct DiagnosticCorrelator;

impl DiagnosticCorrelator {
    /// Find groups of correlated diagnostics.
    ///
    /// Applies three correlation strategies in order of specificity:
    /// 1. Causal patterns (highest confidence)
    /// 2. Same-target compound issues
    /// 3. Temporal proximity
    pub fn correlate(diagnostics: &[Diagnostic]) -> Vec<CorrelatedGroup> {
        if diagnostics.len() < 2 {
            return Vec::new();
        }

        let mut used = vec![false; diagnostics.len()];
        let mut groups = Vec::new();

        // 1. Causal patterns — highest priority, mark as used.
        for i in 0..diagnostics.len() {
            if used[i] {
                continue;
            }
            for j in (i + 1)..diagnostics.len() {
                if used[j] {
                    continue;
                }
                if let Some(pattern) = find_causal_pattern(&diagnostics[i], &diagnostics[j]) {
                    groups.push(CorrelatedGroup {
                        diagnostics: vec![i, j],
                        root_cause: Some(pattern),
                        confidence: 0.9,
                    });
                    used[i] = true;
                    used[j] = true;
                }
            }
        }

        // 2. Same-target, different categories — compound issues.
        for i in 0..diagnostics.len() {
            if used[i] {
                continue;
            }
            let mut compound = vec![i];
            for j in (i + 1)..diagnostics.len() {
                if used[j] {
                    continue;
                }
                if same_target(&diagnostics[i], &diagnostics[j])
                    && diagnostics[i].category != diagnostics[j].category
                    && within_time_window(&diagnostics[i], &diagnostics[j])
                {
                    compound.push(j);
                }
            }
            if compound.len() >= 2 {
                let target_name = target_label(&diagnostics[i]);
                for &idx in &compound {
                    used[idx] = true;
                }
                groups.push(CorrelatedGroup {
                    diagnostics: compound,
                    root_cause: Some(format!("multiple issues on {target_name}")),
                    confidence: 0.7,
                });
            }
        }

        // 3. Time correlation — diagnostics within 60s window.
        for i in 0..diagnostics.len() {
            if used[i] {
                continue;
            }
            let mut temporal = vec![i];
            for j in (i + 1)..diagnostics.len() {
                if used[j] {
                    continue;
                }
                if within_time_window(&diagnostics[i], &diagnostics[j]) {
                    temporal.push(j);
                }
            }
            if temporal.len() >= 2 {
                for &idx in &temporal {
                    used[idx] = true;
                }
                groups.push(CorrelatedGroup {
                    diagnostics: temporal,
                    root_cause: None,
                    confidence: 0.4,
                });
            }
        }

        groups.retain(|g| g.confidence >= MIN_CONFIDENCE);
        groups
    }
}

/// Check if two diagnostics fall within the time correlation window.
fn within_time_window(a: &Diagnostic, b: &Diagnostic) -> bool {
    let dist = if a.detected_at >= b.detected_at {
        a.detected_at.duration_since(b.detected_at)
    } else {
        b.detected_at.duration_since(a.detected_at)
    };
    dist <= TIME_WINDOW
}

/// Check if two diagnostics target the same entity.
fn same_target(a: &Diagnostic, b: &Diagnostic) -> bool {
    a.target == b.target
}

/// Human-readable label for a diagnostic target.
fn target_label(d: &Diagnostic) -> String {
    use aether_core::models::DiagTarget;
    match &d.target {
        DiagTarget::Process { name, pid, .. } => format!("{name} (pid {pid})"),
        DiagTarget::Host(id) => format!("host:{}", id.as_str()),
        DiagTarget::Container { name, .. } => format!("container:{name}"),
        DiagTarget::Disk { mount } => format!("disk:{mount}"),
        DiagTarget::Network { interface } => format!("net:{interface}"),
        _ => "unknown".into(),
    }
}

/// Detect known causal patterns between two diagnostics.
///
/// Returns a root-cause description if the pair matches a known pattern.
fn find_causal_pattern(a: &Diagnostic, b: &Diagnostic) -> Option<String> {
    if !within_time_window(a, b) {
        return None;
    }

    let pair = (a.category, b.category);

    for &(cat_a, cat_b, description) in CAUSAL_PATTERNS {
        if (pair.0 == cat_a && pair.1 == cat_b) || (pair.0 == cat_b && pair.1 == cat_a) {
            return Some(description.to_string());
        }
    }

    None
}

/// Known causal relationships between diagnostic categories.
///
/// Format: (cause_category, effect_category, root_cause_description)
const CAUSAL_PATTERNS: &[(DiagCategory, DiagCategory, &str)] = &[
    // Connection pool saturated → throughput drops
    (
        DiagCategory::ConnectionSurge,
        DiagCategory::ThroughputDrop,
        "connection pool saturation causing throughput drop",
    ),
    // Health check failed + error rate spike → service down
    (
        DiagCategory::HealthCheckFailed,
        DiagCategory::ErrorRateHigh,
        "service failure: health check failed with error rate spike",
    ),
    // Memory growing + latency growing → memory leak causing GC pauses
    (
        DiagCategory::MemoryLeak,
        DiagCategory::LatencyHigh,
        "memory leak likely causing GC pauses and latency increase",
    ),
    // Memory pressure + CPU saturation → OOM thrashing
    (
        DiagCategory::MemoryPressure,
        DiagCategory::CpuSaturation,
        "memory pressure causing CPU thrashing (swap or GC overhead)",
    ),
    // Disk I/O heavy + latency high → disk bottleneck
    (
        DiagCategory::DiskIoHeavy,
        DiagCategory::LatencyHigh,
        "heavy disk I/O causing latency increase",
    ),
    // Thread explosion + CPU saturation → runaway threads
    (
        DiagCategory::ThreadExplosion,
        DiagCategory::CpuSaturation,
        "thread explosion causing CPU saturation",
    ),
    // FD exhaustion + connection surge → resource exhaustion cascade
    (
        DiagCategory::FdExhaustion,
        DiagCategory::ConnectionSurge,
        "file descriptor exhaustion blocking new connections",
    ),
    // Network degradation + error rate → upstream dependency failure
    (
        DiagCategory::NetworkDegradation,
        DiagCategory::ErrorRateHigh,
        "network degradation causing downstream error rate spike",
    ),
    // Disk pressure + crash loop → disk full causing crashes
    (
        DiagCategory::DiskPressure,
        DiagCategory::CrashLoop,
        "disk pressure causing application crash loop",
    ),
];

/// Build a correlation note to append to a diagnostic's recommendation reason.
pub fn correlation_note(group: &CorrelatedGroup, diagnostics: &[Diagnostic]) -> String {
    let related: Vec<String> = group
        .diagnostics
        .iter()
        .map(|&idx| diagnostics[idx].category.to_string())
        .collect();

    match &group.root_cause {
        Some(cause) => format!(
            "Correlated with {} (root cause: {cause})",
            related.join(", ")
        ),
        None => format!("Correlated with {} (time proximity)", related.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use aether_core::metrics::HostId;
    use aether_core::models::{
        DiagTarget, Evidence, Recommendation, RecommendedAction, Severity, Urgency,
    };

    use super::*;

    fn make_diagnostic(
        category: DiagCategory,
        target: DiagTarget,
        detected_at: Instant,
    ) -> Diagnostic {
        Diagnostic {
            id: 0,
            host: HostId::default(),
            target,
            severity: Severity::Warning,
            category,
            summary: format!("test: {category}"),
            evidence: vec![Evidence {
                metric: "test".into(),
                current: 1.0,
                threshold: 0.0,
                trend: None,
                context: "test".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "test".into(),
                },
                reason: "test reason".into(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at,
            resolved_at: None,
        }
    }

    fn process_target(pid: u32, name: &str) -> DiagTarget {
        DiagTarget::Process {
            pid,
            name: name.into(),
        }
    }

    #[test]
    fn test_time_correlation() {
        let now = Instant::now();
        let diags = vec![
            make_diagnostic(DiagCategory::CpuSaturation, process_target(1, "a"), now),
            make_diagnostic(
                DiagCategory::DiskPressure,
                process_target(2, "b"),
                now + Duration::from_secs(30),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert_eq!(groups.len(), 1, "two diagnostics within 60s should group");
        assert_eq!(groups[0].diagnostics.len(), 2);
        assert!(
            groups[0].confidence >= 0.3,
            "time correlation confidence should be >= 0.3"
        );
    }

    #[test]
    fn test_causal_pattern_pool_throughput() {
        let now = Instant::now();
        let diags = vec![
            make_diagnostic(
                DiagCategory::ConnectionSurge,
                process_target(1, "app"),
                now,
            ),
            make_diagnostic(
                DiagCategory::ThroughputDrop,
                process_target(2, "app"),
                now + Duration::from_secs(10),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert_eq!(groups.len(), 1);
        assert!(groups[0].root_cause.is_some());
        assert!(
            groups[0]
                .root_cause
                .as_ref()
                .unwrap()
                .contains("connection pool saturation"),
            "should identify pool saturation as root cause"
        );
        assert!(
            groups[0].confidence >= 0.8,
            "causal patterns should have high confidence"
        );
    }

    #[test]
    fn test_causal_pattern_memory_latency() {
        let now = Instant::now();
        let diags = vec![
            make_diagnostic(DiagCategory::MemoryLeak, process_target(1, "jvm"), now),
            make_diagnostic(
                DiagCategory::LatencyHigh,
                process_target(1, "jvm"),
                now + Duration::from_secs(5),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert_eq!(groups.len(), 1);
        assert!(groups[0]
            .root_cause
            .as_ref()
            .unwrap()
            .contains("memory leak"));
    }

    #[test]
    fn test_no_correlation_distant_time() {
        let now = Instant::now();
        let diags = vec![
            make_diagnostic(DiagCategory::CpuSaturation, process_target(1, "a"), now),
            make_diagnostic(
                DiagCategory::DiskPressure,
                process_target(2, "b"),
                now + Duration::from_secs(120),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert!(
            groups.is_empty(),
            "diagnostics 120s apart should not correlate"
        );
    }

    #[test]
    fn test_same_target_compound_issue() {
        let now = Instant::now();
        let target = process_target(42, "nginx");
        // Use categories without a causal pattern between them.
        let diags = vec![
            make_diagnostic(DiagCategory::CpuSaturation, target.clone(), now),
            make_diagnostic(
                DiagCategory::DiskPressure,
                target.clone(),
                now + Duration::from_secs(5),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert_eq!(groups.len(), 1);
        assert!(groups[0].confidence >= 0.6, "compound issues should have medium-high confidence");
        assert!(groups[0]
            .root_cause
            .as_ref()
            .unwrap()
            .contains("multiple issues"));
    }

    #[test]
    fn test_single_diagnostic_no_groups() {
        let now = Instant::now();
        let diags = vec![make_diagnostic(
            DiagCategory::CpuSaturation,
            process_target(1, "a"),
            now,
        )];

        let groups = DiagnosticCorrelator::correlate(&diags);
        assert!(groups.is_empty(), "single diagnostic cannot form a group");
    }

    #[test]
    fn test_empty_diagnostics() {
        let groups = DiagnosticCorrelator::correlate(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_correlation_note_with_root_cause() {
        let now = Instant::now();
        let diags = vec![
            make_diagnostic(
                DiagCategory::ConnectionSurge,
                process_target(1, "app"),
                now,
            ),
            make_diagnostic(
                DiagCategory::ThroughputDrop,
                process_target(2, "app"),
                now,
            ),
        ];

        let group = CorrelatedGroup {
            diagnostics: vec![0, 1],
            root_cause: Some("pool saturation".into()),
            confidence: 0.9,
        };

        let note = correlation_note(&group, &diags);
        assert!(note.contains("root cause: pool saturation"));
    }

    #[test]
    fn test_causal_pattern_order_independent() {
        let now = Instant::now();

        // Reversed order: effect first, cause second
        let diags = vec![
            make_diagnostic(
                DiagCategory::ThroughputDrop,
                process_target(1, "app"),
                now,
            ),
            make_diagnostic(
                DiagCategory::ConnectionSurge,
                process_target(2, "app"),
                now + Duration::from_secs(5),
            ),
        ];

        let groups = DiagnosticCorrelator::correlate(&diags);

        assert_eq!(groups.len(), 1, "causal pattern should match regardless of order");
        assert!(groups[0].root_cause.is_some());
    }
}

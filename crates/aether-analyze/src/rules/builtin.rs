//! Built-in diagnostic rules for common system health conditions.

use std::time::Duration;

use aether_core::models::{DiagCategory, Severity};

use super::types::{CompareOp, CounterType, LimitSource, Rule, RuleCondition};

/// Returns all built-in diagnostic rules.
pub fn builtin_rules() -> Vec<Rule> {
    vec![
        // 1. Memory approaching OOM (>90% of cgroup limit).
        Rule {
            id: "mem_approaching_oom",
            name: "Memory approaching OOM",
            category: DiagCategory::MemoryPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "mem_bytes",
                limit_source: LimitSource::CgroupMemoryMax,
                percent: 90.0,
            },
            enabled: true,
        },
        // 2. Suspected memory leak (>1MB/min sustained 15min).
        Rule {
            id: "mem_leak_suspected",
            name: "Suspected memory leak",
            category: DiagCategory::MemoryLeak,
            default_severity: Severity::Warning,
            condition: RuleCondition::RateOfChange {
                metric: "mem_bytes",
                // 1MB/min = 1_048_576 / 60 ≈ 17476.3 bytes/sec
                rate_per_sec: 1_048_576.0 / 60.0,
                sustained: Some(Duration::from_secs(15 * 60)),
            },
            enabled: true,
        },
        // 3. CPU saturated (>95% sustained 2min).
        Rule {
            id: "cpu_saturated",
            name: "CPU saturated",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: Some(Duration::from_secs(2 * 60)),
            },
            enabled: true,
        },
        // 4. CPU sustained high (>80% sustained 10min).
        Rule {
            id: "cpu_sustained_high",
            name: "CPU sustained high",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 80.0,
                sustained: Some(Duration::from_secs(10 * 60)),
            },
            enabled: true,
        },
        // 5. Disk almost full (>90% of total).
        Rule {
            id: "disk_almost_full",
            name: "Disk almost full",
            category: DiagCategory::DiskPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "disk_usage_bytes",
                limit_source: LimitSource::DiskTotal,
                percent: 90.0,
            },
            enabled: true,
        },
        // 6. FD approaching limit (>80% of ulimit nofile).
        Rule {
            id: "fd_approaching_limit",
            name: "FD approaching limit",
            category: DiagCategory::FdExhaustion,
            default_severity: Severity::Warning,
            condition: RuleCondition::CapacityPercent {
                metric: "open_fds",
                limit_source: LimitSource::Ulimit("nofile".to_string()),
                percent: 80.0,
            },
            enabled: true,
        },
        // 7. Zombie accumulation (>5 zombie processes).
        Rule {
            id: "zombie_accumulation",
            name: "Zombie process accumulation",
            category: DiagCategory::ZombieAccumulation,
            default_severity: Severity::Warning,
            condition: RuleCondition::Count {
                counter: CounterType::ZombieProcesses,
                op: CompareOp::Gt,
                value: 5,
            },
            enabled: true,
        },
        // 8. Thread explosion (>1000 threads).
        Rule {
            id: "thread_explosion",
            name: "Thread explosion",
            category: DiagCategory::ThreadExplosion,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "thread_count",
                op: CompareOp::Gt,
                value: 1000.0,
                sustained: None,
            },
            enabled: true,
        },
        // 9. Crash loop (uptime <60s AND restart count >3).
        Rule {
            id: "crash_loop",
            name: "Crash loop detected",
            category: DiagCategory::CrashLoop,
            default_severity: Severity::Critical,
            condition: RuleCondition::All(vec![
                RuleCondition::Threshold {
                    metric: "uptime_seconds",
                    op: CompareOp::Lt,
                    value: 60.0,
                    sustained: None,
                },
                RuleCondition::Count {
                    counter: CounterType::RestartCount,
                    op: CompareOp::Gt,
                    value: 3,
                },
            ]),
            enabled: true,
        },
        // 10. Connections growing (>10/min sustained 5min).
        Rule {
            id: "connections_growing",
            name: "Connections growing rapidly",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Info,
            condition: RuleCondition::RateOfChange {
                metric: "established_connections",
                // 10/min = 10/60 ≈ 0.1667 per sec
                rate_per_sec: 10.0 / 60.0,
                sustained: Some(Duration::from_secs(5 * 60)),
            },
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use aether_core::models::Severity;

    use super::*;

    #[test]
    fn test_builtin_count_is_10() {
        assert_eq!(builtin_rules().len(), 10);
    }

    #[test]
    fn test_all_ids_unique() {
        let rules = builtin_rules();
        let ids: HashSet<&str> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), rules.len(), "duplicate rule IDs found");
    }

    #[test]
    fn test_rules_cover_severities() {
        let rules = builtin_rules();
        let has_critical = rules
            .iter()
            .any(|r| matches!(r.default_severity, Severity::Critical));
        let has_warning = rules
            .iter()
            .any(|r| matches!(r.default_severity, Severity::Warning));
        let has_info = rules
            .iter()
            .any(|r| matches!(r.default_severity, Severity::Info));

        assert!(has_critical, "should have at least one Critical rule");
        assert!(has_warning, "should have at least one Warning rule");
        assert!(has_info, "should have at least one Info rule");
    }
}

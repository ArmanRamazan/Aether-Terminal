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
        // 11. Swap growing (>10MB/min sustained 5min).
        Rule {
            id: "mem_swap_growing",
            name: "Swap usage growing",
            category: DiagCategory::MemoryPressure,
            default_severity: Severity::Warning,
            condition: RuleCondition::RateOfChange {
                metric: "swap_bytes",
                // 10MB/min = 10_485_760 / 60 ≈ 174762.7 bytes/sec
                rate_per_sec: 10_485_760.0 / 60.0,
                sustained: Some(Duration::from_secs(5 * 60)),
            },
            enabled: true,
        },
        // 12. RSS doubled (growth ratio > 2.0).
        Rule {
            id: "mem_rss_doubled",
            name: "RSS memory doubled",
            category: DiagCategory::MemoryLeak,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "mem_growth_ratio",
                op: CompareOp::Gt,
                value: 2.0,
                sustained: None,
            },
            enabled: true,
        },
        // 13. CPU cgroup throttled.
        Rule {
            id: "cpu_cgroup_throttled",
            name: "CPU cgroup throttled",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Warning,
            condition: RuleCondition::RateOfChange {
                metric: "cpu_throttled_count",
                rate_per_sec: 0.0,
                sustained: Some(Duration::from_secs(60)),
            },
            enabled: true,
        },
        // 14. High involuntary context switches (>10000/s).
        Rule {
            id: "cpu_context_switches",
            name: "High involuntary context switches",
            category: DiagCategory::CpuSpike,
            default_severity: Severity::Info,
            condition: RuleCondition::Threshold {
                metric: "nonvoluntary_ctxt_switches",
                op: CompareOp::Gt,
                value: 10_000.0,
                sustained: None,
            },
            enabled: true,
        },
        // 15. Heavy disk writes (>100MB/s sustained 5min).
        Rule {
            id: "disk_heavy_write",
            name: "Heavy disk writes",
            category: DiagCategory::DiskIoHeavy,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "disk_write_bytes_per_sec",
                op: CompareOp::Gt,
                value: 100.0 * 1_048_576.0,
                sustained: Some(Duration::from_secs(5 * 60)),
            },
            enabled: true,
        },
        // 16. Heavy disk reads (>100MB/s sustained 5min).
        Rule {
            id: "disk_heavy_read",
            name: "Heavy disk reads",
            category: DiagCategory::DiskIoHeavy,
            default_severity: Severity::Info,
            condition: RuleCondition::Threshold {
                metric: "disk_read_bytes_per_sec",
                op: CompareOp::Gt,
                value: 100.0 * 1_048_576.0,
                sustained: Some(Duration::from_secs(5 * 60)),
            },
            enabled: true,
        },
        // 17. Inode exhaustion (>90% of disk inodes).
        Rule {
            id: "disk_inode_exhaustion",
            name: "Inode exhaustion",
            category: DiagCategory::DiskPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "inode_usage",
                limit_source: LimitSource::DiskTotal,
                percent: 90.0,
            },
            enabled: true,
        },
        // 18. TCP retransmit rate high (>5.0/s).
        Rule {
            id: "net_tcp_retransmits",
            name: "High TCP retransmit rate",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "tcp_retransmit_rate",
                op: CompareOp::Gt,
                value: 5.0,
                sustained: None,
            },
            enabled: true,
        },
        // 19. Too many established connections (>10000).
        Rule {
            id: "net_established_high",
            name: "High established connections",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "established_connections",
                op: CompareOp::Gt,
                value: 10_000.0,
                sustained: None,
            },
            enabled: true,
        },
        // 20. Process stuck in D state (>30s).
        Rule {
            id: "proc_state_d_stuck",
            name: "Process stuck in D state",
            category: DiagCategory::CapacityRisk,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "d_state_seconds",
                op: CompareOp::Gt,
                value: 30.0,
                sustained: None,
            },
            enabled: true,
        },
        // 21. Extreme system load (>4x cores).
        Rule {
            id: "system_load_extreme",
            name: "Extreme system load",
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Critical,
            condition: RuleCondition::Threshold {
                metric: "load_avg_1m_per_core",
                op: CompareOp::Gt,
                value: 4.0,
                sustained: None,
            },
            enabled: true,
        },
        // 22. System memory pressure (>90% host memory).
        Rule {
            id: "system_memory_pressure",
            name: "System memory pressure",
            category: DiagCategory::MemoryPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::CapacityPercent {
                metric: "host_mem_used",
                limit_source: LimitSource::Custom(100.0),
                percent: 90.0,
            },
            enabled: true,
        },
        // 23. OOM kills detected.
        Rule {
            id: "system_oom_kills",
            name: "OOM kills detected",
            category: DiagCategory::MemoryPressure,
            default_severity: Severity::Critical,
            condition: RuleCondition::Count {
                counter: CounterType::OomKills,
                op: CompareOp::Gt,
                value: 0,
            },
            enabled: true,
        },
        // 24. CPU underprovisioned (>90% of cgroup CPU quota).
        Rule {
            id: "config_cpu_underprovisioned",
            name: "CPU underprovisioned",
            category: DiagCategory::ConfigMismatch,
            default_severity: Severity::Warning,
            condition: RuleCondition::CapacityPercent {
                metric: "cpu_usage",
                limit_source: LimitSource::CgroupCpuQuota,
                percent: 90.0,
            },
            enabled: true,
        },
        // 25. Cgroup memory headroom tight (<50MB).
        Rule {
            id: "config_memory_tight",
            name: "Memory headroom tight",
            category: DiagCategory::ConfigMismatch,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "cgroup_memory_headroom_bytes",
                op: CompareOp::Lt,
                // 50MB
                value: 52_428_800.0,
                sustained: None,
            },
            enabled: true,
        },
        // 26. PIDs approaching cgroup limit (>80%).
        Rule {
            id: "config_pids_approaching",
            name: "PIDs approaching cgroup limit",
            category: DiagCategory::ConfigMismatch,
            default_severity: Severity::Warning,
            condition: RuleCondition::CapacityPercent {
                metric: "pids_current",
                limit_source: LimitSource::CgroupPidsMax,
                percent: 80.0,
            },
            enabled: true,
        },
        // 27. High IO wait (>30% sustained 2min).
        Rule {
            id: "io_wait_high",
            name: "High IO wait",
            category: DiagCategory::DiskIoHeavy,
            default_severity: Severity::Warning,
            condition: RuleCondition::Threshold {
                metric: "io_wait_percent",
                op: CompareOp::Gt,
                value: 30.0,
                sustained: Some(Duration::from_secs(2 * 60)),
            },
            enabled: true,
        },
        // 28. Listen queue full (backlog overflows detected).
        Rule {
            id: "net_listen_backlog",
            name: "Listen queue backlog overflow",
            category: DiagCategory::ConnectionSurge,
            default_severity: Severity::Warning,
            condition: RuleCondition::Count {
                counter: CounterType::ListenQueueOverflows,
                op: CompareOp::Gt,
                value: 0,
            },
            enabled: true,
        },
        // 29. Short-lived process with restart indicators.
        Rule {
            id: "proc_uptime_short",
            name: "Short-lived process restarting",
            category: DiagCategory::CrashLoop,
            default_severity: Severity::Info,
            condition: RuleCondition::All(vec![
                RuleCondition::Threshold {
                    metric: "uptime_seconds",
                    op: CompareOp::Lt,
                    value: 10.0,
                    sustained: None,
                },
                RuleCondition::Count {
                    counter: CounterType::RestartCount,
                    op: CompareOp::Gt,
                    value: 0,
                },
            ]),
            enabled: true,
        },
        // 30. Placeholder for correlation-triggered alerts.
        Rule {
            id: "correlated_anomaly",
            name: "Correlated anomaly detected",
            category: DiagCategory::CorrelatedAnomaly,
            default_severity: Severity::Info,
            condition: RuleCondition::Threshold {
                metric: "correlation_score",
                op: CompareOp::Gt,
                value: 0.9,
                sustained: None,
            },
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use aether_core::models::{DiagCategory, Severity};

    use super::*;

    #[test]
    fn test_builtin_count_gte_30() {
        assert!(
            builtin_rules().len() >= 30,
            "expected at least 30 rules, got {}",
            builtin_rules().len()
        );
    }

    #[test]
    fn test_no_duplicate_ids() {
        let rules = builtin_rules();
        let ids: HashSet<&str> = rules.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), rules.len(), "duplicate rule IDs found");
    }

    #[test]
    fn test_all_have_nonempty_names() {
        for rule in builtin_rules() {
            assert!(!rule.id.is_empty(), "rule has empty id");
            assert!(!rule.name.is_empty(), "rule '{}' has empty name", rule.id);
        }
    }

    #[test]
    fn test_all_categories_covered() {
        let rules = builtin_rules();
        let categories: HashSet<_> = rules
            .iter()
            .map(|r| std::mem::discriminant(&r.category))
            .collect();

        let required = [
            DiagCategory::MemoryLeak,
            DiagCategory::MemoryPressure,
            DiagCategory::CpuSaturation,
            DiagCategory::CpuSpike,
            DiagCategory::DiskPressure,
            DiagCategory::DiskIoHeavy,
            DiagCategory::FdExhaustion,
            DiagCategory::ConnectionSurge,
            DiagCategory::ZombieAccumulation,
            DiagCategory::ThreadExplosion,
            DiagCategory::CrashLoop,
            DiagCategory::ConfigMismatch,
            DiagCategory::CapacityRisk,
            DiagCategory::CorrelatedAnomaly,
        ];

        for cat in &required {
            assert!(
                categories.contains(&std::mem::discriminant(cat)),
                "missing rule for category {:?}",
                cat
            );
        }
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

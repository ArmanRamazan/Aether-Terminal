//! Rule engine — evaluates rules against metric data and produces findings.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use aether_core::metrics::HostId;
use aether_core::models::DiagTarget;

use crate::analyzers::trend::TrendAnalyzer;
use crate::store::MetricStore;

use super::types::{LimitSource, ProcessLimits, Rule, RuleCondition, RuleFinding};

/// Evaluates diagnostic rules against metric data.
pub struct RuleEngine {
    rules: Vec<Rule>,
    /// Tracks when a sustained condition was first observed: (rule_id, pid) → first_seen.
    sustained_state: HashMap<(String, u32), Instant>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            sustained_state: HashMap::new(),
        }
    }

    /// Load built-in rules into the engine.
    pub fn load_builtin(&mut self) {
        for rule in super::builtin::builtin_rules() {
            self.rules.push(rule);
        }
    }

    /// Add a rule to the engine.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Disable a rule by id. Returns true if the rule was found.
    pub fn disable_rule(&mut self, id: &str) -> bool {
        for rule in &mut self.rules {
            if rule.id == id {
                rule.enabled = false;
                return true;
            }
        }
        false
    }

    /// Evaluate all enabled rules against every process in the store.
    pub fn evaluate(
        &mut self,
        store: &MetricStore,
        host: &HostId,
        limits_map: &HashMap<u32, ProcessLimits>,
    ) -> Vec<RuleFinding> {
        let trend = TrendAnalyzer;
        let pids: Vec<u32> = store.process_pids(host).into_iter().collect();
        let mut findings = Vec::new();

        for &pid in &pids {
            let empty_limits = ProcessLimits::default();
            let limits = limits_map.get(&pid).unwrap_or(&empty_limits);

            for rule in &self.rules {
                if !rule.enabled {
                    continue;
                }

                let mut matched_values = Vec::new();
                let mut ctx = EvalCtx {
                    store,
                    host,
                    pid,
                    limits,
                    trend: &trend,
                    rule_id: rule.id,
                    sustained_state: &mut self.sustained_state,
                };
                let matches = eval_condition(&mut ctx, &rule.condition, &mut matched_values);

                if matches {
                    findings.push(RuleFinding {
                        rule_id: rule.id,
                        rule_name: rule.name,
                        target: DiagTarget::Process {
                            pid,
                            name: String::new(),
                        },
                        severity: rule.default_severity,
                        category: rule.category,
                        matched_values,
                    });
                }
            }
        }

        findings
    }
}

/// Context passed through recursive condition evaluation.
struct EvalCtx<'a> {
    store: &'a MetricStore,
    host: &'a HostId,
    pid: u32,
    limits: &'a ProcessLimits,
    trend: &'a TrendAnalyzer,
    rule_id: &'a str,
    sustained_state: &'a mut HashMap<(String, u32), Instant>,
}

fn eval_condition(
    ctx: &mut EvalCtx<'_>,
    condition: &RuleCondition,
    matched_values: &mut Vec<(String, f64)>,
) -> bool {
    match condition {
        RuleCondition::Threshold {
            metric,
            op,
            value,
            sustained,
        } => {
            let current = match last_value(ctx.store, ctx.host, ctx.pid, metric) {
                Some(v) => v,
                None => return false,
            };

            if !op.eval_f64(current, *value) {
                clear_sustained(ctx.sustained_state, ctx.rule_id, ctx.pid);
                return false;
            }

            matched_values.push(((*metric).to_string(), current));

            match sustained {
                Some(dur) => check_sustained(ctx.sustained_state, ctx.rule_id, ctx.pid, *dur),
                None => true,
            }
        }

        RuleCondition::RateOfChange {
            metric,
            rate_per_sec,
            sustained,
        } => {
            let series = match ctx.store.get(ctx.host, Some(ctx.pid), metric) {
                Some(s) => s,
                None => return false,
            };

            let slope = ctx.trend.slope(series, Duration::from_secs(60));

            if slope < *rate_per_sec {
                clear_sustained(ctx.sustained_state, ctx.rule_id, ctx.pid);
                return false;
            }

            matched_values.push(((*metric).to_string(), slope));

            match sustained {
                Some(dur) => check_sustained(ctx.sustained_state, ctx.rule_id, ctx.pid, *dur),
                None => true,
            }
        }

        RuleCondition::CapacityPercent {
            metric,
            limit_source,
            percent,
        } => {
            let current = match last_value(ctx.store, ctx.host, ctx.pid, metric) {
                Some(v) => v,
                None => return false,
            };

            let limit = match limit_source {
                LimitSource::CgroupMemoryMax => ctx.limits.cgroup_memory_max.map(|v| v as f64),
                LimitSource::CgroupCpuQuota => ctx.limits.cgroup_cpu_quota.map(|v| v as f64),
                LimitSource::CgroupPidsMax => ctx.limits.cgroup_pids_max.map(|v| v as f64),
                LimitSource::Ulimit(_) => ctx.limits.ulimit_nofile.map(|v| v as f64),
                LimitSource::DiskTotal => ctx.limits.disk_total.map(|v| v as f64),
                LimitSource::Custom(v) => Some(*v),
            };

            let limit = match limit {
                Some(l) if l > 0.0 => l,
                _ => return false,
            };

            let usage_pct = (current / limit) * 100.0;
            if usage_pct < *percent {
                return false;
            }

            matched_values.push(((*metric).to_string(), usage_pct));
            true
        }

        RuleCondition::Count {
            counter: _,
            op,
            value,
        } => {
            // Counter evaluation requires external data not yet available.
            // Placeholder: looks for a "counter" metric in the store.
            let current = match last_value(ctx.store, ctx.host, ctx.pid, "counter") {
                Some(v) => v,
                None => return false,
            };

            if !op.eval_u64(current as u64, *value) {
                return false;
            }

            matched_values.push(("counter".to_string(), current));
            true
        }

        RuleCondition::All(conditions) => {
            for cond in conditions {
                if !eval_condition(ctx, cond, matched_values) {
                    return false;
                }
            }
            true
        }

        RuleCondition::Any(conditions) => {
            for cond in conditions {
                if eval_condition(ctx, cond, matched_values) {
                    return true;
                }
            }
            false
        }
    }
}

/// Get the latest value for a per-process metric.
fn last_value(store: &MetricStore, host: &HostId, pid: u32, metric: &str) -> Option<f64> {
    store
        .get(host, Some(pid), metric)
        .and_then(|s| s.last())
        .map(|s| s.value)
}

/// Check if a sustained condition has been met for the required duration.
fn check_sustained(
    state: &mut HashMap<(String, u32), Instant>,
    rule_id: &str,
    pid: u32,
    required: Duration,
) -> bool {
    let key = (rule_id.to_string(), pid);
    let now = Instant::now();
    let first_seen = state.entry(key).or_insert(now);
    now.duration_since(*first_seen) >= required
}

/// Clear sustained tracking when a condition becomes false.
fn clear_sustained(state: &mut HashMap<(String, u32), Instant>, rule_id: &str, pid: u32) {
    let key = (rule_id.to_string(), pid);
    state.remove(&key);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use aether_core::metrics::{HostId, MetricSample, TimeSeries};
    use aether_core::models::{DiagCategory, Severity};

    use crate::store::MetricStore;

    use super::super::types::{CompareOp, RuleCondition};
    use super::*;

    fn store_push(store: &mut MetricStore, host: &HostId, pid: u32, metric: &str, value: f64) {
        let mut ts = TimeSeries::new(metric, 100);
        ts.labels
            .insert("host".to_string(), host.as_str().to_string());
        ts.labels.insert("pid".to_string(), pid.to_string());
        ts.labels.insert("__name__".to_string(), metric.to_string());
        ts.push_sample(MetricSample {
            timestamp: Instant::now(),
            value,
        });
        store.ingest_remote(vec![ts]);
    }

    fn store_push_at(
        store: &mut MetricStore,
        host: &HostId,
        pid: u32,
        metric: &str,
        timestamp: Instant,
        value: f64,
    ) {
        let mut ts = TimeSeries::new(metric, 100);
        ts.labels
            .insert("host".to_string(), host.as_str().to_string());
        ts.labels.insert("pid".to_string(), pid.to_string());
        ts.labels.insert("__name__".to_string(), metric.to_string());
        ts.push_sample(MetricSample { timestamp, value });
        store.ingest_remote(vec![ts]);
    }

    fn make_store_with_metric(host: &HostId, pid: u32, metric: &str, value: f64) -> MetricStore {
        let mut store = MetricStore::new(100);
        store_push(&mut store, host, pid, metric, value);
        store
    }

    fn make_rule(id: &'static str, condition: RuleCondition) -> Rule {
        Rule {
            id,
            name: id,
            category: DiagCategory::CpuSaturation,
            default_severity: Severity::Warning,
            condition,
            enabled: true,
        }
    }

    #[test]
    fn test_threshold_matches_above() {
        let host = HostId::new("local");
        let store = make_store_with_metric(&host, 1, "cpu_percent", 96.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "high_cpu",
            RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: None,
            },
        ));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert_eq!(findings.len(), 1, "cpu 96 > 95 should match");
        assert_eq!(findings[0].rule_id, "high_cpu");
    }

    #[test]
    fn test_threshold_no_match_below() {
        let host = HostId::new("local");
        let store = make_store_with_metric(&host, 1, "cpu_percent", 50.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "high_cpu",
            RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: None,
            },
        ));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert!(findings.is_empty(), "cpu 50 < 95 should not match");
    }

    #[test]
    fn test_sustained_only_fires_after_duration() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);
        store_push(&mut store, &host, 1, "cpu_percent", 96.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "sustained_cpu",
            RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: Some(Duration::from_secs(2)),
            },
        ));

        // First evaluation — starts the timer but should NOT fire.
        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert!(findings.is_empty(), "should not fire on first check");

        // Simulate 3 seconds passing by backdating the sustained entry.
        let key = ("sustained_cpu".to_string(), 1u32);
        engine
            .sustained_state
            .insert(key, Instant::now() - Duration::from_secs(3));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert_eq!(findings.len(), 1, "should fire after sustained duration");
    }

    #[test]
    fn test_sustained_resets_on_false() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);
        store_push(&mut store, &host, 1, "cpu_percent", 96.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "sustained_cpu",
            RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: Some(Duration::from_secs(2)),
            },
        ));

        // Start sustained timer.
        engine.evaluate(&store, &host, &HashMap::new());
        assert!(
            engine
                .sustained_state
                .contains_key(&("sustained_cpu".to_string(), 1)),
            "timer should be started"
        );

        // Condition becomes false with a new store having low value.
        let mut store2 = MetricStore::new(100);
        store_push(&mut store2, &host, 1, "cpu_percent", 50.0);

        engine.evaluate(&store2, &host, &HashMap::new());
        assert!(
            !engine
                .sustained_state
                .contains_key(&("sustained_cpu".to_string(), 1)),
            "timer should be reset when condition is false"
        );
    }

    #[test]
    fn test_rate_of_change_matches() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);

        // Create a growing series: 2 units/sec over 60 seconds.
        let base = Instant::now() - Duration::from_secs(60);
        for i in 0..60 {
            store_push_at(
                &mut store,
                &host,
                1,
                "mem_bytes",
                base + Duration::from_secs(i),
                (i as f64) * 2.0,
            );
        }

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "mem_growth",
            RuleCondition::RateOfChange {
                metric: "mem_bytes",
                rate_per_sec: 1.0,
                sustained: None,
            },
        ));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert_eq!(findings.len(), 1, "growth rate ~2/s > threshold 1/s");
    }

    #[test]
    fn test_compound_all_both_true() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);
        store_push(&mut store, &host, 1, "cpu_percent", 96.0);
        store_push(&mut store, &host, 1, "mem_bytes", 2_000_000_000.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "both",
            RuleCondition::All(vec![
                RuleCondition::Threshold {
                    metric: "cpu_percent",
                    op: CompareOp::Gt,
                    value: 95.0,
                    sustained: None,
                },
                RuleCondition::Threshold {
                    metric: "mem_bytes",
                    op: CompareOp::Gt,
                    value: 1_000_000_000.0,
                    sustained: None,
                },
            ]),
        ));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert_eq!(findings.len(), 1, "both conditions true → match");
    }

    #[test]
    fn test_compound_any_one_true() {
        let host = HostId::new("local");
        let mut store = MetricStore::new(100);
        store_push(&mut store, &host, 1, "cpu_percent", 50.0);
        store_push(&mut store, &host, 1, "mem_bytes", 2_000_000_000.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "either",
            RuleCondition::Any(vec![
                RuleCondition::Threshold {
                    metric: "cpu_percent",
                    op: CompareOp::Gt,
                    value: 95.0,
                    sustained: None,
                },
                RuleCondition::Threshold {
                    metric: "mem_bytes",
                    op: CompareOp::Gt,
                    value: 1_000_000_000.0,
                    sustained: None,
                },
            ]),
        ));

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert_eq!(findings.len(), 1, "one condition true → Any matches");
    }

    #[test]
    fn test_disable_rule_skips() {
        let host = HostId::new("local");
        let store = make_store_with_metric(&host, 1, "cpu_percent", 96.0);

        let mut engine = RuleEngine::new();
        engine.add_rule(make_rule(
            "high_cpu",
            RuleCondition::Threshold {
                metric: "cpu_percent",
                op: CompareOp::Gt,
                value: 95.0,
                sustained: None,
            },
        ));

        assert!(engine.disable_rule("high_cpu"), "rule should be found");

        let findings = engine.evaluate(&store, &host, &HashMap::new());
        assert!(findings.is_empty(), "disabled rule should not fire");
    }
}

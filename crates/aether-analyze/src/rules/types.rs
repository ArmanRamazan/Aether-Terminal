//! Rule types for the deterministic diagnostic engine.

use std::time::Duration;

use aether_core::models::{DiagCategory, DiagTarget, Severity};

/// Comparison operator for threshold and count conditions.
#[derive(Debug, Clone, Copy)]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl CompareOp {
    /// Evaluate the comparison: `lhs <op> rhs`.
    pub fn eval_f64(self, lhs: f64, rhs: f64) -> bool {
        match self {
            Self::Gt => lhs > rhs,
            Self::Gte => lhs >= rhs,
            Self::Lt => lhs < rhs,
            Self::Lte => lhs <= rhs,
            Self::Eq => (lhs - rhs).abs() < f64::EPSILON,
        }
    }

    /// Evaluate the comparison for unsigned integers.
    pub fn eval_u64(self, lhs: u64, rhs: u64) -> bool {
        match self {
            Self::Gt => lhs > rhs,
            Self::Gte => lhs >= rhs,
            Self::Lt => lhs < rhs,
            Self::Lte => lhs <= rhs,
            Self::Eq => lhs == rhs,
        }
    }
}

/// Source of a resource limit for capacity-percent checks.
#[derive(Debug, Clone)]
pub enum LimitSource {
    CgroupMemoryMax,
    CgroupCpuQuota,
    CgroupPidsMax,
    Ulimit(String),
    DiskTotal,
    Custom(f64),
}

/// Counter type for count-based conditions.
#[derive(Debug, Clone)]
pub enum CounterType {
    ZombieProcesses,
    RestartCount,
    OpenFds,
    ThreadCount,
}

/// A composable condition tree for rule evaluation.
#[derive(Debug, Clone)]
pub enum RuleCondition {
    /// Metric exceeds a static threshold.
    Threshold {
        metric: &'static str,
        op: CompareOp,
        value: f64,
        sustained: Option<Duration>,
    },
    /// Metric is changing faster than a given rate.
    RateOfChange {
        metric: &'static str,
        rate_per_sec: f64,
        sustained: Option<Duration>,
    },
    /// Metric as a percentage of a resource limit.
    CapacityPercent {
        metric: &'static str,
        limit_source: LimitSource,
        percent: f64,
    },
    /// A counter exceeds a threshold.
    Count {
        counter: CounterType,
        op: CompareOp,
        value: u64,
    },
    /// All sub-conditions must be true.
    All(Vec<RuleCondition>),
    /// At least one sub-condition must be true.
    Any(Vec<RuleCondition>),
}

/// A diagnostic rule definition.
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub category: DiagCategory,
    pub default_severity: Severity,
    pub condition: RuleCondition,
    pub enabled: bool,
}

/// A finding produced when a rule matches.
pub struct RuleFinding {
    pub rule_id: &'static str,
    pub rule_name: &'static str,
    pub target: DiagTarget,
    pub severity: Severity,
    pub category: DiagCategory,
    pub matched_values: Vec<(String, f64)>,
}

/// Resource limits for a process (cgroup, ulimit, disk).
#[derive(Debug, Clone, Default)]
pub struct ProcessLimits {
    pub cgroup_memory_max: Option<u64>,
    pub cgroup_cpu_quota: Option<u64>,
    pub cgroup_pids_max: Option<u64>,
    pub ulimit_nofile: Option<u64>,
    pub disk_total: Option<u64>,
}

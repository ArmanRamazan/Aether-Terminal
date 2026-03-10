//! Rule evaluation runtime.
//!
//! Evaluates compiled rules against process state and collects triggered actions.
//! Supports duration-based rules that only fire after a condition holds continuously.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ast::Rule;
use crate::codegen::{CodeGenerator, CompiledRule, JitCompiler, WorldStateFFI};
use crate::error::ScriptError;

/// Action triggered by a matching rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAction {
    /// Name of the rule that fired.
    pub rule_name: String,
    /// Action type: 1=alert, 2=kill, 3=log.
    pub action: u32,
    /// PID of the targeted process.
    pub target_pid: u32,
    /// Severity: 0=info, 1=warning, 2=critical (only meaningful for alerts).
    pub severity: u32,
}

/// Tracks when conditions first became true for duration-based rules.
///
/// Key is `(rule_name, pid)`, value is the instant when the condition first matched.
/// Timer resets when the condition becomes false.
struct DurationTracker {
    timers: HashMap<(String, u32), Instant>,
}

impl DurationTracker {
    fn new() -> Self {
        Self {
            timers: HashMap::new(),
        }
    }

    /// Check if a duration requirement is satisfied.
    ///
    /// Returns `true` if the condition has been continuously met for at least `required`.
    /// Starts or resets the timer based on `condition_met`.
    fn check_duration(
        &mut self,
        rule_name: &str,
        pid: u32,
        condition_met: bool,
        required: Duration,
    ) -> bool {
        let key = (rule_name.to_string(), pid);

        if !condition_met {
            self.timers.remove(&key);
            return false;
        }

        let start = *self.timers.entry(key).or_insert_with(Instant::now);
        start.elapsed() >= required
    }
}

/// Set of JIT-compiled rules ready for evaluation.
///
/// Owns the `JitCompiler` that holds the executable code memory.
/// Rules are valid for the lifetime of this struct.
pub struct CompiledRuleSet {
    rules: Vec<(CompiledRule, Option<Duration>)>,
    tracker: DurationTracker,
    // JitCompiler owns the code memory; must outlive all CompiledRule func_ptrs.
    _jit: JitCompiler,
}

impl CompiledRuleSet {
    /// Compile a set of rules into native code.
    pub fn compile(rules: &[Rule]) -> Result<Self, ScriptError> {
        let mut jit = JitCompiler::new()?;
        let mut codegen = CodeGenerator::new();
        let compiled = rules
            .iter()
            .map(|rule| {
                let compiled = jit.compile_rule(&mut codegen, rule)?;
                Ok((compiled, rule.duration))
            })
            .collect::<Result<Vec<_>, ScriptError>>()?;
        Ok(Self {
            rules: compiled,
            tracker: DurationTracker::new(),
            _jit: jit,
        })
    }

    /// Evaluate all rules against a process state, returning triggered actions.
    ///
    /// Duration rules only fire after the condition holds continuously for the
    /// required time. Non-duration rules fire immediately when matched.
    ///
    /// # Safety
    /// `state` must point to a valid, initialized `WorldStateFFI`.
    pub fn evaluate(&mut self, state: &WorldStateFFI) -> Vec<RuleAction> {
        let state_ptr: *const WorldStateFFI = state;
        let mut actions = Vec::new();

        for (rule, duration) in &self.rules {
            // SAFETY: state_ptr points to a valid WorldStateFFI (borrow above),
            // and self._jit is alive so compiled function pointers are valid.
            let result = unsafe { rule.call(state_ptr) };
            let condition_met = result.matched != 0;

            let should_fire = match duration {
                Some(required) => self.tracker.check_duration(
                    &rule.name,
                    state.pid,
                    condition_met,
                    *required,
                ),
                None => condition_met,
            };

            if should_fire {
                actions.push(RuleAction {
                    rule_name: rule.name.clone(),
                    action: result.action_type,
                    target_pid: result.target_pid,
                    severity: result.severity,
                });
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, CompareOp, Expr, Field, Rule, Severity, Value};
    use crate::codegen::WorldStateFFI;

    fn make_state(pid: u32, cpu: f32, mem: u64) -> WorldStateFFI {
        WorldStateFFI {
            pid,
            cpu_percent: cpu,
            mem_bytes: mem,
            mem_growth_percent: 0.0,
            state: 0,
            hp: 100.0,
            name_ptr: std::ptr::null(),
            name_len: 0,
            process_count: 0,
            processes_ptr: std::ptr::null(),
        }
    }

    fn high_cpu_rule() -> Rule {
        Rule {
            name: "high_cpu".to_string(),
            when_clause: Expr::Comparison {
                field: Field::CpuPercent,
                op: CompareOp::Gt,
                value: Value::Float(90.0),
            },
            duration: None,
            then_clause: Action::Alert {
                severity: Severity::Warning,
            },
        }
    }

    fn high_cpu_duration_rule(secs: u64) -> Rule {
        Rule {
            name: "high_cpu_sustained".to_string(),
            when_clause: Expr::Comparison {
                field: Field::CpuPercent,
                op: CompareOp::Gt,
                value: Value::Float(90.0),
            },
            duration: Some(Duration::from_secs(secs)),
            then_clause: Action::Alert {
                severity: Severity::Critical,
            },
        }
    }

    #[test]
    fn test_compile_and_execute_simple_rule() {
        let mut ruleset =
            CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
        let state = make_state(42, 95.0, 1024);
        let actions = ruleset.evaluate(&state);

        assert_eq!(actions.len(), 1, "rule should match");
        assert_eq!(actions[0].rule_name, "high_cpu");
        assert_eq!(actions[0].target_pid, 42);
        assert_eq!(actions[0].action, 1, "action_type 1 = alert");
        assert_eq!(actions[0].severity, 1, "severity 1 = warning");
    }

    #[test]
    fn test_matching_condition_triggers_action() {
        let kill_rule = Rule {
            name: "kill_zombie".to_string(),
            when_clause: Expr::And(
                Box::new(Expr::Comparison {
                    field: Field::State,
                    op: CompareOp::Eq,
                    value: Value::Int(2), // Zombie
                }),
                Box::new(Expr::Comparison {
                    field: Field::CpuPercent,
                    op: CompareOp::Lt,
                    value: Value::Float(1.0),
                }),
            ),
            duration: None,
            then_clause: Action::Kill,
        };

        let mut ruleset = CompiledRuleSet::compile(&[kill_rule]).expect("compilation failed");

        let mut state = make_state(99, 0.5, 0);
        state.state = 2; // Zombie

        let actions = ruleset.evaluate(&state);
        assert_eq!(actions.len(), 1, "zombie with low cpu should match");
        assert_eq!(actions[0].action, 2, "action_type 2 = kill");
        assert_eq!(actions[0].target_pid, 99);
    }

    #[test]
    fn test_non_matching_condition_no_action() {
        let mut ruleset =
            CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");

        // CPU below threshold
        let state = make_state(10, 50.0, 2048);
        let actions = ruleset.evaluate(&state);

        assert!(actions.is_empty(), "rule should not match when cpu is 50%");
    }

    #[test]
    fn test_rule_fires_only_after_duration_elapsed() {
        let mut ruleset =
            CompiledRuleSet::compile(&[high_cpu_duration_rule(0)]).expect("compilation failed");

        let state = make_state(42, 95.0, 1024);

        // Duration of 0s — fires immediately since condition is met and 0 elapsed is >= 0.
        let actions = ruleset.evaluate(&state);
        assert_eq!(actions.len(), 1, "0s duration should fire immediately");

        // Now test with a non-zero duration that hasn't elapsed yet.
        let mut ruleset =
            CompiledRuleSet::compile(&[high_cpu_duration_rule(3600)]).expect("compilation failed");

        let actions = ruleset.evaluate(&state);
        assert!(
            actions.is_empty(),
            "rule should not fire before duration elapses"
        );

        // Second evaluate still shouldn't fire (3600s hasn't passed).
        let actions = ruleset.evaluate(&state);
        assert!(
            actions.is_empty(),
            "rule should still not fire — duration not yet elapsed"
        );
    }

    #[test]
    fn test_timer_resets_when_condition_false() {
        let mut ruleset =
            CompiledRuleSet::compile(&[high_cpu_duration_rule(0)]).expect("compilation failed");

        let high_state = make_state(42, 95.0, 1024);
        let low_state = make_state(42, 50.0, 1024);

        // Condition met → timer starts.
        let actions = ruleset.evaluate(&high_state);
        assert_eq!(actions.len(), 1, "0s duration fires immediately");

        // Condition not met → timer resets.
        let actions = ruleset.evaluate(&low_state);
        assert!(actions.is_empty(), "should not fire when condition is false");

        // Verify timer was actually reset by checking the tracker is clean.
        // Re-evaluating with condition met should start fresh.
        let actions = ruleset.evaluate(&high_state);
        assert_eq!(
            actions.len(),
            1,
            "should fire again after timer reset and condition re-met"
        );
    }

    #[test]
    fn test_multiple_rules_tracked_independently() {
        let rule_a = Rule {
            name: "rule_a".to_string(),
            when_clause: Expr::Comparison {
                field: Field::CpuPercent,
                op: CompareOp::Gt,
                value: Value::Float(90.0),
            },
            duration: Some(Duration::from_secs(3600)),
            then_clause: Action::Alert {
                severity: Severity::Warning,
            },
        };
        let rule_b = Rule {
            name: "rule_b".to_string(),
            when_clause: Expr::Comparison {
                field: Field::CpuPercent,
                op: CompareOp::Gt,
                value: Value::Float(90.0),
            },
            duration: Some(Duration::from_secs(0)),
            then_clause: Action::Alert {
                severity: Severity::Critical,
            },
        };

        let mut ruleset =
            CompiledRuleSet::compile(&[rule_a, rule_b]).expect("compilation failed");
        let state = make_state(42, 95.0, 1024);

        let actions = ruleset.evaluate(&state);

        // rule_a requires 3600s → should NOT fire.
        // rule_b requires 0s → should fire immediately.
        assert_eq!(actions.len(), 1, "only rule_b should fire");
        assert_eq!(actions[0].rule_name, "rule_b");
    }
}

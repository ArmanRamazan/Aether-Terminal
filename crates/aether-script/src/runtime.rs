//! Rule evaluation runtime.
//!
//! Evaluates compiled rules against process state and collects triggered actions.

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

/// Set of JIT-compiled rules ready for evaluation.
///
/// Owns the `JitCompiler` that holds the executable code memory.
/// Rules are valid for the lifetime of this struct.
pub struct CompiledRuleSet {
    rules: Vec<CompiledRule>,
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
            .map(|rule| jit.compile_rule(&mut codegen, rule))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            rules: compiled,
            _jit: jit,
        })
    }

    /// Evaluate all rules against a process state, returning triggered actions.
    ///
    /// # Safety
    /// `state` must point to a valid, initialized `WorldStateFFI`.
    pub fn evaluate(&self, state: &WorldStateFFI) -> Vec<RuleAction> {
        let state_ptr: *const WorldStateFFI = state;
        let mut actions = Vec::new();

        for rule in &self.rules {
            // SAFETY: state_ptr points to a valid WorldStateFFI (borrow above),
            // and self._jit is alive so compiled function pointers are valid.
            let result = unsafe { rule.call(state_ptr) };

            if result.matched != 0 {
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
            then_clause: Action::Alert {
                severity: Severity::Warning,
            },
        }
    }

    #[test]
    fn test_compile_and_execute_simple_rule() {
        let ruleset = CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");
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
            then_clause: Action::Kill,
        };

        let ruleset = CompiledRuleSet::compile(&[kill_rule]).expect("compilation failed");

        let mut state = make_state(99, 0.5, 0);
        state.state = 2; // Zombie

        let actions = ruleset.evaluate(&state);
        assert_eq!(actions.len(), 1, "zombie with low cpu should match");
        assert_eq!(actions[0].action, 2, "action_type 2 = kill");
        assert_eq!(actions[0].target_pid, 99);
    }

    #[test]
    fn test_non_matching_condition_no_action() {
        let ruleset = CompiledRuleSet::compile(&[high_cpu_rule()]).expect("compilation failed");

        // CPU below threshold
        let state = make_state(10, 50.0, 2048);
        let actions = ruleset.evaluate(&state);

        assert!(actions.is_empty(), "rule should not match when cpu is 50%");
    }
}

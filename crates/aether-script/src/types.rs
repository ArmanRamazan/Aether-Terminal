//! Type checker for the rule DSL.
//!
//! Validates field existence and type compatibility in rule conditions
//! before codegen. Catches errors like `process.cpu > "hello"` early.

use std::collections::HashMap;
use std::fmt;

use crate::ast::{Action, CmpOp, Condition, Expr, Literal, Rule, RuleFile};

/// Types in the Aether DSL type system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AetherType {
    /// Percentage value (0–100+), e.g. `90%`.
    Percent,
    /// Duration in seconds, e.g. `30s`, `5m`.
    Duration,
    /// Signed integer.
    Int,
    /// Floating-point number.
    Float,
    /// String literal.
    Str,
    /// Boolean value.
    Bool,
    /// Process state enum (Running, Sleeping, Zombie, Stopped).
    ProcessState,
    /// Process object (has fields).
    Process,
    /// System object (has fields).
    System,
}

impl fmt::Display for AetherType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Percent => write!(f, "Percent"),
            Self::Duration => write!(f, "Duration"),
            Self::Int => write!(f, "Int"),
            Self::Float => write!(f, "Float"),
            Self::Str => write!(f, "Str"),
            Self::Bool => write!(f, "Bool"),
            Self::ProcessState => write!(f, "ProcessState"),
            Self::Process => write!(f, "Process"),
            Self::System => write!(f, "System"),
        }
    }
}

impl AetherType {
    /// Whether this type is numeric (can participate in ordered comparisons).
    fn is_numeric(self) -> bool {
        matches!(self, Self::Percent | Self::Int | Self::Float | Self::Duration)
    }

    /// Whether two types are compatible for comparison.
    fn compatible_with(self, other: Self) -> bool {
        if self == other {
            return true;
        }
        // Percent and Float are interchangeable in comparisons.
        if self.is_numeric() && other.is_numeric() {
            // Duration is only comparable with Duration.
            if self == Self::Duration || other == Self::Duration {
                return self == Self::Duration && other == Self::Duration;
            }
            return true;
        }
        false
    }
}

/// A type error with a human-readable message.
#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Type environment: maps object names to their field types.
pub(crate) struct TypeEnv {
    objects: HashMap<String, HashMap<String, AetherType>>,
}

impl TypeEnv {
    /// Create a type environment with built-in process and system objects.
    fn with_builtins() -> Self {
        let mut objects = HashMap::new();

        let mut process_fields = HashMap::new();
        process_fields.insert("cpu".to_string(), AetherType::Percent);
        process_fields.insert("mem_bytes".to_string(), AetherType::Int);
        process_fields.insert("mem_growth".to_string(), AetherType::Percent);
        process_fields.insert("state".to_string(), AetherType::ProcessState);
        process_fields.insert("name".to_string(), AetherType::Str);
        process_fields.insert("pid".to_string(), AetherType::Int);
        process_fields.insert("parent".to_string(), AetherType::Str);
        process_fields.insert("hp".to_string(), AetherType::Percent);
        objects.insert("process".to_string(), process_fields);

        let mut system_fields = HashMap::new();
        system_fields.insert("load".to_string(), AetherType::Percent);
        system_fields.insert("total_mem".to_string(), AetherType::Int);
        system_fields.insert("process_count".to_string(), AetherType::Int);
        objects.insert("system".to_string(), system_fields);

        Self { objects }
    }

    /// Resolve a field access to its type.
    fn resolve_field(&self, object: &str, field: &str) -> Result<AetherType, TypeError> {
        let fields = self.objects.get(object).ok_or_else(|| TypeError {
            message: format!("unknown object `{object}`"),
        })?;
        fields.get(field).copied().ok_or_else(|| TypeError {
            message: format!("unknown field `{object}.{field}`"),
        })
    }
}

/// Type checker for rule files.
pub struct TypeChecker {
    env: TypeEnv,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    /// Create a new type checker with built-in type definitions.
    pub fn new() -> Self {
        Self {
            env: TypeEnv::with_builtins(),
        }
    }

    /// Type-check a rule file. Returns all errors found.
    pub fn check(&self, file: &RuleFile) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();
        for rule in &file.rules {
            self.check_rule(rule, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_rule(&self, rule: &Rule, errors: &mut Vec<TypeError>) {
        self.check_condition(&rule.condition, errors);
        for action in &rule.actions {
            self.check_action(action, errors);
        }
    }

    fn check_condition(&self, condition: &Condition, errors: &mut Vec<TypeError>) {
        match condition {
            Condition::Comparison { left, op, right } => {
                self.check_comparison(left, *op, right, errors);
            }
            Condition::Duration { condition, .. } => {
                self.check_condition(condition, errors);
            }
            Condition::And(lhs, rhs) | Condition::Or(lhs, rhs) => {
                self.check_condition(lhs, errors);
                self.check_condition(rhs, errors);
            }
            Condition::Not(inner) => {
                self.check_condition(inner, errors);
            }
        }
    }

    fn check_comparison(
        &self,
        left: &Expr,
        op: CmpOp,
        right: &Expr,
        errors: &mut Vec<TypeError>,
    ) {
        let left_ty = match self.resolve_expr(left) {
            Ok(ty) => ty,
            Err(e) => {
                errors.push(e);
                return;
            }
        };
        let right_ty = match self.resolve_expr(right) {
            Ok(ty) => ty,
            Err(e) => {
                errors.push(e);
                return;
            }
        };

        // Ordered comparisons require numeric types.
        let is_ordered = matches!(op, CmpOp::Gt | CmpOp::Lt | CmpOp::Gte | CmpOp::Lte);
        if is_ordered && !left_ty.is_numeric() {
            errors.push(TypeError {
                message: format!(
                    "cannot use `{op}` on non-numeric type {left_ty}",
                ),
            });
            return;
        }

        if !left_ty.compatible_with(right_ty) {
            errors.push(TypeError {
                message: format!(
                    "type mismatch: cannot compare {left_ty} with {right_ty}",
                ),
            });
        }
    }

    fn resolve_expr(&self, expr: &Expr) -> Result<AetherType, TypeError> {
        match expr {
            Expr::FieldAccess { object, field } => self.env.resolve_field(object, field),
            Expr::Literal(lit) => Ok(Self::literal_type(lit)),
        }
    }

    fn literal_type(lit: &Literal) -> AetherType {
        match lit {
            Literal::Int(_) => AetherType::Int,
            Literal::Float(_) => AetherType::Float,
            Literal::Percent(_) => AetherType::Percent,
            Literal::Duration(_) => AetherType::Duration,
            Literal::Str(_) => AetherType::Str,
        }
    }

    fn check_action(&self, action: &Action, errors: &mut Vec<TypeError>) {
        match action {
            Action::Alert { message, .. } => {
                if message.is_empty() {
                    errors.push(TypeError {
                        message: "alert message cannot be empty".to_string(),
                    });
                }
            }
            Action::Log { message } => {
                if message.is_empty() {
                    errors.push(TypeError {
                        message: "log message cannot be empty".to_string(),
                    });
                }
            }
            Action::Kill => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Severity;
    use crate::lexer::Span;

    fn make_rule(name: &str, condition: Condition, actions: Vec<Action>) -> RuleFile {
        RuleFile {
            rules: vec![Rule {
                name: name.to_string(),
                condition,
                actions,
                span: Span { start: 0, end: 0 },
            }],
        }
    }

    fn cpu_gt_percent(value: f64) -> Condition {
        Condition::Comparison {
            left: Expr::FieldAccess {
                object: "process".to_string(),
                field: "cpu".to_string(),
            },
            op: CmpOp::Gt,
            right: Expr::Literal(Literal::Percent(value)),
        }
    }

    fn alert(msg: &str) -> Action {
        Action::Alert {
            message: msg.to_string(),
            severity: Severity::Warning,
        }
    }

    #[test]
    fn test_valid_rule_passes_type_check() {
        let file = make_rule("high_cpu", cpu_gt_percent(90.0), vec![alert("cpu high")]);
        let checker = TypeChecker::new();
        assert!(checker.check(&file).is_ok(), "valid rule should pass");
    }

    #[test]
    fn test_percent_vs_float_comparison_passes() {
        let condition = Condition::Comparison {
            left: Expr::FieldAccess {
                object: "process".to_string(),
                field: "cpu".to_string(),
            },
            op: CmpOp::Gt,
            right: Expr::Literal(Literal::Float(0.9)),
        };
        let file = make_rule("cpu_float", condition, vec![alert("cpu high")]);
        let checker = TypeChecker::new();
        assert!(
            checker.check(&file).is_ok(),
            "Percent vs Float should be compatible",
        );
    }

    #[test]
    fn test_percent_vs_string_comparison_fails() {
        let condition = Condition::Comparison {
            left: Expr::FieldAccess {
                object: "process".to_string(),
                field: "cpu".to_string(),
            },
            op: CmpOp::Gt,
            right: Expr::Literal(Literal::Str("high".to_string())),
        };
        let file = make_rule("bad_compare", condition, vec![alert("oops")]);
        let checker = TypeChecker::new();
        let err = checker.check(&file).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(
            err[0].message.contains("type mismatch") || err[0].message.contains("non-numeric"),
            "should mention type mismatch or non-numeric, got: {}",
            err[0].message,
        );
    }

    #[test]
    fn test_unknown_field_produces_error() {
        let condition = Condition::Comparison {
            left: Expr::FieldAccess {
                object: "process".to_string(),
                field: "nonexistent".to_string(),
            },
            op: CmpOp::Gt,
            right: Expr::Literal(Literal::Int(0)),
        };
        let file = make_rule("bad_field", condition, vec![alert("oops")]);
        let checker = TypeChecker::new();
        let err = checker.check(&file).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(
            err[0].message.contains("unknown field"),
            "should mention unknown field, got: {}",
            err[0].message,
        );
    }

    #[test]
    fn test_unknown_object_produces_error() {
        let condition = Condition::Comparison {
            left: Expr::FieldAccess {
                object: "network".to_string(),
                field: "bytes".to_string(),
            },
            op: CmpOp::Gt,
            right: Expr::Literal(Literal::Int(0)),
        };
        let file = make_rule("bad_object", condition, vec![alert("oops")]);
        let checker = TypeChecker::new();
        let err = checker.check(&file).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(
            err[0].message.contains("unknown object"),
            "should mention unknown object, got: {}",
            err[0].message,
        );
    }
}

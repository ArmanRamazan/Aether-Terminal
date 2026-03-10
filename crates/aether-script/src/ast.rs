//! AST types for the rule DSL.
//!
//! Produced by the parser and consumed by the type checker and code generator.

use std::fmt;

use crate::lexer::Span;

/// A parsed rule file containing one or more rules.
#[derive(Debug, Clone)]
pub struct RuleFile {
    pub rules: Vec<Rule>,
}

/// A single rule: `rule <name> { when <condition> then <actions> }`.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub condition: Condition,
    pub actions: Vec<Action>,
    pub span: Span,
}

impl Rule {
    /// Extract duration in seconds if the top-level condition is Duration-wrapped.
    pub fn duration_secs(&self) -> Option<u64> {
        if let Condition::Duration { seconds, .. } = &self.condition {
            Some(*seconds)
        } else {
            None
        }
    }

    /// Get the evaluatable condition, unwrapping a Duration wrapper if present.
    pub fn eval_condition(&self) -> &Condition {
        if let Condition::Duration { condition, .. } = &self.condition {
            condition
        } else {
            &self.condition
        }
    }
}

/// Boolean condition tree for the `when` clause.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Field comparison: `<expr> <op> <expr>`.
    Comparison { left: Expr, op: CmpOp, right: Expr },
    /// Duration guard: inner condition must hold for the given seconds.
    Duration {
        condition: Box<Condition>,
        seconds: u64,
    },
    /// Logical AND of two conditions.
    And(Box<Condition>, Box<Condition>),
    /// Logical OR of two conditions.
    Or(Box<Condition>, Box<Condition>),
    /// Logical NOT of a condition.
    Not(Box<Condition>),
}

/// Value expression: field access or literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Field access: `process.cpu`.
    FieldAccess { object: String, field: String },
    /// Literal value.
    Literal(Literal),
}

/// Literal values in expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Percent(f64),
    Duration(u64),
    Str(String),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gt => write!(f, ">"),
            Self::Lt => write!(f, "<"),
            Self::Gte => write!(f, ">="),
            Self::Lte => write!(f, "<="),
            Self::Eq => write!(f, "=="),
            Self::Neq => write!(f, "!="),
        }
    }
}

/// Alert severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    /// Numeric encoding for Cranelift IR constants.
    pub(crate) fn as_i64(self) -> i64 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Critical => 2,
        }
    }
}

/// Action to execute when the `when` clause matches.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Alert { message: String, severity: Severity },
    Kill,
    Log { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_node_construction() {
        let rule = Rule {
            name: "high_cpu".to_string(),
            condition: Condition::Comparison {
                left: Expr::FieldAccess {
                    object: "process".to_string(),
                    field: "cpu".to_string(),
                },
                op: CmpOp::Gt,
                right: Expr::Literal(Literal::Percent(90.0)),
            },
            actions: vec![Action::Alert {
                message: "CPU too high".to_string(),
                severity: Severity::Critical,
            }],
            span: Span { start: 0, end: 100 },
        };

        assert_eq!(rule.name, "high_cpu");
        assert_eq!(rule.actions.len(), 1);
        assert!(rule.duration_secs().is_none(), "no duration wrapper");

        match &rule.condition {
            Condition::Comparison { left, op, right } => {
                assert_eq!(
                    *left,
                    Expr::FieldAccess {
                        object: "process".to_string(),
                        field: "cpu".to_string(),
                    },
                );
                assert_eq!(*op, CmpOp::Gt);
                assert_eq!(*right, Expr::Literal(Literal::Percent(90.0)));
            }
            _ => panic!("expected Comparison condition"),
        }

        // Duration-wrapped rule.
        let duration_rule = Rule {
            name: "sustained".to_string(),
            condition: Condition::Duration {
                condition: Box::new(Condition::Comparison {
                    left: Expr::FieldAccess {
                        object: "process".to_string(),
                        field: "mem_growth".to_string(),
                    },
                    op: CmpOp::Gt,
                    right: Expr::Literal(Literal::Percent(5.0)),
                }),
                seconds: 60,
            },
            actions: vec![Action::Alert {
                message: "memory leak".to_string(),
                severity: Severity::Warning,
            }],
            span: Span { start: 0, end: 80 },
        };

        assert_eq!(duration_rule.duration_secs(), Some(60));
        assert!(
            matches!(duration_rule.eval_condition(), Condition::Comparison { .. }),
            "eval_condition should unwrap Duration",
        );
    }

    #[test]
    fn test_cmpop_display() {
        assert_eq!(CmpOp::Gt.to_string(), ">");
        assert_eq!(CmpOp::Lt.to_string(), "<");
        assert_eq!(CmpOp::Gte.to_string(), ">=");
        assert_eq!(CmpOp::Lte.to_string(), "<=");
        assert_eq!(CmpOp::Eq.to_string(), "==");
        assert_eq!(CmpOp::Neq.to_string(), "!=");
    }
}

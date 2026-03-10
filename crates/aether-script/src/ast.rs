//! AST types for the rule DSL.
//!
//! Produced by the parser and consumed by the type checker and code generator.

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type;

/// A single rule definition: `rule <name> { when <expr> then <action> }`.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub when_clause: Expr,
    pub then_clause: Action,
}

/// Boolean expression tree for the `when` clause.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Field comparison: `process.<field> <op> <value>`.
    Comparison {
        field: Field,
        op: CompareOp,
        value: Value,
    },
    /// Logical AND of two expressions.
    And(Box<Expr>, Box<Expr>),
    /// Logical OR of two expressions.
    Or(Box<Expr>, Box<Expr>),
}

/// A process field accessible in rule conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Pid,
    CpuPercent,
    MemBytes,
    MemGrowthPercent,
    State,
    Hp,
}

impl Field {
    /// Byte offset and Cranelift IR type for this field in `WorldStateFFI`.
    pub(crate) fn offset_and_type(self) -> (i32, Type) {
        match self {
            Self::Pid => (0, types::I32),
            Self::CpuPercent => (4, types::F32),
            Self::MemBytes => (8, types::I64),
            Self::MemGrowthPercent => (16, types::F32),
            Self::State => (20, types::I32),
            Self::Hp => (24, types::F32),
        }
    }
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
}

/// A literal value in rule conditions.
#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i64),
    Float(f64),
}

/// Action to execute when the `when` clause matches.
#[derive(Debug, Clone)]
pub enum Action {
    Alert { severity: Severity },
    Kill,
    Log,
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

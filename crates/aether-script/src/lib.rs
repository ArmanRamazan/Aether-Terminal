//! JIT-compiled rule DSL for user-defined monitoring rules.
//!
//! Custom lexer (logos) and recursive-descent parser produce an AST that is
//! type-checked and compiled to native code via Cranelift for fast evaluation.

pub mod ast;
pub mod codegen;
pub mod error;
pub mod runtime;

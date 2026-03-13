//! JIT-compiled rule DSL for user-defined monitoring rules.
//!
//! Custom lexer (logos) and recursive-descent parser produce an AST that is
//! type-checked and compiled to native code via Cranelift for fast evaluation.

pub mod ast;
pub(crate) mod codegen;
pub mod engine;
pub mod error;
pub mod hot_reload;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod types;

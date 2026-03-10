//! Error types for the aether-script crate.

/// Errors produced during script compilation and evaluation.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// Lexer encountered an unexpected character or invalid token.
    #[error("lex error: {0}")]
    Lex(String),

    /// Parser encountered unexpected token or malformed syntax.
    #[error("parse error: {0}")]
    Parse(String),

    /// Type checking failed.
    #[error("type error: {0}")]
    Type(String),

    /// Cranelift IR generation or verification failed.
    #[error("compile error: {0}")]
    Compile(String),

    /// File I/O error during hot-reload.
    #[error("io error: {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

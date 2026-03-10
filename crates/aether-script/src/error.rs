//! Error types for the aether-script crate.

/// Errors produced during script compilation and evaluation.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// Lexer or parser error with source location.
    #[error("parse error at {line}:{col}: {message}")]
    Parse {
        line: usize,
        col: usize,
        message: String,
    },

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

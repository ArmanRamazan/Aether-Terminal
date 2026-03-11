//! Error types for the analyze crate.

/// Errors produced by the diagnostic engine.
#[derive(Debug, thiserror::Error)]
pub enum AnalyzeError {
    #[error("collector error: {0}")]
    Collector(String),
    #[error("analyzer error: {0}")]
    Analyzer(String),
    #[error("rule evaluation error: {0}")]
    Rule(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_error_display() {
        let err = AnalyzeError::Collector("procfs read failed".into());
        assert_eq!(err.to_string(), "collector error: procfs read failed");

        let err = AnalyzeError::Analyzer("trend calculation failed".into());
        assert_eq!(err.to_string(), "analyzer error: trend calculation failed");

        let err = AnalyzeError::Rule("invalid threshold".into());
        assert_eq!(err.to_string(), "rule evaluation error: invalid threshold");

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = AnalyzeError::from(io_err);
        assert_eq!(err.to_string(), "I/O error: file missing");
    }
}

use async_trait::async_trait;

use aether_core::models::{Diagnostic, Severity};
use aether_core::traits::OutputSink;

use crate::serialize::diagnostic_to_json;

/// Output format for stdout/file sinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// One JSON line per diagnostic.
    Json,
    /// Human-readable text format.
    Text,
}

impl OutputFormat {
    /// Parse from config string.
    pub fn from_str_config(s: &str) -> Self {
        match s {
            "text" => Self::Text,
            _ => Self::Json,
        }
    }
}

/// Prints diagnostics to stdout.
pub struct StdoutSink {
    format: OutputFormat,
    min_severity: Severity,
}

impl StdoutSink {
    /// Create a new stdout sink.
    pub fn new(format: OutputFormat, min_severity: Severity) -> Self {
        Self {
            format,
            min_severity,
        }
    }
}

#[async_trait]
impl OutputSink for StdoutSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.format {
            OutputFormat::Json => {
                let json = diagnostic_to_json(diagnostic);
                println!("{json}");
            }
            OutputFormat::Text => {
                println!(
                    "[{sev}] {target:?}: {summary}",
                    sev = diagnostic.severity,
                    target = diagnostic.target,
                    summary = diagnostic.summary,
                );
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "stdout"
    }

    fn min_severity(&self) -> Severity {
        self.min_severity
    }
}

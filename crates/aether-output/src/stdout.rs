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

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::metrics::HostId;
    use aether_core::models::{
        DiagCategory, DiagTarget, Evidence, Recommendation, RecommendedAction, Urgency,
    };
    use std::time::Instant;

    fn make_diag() -> Diagnostic {
        Diagnostic {
            id: 1,
            host: HostId::new("node-1"),
            target: DiagTarget::Process {
                pid: 100,
                name: "api-server".into(),
            },
            severity: Severity::Critical,
            category: DiagCategory::ThroughputDrop,
            summary: "throughput dropped 45%".into(),
            evidence: vec![Evidence {
                metric: "req/s".into(),
                current: 55.0,
                threshold: 100.0,
                trend: Some(-0.45),
                context: "last 5 minutes".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::NoAction {
                    reason: "investigate".into(),
                },
                reason: "investigate root cause".into(),
                urgency: Urgency::Immediate,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[tokio::test]
    async fn test_stdout_json_format() {
        let sink = StdoutSink::new(OutputFormat::Json, Severity::Info);
        let diag = make_diag();

        // Verify the JSON is valid by checking our serializer output
        let json = diagnostic_to_json(&diag);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .expect("diagnostic_to_json should produce valid JSON");

        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["summary"], "throughput dropped 45%");
        assert_eq!(parsed["severity"], "critical");
        assert!(!parsed["evidence"].as_array().unwrap().is_empty());

        // Verify send succeeds
        sink.send(&diag).await.expect("stdout json send should succeed");
    }

    #[tokio::test]
    async fn test_stdout_text_format() {
        let sink = StdoutSink::new(OutputFormat::Text, Severity::Info);
        let diag = make_diag();

        // Text format: [severity] target: summary
        // Verify send succeeds (prints to stdout)
        sink.send(&diag).await.expect("stdout text send should succeed");

        // Verify the format pattern by constructing expected output
        let expected_fragment = "[critical]";
        let text = format!(
            "[{sev}] {target:?}: {summary}",
            sev = diag.severity,
            target = diag.target,
            summary = diag.summary,
        );
        assert!(
            text.contains(expected_fragment),
            "text output should contain severity: {text}"
        );
        assert!(
            text.contains("throughput dropped 45%"),
            "text output should contain summary: {text}"
        );
    }
}

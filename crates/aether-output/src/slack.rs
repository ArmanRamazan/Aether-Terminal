use async_trait::async_trait;

use aether_core::models::{Diagnostic, Severity};
use aether_core::traits::OutputSink;

/// Sends diagnostics to a Slack Incoming Webhook using Block Kit formatting.
pub struct SlackSink {
    webhook_url: String,
    min_severity: Severity,
    client: reqwest::Client,
}

impl SlackSink {
    /// Create a new Slack sink.
    pub fn new(webhook_url: String, min_severity: Severity) -> Self {
        Self {
            webhook_url,
            min_severity,
            client: reqwest::Client::new(),
        }
    }
}

/// Build the Slack Block Kit payload for a diagnostic.
pub(crate) fn build_slack_payload(diagnostic: &Diagnostic) -> serde_json::Value {
    let color = match diagnostic.severity {
        Severity::Critical => "#dc3545",
        Severity::Warning => "#ffc107",
        Severity::Info => "#0d6efd",
        _ => "#6c757d",
    };

    let evidence_text: String = diagnostic
        .evidence
        .iter()
        .map(|e| format!("- `{}`: {:.1} (threshold: {:.1})", e.metric, e.current, e.threshold))
        .collect::<Vec<_>>()
        .join("\n");

    serde_json::json!({
        "attachments": [{
            "color": color,
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!(
                            "*[{}] {}*\nTarget: `{:?}`",
                            diagnostic.severity, diagnostic.summary, diagnostic.target
                        )
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": evidence_text
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("> {}", diagnostic.recommendation.reason)
                    }
                }
            ]
        }]
    })
}

#[async_trait]
impl OutputSink for SlackSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let payload = build_slack_payload(diagnostic);

        self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    fn name(&self) -> &str {
        "slack"
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
            host: HostId::new("test-host"),
            target: DiagTarget::Process {
                pid: 42,
                name: "nginx".into(),
            },
            severity: Severity::Critical,
            category: DiagCategory::CpuSpike,
            summary: "CPU spike on nginx".into(),
            evidence: vec![Evidence {
                metric: "cpu_percent".into(),
                current: 95.0,
                threshold: 80.0,
                trend: Some(5.0),
                context: "rising over 10 min".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "CPU usage".into(),
                },
                reason: "CPU above threshold".into(),
                urgency: Urgency::Immediate,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[test]
    fn test_slack_format() {
        let diag = make_diag();
        let payload = build_slack_payload(&diag);

        // Top-level has attachments array
        let attachments = payload["attachments"].as_array().expect("attachments array");
        assert_eq!(attachments.len(), 1);

        let attachment = &attachments[0];
        assert_eq!(attachment["color"], "#dc3545"); // Critical = red

        let blocks = attachment["blocks"].as_array().expect("blocks array");
        assert_eq!(blocks.len(), 3);

        // First block: summary with severity and target
        let summary_text = blocks[0]["text"]["text"].as_str().expect("summary text");
        assert!(summary_text.contains("[critical]"), "should contain severity");
        assert!(summary_text.contains("CPU spike on nginx"), "should contain summary");

        // Second block: evidence
        let evidence_text = blocks[1]["text"]["text"].as_str().expect("evidence text");
        assert!(evidence_text.contains("cpu_percent"), "should contain metric name");
        assert!(evidence_text.contains("95.0"), "should contain current value");
        assert!(evidence_text.contains("80.0"), "should contain threshold");

        // Third block: recommendation quote
        let rec_text = blocks[2]["text"]["text"].as_str().expect("recommendation text");
        assert!(rec_text.starts_with('>'), "should be a quote block");
        assert!(rec_text.contains("CPU above threshold"), "should contain reason");
    }

    #[test]
    fn test_slack_warning_color() {
        let mut diag = make_diag();
        diag.severity = Severity::Warning;
        let payload = build_slack_payload(&diag);
        assert_eq!(payload["attachments"][0]["color"], "#ffc107");
    }

    #[test]
    fn test_slack_info_color() {
        let mut diag = make_diag();
        diag.severity = Severity::Info;
        let payload = build_slack_payload(&diag);
        assert_eq!(payload["attachments"][0]["color"], "#0d6efd");
    }
}

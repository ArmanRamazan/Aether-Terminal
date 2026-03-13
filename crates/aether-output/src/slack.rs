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

#[async_trait]
impl OutputSink for SlackSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        let payload = serde_json::json!({
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
        });

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

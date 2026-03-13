use async_trait::async_trait;

use aether_core::models::{Diagnostic, Severity};
use aether_core::traits::OutputSink;

/// Sends diagnostics to a Discord Webhook using embed formatting.
pub struct DiscordSink {
    webhook_url: String,
    min_severity: Severity,
    client: reqwest::Client,
}

impl DiscordSink {
    /// Create a new Discord sink.
    pub fn new(webhook_url: String, min_severity: Severity) -> Self {
        Self {
            webhook_url,
            min_severity,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OutputSink for DiscordSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let color = match diagnostic.severity {
            Severity::Critical => 0xdc3545u32,
            Severity::Warning => 0xffc107,
            Severity::Info => 0x0d6efd,
            _ => 0x6c757d,
        };

        let fields: Vec<serde_json::Value> = diagnostic
            .evidence
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.metric,
                    "value": format!("{:.1} (threshold: {:.1})", e.current, e.threshold),
                    "inline": true,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "embeds": [{
                "title": format!("[{}] {}", diagnostic.severity, diagnostic.summary),
                "description": format!(
                    "**Target:** `{:?}`\n**Recommendation:** {}",
                    diagnostic.target, diagnostic.recommendation.reason
                ),
                "color": color,
                "fields": fields,
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
        "discord"
    }

    fn min_severity(&self) -> Severity {
        self.min_severity
    }
}

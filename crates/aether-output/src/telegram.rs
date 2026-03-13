use async_trait::async_trait;

use aether_core::models::{Diagnostic, Severity};
use aether_core::traits::OutputSink;

/// Sends diagnostics to a Telegram chat via Bot API.
pub struct TelegramSink {
    bot_token: String,
    chat_id: String,
    min_severity: Severity,
    client: reqwest::Client,
}

impl TelegramSink {
    /// Create a new Telegram sink.
    pub fn new(bot_token: String, chat_id: String, min_severity: Severity) -> Self {
        Self {
            bot_token,
            chat_id,
            min_severity,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OutputSink for TelegramSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let icon = match diagnostic.severity {
            Severity::Critical => "\u{1f534}",
            Severity::Warning => "\u{1f7e1}",
            Severity::Info => "\u{1f535}",
            _ => "\u{26aa}",
        };

        let evidence_lines: String = diagnostic
            .evidence
            .iter()
            .map(|e| format!("  `{}`: {:.1} (threshold: {:.1})", e.metric, e.current, e.threshold))
            .collect::<Vec<_>>()
            .join("\n");

        let text = format!(
            "{icon} *\\[{sev}\\] {summary}*\n\
             Target: `{target:?}`\n\
             {evidence}\n\
             _{reason}_",
            sev = diagnostic.severity,
            summary = diagnostic.summary,
            target = diagnostic.target,
            evidence = evidence_lines,
            reason = diagnostic.recommendation.reason,
        );

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "MarkdownV2",
        });

        self.client
            .post(&url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    fn name(&self) -> &str {
        "telegram"
    }

    fn min_severity(&self) -> Severity {
        self.min_severity
    }
}

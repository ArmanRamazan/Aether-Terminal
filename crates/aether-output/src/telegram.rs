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

/// Build the Telegram MarkdownV2 message text for a diagnostic.
pub(crate) fn build_telegram_text(diagnostic: &Diagnostic) -> String {
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

    format!(
        "{icon} *\\[{sev}\\] {summary}*\n\
         Target: `{target:?}`\n\
         {evidence}\n\
         _{reason}_",
        sev = diagnostic.severity,
        summary = diagnostic.summary,
        target = diagnostic.target,
        evidence = evidence_lines,
        reason = diagnostic.recommendation.reason,
    )
}

#[async_trait]
impl OutputSink for TelegramSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let text = build_telegram_text(diagnostic);

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
    fn test_telegram_format() {
        let diag = make_diag();
        let text = build_telegram_text(&diag);

        // Contains red circle emoji for critical
        assert!(text.contains('\u{1f534}'), "should contain red circle for critical");

        // Contains severity in escaped brackets
        assert!(text.contains("\\[critical\\]"), "should contain escaped severity");

        // Contains summary in bold
        assert!(text.contains("CPU spike on nginx"), "should contain summary");

        // Contains target
        assert!(text.contains("Target:"), "should contain target label");

        // Contains evidence
        assert!(text.contains("cpu_percent"), "should contain metric name");
        assert!(text.contains("95.0"), "should contain current value");
        assert!(text.contains("80.0"), "should contain threshold");

        // Contains recommendation in italics
        assert!(text.contains("_CPU above threshold_"), "should contain reason in italics");
    }

    #[test]
    fn test_telegram_warning_icon() {
        let mut diag = make_diag();
        diag.severity = Severity::Warning;
        let text = build_telegram_text(&diag);
        assert!(text.contains('\u{1f7e1}'), "should contain yellow circle for warning");
    }

    #[test]
    fn test_telegram_info_icon() {
        let mut diag = make_diag();
        diag.severity = Severity::Info;
        let text = build_telegram_text(&diag);
        assert!(text.contains('\u{1f535}'), "should contain blue circle for info");
    }
}

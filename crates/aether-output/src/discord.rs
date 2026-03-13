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

/// Build the Discord embed payload for a diagnostic.
pub(crate) fn build_discord_payload(diagnostic: &Diagnostic) -> serde_json::Value {
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

    serde_json::json!({
        "embeds": [{
            "title": format!("[{}] {}", diagnostic.severity, diagnostic.summary),
            "description": format!(
                "**Target:** `{:?}`\n**Recommendation:** {}",
                diagnostic.target, diagnostic.recommendation.reason
            ),
            "color": color,
            "fields": fields,
        }]
    })
}

#[async_trait]
impl OutputSink for DiscordSink {
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let payload = build_discord_payload(diagnostic);

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
    fn test_discord_format() {
        let diag = make_diag();
        let payload = build_discord_payload(&diag);

        // Top-level has embeds array
        let embeds = payload["embeds"].as_array().expect("embeds array");
        assert_eq!(embeds.len(), 1);

        let embed = &embeds[0];

        // Title contains severity and summary
        let title = embed["title"].as_str().expect("title");
        assert!(title.contains("[critical]"), "should contain severity");
        assert!(title.contains("CPU spike on nginx"), "should contain summary");

        // Color is red for critical
        assert_eq!(embed["color"], 0xdc3545u32);

        // Description contains target and recommendation
        let desc = embed["description"].as_str().expect("description");
        assert!(desc.contains("Target:"), "should contain target label");
        assert!(desc.contains("Recommendation:"), "should contain recommendation label");
        assert!(desc.contains("CPU above threshold"), "should contain reason");

        // Fields contain evidence
        let fields = embed["fields"].as_array().expect("fields array");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "cpu_percent");
        let value = fields[0]["value"].as_str().expect("field value");
        assert!(value.contains("95.0"), "should contain current value");
        assert!(value.contains("80.0"), "should contain threshold");
        assert!(fields[0]["inline"].as_bool().expect("inline bool"));
    }

    #[test]
    fn test_discord_warning_color() {
        let mut diag = make_diag();
        diag.severity = Severity::Warning;
        let payload = build_discord_payload(&diag);
        assert_eq!(payload["embeds"][0]["color"], 0xffc107u32);
    }
}

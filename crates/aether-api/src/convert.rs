//! Conversion functions between aether-core types and proto types.

use std::time::{SystemTime, UNIX_EPOCH};

use aether_core::{Diagnostic, Target};

use crate::proto;

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn diag_target_name(target: &aether_core::DiagTarget) -> String {
    match target {
        aether_core::DiagTarget::Process { pid, name } => format!("process:{name}({pid})"),
        aether_core::DiagTarget::Host(id) => format!("host:{id}"),
        aether_core::DiagTarget::Container { name, .. } => format!("container:{name}"),
        aether_core::DiagTarget::Disk { mount } => format!("disk:{mount}"),
        aether_core::DiagTarget::Network { interface } => format!("network:{interface}"),
        _ => "unknown".to_string(),
    }
}

impl From<&Diagnostic> for proto::Diagnostic {
    fn from(d: &Diagnostic) -> Self {
        proto::Diagnostic {
            id: d.id,
            target_name: diag_target_name(&d.target),
            severity: d.severity.to_string(),
            category: d.category.to_string(),
            summary: d.summary.clone(),
            recommendation: d.recommendation.reason.clone(),
        }
    }
}

impl From<&Target> for proto::Target {
    fn from(t: &Target) -> Self {
        proto::Target {
            id: t.name.clone(),
            name: t.name.clone(),
            kind: t.kind.to_string(),
            endpoints: t.endpoints.clone(),
        }
    }
}

impl From<&aether_core::events::IntegrationEvent> for proto::IntegrationEvent {
    fn from(event: &aether_core::events::IntegrationEvent) -> Self {
        let (event_type, payload) = match event {
            aether_core::events::IntegrationEvent::DiagnosticCreated {
                diagnostic_id,
                severity,
                summary,
            } => (
                "diagnostic_created".to_string(),
                format!("{{\"diagnostic_id\":{diagnostic_id},\"severity\":\"{severity}\",\"summary\":\"{summary}\"}}"),
            ),
            aether_core::events::IntegrationEvent::DiagnosticResolved { diagnostic_id } => (
                "diagnostic_resolved".to_string(),
                format!("{{\"diagnostic_id\":{diagnostic_id}}}"),
            ),
            aether_core::events::IntegrationEvent::ActionProposed {
                action_id,
                description,
            } => (
                "action_proposed".to_string(),
                format!("{{\"action_id\":\"{action_id}\",\"description\":\"{description}\"}}"),
            ),
            aether_core::events::IntegrationEvent::ActionApproved { action_id } => (
                "action_approved".to_string(),
                format!("{{\"action_id\":\"{action_id}\"}}"),
            ),
            aether_core::events::IntegrationEvent::ActionDenied { action_id } => (
                "action_denied".to_string(),
                format!("{{\"action_id\":\"{action_id}\"}}"),
            ),
            aether_core::events::IntegrationEvent::ActionExecuted { action_id, success } => (
                "action_executed".to_string(),
                format!("{{\"action_id\":\"{action_id}\",\"success\":{success}}}"),
            ),
            aether_core::events::IntegrationEvent::TargetDiscovered { name, kind } => (
                "target_discovered".to_string(),
                format!("{{\"name\":\"{name}\",\"kind\":\"{kind}\"}}"),
            ),
            aether_core::events::IntegrationEvent::TargetLost { name } => (
                "target_lost".to_string(),
                format!("{{\"name\":\"{name}\"}}"),
            ),
            _ => ("unknown".to_string(), "{}".to_string()),
        };

        proto::IntegrationEvent {
            event_type,
            payload,
            timestamp: now_unix_ms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::events::IntegrationEvent;
    use aether_core::{
        DiagCategory, DiagTarget, Diagnostic, Evidence, HostId, Recommendation, RecommendedAction,
        Severity, Target, TargetKind, Urgency,
    };
    use std::collections::HashMap;

    #[test]
    fn test_event_conversion_diagnostic_created() {
        let event = IntegrationEvent::DiagnosticCreated {
            diagnostic_id: 42,
            severity: "critical".to_string(),
            summary: "OOM detected".to_string(),
        };
        let proto_event: proto::IntegrationEvent = (&event).into();
        assert_eq!(proto_event.event_type, "diagnostic_created");
        assert!(proto_event.payload.contains("42"));
        assert!(proto_event.payload.contains("critical"));
        assert!(proto_event.payload.contains("OOM detected"));
        assert!(proto_event.timestamp > 0);
    }

    #[test]
    fn test_event_conversion_diagnostic_resolved() {
        let event = IntegrationEvent::DiagnosticResolved { diagnostic_id: 7 };
        let proto_event: proto::IntegrationEvent = (&event).into();
        assert_eq!(proto_event.event_type, "diagnostic_resolved");
        assert!(proto_event.payload.contains("7"));
    }

    #[test]
    fn test_event_conversion_action_proposed() {
        let event = IntegrationEvent::ActionProposed {
            action_id: "act-1".to_string(),
            description: "restart nginx".to_string(),
        };
        let proto_event: proto::IntegrationEvent = (&event).into();
        assert_eq!(proto_event.event_type, "action_proposed");
        assert!(proto_event.payload.contains("act-1"));
        assert!(proto_event.payload.contains("restart nginx"));
    }

    #[test]
    fn test_event_conversion_target_discovered() {
        let event = IntegrationEvent::TargetDiscovered {
            name: "web-server".to_string(),
            kind: "service".to_string(),
        };
        let proto_event: proto::IntegrationEvent = (&event).into();
        assert_eq!(proto_event.event_type, "target_discovered");
        assert!(proto_event.payload.contains("web-server"));
    }

    #[test]
    fn test_diagnostic_conversion() {
        let diag = Diagnostic {
            id: 1,
            host: HostId::new("node-1"),
            target: DiagTarget::Host(HostId::new("node-1")),
            severity: Severity::Critical,
            category: DiagCategory::CpuSaturation,
            summary: "CPU at 99%".to_string(),
            evidence: vec![Evidence {
                metric: "cpu_percent".to_string(),
                current: 99.0,
                threshold: 90.0,
                trend: None,
                context: "CPU usage exceeds threshold".to_string(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Restart {
                    reason: "reduce load".to_string(),
                },
                reason: "reduce load".to_string(),
                urgency: Urgency::Immediate,
                auto_executable: false,
            },
            detected_at: std::time::Instant::now(),
            resolved_at: None,
        };
        let proto_diag: proto::Diagnostic = (&diag).into();
        assert_eq!(proto_diag.id, 1);
        assert_eq!(proto_diag.target_name, "host:node-1");
        assert_eq!(proto_diag.severity, "critical");
        assert_eq!(proto_diag.summary, "CPU at 99%");
        assert_eq!(proto_diag.recommendation, "reduce load");
    }

    #[test]
    fn test_target_conversion() {
        let target = Target {
            name: "my-svc".to_string(),
            kind: TargetKind::Service,
            endpoints: vec!["http://localhost:8080".to_string()],
            labels: HashMap::new(),
        };
        let proto_target: proto::Target = (&target).into();
        assert_eq!(proto_target.id, "my-svc");
        assert_eq!(proto_target.name, "my-svc");
        assert_eq!(proto_target.endpoints, vec!["http://localhost:8080"]);
    }
}

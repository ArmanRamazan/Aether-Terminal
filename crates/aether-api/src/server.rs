//! gRPC server implementing the AetherService.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};
use tracing::{debug, warn};

use aether_core::event_bus::EventBus;
use aether_core::{
    AgentAction, ArbiterQueue, Diagnostic, Severity, Target,
};

use crate::proto;

/// gRPC server for machine-to-machine integration.
///
/// Exposes diagnostics, targets, event streaming, and action execution
/// to external projects (Autoscaler, Service Graph, Auto-Fix Agent).
pub struct AetherGrpcServer<E: EventBus> {
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    targets: Arc<Mutex<Vec<Target>>>,
    event_bus: Arc<E>,
    arbiter: Arc<Mutex<ArbiterQueue>>,
}

impl<E: EventBus> AetherGrpcServer<E> {
    /// Create a new gRPC server with shared state.
    pub fn new(
        diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
        targets: Arc<Mutex<Vec<Target>>>,
        event_bus: Arc<E>,
        arbiter: Arc<Mutex<ArbiterQueue>>,
    ) -> Self {
        Self {
            diagnostics,
            targets,
            event_bus,
            arbiter,
        }
    }
}

#[allow(clippy::result_large_err)] // tonic::Status is large by design
fn lock_or_status<'a, T>(lock: &'a Mutex<T>, name: &str) -> Result<std::sync::MutexGuard<'a, T>, Status> {
    lock.lock()
        .map_err(|_| Status::internal(format!("{name} lock poisoned")))
}

fn diag_target_name(target: &aether_core::DiagTarget) -> String {
    match target {
        aether_core::DiagTarget::Process { pid, name } => format!("process:{name}({pid})"),
        aether_core::DiagTarget::Host(id) => format!("host:{id}"),
        aether_core::DiagTarget::Container { name, .. } => format!("container:{name}"),
        aether_core::DiagTarget::Disk { mount } => format!("disk:{mount}"),
        aether_core::DiagTarget::Network { interface } => format!("network:{interface}"),
        _ => "unknown".to_string(),
    }
}

fn core_to_proto_diagnostic(d: &Diagnostic) -> proto::Diagnostic {
    proto::Diagnostic {
        id: d.id,
        target_name: diag_target_name(&d.target),
        severity: d.severity.to_string(),
        category: d.category.to_string(),
        summary: d.summary.clone(),
        recommendation: d.recommendation.reason.clone(),
    }
}

fn core_to_proto_target(t: &Target) -> proto::Target {
    proto::Target {
        id: t.name.clone(),
        name: t.name.clone(),
        kind: t.kind.to_string(),
        endpoints: t.endpoints.clone(),
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn integration_to_proto(event: &aether_core::events::IntegrationEvent) -> proto::IntegrationEvent {
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

fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_lowercase().as_str() {
        "info" => Some(Severity::Info),
        "warning" => Some(Severity::Warning),
        "critical" => Some(Severity::Critical),
        _ => None,
    }
}

#[tonic::async_trait]
impl<E: EventBus + 'static> proto::aether_service_server::AetherService for AetherGrpcServer<E> {
    async fn get_diagnostics(
        &self,
        request: Request<proto::GetDiagnosticsRequest>,
    ) -> Result<Response<proto::GetDiagnosticsResponse>, Status> {
        let req = request.into_inner();
        let diags = lock_or_status(&self.diagnostics, "diagnostics")?;

        let severity_filter = req.severity_filter.as_deref().and_then(parse_severity);
        let target_filter = req.target_filter.as_deref();

        let diagnostics: Vec<proto::Diagnostic> = diags
            .iter()
            .filter(|d| d.resolved_at.is_none())
            .filter(|d| severity_filter.is_none_or(|s| d.severity == s))
            .filter(|d| {
                target_filter.is_none_or(|t| diag_target_name(&d.target).contains(t))
            })
            .map(core_to_proto_diagnostic)
            .collect();

        debug!(count = diagnostics.len(), "GetDiagnostics response");
        Ok(Response::new(proto::GetDiagnosticsResponse { diagnostics }))
    }

    async fn get_targets(
        &self,
        _request: Request<proto::GetTargetsRequest>,
    ) -> Result<Response<proto::GetTargetsResponse>, Status> {
        let targets = lock_or_status(&self.targets, "targets")?;

        let targets: Vec<proto::Target> = targets.iter().map(core_to_proto_target).collect();

        debug!(count = targets.len(), "GetTargets response");
        Ok(Response::new(proto::GetTargetsResponse { targets }))
    }

    type StreamEventsStream =
        std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::IntegrationEvent, Status>> + Send>>;

    async fn stream_events(
        &self,
        request: Request<proto::StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();
        let severity_filter = req.severity_filter.clone();
        let rx = self.event_bus.subscribe();

        debug!(?severity_filter, "StreamEvents subscription started");

        let stream = BroadcastStream::new(rx)
            .filter_map(move |result| {
                match result {
                    Ok(event) => {
                        // Apply severity filter for diagnostic events.
                        if let Some(ref filter) = severity_filter {
                            if let aether_core::events::IntegrationEvent::DiagnosticCreated {
                                ref severity,
                                ..
                            } = event
                            {
                                if severity != filter {
                                    return None;
                                }
                            }
                        }
                        Some(Ok(integration_to_proto(&event)))
                    }
                    Err(e) => {
                        warn!(error = %e, "broadcast stream lagged");
                        None
                    }
                }
            });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn execute_action(
        &self,
        request: Request<proto::ExecuteActionRequest>,
    ) -> Result<Response<proto::ExecuteActionResponse>, Status> {
        let req = request.into_inner();

        let action = match req.action_type.as_str() {
            "kill_process" => {
                let pid: u32 = req
                    .parameters
                    .get("pid")
                    .ok_or_else(|| Status::invalid_argument("missing 'pid' parameter"))?
                    .parse()
                    .map_err(|_| Status::invalid_argument("invalid 'pid' value"))?;
                AgentAction::KillProcess { pid }
            }
            "restart_service" => {
                let name = req
                    .parameters
                    .get("name")
                    .ok_or_else(|| Status::invalid_argument("missing 'name' parameter"))?
                    .clone();
                AgentAction::RestartService { name }
            }
            "inspect" => {
                let pid: u32 = req
                    .parameters
                    .get("pid")
                    .ok_or_else(|| Status::invalid_argument("missing 'pid' parameter"))?
                    .parse()
                    .map_err(|_| Status::invalid_argument("invalid 'pid' value"))?;
                AgentAction::Inspect { pid }
            }
            "custom_script" => {
                let command = req
                    .parameters
                    .get("command")
                    .ok_or_else(|| Status::invalid_argument("missing 'command' parameter"))?
                    .clone();
                AgentAction::CustomScript { command }
            }
            other => return Err(Status::invalid_argument(format!("unknown action type: {other}"))),
        };

        let target_pid: u32 = req
            .target
            .parse()
            .unwrap_or(0);

        let mut arbiter = lock_or_status(&self.arbiter, "arbiter")?;
        let action_id = arbiter.enqueue(action, target_pid, "grpc-api");

        debug!(action_id = %action_id, "ExecuteAction enqueued");
        Ok(Response::new(proto::ExecuteActionResponse {
            action_id,
            status: "pending_approval".to_string(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::event_bus::InProcessEventBus;

    fn make_server() -> AetherGrpcServer<InProcessEventBus> {
        AetherGrpcServer::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(InProcessEventBus::new(16)),
            Arc::new(Mutex::new(ArbiterQueue::default())),
        )
    }

    #[test]
    fn test_server_construction() {
        let server = make_server();
        assert!(server.diagnostics.lock().is_ok());
        assert!(server.targets.lock().is_ok());
        assert!(server.arbiter.lock().is_ok());
    }

    #[test]
    fn test_parse_severity() {
        assert_eq!(parse_severity("info"), Some(Severity::Info));
        assert_eq!(parse_severity("WARNING"), Some(Severity::Warning));
        assert_eq!(parse_severity("critical"), Some(Severity::Critical));
        assert_eq!(parse_severity("bogus"), None);
    }

    #[test]
    fn test_integration_to_proto() {
        let event = aether_core::events::IntegrationEvent::DiagnosticCreated {
            diagnostic_id: 42,
            severity: "critical".to_string(),
            summary: "OOM".to_string(),
        };
        let proto_event = integration_to_proto(&event);
        assert_eq!(proto_event.event_type, "diagnostic_created");
        assert!(proto_event.payload.contains("42"));
        assert!(proto_event.timestamp > 0);
    }

    #[tokio::test]
    async fn test_get_diagnostics_empty() {
        use proto::aether_service_server::AetherService;

        let server = make_server();
        let req = Request::new(proto::GetDiagnosticsRequest {
            severity_filter: None,
            target_filter: None,
        });
        let resp = server.get_diagnostics(req).await.unwrap();
        assert!(resp.into_inner().diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_get_targets_empty() {
        use proto::aether_service_server::AetherService;

        let server = make_server();
        let req = Request::new(proto::GetTargetsRequest {});
        let resp = server.get_targets(req).await.unwrap();
        assert!(resp.into_inner().targets.is_empty());
    }

    #[tokio::test]
    async fn test_execute_action_kill_process() {
        use proto::aether_service_server::AetherService;

        let server = make_server();
        let mut params = std::collections::HashMap::new();
        params.insert("pid".to_string(), "1234".to_string());

        let req = Request::new(proto::ExecuteActionRequest {
            action_type: "kill_process".to_string(),
            target: "1234".to_string(),
            parameters: params,
        });
        let resp = server.execute_action(req).await.unwrap();
        let inner = resp.into_inner();
        assert_eq!(inner.status, "pending_approval");
        assert!(!inner.action_id.is_empty());

        // Verify it was enqueued in the arbiter.
        let arbiter = server.arbiter.lock().unwrap();
        assert_eq!(arbiter.pending_count(), 1);
    }

    #[tokio::test]
    async fn test_execute_action_unknown_type() {
        use proto::aether_service_server::AetherService;

        let server = make_server();
        let req = Request::new(proto::ExecuteActionRequest {
            action_type: "nuke_everything".to_string(),
            target: "0".to_string(),
            parameters: std::collections::HashMap::new(),
        });
        let result = server.execute_action(req).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }
}

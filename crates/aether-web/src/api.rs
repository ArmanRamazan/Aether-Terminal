//! REST API handlers for process, connection, stats, arbiter, and diagnostic endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use aether_core::models::Diagnostic;
use aether_core::AgentAction;

use crate::state::SharedState;

// ── Response structs ──────────────────────────────────────────────────

/// JSON representation of a process node.
#[derive(Debug, Serialize)]
pub struct ProcessResponse {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    pub state: String,
    pub hp: f32,
    pub xp: u32,
    pub position: [f32; 3],
}

/// JSON representation of a single process with its connections.
#[derive(Debug, Serialize)]
pub struct ProcessDetailResponse {
    #[serde(flatten)]
    pub process: ProcessResponse,
    pub connections: Vec<ConnectionResponse>,
}

/// JSON representation of a network edge.
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub from_pid: u32,
    pub to_pid: u32,
    pub protocol: String,
    pub bytes_per_sec: u64,
}

/// Aggregate system statistics.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub process_count: usize,
    pub total_cpu: f32,
    pub total_memory: u64,
    pub avg_hp: f32,
}

/// JSON representation of a pending arbiter action.
#[derive(Debug, Serialize)]
pub struct ArbiterActionResponse {
    pub id: String,
    pub source: String,
    pub action: String,
    pub pid: u32,
}

/// JSON representation of a diagnostic finding.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticResponse {
    pub id: u64,
    pub host: String,
    pub target_type: String,
    pub target_name: String,
    pub severity: String,
    pub category: String,
    pub summary: String,
    pub evidence: Vec<EvidenceResponse>,
    pub recommendation: RecommendationResponse,
}

/// A single piece of evidence in a diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceResponse {
    pub metric: String,
    pub current: f64,
    pub threshold: f64,
    pub context: String,
}

/// Recommended action for a diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct RecommendationResponse {
    pub action: String,
    pub reason: String,
    pub urgency: String,
}

/// Aggregate diagnostic severity counts.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticStatsResponse {
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
    pub total: u32,
}

/// Query parameters for filtering diagnostics.
#[derive(Debug, Deserialize)]
pub struct DiagnosticFilter {
    pub severity: Option<String>,
    pub host: Option<String>,
}

/// Top CPU process entry.
#[derive(Debug, Serialize)]
pub struct TopCpuProcess {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
}

/// Active diagnostic counts by severity.
#[derive(Debug, Serialize)]
pub struct DiagnosticsActive {
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
}

/// GET /api/metrics/summary response.
#[derive(Debug, Serialize)]
pub struct MetricsSummaryResponse {
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub load_avg: [f64; 3],
    pub diagnostics_active: DiagnosticsActive,
    pub process_count: u32,
    pub top_cpu_processes: Vec<TopCpuProcess>,
}

/// Query parameters for metrics history.
#[derive(Debug, Deserialize)]
pub struct MetricsHistoryQuery {
    pub metric: String,
    #[serde(default = "default_duration")]
    pub duration: u64,
}

fn default_duration() -> u64 {
    300
}

/// A single time-series sample in API responses.
#[derive(Debug, Serialize)]
pub struct SampleResponse {
    pub timestamp: u64,
    pub value: f64,
}

/// GET /api/metrics/history response.
#[derive(Debug, Serialize)]
pub struct MetricsHistoryResponse {
    pub metric: String,
    pub samples: Vec<SampleResponse>,
}

// ── Handlers ──────────────────────────────────────────────────────────

/// GET /api/processes — all processes as JSON array.
pub async fn list_processes(State(state): State<SharedState>) -> Json<Vec<ProcessResponse>> {
    let world = state.world.read().expect("world lock poisoned");
    let processes = world.processes().map(process_to_response).collect();
    Json(processes)
}

/// GET /api/processes/:pid — single process with its connections.
pub async fn get_process(
    State(state): State<SharedState>,
    Path(pid): Path<u32>,
) -> Result<Json<ProcessDetailResponse>, StatusCode> {
    let world = state.world.read().expect("world lock poisoned");

    let node = world.find_by_pid(pid).ok_or(StatusCode::NOT_FOUND)?;
    let process = process_to_response(node);

    let connections = world
        .edge_pairs_with_data()
        .into_iter()
        .filter(|(from, to, _)| *from == pid || *to == pid)
        .map(|(from, to, edge)| ConnectionResponse {
            from_pid: from,
            to_pid: to,
            protocol: edge.protocol.to_string(),
            bytes_per_sec: edge.bytes_per_sec,
        })
        .collect();

    Ok(Json(ProcessDetailResponse {
        process,
        connections,
    }))
}

/// GET /api/connections — all edges as JSON array.
pub async fn list_connections(
    State(state): State<SharedState>,
) -> Json<Vec<ConnectionResponse>> {
    let world = state.world.read().expect("world lock poisoned");
    let connections = world
        .edge_pairs_with_data()
        .into_iter()
        .map(|(from, to, edge)| ConnectionResponse {
            from_pid: from,
            to_pid: to,
            protocol: edge.protocol.to_string(),
            bytes_per_sec: edge.bytes_per_sec,
        })
        .collect();
    Json(connections)
}

/// GET /api/stats — aggregate system statistics.
pub async fn get_stats(State(state): State<SharedState>) -> Json<StatsResponse> {
    let world = state.world.read().expect("world lock poisoned");
    let count = world.process_count();

    let (total_cpu, total_memory, total_hp) =
        world
            .processes()
            .fold((0.0_f32, 0_u64, 0.0_f32), |(cpu, mem, hp), p| {
                (cpu + p.cpu_percent, mem + p.mem_bytes, hp + p.hp)
            });

    let avg_hp = if count > 0 {
        total_hp / count as f32
    } else {
        0.0
    };

    Json(StatsResponse {
        process_count: count,
        total_cpu,
        total_memory,
        avg_hp,
    })
}

/// GET /api/arbiter/pending — pending arbiter actions.
pub async fn list_pending_actions(
    State(state): State<SharedState>,
) -> Json<Vec<ArbiterActionResponse>> {
    let arbiter = state.arbiter.lock().expect("arbiter lock poisoned");
    let actions = arbiter
        .pending()
        .iter()
        .map(|entry| ArbiterActionResponse {
            id: entry.id.clone(),
            source: entry.source.clone(),
            action: format_action(&entry.action),
            pid: entry.pid,
        })
        .collect();
    Json(actions)
}

/// POST /api/arbiter/:id/approve — approve a pending action.
pub async fn approve_action(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut arbiter = state.arbiter.lock().expect("arbiter lock poisoned");
    match arbiter.approve(&id) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

/// POST /api/arbiter/:id/deny — deny a pending action.
pub async fn deny_action(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut arbiter = state.arbiter.lock().expect("arbiter lock poisoned");
    match arbiter.deny(&id) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

// ── Diagnostic Handlers ──────────────────────────────────────────────

/// GET /api/diagnostics — all diagnostics, optionally filtered.
pub async fn list_diagnostics(
    State(state): State<SharedState>,
    Query(filter): Query<DiagnosticFilter>,
) -> Json<Vec<DiagnosticResponse>> {
    let diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    let results = diags
        .iter()
        .filter(|d| {
            if let Some(ref sev) = filter.severity {
                if d.severity.to_string() != sev.to_lowercase() {
                    return false;
                }
            }
            if let Some(ref host) = filter.host {
                if d.host.as_str() != host {
                    return false;
                }
            }
            true
        })
        .map(diagnostic_to_response)
        .collect();
    Json(results)
}

/// GET /api/diagnostics/stats — severity counts.
pub async fn get_diagnostic_stats(
    State(state): State<SharedState>,
) -> Json<DiagnosticStatsResponse> {
    let diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    Json(compute_diagnostic_stats(&diags))
}

/// GET /api/diagnostics/:id — single diagnostic detail.
pub async fn get_diagnostic(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> Result<Json<DiagnosticResponse>, StatusCode> {
    let diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    let diag = diags.iter().find(|d| d.id == id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(diagnostic_to_response(diag)))
}

/// POST /api/diagnostics/:id/dismiss — remove a diagnostic.
pub async fn dismiss_diagnostic(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> StatusCode {
    let mut diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    let before = diags.len();
    diags.retain(|d| d.id != id);
    if diags.len() < before {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// POST /api/diagnostics/:id/execute — queue diagnostic action in arbiter.
pub async fn execute_diagnostic(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> StatusCode {
    let diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    let diag = match diags.iter().find(|d| d.id == id) {
        Some(d) => d,
        None => return StatusCode::NOT_FOUND,
    };

    let agent_action = recommended_to_agent_action(&diag.recommendation.action);
    let Some(action) = agent_action else {
        return StatusCode::UNPROCESSABLE_ENTITY;
    };

    drop(diags);
    let mut arbiter = state.arbiter.lock().expect("arbiter lock poisoned");
    arbiter.submit("diagnostic-engine".to_string(), action);
    StatusCode::OK
}

// ── Metrics Handlers ─────────────────────────────────────────────────

/// GET /api/metrics/summary — current metric values.
pub async fn get_metrics_summary(State(state): State<SharedState>) -> Json<MetricsSummaryResponse> {
    let world = state.world.read().expect("world lock poisoned");

    let count = world.process_count();
    let (total_cpu, total_memory) =
        world
            .processes()
            .fold((0.0_f64, 0_u64), |(cpu, mem), p| {
                (cpu + p.cpu_percent as f64, mem + p.mem_bytes)
            });

    let mut top_cpu: Vec<TopCpuProcess> = world
        .processes()
        .map(|p| TopCpuProcess {
            pid: p.pid,
            name: p.name.clone(),
            cpu: p.cpu_percent,
        })
        .collect();
    top_cpu.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
    top_cpu.truncate(5);

    drop(world);

    let diags = state.diagnostics.lock().expect("diagnostics lock poisoned");
    let diag_stats = compute_diagnostic_stats(&diags);
    drop(diags);

    let sys = state.system_metrics.read().expect("system_metrics lock poisoned");
    let memory_total_bytes = sys.memory_total_bytes;
    let load_avg = sys.load_avg;
    drop(sys);

    Json(MetricsSummaryResponse {
        cpu_percent: total_cpu,
        memory_used_bytes: total_memory,
        memory_total_bytes,
        load_avg,
        diagnostics_active: DiagnosticsActive {
            critical: diag_stats.critical,
            warning: diag_stats.warning,
            info: diag_stats.info,
        },
        process_count: count as u32,
        top_cpu_processes: top_cpu,
    })
}

/// GET /api/metrics/history — time-series data for a named metric.
pub async fn get_metrics_history(
    State(state): State<SharedState>,
    Query(query): Query<MetricsHistoryQuery>,
) -> Json<MetricsHistoryResponse> {
    let store = state.metrics.lock().expect("metrics lock poisoned");
    let samples = store
        .history(&query.metric, query.duration)
        .into_iter()
        .map(|s| SampleResponse {
            timestamp: s.timestamp_ms,
            value: s.value,
        })
        .collect();

    Json(MetricsHistoryResponse {
        metric: query.metric,
        samples,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────

fn process_to_response(p: &aether_core::ProcessNode) -> ProcessResponse {
    ProcessResponse {
        pid: p.pid,
        ppid: p.ppid,
        name: p.name.clone(),
        cpu_percent: p.cpu_percent,
        mem_bytes: p.mem_bytes,
        state: p.state.to_string(),
        hp: p.hp,
        xp: p.xp,
        position: p.position_3d.to_array(),
    }
}

fn format_action(action: &AgentAction) -> String {
    match action {
        AgentAction::KillProcess { pid } => format!("kill_process({pid})"),
        AgentAction::RestartService { name } => format!("restart_service({name})"),
        AgentAction::Inspect { pid } => format!("inspect({pid})"),
        AgentAction::CustomScript { command } => format!("custom_script({command})"),
    }
}

pub(crate) fn diagnostic_to_response(d: &Diagnostic) -> DiagnosticResponse {
    use aether_core::models::DiagTarget;

    let (target_type, target_name) = match &d.target {
        DiagTarget::Process { pid, name } => ("process".to_string(), format!("{name} (pid {pid})")),
        DiagTarget::Host(id) => ("host".to_string(), id.as_str().to_string()),
        DiagTarget::Container { id, name } => ("container".to_string(), format!("{name} ({id})")),
        DiagTarget::Disk { mount } => ("disk".to_string(), mount.clone()),
        DiagTarget::Network { interface } => ("network".to_string(), interface.clone()),
        _ => ("unknown".to_string(), "unknown".to_string()),
    };

    DiagnosticResponse {
        id: d.id,
        host: d.host.as_str().to_string(),
        target_type,
        target_name,
        severity: d.severity.to_string(),
        category: d.category.to_string(),
        summary: d.summary.clone(),
        evidence: d
            .evidence
            .iter()
            .map(|e| EvidenceResponse {
                metric: e.metric.clone(),
                current: e.current,
                threshold: e.threshold,
                context: e.context.clone(),
            })
            .collect(),
        recommendation: RecommendationResponse {
            action: d.recommendation.action.to_string(),
            reason: d.recommendation.reason.clone(),
            urgency: d.recommendation.urgency.to_string(),
        },
    }
}

pub(crate) fn compute_diagnostic_stats(diags: &[Diagnostic]) -> DiagnosticStatsResponse {
    use aether_core::models::Severity;

    let mut critical = 0u32;
    let mut warning = 0u32;
    let mut info = 0u32;
    for d in diags {
        match d.severity {
            Severity::Critical => critical += 1,
            Severity::Warning => warning += 1,
            Severity::Info => info += 1,
        }
    }
    DiagnosticStatsResponse {
        critical,
        warning,
        info,
        total: critical + warning + info,
    }
}

fn recommended_to_agent_action(
    action: &aether_core::models::RecommendedAction,
) -> Option<AgentAction> {
    use aether_core::models::RecommendedAction;

    match action {
        RecommendedAction::KillProcess { pid, .. } => Some(AgentAction::KillProcess { pid: *pid }),
        RecommendedAction::Restart { reason } => {
            Some(AgentAction::RestartService { name: reason.clone() })
        }
        RecommendedAction::Investigate { what } => {
            Some(AgentAction::CustomScript { command: format!("investigate: {what}") })
        }
        RecommendedAction::NoAction { .. } => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex, RwLock};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use std::time::Instant;

    use aether_core::models::{
        ConnectionState, DiagCategory, DiagTarget, Diagnostic, Evidence, NetworkEdge, ProcessNode,
        ProcessState, Protocol, RecommendedAction, Recommendation, Severity, Urgency,
    };
    use aether_core::{ArbiterQueue, WorldGraph};
    use glam::Vec3;

    use crate::server::router;
    use crate::state::SharedState;

    fn make_process(pid: u32, cpu: f32, mem: u64, hp: f32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp,
            xp: 10,
            position_3d: Vec3::new(1.0, 2.0, 3.0),
        }
    }

    fn make_edge(source_pid: u32) -> NetworkEdge {
        NetworkEdge {
            source_pid,
            dest_pid: None,
            dest: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
            protocol: Protocol::TCP,
            bytes_per_sec: 1024,
            state: ConnectionState::Established,
        }
    }

    fn test_state() -> SharedState {
        SharedState::new(
            Arc::new(RwLock::new(WorldGraph::new())),
            Arc::new(Mutex::new(ArbiterQueue::default())),
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    #[tokio::test]
    async fn test_processes_endpoint_returns_json() {
        let state = test_state();
        {
            let mut world = state.world.write().unwrap();
            world.add_process(make_process(1, 10.0, 1024, 100.0));
            world.add_process(make_process(2, 20.0, 2048, 80.0));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/processes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 2, "should return 2 processes");
        assert!(json.iter().any(|p| p["pid"] == 1));
        assert!(json.iter().any(|p| p["pid"] == 2));
    }

    #[tokio::test]
    async fn test_process_by_pid_found() {
        let state = test_state();
        {
            let mut world = state.world.write().unwrap();
            world.add_process(make_process(42, 50.0, 4096, 90.0));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/processes/42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["pid"], 42);
        assert_eq!(json["name"], "proc-42");
        assert_eq!(json["state"], "Running");
        assert_eq!(json["position"], serde_json::json!([1.0, 2.0, 3.0]));
    }

    #[tokio::test]
    async fn test_process_by_pid_not_found_returns_404() {
        let state = test_state();
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/processes/999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_stats_endpoint_returns_aggregates() {
        let state = test_state();
        {
            let mut world = state.world.write().unwrap();
            world.add_process(make_process(1, 10.0, 1000, 80.0));
            world.add_process(make_process(2, 30.0, 2000, 60.0));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["process_count"], 2);
        assert_eq!(json["total_cpu"], 40.0);
        assert_eq!(json["total_memory"], 3000);
        assert_eq!(json["avg_hp"], 70.0);
    }

    #[tokio::test]
    async fn test_connections_endpoint_returns_array() {
        let state = test_state();
        {
            let mut world = state.world.write().unwrap();
            world.add_process(make_process(1, 10.0, 1024, 100.0));
            world.add_process(make_process(2, 20.0, 2048, 80.0));
            world.add_connection(1, 2, make_edge(1));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/connections")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["from_pid"], 1);
        assert_eq!(json[0]["to_pid"], 2);
        assert_eq!(json[0]["protocol"], "TCP");
        assert_eq!(json[0]["bytes_per_sec"], 1024);
    }

    fn make_diagnostic(id: u64, severity: Severity, host: &str) -> Diagnostic {
        use aether_core::metrics::HostId;

        Diagnostic {
            id,
            host: HostId::new(host),
            target: DiagTarget::Process {
                pid: 42,
                name: "nginx".to_string(),
            },
            severity,
            category: DiagCategory::CpuSpike,
            summary: format!("test diagnostic {id}"),
            evidence: vec![Evidence {
                metric: "cpu_percent".to_string(),
                current: 95.0,
                threshold: 80.0,
                trend: None,
                context: "high cpu".to_string(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "cpu usage".to_string(),
                },
                reason: "cpu above threshold".to_string(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[tokio::test]
    async fn test_diagnostics_endpoint_returns_array() {
        let state = test_state();
        {
            let mut diags = state.diagnostics.lock().unwrap();
            diags.push(make_diagnostic(1, Severity::Warning, "host-1"));
            diags.push(make_diagnostic(2, Severity::Critical, "host-1"));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/diagnostics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 2, "should return 2 diagnostics");
        assert_eq!(json[0]["id"], 1);
        assert_eq!(json[1]["id"], 2);
        assert_eq!(json[0]["severity"], "warning");
        assert_eq!(json[1]["severity"], "critical");
    }

    #[tokio::test]
    async fn test_diagnostics_stats_counts() {
        let state = test_state();
        {
            let mut diags = state.diagnostics.lock().unwrap();
            diags.push(make_diagnostic(1, Severity::Info, "h"));
            diags.push(make_diagnostic(2, Severity::Warning, "h"));
            diags.push(make_diagnostic(3, Severity::Critical, "h"));
            diags.push(make_diagnostic(4, Severity::Critical, "h"));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/diagnostics/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["critical"], 2);
        assert_eq!(json["warning"], 1);
        assert_eq!(json["info"], 1);
        assert_eq!(json["total"], 4);
    }

    #[tokio::test]
    async fn test_metrics_summary_returns_system_metrics() {
        let state = test_state();
        {
            let mut world = state.world.write().unwrap();
            world.add_process(make_process(1, 25.0, 1024, 100.0));
        }
        state.update_system_metrics(16_000_000_000, [1.5, 1.2, 0.9]);

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["memory_total_bytes"], 16_000_000_000u64);
        assert_eq!(json["load_avg"][0], 1.5);
        assert_eq!(json["load_avg"][1], 1.2);
        assert_eq!(json["load_avg"][2], 0.9);
        assert_eq!(json["process_count"], 1);
        assert!(json["memory_used_bytes"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_diagnostics_filter_by_severity() {
        let state = test_state();
        {
            let mut diags = state.diagnostics.lock().unwrap();
            diags.push(make_diagnostic(1, Severity::Info, "h"));
            diags.push(make_diagnostic(2, Severity::Critical, "h"));
            diags.push(make_diagnostic(3, Severity::Critical, "h"));
        }

        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/diagnostics?severity=critical")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 2, "only critical diagnostics returned");
        assert!(json.iter().all(|d| d["severity"] == "critical"));
    }
}

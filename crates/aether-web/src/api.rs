//! REST API handlers for process, connection, stats, and arbiter endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

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
    pub id: usize,
    pub source: String,
    pub action: String,
}

// ── Handlers ──────────────────────────────────────────────────────────

/// GET /api/processes — all processes as JSON array.
pub async fn list_processes(State(state): State<SharedState>) -> Json<Vec<ProcessResponse>> {
    let world = state.world.read().await;
    let processes = world.processes().map(process_to_response).collect();
    Json(processes)
}

/// GET /api/processes/:pid — single process with its connections.
pub async fn get_process(
    State(state): State<SharedState>,
    Path(pid): Path<u32>,
) -> Result<Json<ProcessDetailResponse>, StatusCode> {
    let world = state.world.read().await;

    let node = world.find_by_pid(pid).ok_or(StatusCode::NOT_FOUND)?;
    let process = process_to_response(node);

    let connections = world
        .edge_pairs_with_data()
        .into_iter()
        .filter(|(from, to, _)| *from == pid || *to == pid)
        .map(|(from, to, edge)| ConnectionResponse {
            from_pid: from,
            to_pid: to,
            protocol: format!("{:?}", edge.protocol),
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
    let world = state.world.read().await;
    let connections = world
        .edge_pairs_with_data()
        .into_iter()
        .map(|(from, to, edge)| ConnectionResponse {
            from_pid: from,
            to_pid: to,
            protocol: format!("{:?}", edge.protocol),
            bytes_per_sec: edge.bytes_per_sec,
        })
        .collect();
    Json(connections)
}

/// GET /api/stats — aggregate system statistics.
pub async fn get_stats(State(state): State<SharedState>) -> Json<StatsResponse> {
    let world = state.world.read().await;
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
    let arbiter = state.arbiter.lock().await;
    let actions = arbiter
        .pending()
        .enumerate()
        .map(|(id, entry)| ArbiterActionResponse {
            id,
            source: entry.source.clone(),
            action: format_action(&entry.action),
        })
        .collect();
    Json(actions)
}

/// POST /api/arbiter/:id/approve — approve a pending action.
pub async fn approve_action(
    State(state): State<SharedState>,
    Path(id): Path<usize>,
) -> StatusCode {
    let mut arbiter = state.arbiter.lock().await;
    if arbiter.approve(id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// POST /api/arbiter/:id/deny — deny a pending action.
pub async fn deny_action(
    State(state): State<SharedState>,
    Path(id): Path<usize>,
) -> StatusCode {
    let mut arbiter = state.arbiter.lock().await;
    if arbiter.deny(id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn process_to_response(p: &aether_core::ProcessNode) -> ProcessResponse {
    ProcessResponse {
        pid: p.pid,
        ppid: p.ppid,
        name: p.name.clone(),
        cpu_percent: p.cpu_percent,
        mem_bytes: p.mem_bytes,
        state: format!("{:?}", p.state),
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tokio::sync::{Mutex, RwLock};
    use tower::ServiceExt;

    use aether_core::models::{
        ConnectionState, NetworkEdge, ProcessNode, ProcessState, Protocol,
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
        )
    }

    #[tokio::test]
    async fn test_processes_endpoint_returns_json() {
        let state = test_state();
        {
            let mut world = state.world.write().await;
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
            let mut world = state.world.write().await;
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
            let mut world = state.world.write().await;
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
            let mut world = state.world.write().await;
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
}

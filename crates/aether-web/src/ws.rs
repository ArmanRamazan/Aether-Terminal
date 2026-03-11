//! WebSocket handler for real-time world state push.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use crate::api::{ConnectionResponse, ProcessResponse, StatsResponse};
use crate::state::SharedState;

const PUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Full world state snapshot pushed to clients.
#[derive(Debug, Serialize)]
pub struct WorldUpdate {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub processes: Vec<ProcessResponse>,
    pub connections: Vec<ConnectionResponse>,
    pub stats: StatsResponse,
    pub timestamp: u64,
}

/// Messages sent by the client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    SelectProcess { pid: u32 },
    ArbiterAction { action: String, action_id: String },
}

/// GET /ws — upgrade to WebSocket.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();

    use futures_util::{SinkExt, StreamExt};

    let push_state = state.clone();
    let mut push_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(PUSH_INTERVAL);
        loop {
            interval.tick().await;
            let update = build_world_update(&push_state).await;
            let json = match serde_json::to_string(&update) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("failed to serialize world update: {e}");
                    continue;
                }
            };
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    let recv_state = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => break,
                _ => continue,
            };
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(client_msg) => handle_client_message(client_msg, &recv_state).await,
                Err(e) => tracing::warn!("invalid client message: {e}"),
            }
        }
    });

    tokio::select! {
        _ = &mut push_task => recv_task.abort(),
        _ = &mut recv_task => push_task.abort(),
    }
}

async fn build_world_update(state: &SharedState) -> WorldUpdate {
    let world = state.world.read().await;

    let processes = world
        .processes()
        .map(|p| ProcessResponse {
            pid: p.pid,
            ppid: p.ppid,
            name: p.name.clone(),
            cpu_percent: p.cpu_percent,
            mem_bytes: p.mem_bytes,
            state: format!("{:?}", p.state),
            hp: p.hp,
            xp: p.xp,
            position: p.position_3d.to_array(),
        })
        .collect();

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

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    WorldUpdate {
        type_: "world_state",
        processes,
        connections,
        stats: StatsResponse {
            process_count: count,
            total_cpu,
            total_memory,
            avg_hp,
        },
        timestamp,
    }
}

async fn handle_client_message(msg: ClientMessage, state: &SharedState) {
    match msg {
        ClientMessage::SelectProcess { pid } => {
            tracing::debug!("client selected process {pid}");
        }
        ClientMessage::ArbiterAction { action, action_id } => {
            let id: usize = match action_id.parse() {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!("invalid action_id: {action_id}");
                    return;
                }
            };
            let mut arbiter = state.arbiter.lock().await;
            let result = match action.as_str() {
                "approve" => arbiter.approve(id),
                "deny" => arbiter.deny(id),
                other => {
                    tracing::warn!("unknown arbiter action: {other}");
                    return;
                }
            };
            if result {
                tracing::info!("arbiter action {action} on {action_id} succeeded");
            } else {
                tracing::warn!("arbiter action {action} on {action_id}: not found");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::{Mutex, RwLock};
    use tokio_tungstenite::tungstenite;

    use aether_core::models::{ProcessNode, ProcessState};
    use aether_core::{ArbiterQueue, WorldGraph};
    use glam::Vec3;

    use super::*;
    use crate::server::router;

    fn make_process(pid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: 10.0,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 10,
            position_3d: Vec3::ZERO,
        }
    }

    fn test_state() -> SharedState {
        SharedState::new(
            Arc::new(RwLock::new(WorldGraph::new())),
            Arc::new(Mutex::new(ArbiterQueue::default())),
        )
    }

    #[tokio::test]
    async fn test_ws_upgrade_succeeds() {
        let state = test_state();
        let app = router(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let url = format!("ws://127.0.0.1:{}/ws", addr.port());
        let (ws_stream, resp) =
            tokio_tungstenite::connect_async(&url).await.unwrap();
        assert_eq!(
            resp.status(),
            tungstenite::http::StatusCode::SWITCHING_PROTOCOLS,
            "WebSocket upgrade should return 101"
        );
        drop(ws_stream);
    }

    #[tokio::test]
    async fn test_world_update_serializes() {
        let state = test_state();
        {
            let mut world = state.world.write().await;
            world.add_process(make_process(1));
            world.add_process(make_process(2));
        }

        let update = build_world_update(&state).await;
        let json = serde_json::to_value(&update).unwrap();

        assert_eq!(json["type"], "world_state");
        assert_eq!(json["processes"].as_array().unwrap().len(), 2);
        assert!(json["timestamp"].as_u64().unwrap() > 0);
        assert_eq!(json["stats"]["process_count"], 2);
    }
}

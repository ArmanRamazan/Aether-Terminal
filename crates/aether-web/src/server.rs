use std::net::SocketAddr;

use axum::routing::{get, post};
use axum::Router;
use tokio_util::sync::CancellationToken;

use crate::api;
use crate::state::SharedState;
use crate::ws;

/// Build the axum router with all routes and shared state.
pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(|| async { "Aether Web UI" }))
        .route("/api/processes", get(api::list_processes))
        .route("/api/processes/{pid}", get(api::get_process))
        .route("/api/connections", get(api::list_connections))
        .route("/api/stats", get(api::get_stats))
        .route("/api/arbiter/pending", get(api::list_pending_actions))
        .route("/api/arbiter/{id}/approve", post(api::approve_action))
        .route("/api/arbiter/{id}/deny", post(api::deny_action))
        .route("/ws", get(ws::ws_handler))
        .with_state(state)
}

/// Start the web server with graceful shutdown.
pub async fn serve(state: SharedState, port: u16, cancel: CancellationToken) {
    let app = router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("aether-web listening on {addr}");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind {addr}: {e}");
            return;
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await
        .unwrap_or_else(|e| tracing::error!("web server error: {e}"));
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, RwLock};

    use aether_core::{ArbiterQueue, WorldGraph};

    use super::*;

    #[test]
    fn test_server_router_builds() {
        let state = SharedState::new(
            Arc::new(RwLock::new(WorldGraph::new())),
            Arc::new(Mutex::new(ArbiterQueue::default())),
        );
        let _router = router(state);
    }
}

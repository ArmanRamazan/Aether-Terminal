use std::net::SocketAddr;

use axum::routing::{get, post};
use axum::Router;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};

use crate::api;
use crate::embedded;
use crate::state::SharedState;
use crate::ws;

/// Health check response.
async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Build the axum router with all routes and shared state.
pub fn router(state: SharedState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        .route("/api/processes", get(api::list_processes))
        .route("/api/processes/{pid}", get(api::get_process))
        .route("/api/connections", get(api::list_connections))
        .route("/api/stats", get(api::get_stats))
        .route("/api/arbiter/pending", get(api::list_pending_actions))
        .route("/api/arbiter/{id}/approve", post(api::approve_action))
        .route("/api/arbiter/{id}/deny", post(api::deny_action))
        .route("/api/diagnostics", get(api::list_diagnostics))
        .route("/api/diagnostics/stats", get(api::get_diagnostic_stats))
        .route("/api/diagnostics/{id}", get(api::get_diagnostic))
        .route(
            "/api/diagnostics/{id}/dismiss",
            post(api::dismiss_diagnostic),
        )
        .route(
            "/api/diagnostics/{id}/execute",
            post(api::execute_diagnostic),
        )
        .route("/ws", get(ws::ws_handler))
        .fallback(embedded::static_handler)
        .layer(cors)
        .with_state(state)
}

/// Start the web server with graceful shutdown.
pub async fn serve(state: SharedState, port: u16, cancel: CancellationToken) {
    let app = router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("Web UI server listening on http://0.0.0.0:{port}");

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
            Arc::new(Mutex::new(Vec::new())),
        );
        let _router = router(state);
    }
}

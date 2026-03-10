//! SSE/HTTP transport for the MCP server.
//!
//! Runs an Axum HTTP server with JSON-RPC and Server-Sent Events endpoints.
//! Designed to run alongside the TUI as a background tokio task.

use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::handler::server::ServerHandler;
use rmcp::model::CallToolRequestParams;
use serde_json::{json, Value};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

use aether_core::WorldGraph;

use crate::server::McpServer;
use crate::McpError;

/// Shared state for axum request handlers.
struct SseState {
    server: McpServer,
    world: Arc<RwLock<WorldGraph>>,
    cancel: CancellationToken,
}

/// Run the MCP server over HTTP with SSE push notifications.
///
/// Binds to `0.0.0.0:<port>` and serves until the cancellation token fires.
pub(crate) async fn run_sse(
    server: McpServer,
    port: u16,
    cancel: CancellationToken,
) -> Result<(), McpError> {
    let world = Arc::clone(server.world());
    let state = Arc::new(SseState {
        server,
        world,
        cancel: cancel.clone(),
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("MCP SSE server listening on 0.0.0.0:{port}");

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    tracing::info!("MCP SSE server shut down");
    Ok(())
}

/// Build the axum router with all MCP endpoints.
fn build_router(state: Arc<SseState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/mcp", post(jsonrpc_handler))
        .route("/mcp/sse", get(sse_handler))
        .with_state(state)
}

/// GET /health — liveness check.
async fn health_handler() -> Json<Value> {
    Json(json!({"status": "ok", "version": "0.1.0"}))
}

/// POST /mcp — JSON-RPC 2.0 request handler.
async fn jsonrpc_handler(
    State(state): State<Arc<SseState>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    let result = match method {
        "initialize" => handle_initialize(&state.server),
        "notifications/initialized" => Ok(json!({})),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(&state.server, &request),
        _ => Err(json!({"code": -32601, "message": format!("method not found: {method}")})),
    };

    match result {
        Ok(value) => Json(json!({"jsonrpc": "2.0", "id": id, "result": value})),
        Err(error) => Json(json!({"jsonrpc": "2.0", "id": id, "error": error})),
    }
}

/// Handle MCP initialize request.
fn handle_initialize(server: &McpServer) -> Result<Value, Value> {
    let info = server.get_info();
    serde_json::to_value(info).map_err(|e| json!({"code": -32603, "message": e.to_string()}))
}

/// Handle tools/list request.
fn handle_tools_list() -> Result<Value, Value> {
    let tools = crate::server::tool_definitions();
    Ok(json!({"tools": tools}))
}

/// Handle tools/call request by dispatching to the server's tool handler.
fn handle_tools_call(server: &McpServer, request: &Value) -> Result<Value, Value> {
    let params = request
        .get("params")
        .ok_or_else(|| json!({"code": -32602, "message": "missing params"}))?;

    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .map(|n| n.to_owned())
        .ok_or_else(|| json!({"code": -32602, "message": "missing params.name"}))?;

    let arguments = params.get("arguments").and_then(|v| v.as_object()).cloned();

    let mut call_params = CallToolRequestParams::default();
    call_params.name = name.into();
    call_params.arguments = arguments;

    match crate::server::dispatch_tool(server, call_params) {
        Ok(result) => serde_json::to_value(result)
            .map_err(|e| json!({"code": -32603, "message": e.to_string()})),
        Err(e) => Err(serde_json::to_value(e)
            .unwrap_or(json!({"code": -32603, "message": "internal error"}))),
    }
}

/// GET /mcp/sse — Server-Sent Events stream for push notifications.
///
/// Sends periodic `system_update` events with process graph summary.
async fn sse_handler(
    State(state): State<Arc<SseState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(32);
    let world = Arc::clone(&state.world);
    let cancel = state.cancel.child_token();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let summary = build_system_summary(&world);
                    let event = Event::default()
                        .event("system_update")
                        .data(summary.to_string());
                    if tx.send(event).await.is_err() {
                        break; // client disconnected
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok::<_, Infallible>);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Build a JSON summary of the current system state for SSE events.
fn build_system_summary(world: &Arc<RwLock<WorldGraph>>) -> Value {
    let graph = match world.read() {
        Ok(g) => g,
        Err(_) => return json!({"error": "lock poisoned"}),
    };

    json!({
        "process_count": graph.process_count(),
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, RwLock};

    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use aether_core::{AgentAction, WorldGraph};

    use crate::arbiter::ArbiterQueue;

    use crate::server::McpServer;

    fn mock_state() -> super::Arc<super::SseState> {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let (action_tx, _rx) = mpsc::channel::<AgentAction>(16);
        let predictions = Arc::new(Mutex::new(Vec::new()));
        let server = McpServer::new(Arc::clone(&world), arbiter, action_tx, predictions);
        let cancel = CancellationToken::new();
        super::Arc::new(super::SseState {
            server,
            world,
            cancel,
        })
    }

    async fn response_json(
        app: axum::Router,
        request: axum::http::Request<Body>,
    ) -> (axum::http::StatusCode, Value) {
        let response = app.oneshot(request).await.expect("request failed");
        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body collect")
            .to_bytes();
        let json: Value = serde_json::from_slice(&body).expect("parse JSON");
        (status, json)
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let state = mock_state();
        let app = super::build_router(state);

        let request = axum::http::Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("build request");

        let (status, json) = response_json(app, request).await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], "0.1.0");
    }

    #[tokio::test]
    async fn test_post_mcp_tools_list_returns_tools() {
        let state = mock_state();
        let app = super::build_router(state);

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        });

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");

        let (status, json) = response_json(app, request).await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        let tools = json["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 5, "expected 5 tools");
    }

    #[tokio::test]
    async fn test_post_mcp_tool_call_returns_jsonrpc_response() {
        let state = mock_state();
        let app = super::build_router(state);

        let body = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "get_system_topology",
                "arguments": {}
            }
        });

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");

        let (status, json) = response_json(app, request).await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 2);
        assert!(
            json["result"].is_object(),
            "expected result object, got: {json}"
        );
    }

    #[tokio::test]
    async fn test_post_mcp_unknown_method_returns_error() {
        let state = mock_state();
        let app = super::build_router(state);

        let body = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "nonexistent",
            "params": {}
        });

        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
            .expect("build request");

        let (status, json) = response_json(app, request).await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(json["error"]["code"], -32601);
    }
}

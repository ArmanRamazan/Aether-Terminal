//! MCP server with JSON-RPC 2.0 method dispatch.
//!
//! Implements `rmcp::ServerHandler` to expose system data as MCP tools.

use std::future::Future;
use std::sync::{Arc, Mutex, RwLock};

use rmcp::{
    ErrorData as RmcpError,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    RoleServer,
};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::{AgentAction, WorldGraph};

use crate::arbiter::ArbiterQueue;

use crate::McpError;

/// Tool name constants.
const TOOL_GET_SYSTEM_TOPOLOGY: &str = "get_system_topology";
const TOOL_INSPECT_PROCESS: &str = "inspect_process";
const TOOL_LIST_ANOMALIES: &str = "list_anomalies";
const TOOL_EXECUTE_ACTION: &str = "execute_action";

/// MCP server exposing system data as tools for AI agents.
#[allow(dead_code)]
pub struct McpServer {
    world: Arc<RwLock<WorldGraph>>,
    arbiter: Arc<Mutex<ArbiterQueue>>,
    action_tx: mpsc::Sender<AgentAction>,
}

impl McpServer {
    /// Create a new MCP server with shared state.
    pub fn new(
        world: Arc<RwLock<WorldGraph>>,
        arbiter: Arc<Mutex<ArbiterQueue>>,
        action_tx: mpsc::Sender<AgentAction>,
    ) -> Self {
        Self {
            world,
            arbiter,
            action_tx,
        }
    }

    /// Run in stdio transport mode (blocks until cancelled or EOF).
    ///
    /// Used with `--mcp-stdio` flag. Reads JSON-RPC from stdin, writes to stdout.
    /// TUI must NOT be active when using this mode.
    pub async fn run_stdio(self, cancel: CancellationToken) -> Result<(), McpError> {
        crate::transport::stdio::run_stdio(self, cancel).await
    }

    /// Run SSE/HTTP transport on the given port (blocks until cancelled).
    ///
    /// Used with `--mcp-sse <PORT>` flag. Runs alongside TUI as a background task.
    pub async fn run_sse(self, port: u16, cancel: CancellationToken) -> Result<(), McpError> {
        crate::transport::sse::run_sse(self, port, cancel).await
    }

    /// Shared reference to the world graph.
    pub(crate) fn world(&self) -> &Arc<RwLock<WorldGraph>> {
        &self.world
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("aether-terminal", "0.1.0"))
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, RmcpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: tool_definitions(),
            ..Default::default()
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, RmcpError>> + Send + '_ {
        std::future::ready(dispatch_tool(self, request))
    }
}

/// Build the list of tool definitions exposed by this server.
pub(crate) fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            TOOL_GET_SYSTEM_TOPOLOGY,
            "Get full system topology as a process graph with connections and summary stats",
            empty_schema(),
        ),
        Tool::new(
            TOOL_INSPECT_PROCESS,
            "Inspect a specific process by PID, returning details, connections, and health",
            pid_schema(),
        ),
        Tool::new(
            TOOL_LIST_ANOMALIES,
            "List anomalous processes: low HP, high CPU, zombies",
            empty_schema(),
        ),
        Tool::new(
            TOOL_EXECUTE_ACTION,
            "Submit an action for human approval via the Arbiter queue",
            action_schema(),
        ),
    ]
}

/// Dispatch a tool call to the appropriate handler.
pub(crate) fn dispatch_tool(
    server: &McpServer,
    request: CallToolRequestParams,
) -> Result<CallToolResult, RmcpError> {
    match request.name.as_ref() {
        TOOL_GET_SYSTEM_TOPOLOGY => {
            let result = crate::tools::get_system_topology(&server.world);
            Ok(CallToolResult::success(vec![Content::text(
                result.to_string(),
            )]))
        }
        TOOL_INSPECT_PROCESS => {
            let pid = request
                .arguments
                .as_ref()
                .and_then(|args| args.get("pid"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .ok_or_else(|| {
                    RmcpError::new(
                        rmcp::model::ErrorCode::INVALID_PARAMS,
                        "missing required parameter: pid",
                        None,
                    )
                })?;
            match crate::tools::inspect_process(&server.world, pid) {
                Ok(result) => Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )])),
                Err(msg) => Ok(CallToolResult::error(vec![Content::text(msg)])),
            }
        }
        TOOL_LIST_ANOMALIES => {
            let result = crate::tools::list_anomalies(&server.world);
            Ok(CallToolResult::success(vec![Content::text(
                result.to_string(),
            )]))
        }
        TOOL_EXECUTE_ACTION => {
            let args = request.arguments.as_ref();
            let action = args
                .and_then(|a| a.get("action"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    RmcpError::new(
                        rmcp::model::ErrorCode::INVALID_PARAMS,
                        "missing required parameter: action",
                        None,
                    )
                })?;
            let pid = args
                .and_then(|a| a.get("pid"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(0);
            match crate::tools::execute_action(&server.arbiter, action, pid) {
                Ok(result) => Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )])),
                Err(msg) => Ok(CallToolResult::error(vec![Content::text(msg)])),
            }
        }
        other => Err(RmcpError::new(
            rmcp::model::ErrorCode::METHOD_NOT_FOUND,
            format!("unknown tool: {other}"),
            None,
        )),
    }
}

/// Empty JSON Schema (no parameters).
fn empty_schema() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::from_iter([("type".to_owned(), json!("object"))])
}

/// JSON Schema requiring a `pid` parameter.
fn pid_schema() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::from_iter([
        ("type".to_owned(), json!("object")),
        (
            "properties".to_owned(),
            json!({"pid": {"type": "integer", "description": "Process ID"}}),
        ),
        ("required".to_owned(), json!(["pid"])),
    ])
}

/// JSON Schema for execute_action: action type + optional pid.
fn action_schema() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::from_iter([
        ("type".to_owned(), json!("object")),
        (
            "properties".to_owned(),
            json!({
                "action": {
                    "type": "string",
                    "enum": ["kill", "restart", "inspect"],
                    "description": "Action to execute"
                },
                "pid": {
                    "type": "integer",
                    "description": "Target process ID"
                }
            }),
        ),
        ("required".to_owned(), json!(["action"])),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_server() -> McpServer {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let (action_tx, _rx) = mpsc::channel(16);
        McpServer::new(world, arbiter, action_tx)
    }

    fn make_call(name: &'static str) -> CallToolRequestParams {
        let mut params = CallToolRequestParams::default();
        params.name = name.into();
        params
    }

    #[test]
    fn test_new_creates_server() {
        let server = mock_server();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "aether-terminal");
        assert_eq!(info.server_info.version, "0.1.0");
        assert!(info.capabilities.tools.is_some(), "tools capability enabled");
    }

    #[test]
    fn test_tool_list_includes_all_four_tools() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names.len(), 4);
        assert!(names.contains(&"get_system_topology"));
        assert!(names.contains(&"inspect_process"));
        assert!(names.contains(&"list_anomalies"));
        assert!(names.contains(&"execute_action"));
    }

    #[test]
    fn test_dispatch_unknown_tool_returns_error() {
        let server = mock_server();
        let result = dispatch_tool(&server, make_call("nonexistent"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, rmcp::model::ErrorCode::METHOD_NOT_FOUND);
    }

    #[test]
    fn test_dispatch_get_system_topology_returns_json() {
        let server = mock_server();
        let result = dispatch_tool(&server, make_call("get_system_topology"))
            .expect("should succeed");
        assert!(!result.content.is_empty());
        let text = match &result.content[0].raw {
            rmcp::model::RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        assert!(parsed["processes"].is_array());
        assert!(parsed["connections"].is_array());
        assert!(parsed["summary"].is_object());
    }

    #[test]
    fn test_dispatch_execute_action_returns_pending_approval() {
        let server = mock_server();
        let mut params = CallToolRequestParams::default();
        params.name = "execute_action".into();
        params.arguments = Some(serde_json::from_value(json!({"action": "kill", "pid": 42})).unwrap());
        let result = dispatch_tool(&server, params).expect("should succeed");

        let text = match &result.content[0].raw {
            rmcp::model::RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        assert_eq!(parsed["status"], "pending_approval");
        assert!(parsed["action_id"].is_string());
    }

    #[test]
    fn test_dispatch_execute_action_missing_action_returns_error() {
        let server = mock_server();
        let mut params = CallToolRequestParams::default();
        params.name = "execute_action".into();
        params.arguments = Some(serde_json::from_value(json!({"pid": 1})).unwrap());
        let result = dispatch_tool(&server, params);
        assert!(result.is_err());
    }
}

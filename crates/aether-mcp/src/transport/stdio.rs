//! Stdio transport for MCP server.
//!
//! Reads JSON-RPC from stdin, writes responses to stdout.
//! Used by Claude Desktop via `--mcp-stdio` flag.
//! TUI must NOT be active when this transport is running.

use rmcp::ServiceExt;
use tokio_util::sync::CancellationToken;

use crate::McpError;
use crate::server::McpServer;

/// Run the MCP server over stdin/stdout.
///
/// Blocks until the peer disconnects (EOF) or the cancellation token fires.
/// Stdout is exclusively owned by JSON-RPC — logs must go to stderr.
pub async fn run_stdio(server: McpServer, cancel: CancellationToken) -> Result<(), McpError> {
    tracing::info!("MCP stdio transport starting");

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let service = server
        .serve_with_ct(transport, cancel)
        .await
        .map_err(Box::new)?;

    tracing::info!("MCP stdio transport initialized, awaiting requests");
    service.waiting().await?;

    tracing::info!("MCP stdio transport shut down");
    Ok(())
}

/// Serve MCP over an arbitrary async read/write pair.
///
/// Used by tests to inject duplex streams instead of real stdin/stdout.
#[cfg(test)]
async fn serve_on_transport(
    server: McpServer,
    reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    writer: impl tokio::io::AsyncWrite + Unpin + Send + 'static,
    cancel: CancellationToken,
) -> Result<(), McpError> {
    let service = server
        .serve_with_ct((reader, writer), cancel)
        .await
        .map_err(Box::new)?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, RwLock};

    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use aether_core::{AgentAction, ArbiterQueue, WorldGraph};

    use crate::server::McpServer;

    fn mock_server() -> McpServer {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let (action_tx, _rx) = mpsc::channel::<AgentAction>(16);
        McpServer::new(world, arbiter, action_tx)
    }

    /// Send a JSON-RPC line and read the response line.
    async fn send_and_recv(
        writer: &mut (impl AsyncWriteExt + Unpin),
        reader: &mut BufReader<impl tokio::io::AsyncRead + Unpin>,
        msg: &serde_json::Value,
    ) -> serde_json::Value {
        let mut line = serde_json::to_string(msg).expect("serialize");
        line.push('\n');
        writer.write_all(line.as_bytes()).await.expect("write");
        writer.flush().await.expect("flush");

        let mut response_line = String::new();
        reader.read_line(&mut response_line).await.expect("read");
        serde_json::from_str(&response_line).expect("parse response JSON")
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(
        writer: &mut (impl AsyncWriteExt + Unpin),
        msg: &serde_json::Value,
    ) {
        let mut line = serde_json::to_string(msg).expect("serialize");
        line.push('\n');
        writer.write_all(line.as_bytes()).await.expect("write");
        writer.flush().await.expect("flush");
    }

    #[tokio::test]
    async fn test_stdio_initialize_handshake_succeeds() {
        let server = mock_server();
        let cancel = CancellationToken::new();

        // Duplex: client_write → server reads, server writes → client_read
        let (client_stream, server_stream) = tokio::io::duplex(8192);
        let (server_read, server_write) = tokio::io::split(server_stream);
        let (client_read, mut client_write) = tokio::io::split(client_stream);
        let mut client_reader = BufReader::new(client_read);

        let server_cancel = cancel.clone();
        let server_handle = tokio::spawn(async move {
            super::serve_on_transport(server, server_read, server_write, server_cancel).await
        });

        // Step 1: Send initialize request
        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "0.1.0" }
            }
        });
        let response = send_and_recv(&mut client_write, &mut client_reader, &init_request).await;

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"].is_object(), "expected result object");
        assert_eq!(response["result"]["serverInfo"]["name"], "aether-terminal");

        // Step 2: Send initialized notification
        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        send_notification(&mut client_write, &initialized).await;

        // Cleanup: drop client writer to trigger EOF, then cancel
        drop(client_write);
        cancel.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    async fn test_stdio_tool_call_returns_valid_jsonrpc() {
        let server = mock_server();
        let cancel = CancellationToken::new();

        let (client_stream, server_stream) = tokio::io::duplex(8192);
        let (server_read, server_write) = tokio::io::split(server_stream);
        let (client_read, mut client_write) = tokio::io::split(client_stream);
        let mut client_reader = BufReader::new(client_read);

        let server_cancel = cancel.clone();
        tokio::spawn(async move {
            super::serve_on_transport(server, server_read, server_write, server_cancel).await
        });

        // Handshake
        let init = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0.1.0" }
            }
        });
        let _ = send_and_recv(&mut client_write, &mut client_reader, &init).await;
        send_notification(
            &mut client_write,
            &serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        )
        .await;

        // Call tools/list
        let list_request = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        });
        let response = send_and_recv(&mut client_write, &mut client_reader, &list_request).await;

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 2);
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 4, "expected 4 tools");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"get_system_topology"));
        assert!(names.contains(&"inspect_process"));

        // Call a tool
        let call_request = serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "get_system_topology", "arguments": {} }
        });
        let response = send_and_recv(&mut client_write, &mut client_reader, &call_request).await;

        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 3);
        let content = &response["result"]["content"];
        assert!(content.is_array(), "tool result has content array");
        let text = content[0]["text"].as_str().expect("text content");
        let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
        assert!(parsed["processes"].is_array(), "response contains processes");

        drop(client_write);
        cancel.cancel();
    }
}

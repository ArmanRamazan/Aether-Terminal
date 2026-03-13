# Integrations

## Slack

### Setup

1. Go to [Slack API: Incoming Webhooks](https://api.slack.com/messaging/webhooks) and create a new webhook for your workspace.
2. Choose the channel where alerts should be posted.
3. Copy the webhook URL.

### Configuration

```toml
[output.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
severity = "warning"
```

Set the environment variable before running:

```bash
export SLACK_WEBHOOK_URL="https://hooks.slack.com/services/T.../B.../xxx"
aether-terminal --config aether.toml
```

Messages use Slack Block Kit formatting with severity-colored attachments.

---

## Discord

### Setup

1. In your Discord server, go to **Server Settings → Integrations → Webhooks**.
2. Click **New Webhook**, choose a channel, and copy the webhook URL.

### Configuration

```toml
[output.discord]
webhook_url = "${DISCORD_WEBHOOK_URL}"
severity = "warning"
```

```bash
export DISCORD_WEBHOOK_URL="https://discord.com/api/webhooks/123456/abcdef"
aether-terminal --config aether.toml
```

Messages use Discord embed formatting with color-coded severity.

---

## Telegram

### Setup

1. Message [@BotFather](https://t.me/BotFather) on Telegram and create a new bot with `/newbot`.
2. Copy the bot token.
3. Add the bot to your group or channel.
4. Get the chat ID. The easiest way is to send a message to the bot and check:
   ```bash
   curl https://api.telegram.org/bot<TOKEN>/getUpdates
   ```
   Look for `"chat":{"id": ...}` in the response.

### Configuration

```toml
[output.telegram]
bot_token = "${TELEGRAM_BOT_TOKEN}"
chat_id = "${TELEGRAM_CHAT_ID}"
severity = "warning"
```

```bash
export TELEGRAM_BOT_TOKEN="123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
export TELEGRAM_CHAT_ID="-1001234567890"
aether-terminal --config aether.toml
```

Messages use Telegram Markdown formatting.

---

## MCP (Model Context Protocol)

Aether exposes an MCP server for AI agents to inspect system state and propose actions.

### Claude Desktop (stdio)

Add to `~/.config/claude/config.json` (or `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "aether-terminal": {
      "command": "aether-terminal",
      "args": ["--mcp-stdio"]
    }
  }
}
```

Restart Claude Desktop. The tools appear automatically.

### SSE transport (remote)

For multi-client or remote access:

```bash
aether-terminal --mcp-sse 3000
```

Connect any MCP-compatible client to `http://localhost:3000`.

### Available tools

| Tool | Input | Description |
|------|-------|-------------|
| `get_system_topology` | none | Process graph with connections and summary (top 50 processes) |
| `inspect_process` | `{ pid }` | Process details, connections, and recommendations |
| `list_anomalies` | none | Processes with HP < 50, zombies, CPU > 90% |
| `get_network_flows` | none | Active connections with protocol and throughput |
| `execute_action` | `{ action, pid }` | Submit action to Arbiter approval queue |

All actions are human-in-the-loop — `execute_action` adds to the Arbiter queue, nothing runs until approved.

---

## gRPC API

Machine-to-machine API for integrating Aether with external systems.

### Enable

```toml
[api.grpc]
enabled = true
bind = "0.0.0.0:50051"
```

Or via CLI:

```bash
aether-terminal --config aether.toml
```

The gRPC server starts automatically when `api.grpc.enabled = true`.

### Service definition

```protobuf
service AetherService {
    rpc GetDiagnostics(GetDiagnosticsRequest) returns (GetDiagnosticsResponse);
    rpc GetTargets(GetTargetsRequest) returns (GetTargetsResponse);
    rpc StreamEvents(StreamEventsRequest) returns (stream IntegrationEvent);
    rpc ExecuteAction(ExecuteActionRequest) returns (ExecuteActionResponse);
}
```

### Example: grpcurl

List services:

```bash
grpcurl -plaintext localhost:50051 list
```

Get all diagnostics:

```bash
grpcurl -plaintext localhost:50051 aether.v1.AetherService/GetDiagnostics
```

Filter by severity:

```bash
grpcurl -plaintext -d '{"severity_filter": "critical"}' \
  localhost:50051 aether.v1.AetherService/GetDiagnostics
```

Get discovered targets:

```bash
grpcurl -plaintext localhost:50051 aether.v1.AetherService/GetTargets
```

Stream events in real time:

```bash
grpcurl -plaintext localhost:50051 aether.v1.AetherService/StreamEvents
```

Execute an action (goes through Arbiter):

```bash
grpcurl -plaintext -d '{
  "action_type": "restart",
  "target": "api-gateway",
  "parameters": {"reason": "high error rate"}
}' localhost:50051 aether.v1.AetherService/ExecuteAction
```

### Proto file

The full proto definition is at `crates/aether-api/proto/aether.proto`.

---

## Prometheus

Aether both exports and consumes Prometheus metrics.

### Exporting metrics

Enable the `/metrics` endpoint:

```bash
aether-terminal --metrics
# or on a custom port:
aether-terminal --metrics 9091
```

Default port is 9090. The endpoint serves OpenMetrics text format.

**Exported metrics include:**
- Per-process CPU and memory usage
- System aggregates (total CPU, memory, process count)
- Diagnostic counts by severity
- Network throughput and connection states
- Custom metrics from the rules engine

### Prometheus scrape config

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: aether-terminal
    scrape_interval: 15s
    static_configs:
      - targets: ["localhost:9090"]
```

### Consuming metrics from targets

Aether scrapes Prometheus endpoints from discovered or configured targets. Configure the interval in the config file:

```toml
[scrape]
interval_seconds = 15
timeout_seconds = 5
```

Target metrics endpoints are set per target:

```toml
[[targets]]
name = "my-service"
prometheus = "http://localhost:8080/metrics"
```

Or discovered automatically — Aether probes `/metrics` on all discovered services.

### Polling a remote Prometheus server

Aether can also query an existing Prometheus server:

```bash
aether-terminal --prometheus http://prometheus.internal:9090
```

With custom interval:

```bash
aether-terminal --prometheus http://prometheus.internal:9090 --prometheus-interval 30
```

This feeds remote metrics into the diagnostic engine alongside local observations.

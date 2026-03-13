# Configuration Reference

Aether-Terminal is configured via a TOML or YAML file. The format is auto-detected by file extension (`.toml`, `.yaml`, `.yml`).

```bash
aether-terminal --config aether.toml
aether-terminal --config aether.yaml
```

No config file is required — all settings have sensible defaults.

## Environment variable interpolation

Use `${VAR_NAME}` to reference environment variables in any string value:

```toml
[output.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"

[output.telegram]
bot_token = "${TELEGRAM_BOT_TOKEN}"
chat_id = "${TELEGRAM_CHAT_ID}"
```

## CLI flags

CLI flags are applied on top of the config file. They take precedence.

| Flag | Default | Description |
|------|---------|-------------|
| `--config <PATH>` | none | Path to TOML or YAML config file |
| `--log-level <LEVEL>` | `info` | Logging level: trace, debug, info, warn, error |
| `--web [PORT]` | off (8080) | Start web UI on port |
| `--metrics [PORT]` | off (9090) | Expose Prometheus `/metrics` endpoint |
| `--prometheus <URL>` | none | Poll remote Prometheus server |
| `--prometheus-interval <SECS>` | 15 | Remote Prometheus poll interval |
| `--mcp-stdio` | false | Run MCP server on stdin/stdout (no TUI) |
| `--mcp-sse <PORT>` | off (3000) | Run MCP SSE server on port |
| `--rules <PATH>` | none | Load `.aether` rule files |
| `--predict` | false | Enable ML anomaly prediction |
| `--model-path <PATH>` | none | Directory with ONNX model files |
| `--ebpf` | false | Enable eBPF telemetry (Linux, CAP_BPF) |
| `--no-3d` | false | Disable 3D rendering, use flat tables |
| `--no-game` | false | Disable gamification |
| `--no-analyze` | false | Disable diagnostic engine |
| `--analyze-interval <SECS>` | 5 | Diagnostic analysis interval |
| `--theme <NAME>` | `cyberpunk` | Color theme name or path to TOML file |

---

## `[discovery]` — Service discovery

Auto-discovers services on the local machine or Kubernetes cluster.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable auto-discovery |
| `scan_ports` | list of u16 | `[5432, 6379, 8080, 9090, 9200, 27017]` | Ports to scan on localhost |
| `kubernetes` | bool | `false` | Enable Kubernetes API discovery |
| `kubernetes_namespace` | string | none | K8s namespace to scan (all if omitted) |
| `kubernetes_label_selector` | string | none | K8s label selector (e.g. `app=myservice`) |
| `interval_seconds` | u64 | `60` | Re-discovery interval in seconds |

Known service patterns (auto-detected by port):

| Port | Service |
|------|---------|
| 3306 | MySQL |
| 5432 | PostgreSQL |
| 6379 | Redis |
| 8080, 3000, 4000 | Generic HTTP |
| 9090 | Prometheus |
| 9200 | Elasticsearch |
| 27017 | MongoDB |

**TOML example:**

```toml
[discovery]
enabled = true
scan_ports = [5432, 6379, 8080, 9090]
kubernetes = false
interval_seconds = 30
```

**YAML example:**

```yaml
discovery:
  enabled: true
  scan_ports: [5432, 6379, 8080, 9090]
  kubernetes: false
  interval_seconds: 30
```

---

## `[[targets]]` — Explicit monitoring targets

Define services to monitor explicitly. Each target can have any combination of endpoints.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | **required** | Human-readable name |
| `kind` | string | none | Type: `"service"`, `"container"`, `"pod"` |
| `prometheus` | string | none | Prometheus metrics URL |
| `health` | string | none | Health check endpoint URL |
| `probe_tcp` | string | none | TCP probe endpoint (`host:port`) |
| `logs` | string | none | Log file path or glob |
| `labels` | map | `{}` | Arbitrary key-value labels for filtering |

**TOML example:**

```toml
[[targets]]
name = "api-gateway"
kind = "service"
health = "http://localhost:8080/health"
prometheus = "http://localhost:8080/metrics"
labels = { team = "platform", env = "production" }

[[targets]]
name = "postgres-primary"
kind = "service"
probe_tcp = "db.internal:5432"
labels = { team = "data" }

[[targets]]
name = "redis-cache"
probe_tcp = "localhost:6379"
```

**YAML example:**

```yaml
targets:
  - name: api-gateway
    kind: service
    health: http://localhost:8080/health
    prometheus: http://localhost:8080/metrics
    labels:
      team: platform
      env: production

  - name: postgres-primary
    kind: service
    probe_tcp: db.internal:5432
    labels:
      team: data
```

---

## `[thresholds]` — Diagnostic thresholds

Override default thresholds for the diagnostic engine. Values below trigger findings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `throughput_drop_percent` | f64 | `30.0` | Throughput drop threshold (%) |
| `latency_p99_ms` | f64 | `500.0` | P99 latency threshold (ms) |
| `connection_pool_percent` | f64 | `80.0` | Connection pool usage threshold (%) |
| `error_rate_percent` | f64 | `5.0` | Error rate threshold (%) |
| `tls_expiry_days` | u32 | `30` | TLS certificate expiry warning (days) |
| `health_check_timeout_ms` | u64 | `5000` | Health check timeout (ms) |

**TOML example:**

```toml
[thresholds]
error_rate_percent = 2.0
latency_p99_ms = 200.0
tls_expiry_days = 14
health_check_timeout_ms = 3000
```

**YAML example:**

```yaml
thresholds:
  error_rate_percent: 2.0
  latency_p99_ms: 200.0
  tls_expiry_days: 14
  health_check_timeout_ms: 3000
```

---

## `[output]` — Notification channels

Configure where diagnostic findings are sent. All sinks support severity filtering. Findings are deduplicated per (target, category) with a 60-second window.

Severity levels: `"info"`, `"warning"`, `"critical"`.

### `[output.slack]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `webhook_url` | string | **required** | Slack Incoming Webhook URL |
| `severity` | string | `"warning"` | Minimum severity to send |

```toml
[output.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
severity = "warning"
```

### `[output.discord]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `webhook_url` | string | **required** | Discord Webhook URL |
| `severity` | string | `"warning"` | Minimum severity to send |

```toml
[output.discord]
webhook_url = "${DISCORD_WEBHOOK_URL}"
severity = "critical"
```

### `[output.telegram]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token` | string | **required** | Telegram Bot API token |
| `chat_id` | string | **required** | Target chat or channel ID |
| `severity` | string | `"warning"` | Minimum severity to send |

```toml
[output.telegram]
bot_token = "${TELEGRAM_BOT_TOKEN}"
chat_id = "${TELEGRAM_CHAT_ID}"
severity = "warning"
```

### `[output.stdout]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable stdout output |
| `format` | string | `"json"` | Output format: `"json"` or `"text"` |
| `severity` | string | `"info"` | Minimum severity |

```toml
[output.stdout]
enabled = true
format = "text"
severity = "warning"
```

### `[output.file]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | **required** | Output file path |
| `severity` | string | `"info"` | Minimum severity |
| `max_size_mb` | u64 | `100` | Max file size before rotation (MB) |

```toml
[output.file]
path = "/var/log/aether/diagnostics.jsonl"
severity = "info"
max_size_mb = 50
```

---

## `[api]` — gRPC API

### `[api.grpc]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable gRPC server |
| `bind` | string | `"0.0.0.0:50051"` | Bind address and port |

**TOML example:**

```toml
[api.grpc]
enabled = true
bind = "0.0.0.0:50051"
```

**YAML example:**

```yaml
api:
  grpc:
    enabled: true
    bind: "0.0.0.0:50051"
```

---

## `[scrape]` — Prometheus scraping

Controls how often Aether scrapes `/metrics` endpoints from discovered targets.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interval_seconds` | u64 | `15` | Scrape interval |
| `timeout_seconds` | u64 | `5` | Per-scrape timeout |

```toml
[scrape]
interval_seconds = 10
timeout_seconds = 3
```

---

## `[probe]` — Active probing

Controls HTTP health checks and TCP probes for configured targets.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interval_seconds` | u64 | `30` | Probe interval |
| `timeout_seconds` | u64 | `5` | Per-probe timeout |

```toml
[probe]
interval_seconds = 15
timeout_seconds = 3
```

---

## Full example (TOML)

```toml
[discovery]
enabled = true
scan_ports = [5432, 6379, 8080, 9090]
interval_seconds = 30

[[targets]]
name = "api-gateway"
kind = "service"
health = "http://localhost:8080/health"
prometheus = "http://localhost:8080/metrics"
labels = { team = "platform", env = "production" }

[[targets]]
name = "postgres"
probe_tcp = "localhost:5432"

[thresholds]
error_rate_percent = 3.0
latency_p99_ms = 200.0
tls_expiry_days = 14

[output.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
severity = "warning"

[output.stdout]
enabled = true
format = "json"
severity = "info"

[api.grpc]
enabled = true
bind = "0.0.0.0:50051"

[scrape]
interval_seconds = 15
timeout_seconds = 5

[probe]
interval_seconds = 30
timeout_seconds = 5
```

## Full example (YAML)

```yaml
discovery:
  enabled: true
  scan_ports: [5432, 6379, 8080, 9090]
  interval_seconds: 30

targets:
  - name: api-gateway
    kind: service
    health: http://localhost:8080/health
    prometheus: http://localhost:8080/metrics
    labels:
      team: platform
      env: production
  - name: postgres
    probe_tcp: localhost:5432

thresholds:
  error_rate_percent: 3.0
  latency_p99_ms: 200.0
  tls_expiry_days: 14

output:
  slack:
    webhook_url: ${SLACK_WEBHOOK_URL}
    severity: warning
  stdout:
    enabled: true
    format: json
    severity: info

api:
  grpc:
    enabled: true
    bind: "0.0.0.0:50051"

scrape:
  interval_seconds: 15
  timeout_seconds: 5

probe:
  interval_seconds: 30
  timeout_seconds: 5
```

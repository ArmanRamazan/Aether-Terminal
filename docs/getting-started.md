# Getting Started

## Installation

### Build from source

Requires Rust 1.75+ and a C compiler (for SQLite bundled build).

```bash
git clone https://github.com/AquamarineOS/aether-terminal.git
cd aether-terminal
cargo build --release
```

The binary is at `target/release/aether-terminal`.

Optional build features:

```bash
# eBPF telemetry (Linux only, requires kernel headers)
cargo build --release --features ebpf

# Kubernetes service discovery
cargo build --release --features kubernetes

# Everything
cargo build --release --all-features
```

### cargo install

```bash
cargo install --git https://github.com/AquamarineOS/aether-terminal.git
```

### Docker

```bash
docker pull ghcr.io/aquamarineos/aether-terminal:latest
docker run -it --rm --pid=host ghcr.io/aquamarineos/aether-terminal:latest
```

`--pid=host` is required so Aether can see host processes. For eBPF support, add `--privileged`.

## Quick start

No configuration is required. Run with defaults to monitor the local machine:

```bash
aether-terminal
```

This starts the TUI with:
- Process monitoring via sysinfo (cross-platform)
- 3D process graph visualization
- Diagnostic engine (30+ built-in rules)
- Gamification (HP, XP, achievements)

### Useful flags

```bash
# Enable web UI on port 8080
aether-terminal --web

# Enable web UI on custom port
aether-terminal --web 3000

# Expose Prometheus metrics on port 9090
aether-terminal --metrics

# Disable 3D rendering (use flat tables)
aether-terminal --no-3d

# Set log level
aether-terminal --log-level debug

# Load custom rules
aether-terminal --rules ./rules/my-rules.aether

# Enable predictive anomaly detection
aether-terminal --predict --model-path ./models/

# Enable eBPF telemetry (Linux, requires CAP_BPF)
aether-terminal --ebpf
```

All flags can be combined:

```bash
aether-terminal --web --metrics --rules ./rules/ --log-level info
```

## First configuration

Create `aether.toml` to define monitoring targets explicitly:

```toml
# aether.toml

[discovery]
enabled = true
scan_ports = [5432, 6379, 8080, 9090]
interval_seconds = 60

[[targets]]
name = "my-api"
kind = "service"
health = "http://localhost:8080/health"
prometheus = "http://localhost:8080/metrics"

[[targets]]
name = "postgres"
kind = "service"
probe_tcp = "localhost:5432"
labels = { team = "backend", env = "staging" }

[thresholds]
error_rate_percent = 3.0
latency_p99_ms = 200.0
```

Run with the config:

```bash
aether-terminal --config aether.toml
```

YAML is also supported — Aether auto-detects by file extension:

```bash
aether-terminal --config aether.yaml
```

See [configuration.md](configuration.md) for the full reference.

## TUI navigation

| Key | Action |
|-----|--------|
| `F1` | Overview — process table + sparklines |
| `F2` | World 3D — 3D process graph |
| `F3` | Network — connection list |
| `F4` | Arbiter — AI action approval queue |
| `F5` | Rules — JIT rule engine monitor |
| `F6` | Diagnostics — findings viewer |
| `j`/`k` or ↑/↓ | Navigate up/down |
| `Enter` | Select |
| `Esc` | Go back |
| `/` | Search |
| `?` | Help overlay |
| `q` | Quit |

## See diagnostics

Aether runs 30+ diagnostic rules automatically. There are three ways to view findings:

### TUI (F6)

Press `F6` to open the Diagnostics tab. It shows all active findings with severity, category, affected target, and recommended action.

### Web UI

Start with `--web`:

```bash
aether-terminal --web
```

Open `http://localhost:8080` in a browser. The web dashboard shows real-time process graphs, metrics, and diagnostics. It updates via WebSocket every 500ms.

REST endpoints are also available:

```bash
curl http://localhost:8080/api/diagnostics
curl http://localhost:8080/api/diagnostics/stats
curl http://localhost:8080/api/processes
curl http://localhost:8080/api/stats
```

### Stdout output

Add an `[output.stdout]` section to your config:

```toml
[output.stdout]
enabled = true
format = "json"    # or "text"
severity = "warning"
```

Diagnostics matching the severity threshold are printed to stdout as they are detected.

## Connect an AI agent (MCP)

Aether exposes an [MCP](https://modelcontextprotocol.io/) server so AI agents can inspect the system topology, list anomalies, and propose actions.

### Claude Desktop

Add to your Claude Desktop `config.json`:

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

Claude can then call tools like `get_system_topology`, `inspect_process`, `list_anomalies`, and `execute_action`.

### SSE transport

For remote or multi-client access, start the MCP SSE server:

```bash
aether-terminal --mcp-sse 3000
```

The SSE endpoint is available at `http://localhost:3000`. Connect any MCP-compatible client to this URL.

### Available MCP tools

| Tool | Description |
|------|-------------|
| `get_system_topology` | Full process graph with connections and summary |
| `inspect_process` | Deep dive on a specific PID with recommendations |
| `list_anomalies` | Processes with low HP, zombies, high CPU |
| `get_network_flows` | Active connections with protocol and throughput |
| `execute_action` | Submit an action to the Arbiter approval queue |

All actions go through the human-in-the-loop Arbiter — nothing executes without approval.

## Kubernetes

> Helm chart is planned for a future release. For now, deploy via a raw manifest or build a custom image.

Example Kubernetes deployment:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: aether-terminal
spec:
  replicas: 1
  selector:
    matchLabels:
      app: aether-terminal
  template:
    metadata:
      labels:
        app: aether-terminal
    spec:
      containers:
        - name: aether
          image: ghcr.io/aquamarineos/aether-terminal:latest
          args:
            - "--config"
            - "/etc/aether/aether.yaml"
            - "--web"
            - "--metrics"
            - "--no-3d"
          ports:
            - containerPort: 8080
              name: web
            - containerPort: 9090
              name: metrics
            - containerPort: 50051
              name: grpc
          volumeMounts:
            - name: config
              mountPath: /etc/aether
      volumes:
        - name: config
          configMap:
            name: aether-config
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: aether-config
data:
  aether.yaml: |
    discovery:
      enabled: true
      kubernetes: true
      kubernetes_namespace: default
      interval_seconds: 30
    thresholds:
      error_rate_percent: 5.0
      latency_p99_ms: 500.0
    output:
      slack:
        webhook_url: ${SLACK_WEBHOOK_URL}
        severity: warning
    api:
      grpc:
        enabled: true
        bind: "0.0.0.0:50051"
---
apiVersion: v1
kind: Service
metadata:
  name: aether-terminal
spec:
  selector:
    app: aether-terminal
  ports:
    - port: 8080
      name: web
    - port: 9090
      name: metrics
    - port: 50051
      name: grpc
```

Build with Kubernetes discovery support:

```bash
cargo build --release --features kubernetes
```

## Next steps

- [configuration.md](configuration.md) — full config reference
- [integrations.md](integrations.md) — Slack, Discord, Telegram, Prometheus, gRPC setup
- [architecture.md](architecture.md) — system design and crate responsibilities

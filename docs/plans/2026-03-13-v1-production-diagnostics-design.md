# Aether-Terminal v1.0 — Production Diagnostics Platform

> **Status**: Approved design
> **Date**: 2026-03-13
> **Phase**: Pre-implementation (refactoring → v1.0 → v1.1)

## 1. Позиционирование

Aether-Terminal — production diagnostics platform с тремя уровнями интеллекта:

- **Level 1 (Rules)** — детерминистические правила, всегда включены
- **Level 2 (ML)** — ONNX inference, опционально (`--predict`)
- **Level 3 (AI Agent)** — MCP-агенты, опционально (`--mcp-sse`)

Ключевое отличие от btop/htop/glances: не просто показывает метрики, а **диагностирует проблемы** — throughput drop, connection pool exhaustion, latency growth — и рекомендует/выполняет действия с одобрения человека (Arbiter pattern).

### Роль в экосистеме

Aether-Terminal — observer-слой (глаза и нервная система) для self-healing платформы из 5 проектов:

1. **Aether-Terminal** — наблюдение, диагностика, метрики, event bus
2. **K8s Autoscaler** — автоскейлинг ML inference
3. **Service Graph** — граф зависимостей сервисов, topology, трафик
4. **Auto-Fix Agent** — автоматический debug, patch, self-heal
5. **Custom Orchestrator** — собственный container runtime (замена K8s)

Aether предоставляет API и Event Bus, на которые садятся остальные 4 проекта.

### Целевая аудитория

- DevOps/SRE — production monitoring
- Разработчики — debug своих приложений
- AI/MCP энтузиасты — agent-driven infrastructure
- Rust community — portfolio/educational (eBPF, JIT, 3D рендер, компилятор)

## 2. Текущее состояние (MS1–MS13)

12 crates, 13 milestones, ~20K LOC Rust + React frontend:

| Crate | Назначение | Статус |
|-------|-----------|--------|
| aether-core | Domain types, WorldGraph, events, traits | Done |
| aether-ingestion | sysinfo + eBPF bridge, pipeline | Done |
| aether-render | TUI + 3D software rasterizer | Done |
| aether-mcp | MCP server (stdio + SSE) | Done |
| aether-gamification | HP, XP, achievements, SQLite | Done |
| aether-ebpf | eBPF loader, ring buffer, probes | Done |
| aether-predict | ONNX inference, anomaly detection | Done |
| aether-script | DSL lexer/parser, Cranelift JIT | Done |
| aether-analyze | Deterministic diagnostics (30+ rules) | Done |
| aether-metrics | Prometheus exporter + consumer | Done |
| aether-web | React SPA + axum backend | Done |
| aether-terminal | CLI orchestrator | Done |

## 3. Фаза 0: Рефакторинг

Перед добавлением новых crate-ов — аудит и стабилизация AI-сгенерированного кода.

### 3.1 Code Audit

- `cargo clippy --workspace -- -D warnings` — пройти без warnings
- `cargo test --workspace` — все тесты зелёные
- `cargo fmt --check` — форматирование
- Удалить dead code, unused imports, закомментированный код
- Проверить `.unwrap()` в production code — заменить на `?` или `.expect()`
- Проверить error types — каждый crate имеет свой Error enum

### 3.2 API стабилизация

Подготовка к тому, что crates станут библиотеками:

- Ревью `pub` API каждого crate — что должно быть public, что `pub(crate)`
- Убедиться что `lib.rs` содержит только `pub mod` + `pub use` re-exports
- Проверить что traits в aether-core покрывают все нужные абстракции
- Документация (`///`) на всех public items

### 3.3 Архитектурные улучшения

- Унифицировать error handling (все crates → thiserror)
- Проверить channel architecture — нет ли bottleneck-ов или race conditions
- SharedState pattern — единообразный подход во всех crates
- Config validation — подготовка к aether-config

### 3.4 Тестирование

- Покрытие: минимум unit tests для каждого public method
- Integration tests для критических path-ов (ingestion → graph → render)
- Убрать flaky tests (если есть таймауты — увеличить или мокать)

## 4. Фаза 1: v1.0 — Production Observer

### 4.1 Новые crates

#### aether-config

Единый конфиг для всего Aether. Поддержка TOML и YAML (auto-detect по расширению).

```
Ответственность:
- Парсинг aether.toml / aether.yaml
- Валидация конфигурации
- Дефолтные значения
- Env variable interpolation (${VAR_NAME})
- Merge: CLI flags > env vars > config file > defaults
```

Структура конфига:
- `discovery` — auto-discovery настройки (порты, K8s, namespace)
- `targets[]` — явно заданные сервисы (name, prometheus, health, probe_tcp)
- `thresholds` — пороги для правил (throughput_drop_percent, latency_p99_ms, etc.)
- `output` — куда слать алерты (slack, discord, telegram, stdout, file, syslog)
- `api` — gRPC и event bus настройки

Зависимости: serde, toml, serde_yaml, thiserror

#### aether-discovery

Auto-discovery сервисов в инфраструктуре.

```
Ответственность:
- Port scanning (configurable port list)
- /metrics endpoint probing (OpenMetrics detection)
- Known service pattern matching:
  - PostgreSQL (5432) → pg_exporter metrics
  - Redis (6379) → redis_exporter / INFO command
  - nginx (80/443) → stub_status / nginx_exporter
  - Generic HTTP (8080/9090) → /metrics, /health, /healthz
- Kubernetes API integration:
  - List pods by namespace + label selector
  - Extract service endpoints
  - Watch for pod changes (add/remove targets dynamically)
- Output: Vec<DiscoveredTarget> → feeds into MetricStore
```

Зависимости: aether-core, tokio (net), kube (K8s client, feature-gated)

#### aether-prober

Активные проверки здоровья сервисов.

```
Ответственность:
- HTTP health checks (GET /health, /healthz, custom path)
  - Status code, response time, body validation
- TCP connect latency (connect + measure RTT)
- DNS resolution time
- TLS certificate inspection:
  - Expiry date, issuer, SAN validation
  - Warning при <30 дней до expiry
- Configurable intervals per target (default 30s)
- Output: ProbeResult { target, check_type, status, latency, details }
```

Зависимости: aether-core, tokio, reqwest (HTTP), rustls (TLS inspection)

#### aether-output

Output pipeline для диагностик.

```
Ответственность:
- Webhook интеграции (встроенные):
  - Slack (Incoming Webhook, Block Kit formatting)
  - Discord (Webhook API, embed formatting)
  - Telegram (Bot API, Markdown formatting)
- Generic output:
  - stdout (JSON lines)
  - File (append, JSON lines, rotation by size)
  - Syslog (RFC 5424)
- Template engine для сообщений:
  - Severity-based routing (critical → PagerDuty, warning → Slack, info → file)
  - Configurable message format per output
- Rate limiting:
  - Deduplication window (don't spam same diagnostic)
  - Max alerts per minute per channel
```

Зависимости: aether-core, reqwest (webhooks), tokio

#### aether-api

Integration API для внешних сервисов (будущие проекты экосистемы).

```
Ответственность:
- gRPC server (tonic):
  - GetDiagnostics — текущие диагнозы
  - GetMetrics — метрики по target
  - GetTargets — discovered targets
  - StreamEvents — server-streaming диагностических событий
  - ExecuteAction — предложить действие через Arbiter
- Event bus:
  - broadcast channel для diagnostic events
  - Подписка по severity, target, category
  - Event types: DiagnosticCreated, DiagnosticResolved, ActionProposed, ActionExecuted
- Auth (v1.1):
  - API key validation
  - mTLS для gRPC
```

Зависимости: aether-core, tonic, prost, tokio

### 4.2 Расширения существующих crates

#### aether-analyze — Application-Level Rules

Новые правила поверх данных из Prometheus scrape и probers:

| Rule | Источник | Порог (default) |
|------|----------|----------------|
| throughput_drop | prometheus (http_requests_total rate) | >30% drop за 5 мин |
| latency_p99_growth | prometheus (histogram percentile) | >500ms или +100% |
| connection_pool_saturation | prometheus (active_connections / max) | >80% |
| error_rate_spike | prometheus (http_5xx_total rate) | >5% от total |
| disk_io_saturation | prometheus (node_disk_io_time) | >90% utilization |
| memory_leak_trend | prometheus + sysinfo | linear growth >10%/hour |
| health_check_failure | prober (HTTP status != 2xx) | any failure |
| tcp_latency_degradation | prober (TCP RTT) | >3x baseline |
| tls_expiry | prober (certificate) | <30 days |
| dns_resolution_slow | prober (DNS) | >100ms |
| service_dependency_timeout | prober + prometheus | error rate correlates with upstream latency |

Cross-source correlation:
- Log errors ↔ metric spikes (v1.1, после aether-logparser)
- Probe failures ↔ resource exhaustion
- Throughput drop ↔ connection pool saturation (causal chain detection)

#### aether-metrics — Active Prometheus Scrape

Расширить существующий PromQL consumer:

- Active scraping /metrics endpoints (pull model)
- Configurable scrape interval per target (default 15s)
- Metric type detection (counter, gauge, histogram, summary)
- Label extraction и mapping к Target identity
- MetricStore integration — все scraped метрики → unified storage

#### aether-core — Новые типы

```
Target { id, name, kind, endpoints, labels, discovered_at }
TargetKind { Process, Service, Container, Pod }
ServiceHealth { target_id, status, last_check, probe_results }
ProbeResult { target, check_type, status, latency_ms, details, timestamp }
CheckType { HttpHealth, TcpConnect, DnsResolve, TlsCertificate }
ProbeStatus { Healthy, Degraded, Failed }

ConfigEvent { ConfigLoaded, ConfigReloaded, TargetAdded, TargetRemoved }
IntegrationEvent { DiagnosticCreated, DiagnosticResolved, ActionProposed }
```

### 4.3 Инфраструктура

#### CI/CD (GitHub Actions)

```
Workflows:
- ci.yaml: cargo check + clippy + fmt + test (on every push/PR)
- release.yaml: build binaries (Linux x86_64/aarch64, macOS) + Docker image
- security.yaml: cargo audit (weekly)
```

#### Packaging

- `cargo install aether-terminal` — из crates.io
- Docker image: `ghcr.io/arman-ramazan/aether-terminal:latest`
- Helm chart (basic): deploy на K8s с ConfigMap для aether.yaml
- Release binaries: GitHub Releases (tar.gz + sha256)

#### Documentation

- README: обновить с GIF/screenshots, Getting Started, Quick Start
- docs/getting-started.md: пошаговый гайд
- docs/configuration.md: полный reference конфига
- docs/integrations.md: Slack, Discord, Telegram, MCP setup
- docs/api.md: gRPC API reference

## 5. Фаза 2: v1.1 — Deep Diagnostics

Scope (после стабильного v1.0):

- **aether-logparser** — regex + known formats (nginx, PostgreSQL, Redis, systemd)
- **Cross-source correlation** — log errors ↔ metric spikes (causal chain)
- **PagerDuty/OpsGenie** — output интеграции
- **Syslog + generic pipe** — Unix-way output
- **Docker Compose discovery** — docker.sock integration
- **AI agent runbooks** — MCP цепочки действий (multi-step remediation)
- **Plugin system** — user extensions (custom rules, custom outputs, custom probers)
- **aether --demo mode** — фейковые данные для showcase
- **brew install** formula
- **Grafana dashboard templates** — JSON dashboards для scraped метрик

## 6. Порядок реализации v1.0

### Этап 0: Рефакторинг (перед новыми фичами)

1. Code audit (clippy, tests, dead code)
2. Public API review (pub vs pub(crate))
3. Error handling unification
4. Documentation pass (/// на всех pub items)

### Этап 1: Фундамент

5. aether-config — TOML/YAML парсинг, структуры, валидация
6. aether-core — новые типы (Target, ServiceHealth, ProbeResult, events)
7. CLI интеграция — `--config <path>` flag, merge с существующими CLI args

### Этап 2: Data Sources

8. aether-discovery — port scanner + known patterns
9. aether-discovery — K8s API integration (feature-gated)
10. aether-prober — HTTP health checks
11. aether-prober — TCP latency + DNS + TLS
12. aether-metrics — active Prometheus scrape (расширение)

### Этап 3: Intelligence

13. aether-analyze — application-level rules (throughput, latency, pools)
14. aether-analyze — cross-source correlation (probe + metrics)
15. Интеграция: discovery → scrape/probe → analyze → diagnostics

### Этап 4: Output

16. aether-output — Slack webhook
17. aether-output — Discord + Telegram webhooks
18. aether-output — stdout JSON + file output
19. aether-output — severity routing + deduplication

### Этап 5: Integration API

20. aether-api — gRPC server scaffold (tonic)
21. aether-api — GetDiagnostics + StreamEvents
22. aether-api — Event bus (broadcast)

### Этап 6: Packaging & Docs

23. CI/CD — GitHub Actions (build, test, clippy, release)
24. Docker image + Helm chart
25. README + Getting Started + Configuration docs
26. cargo publish preparation

## 7. Dependency Graph (новые crates)

```
aether-terminal (bin)
  +-- aether-config       -> serde, toml, serde_yaml
  +-- aether-discovery    -> aether-core, tokio, kube (optional)
  +-- aether-prober       -> aether-core, tokio, reqwest, rustls
  +-- aether-output       -> aether-core, reqwest, tokio
  +-- aether-api          -> aether-core, tonic, prost, tokio
  +-- (existing 11 crates)
```

Правило сохраняется: все crates зависят ТОЛЬКО от aether-core. Никогда друг от друга.

## 8. Конфиг — полный формат

### TOML (aether.toml)

```toml
[discovery]
enabled = true
scan_ports = [5432, 6379, 8080, 9090, 9200, 27017]
kubernetes = false
# kubernetes_namespace = "production"
# kubernetes_label_selector = "app.kubernetes.io/part-of=myapp"
interval_seconds = 60

[[target]]
name = "api-server"
kind = "service"
prometheus = "http://localhost:9090/metrics"
health = "http://localhost:8080/health"
labels = { team = "backend", env = "production" }

[[target]]
name = "postgresql"
kind = "service"
prometheus = "http://localhost:9187/metrics"
probe_tcp = "localhost:5432"

[[target]]
name = "redis"
kind = "service"
probe_tcp = "localhost:6379"

[thresholds]
throughput_drop_percent = 30
latency_p99_ms = 500
connection_pool_percent = 80
error_rate_percent = 5
tls_expiry_days = 30
health_check_timeout_ms = 5000

[output.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
severity = "warning"
# template = "custom"

[output.discord]
webhook_url = "${DISCORD_WEBHOOK_URL}"
severity = "critical"

[output.telegram]
bot_token = "${TELEGRAM_BOT_TOKEN}"
chat_id = "${TELEGRAM_CHAT_ID}"
severity = "warning"

[output.stdout]
enabled = true
format = "json"
severity = "info"

[output.file]
path = "/var/log/aether/diagnostics.jsonl"
severity = "info"
max_size_mb = 100

[api.grpc]
enabled = true
bind = "0.0.0.0:50051"

[api.events]
broadcast = true

[scrape]
interval_seconds = 15
timeout_seconds = 5

[probe]
interval_seconds = 30
timeout_seconds = 5
```

### YAML (aether.yaml) — идентичная структура

Используется для K8s deployments. Формат auto-detect по расширению файла.

## 9. Метрики успеха

### v1.0 Release Criteria

- [ ] Все существующие тесты проходят
- [ ] cargo clippy --workspace — 0 warnings
- [ ] Config loading (TOML + YAML) работает
- [ ] Auto-discovery находит минимум 3 типа сервисов
- [ ] Prometheus scrape собирает метрики
- [ ] HTTP/TCP probes работают
- [ ] 5+ application-level diagnostic rules работают
- [ ] Slack webhook отправляет алерты
- [ ] gRPC API отвечает на GetDiagnostics
- [ ] Docker image собирается и работает
- [ ] CI pipeline зелёный
- [ ] README с Getting Started guide

### Open Source Readiness

- [ ] LICENSE file (MIT)
- [ ] CONTRIBUTING.md
- [ ] Issue templates
- [ ] PR template
- [ ] Code of Conduct
- [ ] Semantic versioning
- [ ] CHANGELOG.md

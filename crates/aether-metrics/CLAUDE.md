# aether-metrics

## Purpose
Prometheus-compatible metrics subsystem. Exports metrics via `/metrics` endpoint, consumes PromQL from Prometheus servers, and actively scrapes individual service `/metrics` endpoints.

## Modules
- `error.rs` — MetricsError enum (thiserror)
- `exporter/registry.rs` — MetricRegistry, MetricType, MetricDesc, MetricFamily, LabelSet
- `exporter/encode.rs` — OpenMetrics text format encoder
- `exporter/server.rs` — axum HTTP server for `/metrics` endpoint
- `consumer/` — PromQL consumer (query Prometheus server API)
- `scrape/parser.rs` — Prometheus text exposition format parser (type-aware: counter, gauge, histogram, summary)
- `scrape/scraper.rs` — PrometheusScraper: HTTP scraper that fetches targets, implements DataSource trait

## Rules
- Depends ONLY on aether-core (hexagonal architecture)
- Labels in output must be sorted alphabetically (BTreeMap)
- Registry is designed for single-writer usage (no internal locking)
- No .unwrap() in production code

## Testing
```bash
cargo test -p aether-metrics
```

## Key Dependencies
- aether-core (path dependency)
- axum (HTTP server for /metrics endpoint)
- tokio (async runtime)
- thiserror (error types)
- tracing (instrumentation)
- serde, serde_json (serialization)
- reqwest (PromQL consumer)

# aether-metrics

## Purpose
Prometheus-compatible metrics exporter. Provides a MetricRegistry for collecting gauges, counters, and histograms, and encodes them in OpenMetrics text exposition format for scraping via `/metrics` endpoint.

## Modules
- `error.rs` — MetricsError enum (thiserror)
- `exporter/registry.rs` — MetricRegistry, MetricType, MetricDesc, MetricFamily, LabelSet
- `exporter/encode.rs` — OpenMetrics text format encoder

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

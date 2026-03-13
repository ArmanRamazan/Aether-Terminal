# aether-prober

## Purpose
Network prober — performs HTTP health checks and TCP connectivity probes against discovered targets. Produces `CollectedMetric` values (latency, status) for ingestion into the diagnostic pipeline.

## Modules
- `engine.rs` — ProberEngine (implements DataSource trait)
- `error.rs` — ProberError enum (thiserror)

## Data Flow
```
Targets (Arc<RwLock<Vec<Target>>>) → ProberEngine.collect() → Vec<CollectedMetric>
                                                            ↓ (wired in main.rs)
                                                      Vec<TimeSeries> → MetricStore → AnalyzeEngine
```

## Rules
- Depends ONLY on aether-core (hexagonal architecture)
- Implements `DataSource` trait from `aether-core::traits`
- No `.unwrap()` in production code
- Timeout is configurable per-engine, not per-target

## Testing
```bash
cargo test -p aether-prober
```

## Key Dependencies
- aether-core (path dependency)
- async-trait (for DataSource impl)
- reqwest (HTTP health checks)
- tokio (TCP connect, async runtime)
- thiserror (error types)
- tracing (instrumentation)

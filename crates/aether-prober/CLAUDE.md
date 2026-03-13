# aether-prober

## Purpose
Network prober — performs HTTP health checks, TCP connectivity probes, DNS resolution checks, and TLS certificate validation against discovered targets. Produces `CollectedMetric` values (latency, status) for ingestion into the diagnostic pipeline.

## Modules
- `http.rs` — HttpProber (HTTP GET health checks, returns ProbeResult)
- `tcp.rs` — TcpProber (TCP connect with timeout, returns ProbeResult)
- `dns.rs` — DnsProber (DNS resolution via tokio::net::lookup_host, returns ProbeResult)
- `tls.rs` — TlsProber (TLS handshake via reqwest HTTPS HEAD, returns ProbeResult)
- `engine.rs` — ProberEngine (orchestrates all probers, implements DataSource trait)
- `error.rs` — ProberError enum (thiserror)

## Data Flow
```
Targets (Arc<RwLock<Vec<Target>>>) → ProberEngine.collect() → Vec<CollectedMetric>
                                                            ↓ (wired in main.rs)
                                                      Vec<TimeSeries> → MetricStore → AnalyzeEngine
```

Individual probers return `ProbeResult` (from aether-core), engine converts to `CollectedMetric` via `probe_result_to_metrics()`.

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
- reqwest (HTTP health checks, TLS validation)
- tokio (TCP connect, DNS resolution, async runtime)
- thiserror (error types)
- tracing (instrumentation)

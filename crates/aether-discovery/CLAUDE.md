# aether-discovery

## Purpose
Auto-discovery crate. Scans ports, probes /metrics endpoints, matches known service patterns. Implements `ServiceDiscovery` trait from aether-core.

## Modules
- `scanner.rs` — PortScanner: concurrent TCP port scanning with known service matching
- `probe.rs` — MetricsProbe: HTTP probe for Prometheus/OpenMetrics endpoints
- `engine.rs` — DiscoveryEngine: orchestrates scanner + probe, implements ServiceDiscovery trait
- `error.rs` — DiscoveryError enum

## Key Types
- `PortScanner` — scans TCP ports with configurable timeout, maps to service hints
- `OpenPort` — discovered open port with optional service name hint
- `MetricsProbe` — HTTP GET to /metrics, validates Prometheus text format
- `MetricsEndpoint` — confirmed metrics URL with metric count
- `DiscoveryEngine` — combines scanner + probe + known patterns into Target list

## Strict Rules
- Depends ONLY on aether-core (hexagonal architecture)
- `pub(crate)` by default, `pub` only for cross-crate API
- No `.unwrap()` in production code
- All public types: Debug, Clone
- Implements ServiceDiscovery trait from core

## Testing
```bash
cargo test -p aether-discovery
```
- Known port mapping tests
- Localhost scan tests
- Engine construction tests

## Key Dependencies
- tokio (net, time) for async TCP/HTTP
- thiserror (errors)
- tracing (instrumentation)
- async-trait (ServiceDiscovery impl)

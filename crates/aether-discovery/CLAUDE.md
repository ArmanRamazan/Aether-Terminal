# aether-discovery

## Purpose
Auto-discovery crate. Scans ports, probes /metrics endpoints, matches known service patterns. Implements `ServiceDiscovery` trait from aether-core.

## Modules
- `scanner.rs` — PortScanner: concurrent TCP port scanning with known service matching
- `probe.rs` — MetricsProbe: HTTP probe for Prometheus/OpenMetrics endpoints
- `engine.rs` — DiscoveryEngine: orchestrates scanner + probe, implements ServiceDiscovery trait
- `kubernetes.rs` — KubernetesDiscovery: K8s API pod/service discovery (feature-gated: `kubernetes`)
- `error.rs` — DiscoveryError enum

## Key Types
- `PortScanner` — scans TCP ports with configurable timeout, maps to service hints
- `OpenPort` — discovered open port with optional service name hint
- `MetricsProbe` — HTTP GET to /metrics, validates Prometheus text format
- `MetricsEndpoint` — confirmed metrics URL with metric count
- `DiscoveryEngine` — combines scanner + probe + known patterns into Target list
- `KubernetesDiscovery` — discovers pods and services via kube API (requires `kubernetes` feature)

## Strict Rules
- Depends ONLY on aether-core (hexagonal architecture)
- `pub(crate)` by default, `pub` only for cross-crate API
- No `.unwrap()` in production code
- All public types: Debug, Clone
- Implements ServiceDiscovery trait from core

## Features
- `kubernetes` — enables K8s API discovery via `kube` + `k8s-openapi` crates

## Testing
```bash
cargo test -p aether-discovery                       # without K8s
cargo test -p aether-discovery --features kubernetes  # with K8s
```
- Known port mapping tests
- Localhost scan tests
- Engine construction tests
- K8s target construction and endpoint extraction tests (feature-gated)

## Key Dependencies
- tokio (net, time) for async TCP/HTTP
- thiserror (errors)
- tracing (instrumentation)
- async-trait (ServiceDiscovery impl)
- kube, k8s-openapi (optional, `kubernetes` feature)

# aether-api

## Purpose
gRPC server for machine-to-machine integration. Exposes diagnostics, targets, event streaming, and action execution to external Aether ecosystem projects (K8s Autoscaler, Service Graph, Auto-Fix Agent).

Uses tonic (pure-Rust gRPC) and prost (protobuf codegen).

## Modules
- `proto/aether.proto` — Protobuf service and message definitions
- `build.rs` — tonic-build codegen for proto
- `server.rs` — AetherGrpcServer implementing AetherService trait
- `error.rs` — ApiError enum
- `lib.rs` — re-exports, proto module inclusion

## RPCs
- `GetDiagnostics` — current active diagnostics with severity/target filters
- `GetTargets` — discovered monitoring targets
- `StreamEvents` — server-streaming of IntegrationEvents from EventBus
- `ExecuteAction` — propose action through Arbiter (human-in-the-loop)

## Shared State Pattern
```rust
Arc<Mutex<Vec<Diagnostic>>>   // from aether-core
Arc<Mutex<Vec<Target>>>       // from aether-core
Arc<E: EventBus>              // from aether-core::event_bus
Arc<Mutex<ArbiterQueue>>      // from aether-core
```

## Strict Rules
- Depends ONLY on aether-core — hexagonal architecture
- All lock acquisitions return Status::internal on poison — never panic
- ExecuteAction always goes through ArbiterQueue — no direct execution
- Proto changes require backward-compatible additions only
- `pub(crate)` by default, `pub` only for cross-crate API

## Testing
```bash
cargo test -p aether-api
```

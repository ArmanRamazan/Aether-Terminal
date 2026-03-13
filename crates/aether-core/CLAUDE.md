# aether-core

## Purpose
Central library crate. Defines ALL shared types, traits (hexagonal ports), the WorldGraph, event types, and event bus. Every other crate depends on this. This crate has ZERO dependencies on other aether crates.

**This is the most critical crate.** Changes here affect ALL other crates. Be extra careful with public API changes.

## Modules
- `models.rs` — ProcessNode, NetworkEdge, ProcessState, Protocol, SystemSnapshot, Target, TargetKind, Endpoint, ServiceHealth, ProbeResult, MetricSample
- `graph.rs` — WorldGraph (petgraph::StableGraph wrapper with HashMap<pid, NodeIndex> index)
- `events.rs` — SystemEvent, GameEvent, AgentAction, IntegrationEvent enums
- `event_bus.rs` — EventBus trait, InProcessEventBus implementation
- `traits.rs` — SystemProbe, Storage, DataSource, OutputSink, ServiceDiscovery trait definitions
- `arbiter.rs` — ArbiterQueue (single canonical implementation, shared by TUI/Web/MCP)
- `metrics.rs` — TimeSeries, HostId
- `error.rs` — CoreError enum

## Strict Rules
- **ZERO** dependencies on other aether-* crates — this is inviolable
- NO async runtime dependency in models/graph (keep them sync-safe)
- tokio only in traits.rs and event_bus.rs (for async trait methods and channels)
- All public types MUST derive: Debug, Clone. Most also: Serialize, Deserialize
- All public enums MUST have `#[non_exhaustive]` — semver safety
- `pub(crate)` by default. `pub` only for types used by other crates
- `///` doc-comment on EVERY pub item — no exceptions
- `lib.rs` — ONLY `pub mod` + `pub use`. Zero logic.
- WorldGraph methods must be O(1) for pid lookups (use internal HashMap)
- StableGraph (not Graph) — indices must survive node removal
- ArbiterQueue — ONE implementation here. No duplicates in other crates.
- `format!("{:?}")` FORBIDDEN for any value that crosses crate boundaries — use Display or Serialize
- NO `.unwrap()` or `.expect()` in any code (this crate has no tests-only exception for production paths)

## Trait Contracts (for Phase 2 adapter crates)
```rust
trait DataSource: Send + Sync       // Prometheus scraper, prober, log parser
trait OutputSink: Send + Sync       // Slack, Discord, Telegram, stdout, file
trait ServiceDiscovery: Send + Sync // port scan, K8s API
trait EventBus: Send + Sync         // in-process broadcast, gRPC streaming
trait SystemProbe: Send + Sync      // sysinfo, eBPF
trait Storage: Send + Sync          // SQLite, future backends
```

## Testing
```bash
cargo test -p aether-core
```
- Every public method on WorldGraph must have a unit test
- Every trait must have an object-safety compile test: `let _: Box<dyn TraitName>;`
- Serialization roundtrip tests for all public types

## Key Dependencies
- petgraph (StableGraph)
- glam (Vec3 for 3D positions)
- serde + serde_json (serialization)
- tokio (broadcast channel in event_bus, async traits)
- async-trait
- thiserror

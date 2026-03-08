# aether-core

## Purpose
Central library crate. Defines ALL shared types, traits (hexagonal ports), the WorldGraph, and event types. Every other crate depends on this. This crate has ZERO dependencies on other aether crates.

## Modules
- `models.rs` — ProcessNode, NetworkEdge, ProcessState, Protocol, ConnectionState, SystemSnapshot
- `graph.rs` — WorldGraph (petgraph::StableGraph wrapper with HashMap<pid, NodeIndex> index)
- `events.rs` — SystemEvent, GameEvent, AgentAction enums
- `traits.rs` — SystemProbe, Storage trait definitions
- `lib.rs` — re-exports all public types

## Rules
- NO async runtime dependency in models/graph (keep them sync-safe)
- tokio only in traits.rs (for async trait methods)
- All public types must derive: Debug, Clone. Most also: Serialize, Deserialize
- WorldGraph methods must be O(1) for pid lookups (use internal HashMap)
- StableGraph (not Graph) — indices must survive node removal
- NEVER add dependencies on other aether-* crates

## Testing
```bash
cargo test -p aether-core
```
Every public method on WorldGraph must have a unit test.

## Key Dependencies
- petgraph (StableGraph)
- glam (Vec3 for 3D positions)
- serde + serde_json (serialization)
- tokio (broadcast channel in traits)

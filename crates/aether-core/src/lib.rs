//! Core types, traits, graph structures, and event definitions for Aether Terminal.
//!
//! All other aether crates depend on this crate. It defines the shared vocabulary:
//! process models, the world graph, system events, and hexagonal port traits.

pub mod error;
pub mod events;
pub mod graph;
pub mod models;
pub mod traits;

// Re-export key types for convenience.
pub use events::{AgentAction, GameEvent, SystemEvent};
pub use graph::WorldGraph;
pub use models::{
    ConnectionState, NetworkEdge, ProcessNode, ProcessState, Protocol, SystemSnapshot,
};
pub use error::CoreError;
pub use traits::{GameSession, Ranking, Storage, SystemProbe};

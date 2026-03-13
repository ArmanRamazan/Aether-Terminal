//! Core types, traits, graph structures, and event definitions for Aether Terminal.
//!
//! All other aether crates depend on this crate. It defines the shared vocabulary:
//! process models, the world graph, system events, and hexagonal port traits.

pub mod arbiter;
pub mod error;
pub mod events;
pub mod graph;
pub mod metrics;
pub mod models;
pub mod traits;

// Re-export key types for convenience.
pub use arbiter::{ArbiterQueue, PendingAction};
pub use error::CoreError;
pub use events::{AgentAction, GameEvent, SystemEvent};
pub use graph::WorldGraph;
pub use metrics::{HostId, MetricSample, TimeSeries};
pub use models::{
    ConnectionState, DiagCategory, DiagTarget, Diagnostic, Evidence, NetworkEdge, ProcessNode,
    ProcessState, Protocol, Recommendation, RecommendedAction, Severity, SystemSnapshot, Urgency,
};
pub use traits::{GameSession, Ranking, Storage, SystemProbe};

//! gRPC API server for Aether Terminal.
//!
//! Exposes diagnostics, monitoring targets, event streaming, and action execution
//! via gRPC (tonic). Designed for machine-to-machine integration with the
//! broader Aether ecosystem (K8s Autoscaler, Service Graph, Auto-Fix Agent).

pub mod error;
pub mod server;

/// Generated protobuf types and service definitions.
pub mod proto {
    tonic::include_proto!("aether.v1");
}

pub use server::AetherGrpcServer;

//! Predictive AI engine for anomaly detection and resource forecasting.
//!
//! Runs ONNX models via tract (pure-Rust) on sliding windows of process metrics.
//! Feature-gated behind `predict` — compiles as no-op without it.

// Crate internals are staged; integration is feature-gated behind `predict`.
#![allow(dead_code)]

pub mod engine;
pub(crate) mod error;
pub(crate) mod features;
#[cfg(feature = "predict")]
pub(crate) mod inference;
pub mod models;
pub(crate) mod window;

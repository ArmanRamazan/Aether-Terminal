//! Deterministic diagnostic engine with layered analysis for Aether Terminal.

pub(crate) mod analyzers;
pub(crate) mod collectors;
pub mod engine;
pub(crate) mod error;
pub(crate) mod recommendations;
pub(crate) mod rules;
pub(crate) mod store;

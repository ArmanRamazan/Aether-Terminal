//! Deterministic diagnostic engine with layered analysis for Aether Terminal.

// Many analyzers and collectors are implemented but not yet wired into the main
// pipeline. Allow dead code until Phase 2 integration.
#![allow(dead_code)]

pub(crate) mod analyzers;
pub(crate) mod collectors;
pub mod engine;
pub(crate) mod error;
pub(crate) mod recommendations;
pub(crate) mod rules;
pub(crate) mod store;

//! RPG mechanics layer: HP for processes, XP/ranks for users, achievements.
//!
//! Calculates health points based on process metrics, tracks experience and ranks,
//! manages achievement unlocks, and persists state to SQLite.

// Crate is implemented but not yet integrated into the main pipeline (Phase 1).
#![allow(dead_code)]

pub(crate) mod achievements;
pub(crate) mod error;
pub(crate) mod hp;
pub(crate) mod storage;
pub(crate) mod xp;

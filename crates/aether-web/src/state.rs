use std::sync::{Arc, Mutex, RwLock};

use aether_core::models::Diagnostic;
use aether_core::{ArbiterQueue, WorldGraph};

/// Shared application state passed to axum handlers.
///
/// All fields are `Arc`-wrapped, so cloning is cheap.
#[derive(Clone)]
pub struct SharedState {
    pub(crate) world: Arc<RwLock<WorldGraph>>,
    pub(crate) arbiter: Arc<Mutex<ArbiterQueue>>,
    pub(crate) diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
}

impl SharedState {
    /// Create shared state from pre-existing Arc handles.
    pub fn new(
        world: Arc<RwLock<WorldGraph>>,
        arbiter: Arc<Mutex<ArbiterQueue>>,
        diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    ) -> Self {
        Self {
            world,
            arbiter,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_state_clone_is_same_arc() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let state = SharedState::new(
            Arc::clone(&world),
            Arc::clone(&arbiter),
            Arc::clone(&diagnostics),
        );

        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.world, &cloned.world), "clone shares same world Arc");
        assert!(Arc::ptr_eq(&state.arbiter, &cloned.arbiter), "clone shares same arbiter Arc");
        assert!(
            Arc::ptr_eq(&state.diagnostics, &cloned.diagnostics),
            "clone shares same diagnostics Arc"
        );
    }
}

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use aether_core::{ArbiterQueue, WorldGraph};

/// Shared application state passed to axum handlers.
///
/// All fields are `Arc`-wrapped, so cloning is cheap.
#[derive(Clone)]
#[allow(dead_code)] // Fields used by axum handlers (added in subsequent tasks)
pub struct SharedState {
    pub(crate) world: Arc<RwLock<WorldGraph>>,
    pub(crate) arbiter: Arc<Mutex<ArbiterQueue>>,
}

impl SharedState {
    /// Create shared state from pre-existing Arc handles.
    pub fn new(world: Arc<RwLock<WorldGraph>>, arbiter: Arc<Mutex<ArbiterQueue>>) -> Self {
        Self { world, arbiter }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_state_clone_is_same_arc() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let arbiter = Arc::new(Mutex::new(ArbiterQueue::default()));
        let state = SharedState::new(Arc::clone(&world), Arc::clone(&arbiter));

        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.world, &cloned.world), "clone shares same world Arc");
        assert!(Arc::ptr_eq(&state.arbiter, &cloned.arbiter), "clone shares same arbiter Arc");
    }
}

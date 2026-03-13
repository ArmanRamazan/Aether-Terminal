//! Shared arbiter queue for human-in-the-loop action approval.
//!
//! AI agents submit actions via MCP; each gets a UUID and enters the pending
//! queue. A human approves or denies in the TUI or Web UI before execution.

use std::time::Instant;

use crate::AgentAction;

/// A single action awaiting human approval.
#[derive(Debug, Clone)]
pub struct PendingAction {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// The requested agent action.
    pub action: AgentAction,
    /// Target process ID (0 for non-pid actions).
    pub pid: u32,
    /// When the action was submitted.
    pub requested_at: Instant,
    /// Source agent identifier (e.g., "mcp-agent", "RuleEngine").
    pub source: String,
}

/// Shared queue of agent actions awaiting approval.
///
/// Designed to be wrapped in `Arc<Mutex<ArbiterQueue>>` and shared
/// between MCP server, TUI arbiter tab, and Web UI.
#[derive(Debug, Default)]
pub struct ArbiterQueue {
    pending: Vec<PendingAction>,
    history: Vec<(PendingAction, bool)>,
}

impl ArbiterQueue {
    /// Submit an action for approval. Returns the assigned action ID.
    pub fn enqueue(&mut self, action: AgentAction, pid: u32, source: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.pending.push(PendingAction {
            id: id.clone(),
            action,
            pid,
            requested_at: Instant::now(),
            source: source.to_owned(),
        });
        id
    }

    /// Approve a pending action by ID. Removes it from pending, adds to
    /// history, and returns the action for execution.
    pub fn approve(&mut self, id: &str) -> Result<AgentAction, String> {
        let idx = self
            .pending
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| format!("action {id} not found in pending queue"))?;
        let entry = self.pending.remove(idx);
        let action = entry.action.clone();
        self.history.push((entry, true));
        Ok(action)
    }

    /// Deny a pending action by ID. Removes it from pending and adds to history.
    pub fn deny(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .pending
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| format!("action {id} not found in pending queue"))?;
        let entry = self.pending.remove(idx);
        self.history.push((entry, false));
        Ok(())
    }

    /// Drain all approved actions from history, returning them.
    ///
    /// Used by the executor task to poll for approved actions.
    pub fn drain_approved(&mut self) -> Vec<AgentAction> {
        let mut approved = Vec::new();
        let mut remaining = Vec::new();
        for (entry, was_approved) in self.history.drain(..) {
            if was_approved {
                approved.push(entry.action);
            } else {
                remaining.push((entry, false));
            }
        }
        self.history = remaining;
        approved
    }

    /// All currently pending actions.
    pub fn pending(&self) -> &[PendingAction] {
        &self.pending
    }

    /// Number of pending actions.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// History of resolved actions with their approval status.
    pub fn history(&self) -> &[(PendingAction, bool)] {
        &self.history
    }

    /// Submit an action from a channel source (extracts pid from action).
    ///
    /// Convenience wrapper for the arbiter executor pipeline in the binary crate.
    pub fn submit(&mut self, source: String, action: AgentAction) {
        let pid = extract_pid(&action);
        self.enqueue(action, pid, &source);
    }
}

/// Extract a pid from an AgentAction, defaulting to 0 for non-pid variants.
fn extract_pid(action: &AgentAction) -> u32 {
    match action {
        AgentAction::KillProcess { pid } | AgentAction::Inspect { pid } => *pid,
        AgentAction::RestartService { .. } | AgentAction::CustomScript { .. } => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_returns_uuid() {
        let mut q = ArbiterQueue::default();
        let id = q.enqueue(AgentAction::KillProcess { pid: 42 }, 42, "test-agent");
        assert!(!id.is_empty(), "should return non-empty UUID");
        assert_eq!(id.len(), 36, "UUID v4 string is 36 chars");
        assert_eq!(q.pending().len(), 1);
    }

    #[test]
    fn test_approve_removes_from_pending_and_returns_action() {
        let mut q = ArbiterQueue::default();
        let id = q.enqueue(AgentAction::KillProcess { pid: 1 }, 1, "agent");
        let action = q.approve(&id).expect("should approve");
        assert!(matches!(action, AgentAction::KillProcess { pid: 1 }));
        assert!(
            q.pending().is_empty(),
            "pending should be empty after approve"
        );
        assert_eq!(q.history().len(), 1);
        assert!(q.history()[0].1, "should be marked approved");
    }

    #[test]
    fn test_deny_removes_from_pending() {
        let mut q = ArbiterQueue::default();
        let id = q.enqueue(AgentAction::Inspect { pid: 7 }, 7, "agent");
        q.deny(&id).expect("should deny");
        assert!(q.pending().is_empty(), "pending should be empty after deny");
        assert_eq!(q.history().len(), 1);
        assert!(!q.history()[0].1, "should be marked denied");
    }

    #[test]
    fn test_approve_nonexistent_returns_error() {
        let mut q = ArbiterQueue::default();
        let result = q.approve("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_deny_nonexistent_returns_error() {
        let mut q = ArbiterQueue::default();
        let result = q.deny("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_enqueue_multiple_preserves_order() {
        let mut q = ArbiterQueue::default();
        let id1 = q.enqueue(AgentAction::KillProcess { pid: 1 }, 1, "a");
        let id2 = q.enqueue(AgentAction::Inspect { pid: 2 }, 2, "b");
        assert_eq!(q.pending().len(), 2);
        assert_eq!(q.pending()[0].id, id1);
        assert_eq!(q.pending()[1].id, id2);
    }

    #[test]
    fn test_submit_convenience_extracts_pid() {
        let mut q = ArbiterQueue::default();
        q.submit("agent".into(), AgentAction::KillProcess { pid: 42 });
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.pending()[0].pid, 42);
    }

    #[test]
    fn test_drain_approved_returns_only_approved() {
        let mut q = ArbiterQueue::default();
        let id1 = q.enqueue(AgentAction::KillProcess { pid: 1 }, 1, "agent");
        let id2 = q.enqueue(AgentAction::KillProcess { pid: 2 }, 2, "agent");
        q.approve(&id1).unwrap();
        q.deny(&id2).unwrap();
        let approved = q.drain_approved();
        assert_eq!(approved.len(), 1);
        assert!(matches!(approved[0], AgentAction::KillProcess { pid: 1 }));
    }
}

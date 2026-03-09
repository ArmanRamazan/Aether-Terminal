//! Shared arbiter queue for human-in-the-loop action approval.
//!
//! MCP server submits actions here; the TUI arbiter tab displays them
//! for user approval. Approved actions are drained by the executor task.

use std::collections::VecDeque;
use std::time::Instant;

use crate::AgentAction;

/// Resolution status of an arbiter action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Pending,
    Approved,
    Denied,
}

/// A single action entry in the arbiter queue.
#[derive(Debug, Clone)]
pub struct ArbiterEntry {
    /// Source agent that requested the action.
    pub source: String,
    /// The requested action.
    pub action: AgentAction,
    /// When the action was submitted.
    pub created_at: Instant,
    /// Current status.
    pub status: ActionStatus,
}

/// Shared queue of agent actions awaiting approval.
///
/// Designed to be wrapped in `Arc<Mutex<ArbiterQueue>>` and shared
/// between the MCP server (writer) and TUI arbiter tab (reader/approver).
#[derive(Debug, Default)]
pub struct ArbiterQueue {
    entries: VecDeque<ArbiterEntry>,
}

impl ArbiterQueue {
    /// Submit a new action for approval.
    pub fn submit(&mut self, source: String, action: AgentAction) {
        self.entries.push_back(ArbiterEntry {
            source,
            action,
            created_at: Instant::now(),
            status: ActionStatus::Pending,
        });
    }

    /// Approve the pending entry at the given index.
    pub fn approve(&mut self, idx: usize) -> bool {
        if let Some(entry) = self.pending_mut().nth(idx) {
            entry.status = ActionStatus::Approved;
            true
        } else {
            false
        }
    }

    /// Deny the pending entry at the given index.
    pub fn deny(&mut self, idx: usize) -> bool {
        if let Some(entry) = self.pending_mut().nth(idx) {
            entry.status = ActionStatus::Denied;
            true
        } else {
            false
        }
    }

    /// Drain all approved actions, returning them.
    pub fn drain_approved(&mut self) -> Vec<AgentAction> {
        let mut approved = Vec::new();
        self.entries.retain(|entry| {
            if entry.status == ActionStatus::Approved {
                approved.push(entry.action.clone());
                false
            } else {
                entry.status != ActionStatus::Denied
            }
        });
        approved
    }

    /// Iterate over pending entries.
    pub fn pending(&self) -> impl Iterator<Item = &ArbiterEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == ActionStatus::Pending)
    }

    /// Number of pending actions.
    pub fn pending_count(&self) -> usize {
        self.pending().count()
    }

    fn pending_mut(&mut self) -> impl Iterator<Item = &mut ArbiterEntry> {
        self.entries
            .iter_mut()
            .filter(|e| e.status == ActionStatus::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_adds_pending_entry() {
        let mut q = ArbiterQueue::default();
        q.submit("agent".into(), AgentAction::KillProcess { pid: 42 });
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn test_approve_and_drain() {
        let mut q = ArbiterQueue::default();
        q.submit("agent".into(), AgentAction::KillProcess { pid: 1 });
        q.submit("agent".into(), AgentAction::KillProcess { pid: 2 });
        assert!(q.approve(0));
        let approved = q.drain_approved();
        assert_eq!(approved.len(), 1);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn test_deny_removes_on_drain() {
        let mut q = ArbiterQueue::default();
        q.submit("agent".into(), AgentAction::KillProcess { pid: 1 });
        assert!(q.deny(0));
        let approved = q.drain_approved();
        assert!(approved.is_empty());
        assert_eq!(q.pending_count(), 0);
    }
}

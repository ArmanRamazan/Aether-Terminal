//! Arbiter tab — AI action approval panel (F4).
//!
//! Displays pending agent actions for user approval/denial, and a history
//! log of resolved actions. Actions arrive from AI agents via MCP and must
//! be explicitly approved before execution.

use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use aether_core::AgentAction;

use crate::palette::Palette;

/// Maximum number of resolved actions to keep in history.
const HISTORY_CAPACITY: usize = 100;

/// Resolution status of an arbiter action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionStatus {
    Pending,
    Approved,
    Denied,
}

/// Result of handling a key in the Arbiter tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArbiterKeyResult {
    /// Key was consumed by the tab.
    Consumed,
    /// Key was not relevant to this tab.
    NotConsumed,
    /// User pressed 'i' to inspect a PID — switch to Overview tab.
    InspectPid(u32),
}

/// A single action entry in the arbiter queue.
#[derive(Debug, Clone)]
pub(crate) struct ArbiterEntry {
    /// Source agent that requested the action (e.g. "Claude", "MCP Agent").
    pub source: String,
    /// The requested action.
    pub action: AgentAction,
    /// Target process ID (0 if not applicable).
    pub pid: u32,
    /// Target process name.
    pub name: String,
    /// When the action was submitted.
    pub created_at: Instant,
    /// Current status.
    pub status: ActionStatus,
    /// When the action was resolved (approved/denied).
    pub resolved_at: Option<Instant>,
}

/// Queue of pending and resolved arbiter actions.
#[derive(Debug, Default)]
pub(crate) struct ArbiterQueue {
    entries: Vec<ArbiterEntry>,
}

impl ArbiterQueue {
    /// Submit a new action for approval.
    #[allow(dead_code)]
    pub(crate) fn submit(&mut self, source: String, action: AgentAction) {
        let (pid, name) = match &action {
            AgentAction::KillProcess { pid } => (*pid, String::new()),
            AgentAction::RestartService { name } => (0, name.clone()),
            AgentAction::Inspect { pid } => (*pid, String::new()),
            AgentAction::CustomScript { command } => (0, command.clone()),
        };
        self.entries.push(ArbiterEntry {
            source,
            action,
            pid,
            name,
            created_at: Instant::now(),
            status: ActionStatus::Pending,
            resolved_at: None,
        });
    }

    /// Approve the action at the given pending index.
    fn approve(&mut self, pending_idx: usize) {
        if let Some(entry) = self.pending_entries_mut().nth(pending_idx) {
            entry.status = ActionStatus::Approved;
            entry.resolved_at = Some(Instant::now());
        }
        self.trim_history();
    }

    /// Deny the action at the given pending index.
    fn deny(&mut self, pending_idx: usize) {
        if let Some(entry) = self.pending_entries_mut().nth(pending_idx) {
            entry.status = ActionStatus::Denied;
            entry.resolved_at = Some(Instant::now());
        }
        self.trim_history();
    }

    /// Get the PID of the pending action at the given index.
    fn pending_pid(&self, pending_idx: usize) -> Option<u32> {
        self.pending_entries().nth(pending_idx).map(|e| e.pid)
    }

    /// Number of pending actions.
    fn pending_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == ActionStatus::Pending)
            .count()
    }

    /// Number of approved actions.
    fn approved_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == ActionStatus::Approved)
            .count()
    }

    /// Number of denied actions.
    fn denied_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.status == ActionStatus::Denied)
            .count()
    }

    /// Iterator over pending entries (immutable).
    fn pending_entries(&self) -> impl Iterator<Item = &ArbiterEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == ActionStatus::Pending)
    }

    /// Iterator over pending entries (mutable).
    fn pending_entries_mut(&mut self) -> impl Iterator<Item = &mut ArbiterEntry> {
        self.entries
            .iter_mut()
            .filter(|e| e.status == ActionStatus::Pending)
    }

    /// Iterator over resolved entries (most recent first).
    fn history_entries(&self) -> impl Iterator<Item = &ArbiterEntry> {
        self.entries
            .iter()
            .filter(|e| e.status != ActionStatus::Pending)
            .rev()
    }

    /// Trim resolved entries to keep within capacity.
    fn trim_history(&mut self) {
        let resolved: usize = self
            .entries
            .iter()
            .filter(|e| e.status != ActionStatus::Pending)
            .count();
        if resolved > HISTORY_CAPACITY {
            let excess = resolved - HISTORY_CAPACITY;
            let mut removed = 0;
            self.entries.retain(|e| {
                if e.status != ActionStatus::Pending && removed < excess {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
    }
}

/// State for the Arbiter (F4) tab.
#[derive(Debug, Default)]
pub(crate) struct ArbiterTab {
    /// Currently selected pending action index.
    selected_action: Option<usize>,
    /// Action queue (pending + history).
    queue: ArbiterQueue,
}

impl ArbiterTab {
    /// Mutable access to the arbiter queue (for submitting actions).
    #[allow(dead_code)]
    pub(crate) fn queue_mut(&mut self) -> &mut ArbiterQueue {
        &mut self.queue
    }

    /// Handle a key press. Returns the result for the app to interpret.
    pub(crate) fn handle_key(&mut self, code: KeyCode) -> ArbiterKeyResult {
        let pending = self.queue.pending_count();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if pending > 0 {
                    let next = match self.selected_action {
                        Some(i) if i + 1 < pending => i + 1,
                        Some(_) => 0,
                        None => 0,
                    };
                    self.selected_action = Some(next);
                }
                ArbiterKeyResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if pending > 0 {
                    let next = match self.selected_action {
                        Some(0) | None => pending.saturating_sub(1),
                        Some(i) => i - 1,
                    };
                    self.selected_action = Some(next);
                }
                ArbiterKeyResult::Consumed
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(idx) = self.selected_action {
                    self.queue.approve(idx);
                    self.clamp_selection();
                }
                ArbiterKeyResult::Consumed
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(idx) = self.selected_action {
                    self.queue.deny(idx);
                    self.clamp_selection();
                }
                ArbiterKeyResult::Consumed
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                if let Some(idx) = self.selected_action {
                    if let Some(pid) = self.queue.pending_pid(idx) {
                        if pid > 0 {
                            return ArbiterKeyResult::InspectPid(pid);
                        }
                    }
                }
                ArbiterKeyResult::Consumed
            }
            _ => ArbiterKeyResult::NotConsumed,
        }
    }

    /// Render the arbiter tab.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_pending(chunks[0], buf);
        self.render_history(chunks[1], buf);
        self.render_stats(chunks[2], buf);
    }

    /// Render the pending actions list.
    fn render_pending(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(Span::styled(
                "Pending Actions",
                Style::default().fg(Palette::WARNING),
            )))
            .border_style(Style::default().fg(Palette::NEON_BLUE));

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        let now = Instant::now();
        let pending: Vec<&ArbiterEntry> = self.queue.pending_entries().collect();

        if pending.is_empty() {
            let msg = Paragraph::new(Span::styled(
                "  No pending actions",
                Style::default().fg(Palette::NEON_BLUE),
            ));
            Widget::render(msg, inner, buf);
            return;
        }

        let lines: Vec<Line> = pending
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let elapsed = now.duration_since(entry.created_at);
                let time_ago = format_duration(elapsed);
                let action_text = format_action(&entry.action);
                let is_old = elapsed.as_secs() > 60;
                let is_selected = self.selected_action == Some(i);

                let fg = if is_old {
                    Palette::CRITICAL
                } else {
                    Palette::WARNING
                };

                let style = if is_selected {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg)
                };

                let text = if entry.pid > 0 {
                    format!(
                        "[{}] {}: {} PID {} ({}) — {} [Y/N/I]",
                        i + 1,
                        entry.source,
                        action_text,
                        entry.pid,
                        entry.name,
                        time_ago
                    )
                } else {
                    format!(
                        "[{}] {}: {} ({}) — {} [Y/N/I]",
                        i + 1,
                        entry.source,
                        action_text,
                        entry.name,
                        time_ago
                    )
                };

                Line::from(Span::styled(text, style))
            })
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        Widget::render(paragraph, inner, buf);
    }

    /// Render the action history log.
    fn render_history(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(Span::styled(
                "History",
                Style::default().fg(Palette::HEALTHY),
            )))
            .border_style(Style::default().fg(Palette::NEON_BLUE));

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        let history: Vec<&ArbiterEntry> = self.queue.history_entries().collect();

        if history.is_empty() {
            let msg = Paragraph::new(Span::styled(
                "  No history",
                Style::default().fg(Palette::NEON_BLUE),
            ));
            Widget::render(msg, inner, buf);
            return;
        }

        let now = Instant::now();
        let lines: Vec<Line> = history
            .iter()
            .map(|entry| {
                let action_text = format_action(&entry.action);
                let elapsed = entry
                    .resolved_at
                    .map(|t| now.duration_since(t))
                    .unwrap_or_default();
                let time_ago = format_duration(elapsed);

                let (label, color) = match entry.status {
                    ActionStatus::Approved => ("[APPROVED]", Palette::HEALTHY),
                    ActionStatus::Denied => ("[DENIED]", Palette::CRITICAL),
                    ActionStatus::Pending => unreachable!(),
                };

                let text = if entry.pid > 0 {
                    format!(
                        "{} {} PID {} ({}) — {}",
                        label, action_text, entry.pid, entry.name, time_ago
                    )
                } else {
                    format!("{} {} ({}) — {}", label, action_text, entry.name, time_ago)
                };

                Line::from(Span::styled(text, Style::default().fg(color)))
            })
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        Widget::render(paragraph, inner, buf);
    }

    /// Render the stats line at the bottom.
    fn render_stats(&self, area: Rect, buf: &mut Buffer) {
        let stats = Line::from(vec![
            Span::styled("Pending: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                self.queue.pending_count().to_string(),
                Style::default()
                    .fg(Palette::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Approved: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                self.queue.approved_count().to_string(),
                Style::default()
                    .fg(Palette::HEALTHY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Denied: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                self.queue.denied_count().to_string(),
                Style::default()
                    .fg(Palette::CRITICAL)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let paragraph = Paragraph::new(stats);
        Widget::render(paragraph, area, buf);
    }

    /// Clamp selection after removing a pending action.
    fn clamp_selection(&mut self) {
        let pending = self.queue.pending_count();
        if pending == 0 {
            self.selected_action = None;
        } else if let Some(idx) = self.selected_action {
            if idx >= pending {
                self.selected_action = Some(pending - 1);
            }
        }
    }
}

/// Format an [`AgentAction`] as a short human-readable string.
fn format_action(action: &AgentAction) -> &'static str {
    match action {
        AgentAction::KillProcess { .. } => "kill",
        AgentAction::RestartService { .. } => "restart",
        AgentAction::Inspect { .. } => "inspect",
        AgentAction::CustomScript { .. } => "script",
    }
}

/// Format a duration as a human-readable "time ago" string.
fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tab_has_no_selection() {
        let tab = ArbiterTab::default();
        assert_eq!(tab.selected_action, None);
    }

    #[test]
    fn test_submit_adds_pending_entry() {
        let mut queue = ArbiterQueue::default();
        queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 1234 },
        );
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.approved_count(), 0);
        assert_eq!(queue.denied_count(), 0);
    }

    #[test]
    fn test_approve_moves_to_history() {
        let mut queue = ArbiterQueue::default();
        queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 1234 },
        );
        queue.approve(0);
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.approved_count(), 1);
    }

    #[test]
    fn test_deny_moves_to_history() {
        let mut queue = ArbiterQueue::default();
        queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 1234 },
        );
        queue.deny(0);
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.denied_count(), 1);
    }

    #[test]
    fn test_navigate_down_wraps() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "A".to_string(),
            AgentAction::KillProcess { pid: 1 },
        );
        tab.queue.submit(
            "B".to_string(),
            AgentAction::KillProcess { pid: 2 },
        );

        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_action, Some(0));
        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_action, Some(1));
        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_action, Some(0)); // wraps
    }

    #[test]
    fn test_navigate_up_wraps() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "A".to_string(),
            AgentAction::KillProcess { pid: 1 },
        );
        tab.queue.submit(
            "B".to_string(),
            AgentAction::KillProcess { pid: 2 },
        );

        tab.handle_key(KeyCode::Char('k'));
        assert_eq!(tab.selected_action, Some(1)); // wraps to last
        tab.handle_key(KeyCode::Char('k'));
        assert_eq!(tab.selected_action, Some(0));
    }

    #[test]
    fn test_approve_selected_action() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 1234 },
        );
        tab.selected_action = Some(0);

        let result = tab.handle_key(KeyCode::Char('y'));
        assert_eq!(result, ArbiterKeyResult::Consumed);
        assert_eq!(tab.queue.pending_count(), 0);
        assert_eq!(tab.queue.approved_count(), 1);
        assert_eq!(tab.selected_action, None);
    }

    #[test]
    fn test_deny_selected_action() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 1234 },
        );
        tab.selected_action = Some(0);

        let result = tab.handle_key(KeyCode::Char('n'));
        assert_eq!(result, ArbiterKeyResult::Consumed);
        assert_eq!(tab.queue.pending_count(), 0);
        assert_eq!(tab.queue.denied_count(), 1);
    }

    #[test]
    fn test_inspect_returns_pid() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "Claude".to_string(),
            AgentAction::KillProcess { pid: 42 },
        );
        tab.selected_action = Some(0);

        let result = tab.handle_key(KeyCode::Char('i'));
        assert_eq!(result, ArbiterKeyResult::InspectPid(42));
    }

    #[test]
    fn test_inspect_zero_pid_stays_consumed() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "Claude".to_string(),
            AgentAction::RestartService {
                name: "nginx".to_string(),
            },
        );
        tab.selected_action = Some(0);

        let result = tab.handle_key(KeyCode::Char('i'));
        assert_eq!(result, ArbiterKeyResult::Consumed);
    }

    #[test]
    fn test_unknown_key_not_consumed() {
        let mut tab = ArbiterTab::default();
        let result = tab.handle_key(KeyCode::Char('x'));
        assert_eq!(result, ArbiterKeyResult::NotConsumed);
    }

    #[test]
    fn test_clamp_selection_after_approve() {
        let mut tab = ArbiterTab::default();
        tab.queue.submit(
            "A".to_string(),
            AgentAction::KillProcess { pid: 1 },
        );
        tab.queue.submit(
            "B".to_string(),
            AgentAction::KillProcess { pid: 2 },
        );
        tab.selected_action = Some(1);

        // Approve the second (and last) pending action
        tab.handle_key(KeyCode::Char('y'));
        // Selection should clamp to the remaining pending action
        assert_eq!(tab.selected_action, Some(0));
    }

    #[test]
    fn test_format_duration_seconds() {
        let d = std::time::Duration::from_secs(30);
        assert_eq!(format_duration(d), "30s ago");
    }

    #[test]
    fn test_format_duration_minutes() {
        let d = std::time::Duration::from_secs(120);
        assert_eq!(format_duration(d), "2m ago");
    }

    #[test]
    fn test_format_duration_hours() {
        let d = std::time::Duration::from_secs(7200);
        assert_eq!(format_duration(d), "2h ago");
    }

    #[test]
    fn test_history_capacity_trim() {
        let mut queue = ArbiterQueue::default();
        for i in 0..(HISTORY_CAPACITY + 10) {
            queue.submit(
                "A".to_string(),
                AgentAction::KillProcess { pid: i as u32 },
            );
        }
        // Approve all
        for _ in 0..(HISTORY_CAPACITY + 10) {
            queue.approve(0);
        }
        let resolved = queue
            .entries
            .iter()
            .filter(|e| e.status != ActionStatus::Pending)
            .count();
        assert!(resolved <= HISTORY_CAPACITY);
    }

    #[test]
    fn test_navigate_empty_queue_noop() {
        let mut tab = ArbiterTab::default();
        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_action, None);
    }
}

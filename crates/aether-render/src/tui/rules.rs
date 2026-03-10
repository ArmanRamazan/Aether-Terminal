//! Rules tab — JIT rule engine monitor (F5).
//!
//! Displays active DSL rules, per-rule match counts, engine stats,
//! and details of the selected rule. Navigation via j/k.

use std::sync::{Arc, Mutex};

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::palette::Palette;

/// Snapshot of engine stats pushed from the engine task.
#[derive(Debug, Clone, Default)]
pub struct RulesDisplayState {
    /// Rule names in display order.
    pub rule_names: Vec<String>,
    /// Per-rule match counts (parallel to `rule_names`).
    pub match_counts: Vec<u64>,
    /// Total evaluations across all snapshots.
    pub total_evaluations: u64,
    /// Total actions triggered.
    pub total_actions: u64,
}

/// Result of handling a key in the Rules tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RulesKeyResult {
    /// Key was consumed by the tab.
    Consumed,
    /// Key was not relevant to this tab.
    NotConsumed,
}

/// State for the Rules (F5) tab.
#[derive(Default)]
pub(crate) struct RulesTab {
    /// Currently selected rule index.
    selected_rule: Option<usize>,
    /// Shared display state updated by the engine forwarder task.
    display_state: Option<Arc<Mutex<RulesDisplayState>>>,
}

impl RulesTab {
    /// Set the shared display state (called from App after main.rs wiring).
    pub(crate) fn set_display_state(&mut self, state: Arc<Mutex<RulesDisplayState>>) {
        self.display_state = Some(state);
    }

    /// Handle a key press. Returns the result for the app to interpret.
    pub(crate) fn handle_key(&mut self, code: KeyCode) -> RulesKeyResult {
        let count = self.rule_count();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    let next = match self.selected_rule {
                        Some(i) if i + 1 < count => i + 1,
                        Some(_) => 0,
                        None => 0,
                    };
                    self.selected_rule = Some(next);
                }
                RulesKeyResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    let next = match self.selected_rule {
                        Some(0) | None => count.saturating_sub(1),
                        Some(i) => i - 1,
                    };
                    self.selected_rule = Some(next);
                }
                RulesKeyResult::Consumed
            }
            _ => RulesKeyResult::NotConsumed,
        }
    }

    /// Render the rules tab.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        self.render_rule_list(chunks[0], buf);
        self.render_stats(chunks[1], buf);
    }

    /// Render the list of active rules with match counts.
    fn render_rule_list(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(Span::styled(
                "Active Rules",
                Style::default().fg(Palette::NEON_BLUE),
            )))
            .border_style(Style::default().fg(Palette::NEON_BLUE));

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        let state = self.load_state();

        if state.rule_names.is_empty() {
            let msg = if self.display_state.is_some() {
                "  No rules loaded"
            } else {
                "  Rules engine not active (use --rules <PATH>)"
            };
            let paragraph =
                Paragraph::new(Span::styled(msg, Style::default().fg(Palette::NEON_BLUE)));
            Widget::render(paragraph, inner, buf);
            return;
        }

        let lines: Vec<Line> = state
            .rule_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let count = state.match_counts.get(i).copied().unwrap_or(0);
                let is_selected = self.selected_rule == Some(i);

                let style = if is_selected {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(Palette::NEON_BLUE)
                        .add_modifier(Modifier::BOLD)
                } else if count > 0 {
                    Style::default().fg(Palette::WARNING)
                } else {
                    Style::default().fg(Palette::DATA)
                };

                let text = format!("  [{:>2}] {:<30} matches: {}", i + 1, name, count);
                Line::from(Span::styled(text, style))
            })
            .collect();

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        Widget::render(paragraph, inner, buf);
    }

    /// Render the stats summary at the bottom.
    fn render_stats(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(Span::styled(
                "Engine Stats",
                Style::default().fg(Palette::HEALTHY),
            )))
            .border_style(Style::default().fg(Palette::NEON_BLUE));

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        let state = self.load_state();

        let stats = Line::from(vec![
            Span::styled("Rules: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                state.rule_names.len().to_string(),
                Style::default()
                    .fg(Palette::DATA)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Evaluations: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                state.total_evaluations.to_string(),
                Style::default()
                    .fg(Palette::DATA)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | Actions: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                state.total_actions.to_string(),
                Style::default()
                    .fg(Palette::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let paragraph = Paragraph::new(stats);
        Widget::render(paragraph, inner, buf);
    }

    fn rule_count(&self) -> usize {
        self.display_state
            .as_ref()
            .and_then(|s| s.lock().ok())
            .map(|s| s.rule_names.len())
            .unwrap_or(0)
    }

    fn load_state(&self) -> RulesDisplayState {
        self.display_state
            .as_ref()
            .and_then(|s| s.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tab_has_no_selection() {
        let tab = RulesTab::default();
        assert_eq!(tab.selected_rule, None);
    }

    #[test]
    fn test_navigate_empty_noop() {
        let mut tab = RulesTab::default();
        let result = tab.handle_key(KeyCode::Char('j'));
        assert_eq!(result, RulesKeyResult::Consumed);
        assert_eq!(tab.selected_rule, None);
    }

    #[test]
    fn test_navigate_with_rules() {
        let state = Arc::new(Mutex::new(RulesDisplayState {
            rule_names: vec!["rule_a".into(), "rule_b".into()],
            match_counts: vec![0, 5],
            total_evaluations: 100,
            total_actions: 5,
        }));
        let mut tab = RulesTab::default();
        tab.set_display_state(state);

        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_rule, Some(0));

        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_rule, Some(1));

        // Wraps
        tab.handle_key(KeyCode::Char('j'));
        assert_eq!(tab.selected_rule, Some(0));
    }

    #[test]
    fn test_navigate_up_wraps() {
        let state = Arc::new(Mutex::new(RulesDisplayState {
            rule_names: vec!["rule_a".into(), "rule_b".into()],
            match_counts: vec![0, 0],
            total_evaluations: 0,
            total_actions: 0,
        }));
        let mut tab = RulesTab::default();
        tab.set_display_state(state);

        tab.handle_key(KeyCode::Char('k'));
        assert_eq!(tab.selected_rule, Some(1)); // wraps to last
    }

    #[test]
    fn test_unknown_key_not_consumed() {
        let mut tab = RulesTab::default();
        let result = tab.handle_key(KeyCode::Char('x'));
        assert_eq!(result, RulesKeyResult::NotConsumed);
    }

    #[test]
    fn test_render_no_engine() {
        let tab = RulesTab::default();
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf);

        // Read each row as a string to find the hint text
        let mut found = false;
        for y in 0..20 {
            let row: String = (0..80u16)
                .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect();
            if row.contains("--rules") {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "should display --rules hint when engine is not active"
        );
    }
}

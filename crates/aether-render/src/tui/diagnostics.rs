//! Diagnostics tab — findings list with detail panel (F6).
//!
//! Displays diagnostic findings from the analysis engine with severity
//! coloring, evidence details, and actionable recommendations.

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use aether_core::{DiagTarget, Diagnostic, Severity, Urgency};

use crate::palette::Palette;

/// Action returned from key handling in the Diagnostics tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiagnosticAction {
    /// Execute the recommended action for a diagnostic.
    Execute(u64),
    /// Dismiss a diagnostic finding.
    Dismiss(u64),
    /// Mute a rule so it stops generating findings.
    #[allow(dead_code)]
    MuteRule(String),
}

/// State for the Diagnostics (F6) tab.
#[derive(Default)]
pub(crate) struct DiagnosticsTab {
    /// Currently selected row in the list.
    selected: usize,
    /// Scroll offset for the list panel.
    scroll_offset: usize,
    /// Scroll offset for the detail panel.
    detail_scroll: usize,
}

impl DiagnosticsTab {
    /// Render the diagnostics tab into the given area.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer, diagnostics: &[Diagnostic]) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        self.render_list(chunks[0], buf, diagnostics);
        self.render_detail(chunks[1], buf, diagnostics);
    }

    /// Handle a key press. Returns an action if the key triggers one.
    pub(crate) fn handle_key(
        &mut self,
        code: KeyCode,
        diagnostics: &[Diagnostic],
    ) -> Option<DiagnosticAction> {
        let count = diagnostics.len();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.selected = if self.selected + 1 < count {
                        self.selected + 1
                    } else {
                        0
                    };
                    self.detail_scroll = 0;
                    self.adjust_scroll(count);
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    self.selected = if self.selected == 0 {
                        count - 1
                    } else {
                        self.selected - 1
                    };
                    self.detail_scroll = 0;
                    self.adjust_scroll(count);
                }
                None
            }
            KeyCode::Enter => diagnostics
                .get(self.selected)
                .map(|d| DiagnosticAction::Execute(d.id)),
            KeyCode::Char('d') => diagnostics
                .get(self.selected)
                .map(|d| DiagnosticAction::Dismiss(d.id)),
            KeyCode::Char('m') => {
                // TODO: extract rule_id from diagnostic when rule tracking is added
                None
            }
            _ => None,
        }
    }

    /// Render the list panel (left side).
    fn render_list(&self, area: Rect, buf: &mut Buffer, diagnostics: &[Diagnostic]) {
        let block = Block::default()
            .title(" Diagnostics ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Palette::NEON_BLUE));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 {
            return;
        }

        // Header: severity counts
        let counts = severity_counts(diagnostics);
        let header = Line::from(vec![
            Span::styled("■ ", Style::default().fg(Palette::DIAGNOSTIC_CRITICAL)),
            Span::styled(
                format!("{} Critical  ", counts.0),
                Style::default().fg(Palette::DIAGNOSTIC_CRITICAL),
            ),
            Span::styled("■ ", Style::default().fg(Palette::DIAGNOSTIC_WARNING)),
            Span::styled(
                format!("{} Warning  ", counts.1),
                Style::default().fg(Palette::DIAGNOSTIC_WARNING),
            ),
            Span::styled("■ ", Style::default().fg(Palette::DIAGNOSTIC_INFO)),
            Span::styled(
                format!("{} Info", counts.2),
                Style::default().fg(Palette::DIAGNOSTIC_INFO),
            ),
        ]);
        let header_area = Rect::new(inner.x, inner.y, inner.width, 1);
        Paragraph::new(header).render(header_area, buf);

        // Rows
        let rows_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));
        let visible_count = rows_area.height as usize;

        for (i, diag) in diagnostics
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_count)
        {
            let y = rows_area.y + (i - self.scroll_offset) as u16;
            let row_area = Rect::new(rows_area.x, y, rows_area.width, 1);
            let is_selected = i == self.selected;

            let sev_style = severity_style(diag.severity);
            let icon = severity_icon(diag.severity);
            let target_name = target_short_name(&diag.target);

            let mut spans = vec![
                Span::styled(format!("{icon} "), sev_style),
                Span::styled(
                    truncate(&target_name, 16),
                    Style::default().fg(Palette::DATA),
                ),
                Span::raw(" "),
                Span::styled(
                    truncate(&diag.summary, rows_area.width.saturating_sub(22) as usize),
                    Style::default().fg(Palette::DATA),
                ),
            ];

            let style = if is_selected {
                Style::default()
                    .bg(Palette::NEON_BLUE)
                    .fg(Palette::BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            if is_selected {
                // Override span styles for selected row
                spans = spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, style))
                    .collect();
            }

            let line = Line::from(spans);
            Paragraph::new(line).render(row_area, buf);
        }
    }

    /// Render the detail panel (right side).
    fn render_detail(&self, area: Rect, buf: &mut Buffer, diagnostics: &[Diagnostic]) {
        let selected_diag = diagnostics.get(self.selected);
        let title = match selected_diag {
            Some(d) => format!(" {} ", d.summary),
            None => " Details ".to_string(),
        };

        let block = Block::default()
            .title(truncate(&title, area.width.saturating_sub(2) as usize))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Palette::NEON_BLUE));
        let inner = block.inner(area);
        block.render(area, buf);

        let Some(diag) = selected_diag else {
            let hint = Paragraph::new("Select a diagnostic to view details")
                .style(Style::default().fg(Palette::DATA));
            hint.render(inner, buf);
            return;
        };

        let mut lines: Vec<Line> = Vec::new();

        // Target
        lines.push(Line::from(vec![
            Span::styled("Target: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                target_display(&diag.target),
                Style::default().fg(Palette::DATA),
            ),
        ]));

        // Severity + Category
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:?} ", diag.severity),
                severity_style(diag.severity).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" {:?} ", diag.category),
                Style::default()
                    .fg(Palette::DATA)
                    .add_modifier(Modifier::DIM),
            ),
        ]));

        lines.push(Line::raw(""));

        // Evidence
        if !diag.evidence.is_empty() {
            lines.push(Line::from(Span::styled(
                "Evidence:",
                Style::default()
                    .fg(Palette::NEON_BLUE)
                    .add_modifier(Modifier::BOLD),
            )));
            for ev in &diag.evidence {
                let trend_str = match ev.trend {
                    Some(t) if t > 0.01 => format!(" ↑ {t:.1}"),
                    Some(t) if t < -0.01 => format!(" ↓ {t:.1}"),
                    Some(_) => " → stable".to_string(),
                    None => String::new(),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {}: ", ev.metric),
                        Style::default().fg(Palette::DATA),
                    ),
                    Span::styled(
                        format!("{:.1}", ev.current),
                        severity_style(diag.severity),
                    ),
                    Span::styled(
                        format!(" / {:.1}", ev.threshold),
                        Style::default().fg(Palette::DATA).add_modifier(Modifier::DIM),
                    ),
                    Span::styled(trend_str, Style::default().fg(Palette::PREDICTION)),
                ]));
                if !ev.context.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("    {}", ev.context),
                        Style::default().fg(Palette::DATA).add_modifier(Modifier::DIM),
                    )));
                }
            }
        }

        lines.push(Line::raw(""));

        // Recommendation
        lines.push(Line::from(Span::styled(
            "Recommendation:",
            Style::default()
                .fg(Palette::NEON_BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("  Action: ", Style::default().fg(Palette::DATA)),
            Span::styled(
                format!("{:?}", diag.recommendation.action),
                Style::default().fg(Palette::HEALTHY),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Reason: ", Style::default().fg(Palette::DATA)),
            Span::styled(
                &diag.recommendation.reason,
                Style::default().fg(Palette::DATA),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Urgency: ", Style::default().fg(Palette::DATA)),
            Span::styled(
                format!("{:?}", diag.recommendation.urgency),
                urgency_style(diag.recommendation.urgency),
            ),
        ]));

        lines.push(Line::raw(""));

        // Keybinding hints
        lines.push(Line::from(vec![
            Span::styled("[Enter]", Style::default().fg(Palette::HEALTHY)),
            Span::styled(" Execute  ", Style::default().fg(Palette::DATA)),
            Span::styled("[d]", Style::default().fg(Palette::WARNING)),
            Span::styled(" Dismiss  ", Style::default().fg(Palette::DATA)),
            Span::styled("[m]", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(" Mute rule", Style::default().fg(Palette::DATA)),
        ]));

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll as u16, 0));
        paragraph.render(inner, buf);
    }

    /// Keep the selected row visible within the scroll window.
    fn adjust_scroll(&mut self, _count: usize) {
        // Simple adjustment — will be refined when we know visible height.
        // For now, keep selected in a reasonable range.
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        // We'll clamp the bottom in render since we need area height there.
    }
}

/// Return style for a given severity level.
fn severity_style(severity: Severity) -> Style {
    let color = match severity {
        Severity::Critical => Palette::DIAGNOSTIC_CRITICAL,
        Severity::Warning => Palette::DIAGNOSTIC_WARNING,
        Severity::Info => Palette::DIAGNOSTIC_INFO,
    };
    Style::default().fg(color)
}

/// Return style for a given urgency level.
fn urgency_style(urgency: Urgency) -> Style {
    let color = match urgency {
        Urgency::Immediate => Palette::DIAGNOSTIC_CRITICAL,
        Urgency::Soon => Palette::DIAGNOSTIC_WARNING,
        Urgency::Planning => Palette::NEON_BLUE,
        Urgency::Informational => Palette::DIAGNOSTIC_INFO,
    };
    Style::default().fg(color)
}

/// Severity icon character.
fn severity_icon(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "■",
        Severity::Warning => "●",
        Severity::Info => "●",
    }
}

/// Count diagnostics by severity: (critical, warning, info).
fn severity_counts(diagnostics: &[Diagnostic]) -> (usize, usize, usize) {
    let mut critical = 0;
    let mut warning = 0;
    let mut info = 0;
    for d in diagnostics {
        match d.severity {
            Severity::Critical => critical += 1,
            Severity::Warning => warning += 1,
            Severity::Info => info += 1,
        }
    }
    (critical, warning, info)
}

/// Short name for a diagnostic target (for the list column).
fn target_short_name(target: &DiagTarget) -> String {
    match target {
        DiagTarget::Process { name, .. } => name.clone(),
        DiagTarget::Host(id) => id.as_str().to_string(),
        DiagTarget::Container { name, .. } => name.clone(),
        DiagTarget::Disk { mount } => mount.clone(),
        DiagTarget::Network { interface } => interface.clone(),
        _ => "unknown".to_string(),
    }
}

/// Verbose display of a diagnostic target (for detail panel).
fn target_display(target: &DiagTarget) -> String {
    match target {
        DiagTarget::Process { pid, name } => format!("Process: {name} (pid {pid})"),
        DiagTarget::Host(id) => format!("Host: {}", id.as_str()),
        DiagTarget::Container { name, id } => format!("Container: {name} ({id})"),
        DiagTarget::Disk { mount } => format!("Disk: {mount}"),
        DiagTarget::Network { interface } => format!("Network: {interface}"),
        _ => "Unknown target".to_string(),
    }
}

/// Truncate a string to fit a max width, adding "…" if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 1 {
        format!("{}…", &s[..max - 1])
    } else {
        s[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use aether_core::{DiagCategory, Evidence, HostId, Recommendation, RecommendedAction};

    fn sample_diagnostic(id: u64, severity: Severity) -> Diagnostic {
        Diagnostic {
            id,
            host: HostId::new("test-host"),
            target: DiagTarget::Process {
                pid: 42,
                name: "test-proc".to_string(),
            },
            severity,
            category: DiagCategory::CpuSpike,
            summary: "CPU spike detected".to_string(),
            evidence: vec![Evidence {
                metric: "cpu_percent".to_string(),
                current: 95.0,
                threshold: 80.0,
                trend: Some(2.5),
                context: "sustained for 30s".to_string(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "high CPU usage".to_string(),
                },
                reason: "CPU above threshold".to_string(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[test]
    fn test_severity_style_maps_to_palette() {
        let s = severity_style(Severity::Critical);
        assert_eq!(s.fg, Some(Palette::DIAGNOSTIC_CRITICAL.into()));

        let s = severity_style(Severity::Warning);
        assert_eq!(s.fg, Some(Palette::DIAGNOSTIC_WARNING.into()));

        let s = severity_style(Severity::Info);
        assert_eq!(s.fg, Some(Palette::DIAGNOSTIC_INFO.into()));
    }

    #[test]
    fn test_severity_counts() {
        let diags = vec![
            sample_diagnostic(1, Severity::Critical),
            sample_diagnostic(2, Severity::Warning),
            sample_diagnostic(3, Severity::Info),
            sample_diagnostic(4, Severity::Critical),
        ];
        assert_eq!(severity_counts(&diags), (2, 1, 1));
    }

    #[test]
    fn test_severity_counts_empty() {
        assert_eq!(severity_counts(&[]), (0, 0, 0));
    }

    #[test]
    fn test_navigate_down_wraps() {
        let mut tab = DiagnosticsTab::default();
        let diags = vec![
            sample_diagnostic(1, Severity::Info),
            sample_diagnostic(2, Severity::Warning),
        ];

        tab.handle_key(KeyCode::Char('j'), &diags);
        assert_eq!(tab.selected, 1);

        tab.handle_key(KeyCode::Char('j'), &diags);
        assert_eq!(tab.selected, 0, "should wrap to start");
    }

    #[test]
    fn test_navigate_up_wraps() {
        let mut tab = DiagnosticsTab::default();
        let diags = vec![
            sample_diagnostic(1, Severity::Info),
            sample_diagnostic(2, Severity::Warning),
        ];

        tab.handle_key(KeyCode::Char('k'), &diags);
        assert_eq!(tab.selected, 1, "should wrap to end");
    }

    #[test]
    fn test_enter_returns_execute() {
        let mut tab = DiagnosticsTab::default();
        let diags = vec![sample_diagnostic(42, Severity::Critical)];

        let action = tab.handle_key(KeyCode::Enter, &diags);
        assert_eq!(action, Some(DiagnosticAction::Execute(42)));
    }

    #[test]
    fn test_d_returns_dismiss() {
        let mut tab = DiagnosticsTab::default();
        let diags = vec![sample_diagnostic(7, Severity::Warning)];

        let action = tab.handle_key(KeyCode::Char('d'), &diags);
        assert_eq!(action, Some(DiagnosticAction::Dismiss(7)));
    }

    #[test]
    fn test_empty_diagnostics_no_action() {
        let mut tab = DiagnosticsTab::default();
        assert_eq!(tab.handle_key(KeyCode::Char('j'), &[]), None);
        assert_eq!(tab.handle_key(KeyCode::Enter, &[]), None);
    }

    #[test]
    fn test_target_short_name_process() {
        let target = DiagTarget::Process {
            pid: 1,
            name: "firefox".to_string(),
        };
        assert_eq!(target_short_name(&target), "firefox");
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 6), "hello…");
    }

    #[test]
    fn test_render_no_panic_empty() {
        let tab = DiagnosticsTab::default();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &[]);
    }

    #[test]
    fn test_render_no_panic_with_data() {
        let tab = DiagnosticsTab::default();
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        let diags = vec![
            sample_diagnostic(1, Severity::Critical),
            sample_diagnostic(2, Severity::Warning),
            sample_diagnostic(3, Severity::Info),
        ];
        tab.render(area, &mut buf, &diags);
    }
}

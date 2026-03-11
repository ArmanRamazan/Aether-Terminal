//! Overview tab — sortable process table (F1) with detail panel.
//!
//! Renders all processes from the [`WorldGraph`] as a ratatui [`Table`] with
//! color-coded rows based on CPU load. Supports keyboard-driven sorting,
//! scrolling, row selection, and a detail panel (Enter/Esc).

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table, Widget, Wrap};

use aether_core::models::{DiagTarget, ProcessState, Severity};
use aether_core::{Diagnostic, WorldGraph};

use crate::palette::{self, Palette};
use crate::PredictionDisplay;

/// Column by which the process table can be sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SortColumn {
    /// Process ID.
    Pid,
    /// Process name.
    Name,
    /// CPU usage percentage.
    #[default]
    Cpu,
    /// Memory in bytes.
    Mem,
    /// OS process state.
    State,
    /// Health points.
    Hp,
}

/// State for the Overview (F1) process table.
#[derive(Debug)]
pub(crate) struct OverviewTab {
    /// Currently selected row index (if any).
    selected_row: Option<usize>,
    /// Column used for sorting.
    sort_column: SortColumn,
    /// Sort direction — `true` for ascending.
    sort_ascending: bool,
    /// Number of rows scrolled past the top of the visible area.
    scroll_offset: usize,
    /// PID of the process whose detail panel is open (Enter to open, Esc to close).
    detail_pid: Option<u32>,
}

impl Default for OverviewTab {
    fn default() -> Self {
        Self {
            selected_row: None,
            sort_column: SortColumn::Cpu,
            sort_ascending: false,
            scroll_offset: 0,
            detail_pid: None,
        }
    }
}

impl OverviewTab {
    /// Current sort column.
    pub(crate) fn sort_column(&self) -> SortColumn {
        self.sort_column
    }

    /// Open the detail panel for the given PID (used by World3D inspect).
    pub(crate) fn set_detail_pid(&mut self, pid: u32) {
        self.detail_pid = Some(pid);
    }

    /// Current sort direction.
    pub(crate) fn sort_ascending(&self) -> bool {
        self.sort_ascending
    }

    /// Handle a navigation key.
    ///
    /// `sorted_pids` maps each row index (in current sort order) to a PID.
    /// Pass an empty slice when the world graph is unavailable.
    pub(crate) fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        process_count: usize,
        sorted_pids: &[u32],
    ) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection_down(process_count),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection_up(),
            KeyCode::Enter => {
                if let Some(idx) = self.selected_row {
                    if let Some(&pid) = sorted_pids.get(idx) {
                        self.detail_pid = Some(pid);
                    }
                }
            }
            KeyCode::Esc => {
                self.detail_pid = None;
            }
            _ => {}
        }
    }

    /// Handle a sort-mode key press. Returns `true` if the key was consumed.
    pub(crate) fn handle_sort_key(&mut self, code: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        let col = match code {
            KeyCode::Char('p') => SortColumn::Pid,
            KeyCode::Char('n') => SortColumn::Name,
            KeyCode::Char('c') => SortColumn::Cpu,
            KeyCode::Char('m') => SortColumn::Mem,
            KeyCode::Char('t') => SortColumn::State,
            KeyCode::Char('h') => SortColumn::Hp,
            _ => return false,
        };
        if self.sort_column == col {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = col;
            self.sort_ascending = false;
        }
        true
    }

    /// Render the process table (and detail panel if open) into `buf`.
    pub(crate) fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        world: &WorldGraph,
        predictions: &[PredictionDisplay],
        diagnostics: &[Diagnostic],
    ) {
        // Split off a predictions panel at the bottom when predictions exist.
        let (main_area, predictions_area) = if predictions.is_empty() {
            (area, None)
        } else {
            let pred_height = (predictions.len() as u16 + 2).min(8);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(pred_height)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        };

        let (table_area, detail_area) = if self.detail_pid.is_some() {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(main_area);
            (chunks[0], Some(chunks[1]))
        } else {
            (main_area, None)
        };

        self.render_table(table_area, buf, world, predictions, diagnostics);

        if let (Some(pid), Some(panel_area)) = (self.detail_pid, detail_area) {
            render_detail_panel(panel_area, buf, world, pid);
        }

        if let Some(pred_area) = predictions_area {
            render_predictions_panel(pred_area, buf, predictions);
        }
    }

    /// Render the process table into the given area.
    fn render_table(
        &self,
        area: Rect,
        buf: &mut Buffer,
        world: &WorldGraph,
        predictions: &[PredictionDisplay],
        diagnostics: &[Diagnostic],
    ) {
        let predicted_pids: HashSet<u32> = predictions.iter().map(|p| p.pid).collect();
        let mut rows = collect_sorted_rows(world, self.sort_column, self.sort_ascending);

        // Apply scroll offset.
        let total = rows.len();
        let offset = self.scroll_offset.min(total.saturating_sub(1));
        rows = rows.into_iter().skip(offset).collect();

        let styled_rows: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, cols)| {
                let global_idx = offset + i;
                let cpu: f32 = cols[2].trim_end_matches('%').parse().unwrap_or(0.0);
                let fg = palette::color_for_load(cpu);
                let pid: u32 = cols[0].parse().unwrap_or(0);

                let style = if self.selected_row == Some(global_idx) {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg)
                };

                let mut cells: Vec<Cell> =
                    cols.iter().map(|c| Cell::from(c.as_str())).collect();

                // Diag column: show highest-severity diagnostic for this PID.
                cells.push(format_diag_cell(pid, diagnostics));

                if predicted_pids.contains(&pid) {
                    cells.push(
                        Cell::from("⚠").style(Style::default().fg(Palette::PREDICTION)),
                    );
                } else {
                    cells.push(Cell::from(""));
                }

                Row::new(cells).style(style)
            })
            .collect();

        let header_style = Style::default()
            .fg(Palette::HEALTHY)
            .add_modifier(Modifier::BOLD);

        let sort_indicator = |col: SortColumn| -> &str {
            if col == self.sort_column {
                if self.sort_ascending {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            }
        };

        let header = Row::new(vec![
            Cell::from(format!("PID{}", sort_indicator(SortColumn::Pid))),
            Cell::from(format!("Name{}", sort_indicator(SortColumn::Name))),
            Cell::from(format!("CPU%{}", sort_indicator(SortColumn::Cpu))),
            Cell::from(format!("MEM{}", sort_indicator(SortColumn::Mem))),
            Cell::from(format!("State{}", sort_indicator(SortColumn::State))),
            Cell::from(format!("HP{}", sort_indicator(SortColumn::Hp))),
            Cell::from("Diag"),
            Cell::from("Pred").style(Style::default().fg(Palette::PREDICTION)),
        ])
        .style(header_style);

        let widths = [
            Constraint::Percentage(8),
            Constraint::Percentage(20),
            Constraint::Percentage(9),
            Constraint::Percentage(13),
            Constraint::Percentage(10),
            Constraint::Percentage(8),
            Constraint::Percentage(22),
            Constraint::Percentage(10),
        ];

        let title = format!("Overview [F1] — {} processes", total);
        let table = Table::new(styled_rows, widths).header(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(Span::styled(
                    title,
                    Style::default().fg(Palette::HEALTHY),
                )))
                .border_style(Style::default().fg(Palette::NEON_BLUE)),
        );

        Widget::render(table, area, buf);
    }

    /// Move selection down by one row, wrapping scroll if needed.
    fn move_selection_down(&mut self, process_count: usize) {
        if process_count == 0 {
            return;
        }
        let next = match self.selected_row {
            Some(i) if i + 1 < process_count => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.selected_row = Some(next);
        self.ensure_visible(next);
    }

    /// Move selection up by one row.
    fn move_selection_up(&mut self) {
        let next = match self.selected_row {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.selected_row = Some(next);
        self.ensure_visible(next);
    }

    /// Adjust scroll offset so that `row` is in the visible window.
    fn ensure_visible(&mut self, row: usize) {
        if row < self.scroll_offset {
            self.scroll_offset = row;
        }
        // We don't know the visible height here; a generous window keeps it usable.
        // The actual clamp happens in render().
    }
}

/// Build a [`Cell`] for the Diag column showing the highest-severity diagnostic for `pid`.
fn format_diag_cell(pid: u32, diagnostics: &[Diagnostic]) -> Cell<'static> {
    let best = diagnostics
        .iter()
        .filter(|d| matches!(&d.target, DiagTarget::Process { pid: p, .. } if *p == pid))
        .max_by_key(|d| d.severity);

    let Some(diag) = best else {
        return Cell::from("");
    };

    let (icon, color) = match diag.severity {
        Severity::Critical => ("\u{25a0}", Palette::DIAGNOSTIC_CRITICAL),
        Severity::Warning => ("\u{25a0}", Palette::DIAGNOSTIC_WARNING),
        Severity::Info => ("\u{25cf}", Palette::DIAGNOSTIC_INFO),
    };

    let summary: String = if diag.summary.chars().count() > 15 {
        let truncated: String = diag.summary.chars().take(15).collect();
        format!("{truncated}\u{2026}")
    } else {
        diag.summary.clone()
    };

    Cell::from(format!("{icon} {summary}")).style(Style::default().fg(color))
}

/// Collect process data from the world graph, sort it, and return as string rows.
fn collect_sorted_rows(
    world: &WorldGraph,
    sort_column: SortColumn,
    ascending: bool,
) -> Vec<[String; 6]> {
    let mut rows: Vec<[String; 6]> = world
        .processes()
        .map(|p| {
            [
                p.pid.to_string(),
                p.name.clone(),
                format!("{:.1}%", p.cpu_percent),
                format_mem(p.mem_bytes),
                format_state(p.state),
                format!("{:.0}", p.hp),
            ]
        })
        .collect();

    rows.sort_by(|a, b| {
        let cmp = match sort_column {
            SortColumn::Pid => {
                let pa: u32 = a[0].parse().unwrap_or(0);
                let pb: u32 = b[0].parse().unwrap_or(0);
                pa.cmp(&pb)
            }
            SortColumn::Name => a[1].to_lowercase().cmp(&b[1].to_lowercase()),
            SortColumn::Cpu => {
                let ca = parse_cpu(&a[2]);
                let cb = parse_cpu(&b[2]);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            }
            SortColumn::Mem => {
                let ma = parse_mem_sort_key(&a[3]);
                let mb = parse_mem_sort_key(&b[3]);
                ma.cmp(&mb)
            }
            SortColumn::State => a[4].cmp(&b[4]),
            SortColumn::Hp => {
                let ha: f32 = a[5].parse().unwrap_or(0.0);
                let hb: f32 = b[5].parse().unwrap_or(0.0);
                ha.partial_cmp(&hb).unwrap_or(std::cmp::Ordering::Equal)
            }
        };
        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });

    rows
}

/// Extract sorted PIDs from the world graph in the same order as [`collect_sorted_rows`].
pub(crate) fn collect_sorted_pids(
    world: &WorldGraph,
    sort_column: SortColumn,
    ascending: bool,
) -> Vec<u32> {
    collect_sorted_rows(world, sort_column, ascending)
        .iter()
        .filter_map(|row| row[0].parse().ok())
        .collect()
}

/// Render the process detail panel showing full info for a single process.
fn render_detail_panel(area: Rect, buf: &mut Buffer, world: &WorldGraph, pid: u32) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Palette::NEON_BLUE))
        .style(Style::default().bg(Palette::BG));

    let inner = block.inner(area);
    Widget::render(block, area, buf);

    let Some(proc) = world.find_by_pid(pid) else {
        let msg = Paragraph::new("Process exited").style(Style::default().fg(Palette::CRITICAL));
        Widget::render(msg, inner, buf);
        return;
    };

    // Layout: title(1) + info(5) + gap(1) + hp_label(1) + hp_gauge(1) + xp(1) + gap(1) + conn_title(1) + conn_list(rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(5), // info lines
            Constraint::Length(1), // gap
            Constraint::Length(1), // HP label
            Constraint::Length(1), // HP gauge
            Constraint::Length(1), // XP
            Constraint::Length(1), // gap
            Constraint::Length(1), // connections header
            Constraint::Min(0),    // connections list
        ])
        .split(inner);

    render_detail_title(chunks[0], buf, proc.pid, &proc.name);
    render_detail_info(chunks[1], buf, proc);
    render_detail_hp(chunks[3], chunks[4], buf, proc.hp);
    render_detail_xp(chunks[5], buf, proc.xp);
    render_detail_connections(chunks[7], chunks[8], buf, world, pid);
}

/// Render the detail panel title line.
fn render_detail_title(area: Rect, buf: &mut Buffer, pid: u32, name: &str) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("{name} "),
            Style::default()
                .fg(Palette::DATA)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[PID {pid}]"),
            Style::default().fg(Palette::NEON_BLUE),
        ),
    ]));
    Widget::render(title, area, buf);
}

/// Render process info fields (ppid, state, cpu, mem).
fn render_detail_info(area: Rect, buf: &mut Buffer, proc: &aether_core::models::ProcessNode) {
    let lines = vec![
        Line::from(vec![
            Span::styled("PPID:  ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(proc.ppid.to_string(), Style::default().fg(Palette::DATA)),
        ]),
        Line::from(vec![
            Span::styled("State: ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(format_state(proc.state), Style::default().fg(Palette::DATA)),
        ]),
        Line::from(vec![
            Span::styled("CPU:   ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                format!("{:.1}%", proc.cpu_percent),
                Style::default().fg(palette::color_for_load(proc.cpu_percent)),
            ),
        ]),
        Line::from(vec![
            Span::styled("MEM:   ", Style::default().fg(Palette::NEON_BLUE)),
            Span::styled(
                format_mem(proc.mem_bytes),
                Style::default().fg(Palette::DATA),
            ),
        ]),
    ];
    let paragraph = Paragraph::new(lines);
    Widget::render(paragraph, area, buf);
}

/// Render the HP label and gauge bar.
fn render_detail_hp(label_area: Rect, gauge_area: Rect, buf: &mut Buffer, hp: f32) {
    let hp_color = palette::color_for_hp(hp);
    let label = Paragraph::new(Line::from(vec![
        Span::styled("HP:    ", Style::default().fg(Palette::NEON_BLUE)),
        Span::styled(format!("{:.0}/100", hp), Style::default().fg(hp_color)),
    ]));
    Widget::render(label, label_area, buf);

    let ratio = (hp / 100.0).clamp(0.0, 1.0) as f64;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(hp_color).bg(Palette::BG))
        .ratio(ratio)
        .label("");
    Widget::render(gauge, gauge_area, buf);
}

/// Render the XP display line.
fn render_detail_xp(area: Rect, buf: &mut Buffer, xp: u32) {
    let line = Paragraph::new(Line::from(vec![
        Span::styled("XP:    ", Style::default().fg(Palette::NEON_BLUE)),
        Span::styled(xp.to_string(), Style::default().fg(Palette::XP_PURPLE)),
    ]));
    Widget::render(line, area, buf);
}

/// Render the connections header and list for a process.
fn render_detail_connections(
    header_area: Rect,
    list_area: Rect,
    buf: &mut Buffer,
    world: &WorldGraph,
    pid: u32,
) {
    let header = Paragraph::new(Span::styled(
        "Connections",
        Style::default()
            .fg(Palette::HEALTHY)
            .add_modifier(Modifier::BOLD),
    ));
    Widget::render(header, header_area, buf);

    let edges: Vec<_> = world.edges().filter(|e| e.source_pid == pid).collect();

    if edges.is_empty() {
        let msg = Paragraph::new(Span::styled(
            "  (none)",
            Style::default().fg(Palette::NEON_BLUE),
        ));
        Widget::render(msg, list_area, buf);
        return;
    }

    let lines: Vec<Line> = edges
        .iter()
        .map(|e| {
            Line::from(vec![
                Span::styled(
                    format!("  {:?}", e.protocol),
                    Style::default().fg(Palette::NEON_BLUE),
                ),
                Span::styled(
                    format!(" → {} ", e.dest),
                    Style::default().fg(Palette::DATA),
                ),
                Span::styled(
                    format!("({:?})", e.state),
                    Style::default().fg(Palette::HEALTHY),
                ),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    Widget::render(paragraph, list_area, buf);
}

/// Format bytes into a human-readable string (B, KB, MB, GB).
fn format_mem(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format a process state as a short label.
fn format_state(state: ProcessState) -> String {
    match state {
        ProcessState::Running => "Run".to_string(),
        ProcessState::Sleeping => "Sleep".to_string(),
        ProcessState::Zombie => "Zombie".to_string(),
        ProcessState::Stopped => "Stop".to_string(),
    }
}

/// Render the predictions panel showing anomalies sorted by ETA.
fn render_predictions_panel(area: Rect, buf: &mut Buffer, predictions: &[PredictionDisplay]) {
    let mut sorted: Vec<&PredictionDisplay> = predictions.iter().collect();
    sorted.sort_by(|a, b| {
        a.eta_seconds
            .partial_cmp(&b.eta_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let lines: Vec<Line> = sorted
        .iter()
        .take(area.height.saturating_sub(2) as usize) // fit within borders
        .map(|p| {
            Line::from(vec![
                Span::styled("  ⚠ ", Style::default().fg(Palette::PREDICTION)),
                Span::styled(
                    format!("{:<16}", p.process_name),
                    Style::default().fg(Palette::DATA),
                ),
                Span::styled(
                    format!("{:<14}", p.anomaly_label),
                    Style::default().fg(Palette::WARNING),
                ),
                Span::styled(
                    format!("{:>3.0}%  ", p.confidence * 100.0),
                    Style::default().fg(Palette::PREDICTION),
                ),
                Span::styled(
                    format!("ETA: {}", format_eta(p.eta_seconds)),
                    Style::default().fg(Palette::NEON_BLUE),
                ),
            ])
        })
        .collect();

    let title = format!("Predictions — {} anomalies", predictions.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled(
            title,
            Style::default().fg(Palette::PREDICTION),
        )))
        .border_style(Style::default().fg(Palette::PREDICTION));

    let paragraph = Paragraph::new(lines).block(block);
    Widget::render(paragraph, area, buf);
}

/// Format ETA seconds into a human-readable string.
fn format_eta(seconds: f32) -> String {
    if seconds < 60.0 {
        format!("{:.0}s", seconds)
    } else {
        format!("{:.0}m", seconds / 60.0)
    }
}

/// Parse CPU percentage from a formatted string like "12.3%".
fn parse_cpu(s: &str) -> f32 {
    s.trim_end_matches('%').parse().unwrap_or(0.0)
}

/// Parse memory string back to bytes for sorting.
fn parse_mem_sort_key(s: &str) -> u64 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix(" GB") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0 * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" MB") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" KB") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" B") {
        v.parse::<u64>().unwrap_or(0)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::ProcessNode;
    use glam::Vec3;

    fn make_process(pid: u32, name: &str, cpu: f32, mem: u64, hp: f32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: name.to_string(),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_default_sort_is_cpu_descending() {
        let tab = OverviewTab::default();
        assert_eq!(tab.sort_column, SortColumn::Cpu);
        assert!(!tab.sort_ascending);
    }

    #[test]
    fn test_sort_toggle_reverses_direction() {
        let mut tab = OverviewTab::default();
        // CPU is already selected, pressing 'c' toggles direction.
        tab.handle_sort_key(crossterm::event::KeyCode::Char('c'));
        assert!(tab.sort_ascending);
        tab.handle_sort_key(crossterm::event::KeyCode::Char('c'));
        assert!(!tab.sort_ascending);
    }

    #[test]
    fn test_sort_key_changes_column() {
        let mut tab = OverviewTab::default();
        assert!(tab.handle_sort_key(crossterm::event::KeyCode::Char('m')));
        assert_eq!(tab.sort_column, SortColumn::Mem);
        assert!(!tab.sort_ascending);
    }

    #[test]
    fn test_sort_key_unknown_returns_false() {
        let mut tab = OverviewTab::default();
        assert!(!tab.handle_sort_key(crossterm::event::KeyCode::Char('z')));
    }

    #[test]
    fn test_collect_sorted_rows_by_cpu_desc() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "low", 10.0, 1024, 100.0));
        world.add_process(make_process(2, "high", 90.0, 2048, 50.0));
        world.add_process(make_process(3, "mid", 50.0, 512, 75.0));

        let rows = collect_sorted_rows(&world, SortColumn::Cpu, false);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], "2"); // highest CPU first
        assert_eq!(rows[1][0], "3");
        assert_eq!(rows[2][0], "1");
    }

    #[test]
    fn test_collect_sorted_rows_by_pid_asc() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(30, "c", 10.0, 1024, 100.0));
        world.add_process(make_process(10, "a", 20.0, 2048, 50.0));
        world.add_process(make_process(20, "b", 15.0, 512, 75.0));

        let rows = collect_sorted_rows(&world, SortColumn::Pid, true);
        assert_eq!(rows[0][0], "10");
        assert_eq!(rows[1][0], "20");
        assert_eq!(rows[2][0], "30");
    }

    #[test]
    fn test_format_mem_scales() {
        assert_eq!(format_mem(500), "500 B");
        assert_eq!(format_mem(2048), "2.0 KB");
        assert_eq!(format_mem(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_mem(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn test_format_state_labels() {
        assert_eq!(format_state(ProcessState::Running), "Run");
        assert_eq!(format_state(ProcessState::Sleeping), "Sleep");
        assert_eq!(format_state(ProcessState::Zombie), "Zombie");
        assert_eq!(format_state(ProcessState::Stopped), "Stop");
    }

    #[test]
    fn test_move_selection_down_wraps() {
        let mut tab = OverviewTab::default();
        tab.move_selection_down(3);
        assert_eq!(tab.selected_row, Some(0));
        tab.move_selection_down(3);
        assert_eq!(tab.selected_row, Some(1));
        tab.move_selection_down(3);
        assert_eq!(tab.selected_row, Some(2));
        tab.move_selection_down(3);
        assert_eq!(tab.selected_row, Some(0)); // wraps
    }

    #[test]
    fn test_move_selection_up_stops_at_zero() {
        let mut tab = OverviewTab::default();
        tab.selected_row = Some(1);
        tab.move_selection_up();
        assert_eq!(tab.selected_row, Some(0));
        tab.move_selection_up();
        assert_eq!(tab.selected_row, Some(0)); // stays at 0
    }

    #[test]
    fn test_move_selection_empty_noop() {
        let mut tab = OverviewTab::default();
        tab.move_selection_down(0);
        assert_eq!(tab.selected_row, None);
    }

    #[test]
    fn test_enter_opens_detail_panel() {
        let mut tab = OverviewTab::default();
        tab.selected_row = Some(1);
        let pids = vec![10, 20, 30];
        tab.handle_key(crossterm::event::KeyCode::Enter, 3, &pids);
        assert_eq!(tab.detail_pid, Some(20));
    }

    #[test]
    fn test_enter_without_selection_does_nothing() {
        let mut tab = OverviewTab::default();
        let pids = vec![10, 20];
        tab.handle_key(crossterm::event::KeyCode::Enter, 2, &pids);
        assert_eq!(tab.detail_pid, None);
    }

    #[test]
    fn test_esc_closes_detail_panel() {
        let mut tab = OverviewTab::default();
        tab.detail_pid = Some(42);
        tab.handle_key(crossterm::event::KeyCode::Esc, 3, &[10, 20, 30]);
        assert_eq!(tab.detail_pid, None);
    }

    #[test]
    fn test_collect_sorted_pids_matches_rows() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "low", 10.0, 1024, 100.0));
        world.add_process(make_process(2, "high", 90.0, 2048, 50.0));

        let pids = collect_sorted_pids(&world, SortColumn::Cpu, false);
        let rows = collect_sorted_rows(&world, SortColumn::Cpu, false);
        assert_eq!(pids.len(), rows.len());
        for (pid, row) in pids.iter().zip(rows.iter()) {
            assert_eq!(*pid, row[0].parse::<u32>().unwrap());
        }
    }

    #[test]
    fn test_default_detail_pid_is_none() {
        let tab = OverviewTab::default();
        assert_eq!(tab.detail_pid, None);
    }

    #[test]
    fn test_format_eta_seconds() {
        assert_eq!(format_eta(30.0), "30s");
        assert_eq!(format_eta(59.9), "60s");
    }

    #[test]
    fn test_format_eta_minutes() {
        assert_eq!(format_eta(60.0), "1m");
        assert_eq!(format_eta(120.0), "2m");
    }

    fn make_prediction(pid: u32, name: &str, eta: f32) -> PredictionDisplay {
        PredictionDisplay {
            pid,
            process_name: name.to_string(),
            anomaly_label: "OOM".to_string(),
            confidence: 0.85,
            eta_seconds: eta,
        }
    }

    #[test]
    fn test_render_with_predictions_no_panic() {
        let tab = OverviewTab::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "proc_a", 50.0, 1024, 80.0));

        let predictions = vec![make_prediction(1, "proc_a", 30.0)];

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &world, &predictions, &[]);
    }

    #[test]
    fn test_render_without_predictions_no_panic() {
        let tab = OverviewTab::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "proc_a", 50.0, 1024, 80.0));

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &world, &[], &[]);
    }

    fn make_diagnostic(pid: u32, severity: Severity, summary: &str) -> Diagnostic {
        use aether_core::models::{
            DiagCategory, Evidence, Recommendation, RecommendedAction, Urgency,
        };
        use aether_core::metrics::HostId;
        Diagnostic {
            id: 1,
            host: HostId::default(),
            target: DiagTarget::Process {
                pid,
                name: "test".to_string(),
            },
            severity,
            category: DiagCategory::CpuSaturation,
            summary: summary.to_string(),
            evidence: vec![Evidence {
                metric: "cpu".to_string(),
                current: 95.0,
                threshold: 90.0,
                trend: None,
                context: String::new(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: "high cpu".to_string(),
                },
                reason: "test".to_string(),
                urgency: Urgency::Soon,
                auto_executable: false,
            },
            detected_at: std::time::Instant::now(),
            resolved_at: None,
        }
    }

    /// Render a single [`Cell`] into a buffer and return its text content.
    fn cell_text(cell: Cell) -> String {
        let row = Row::new(vec![cell]);
        let widths = [Constraint::Length(40)];
        let table = Table::new(vec![row], widths);
        let area = Rect::new(0, 0, 40, 2);
        let mut buf = Buffer::empty(area);
        Widget::render(table, area, &mut buf);
        (0..40u16)
            .map(|x| buf[(x, 0u16)].symbol().to_string())
            .collect::<String>()
            .trim()
            .to_string()
    }

    #[test]
    fn test_diag_cell_empty_when_no_diagnostics() {
        let cell = format_diag_cell(42, &[]);
        assert_eq!(cell_text(cell), "");
    }

    #[test]
    fn test_diag_cell_shows_highest_severity() {
        let diags = vec![
            make_diagnostic(1, Severity::Info, "info msg"),
            make_diagnostic(1, Severity::Critical, "crit msg"),
            make_diagnostic(1, Severity::Warning, "warn msg"),
        ];
        let cell = format_diag_cell(1, &diags);
        let text = cell_text(cell);
        assert!(text.contains("crit msg"), "should show critical: {text}");
    }

    #[test]
    fn test_diag_cell_truncates_long_summary() {
        let diags = vec![make_diagnostic(1, Severity::Warning, "this is a very long summary text")];
        let cell = format_diag_cell(1, &diags);
        let text = cell_text(cell);
        assert!(text.contains('\u{2026}'), "should contain ellipsis: {text}");
        assert!(!text.contains("summary text"), "should be truncated: {text}");
    }

    #[test]
    fn test_diag_cell_no_match_for_different_pid() {
        let diags = vec![make_diagnostic(1, Severity::Critical, "crit")];
        let cell = format_diag_cell(99, &diags);
        assert_eq!(cell_text(cell), "");
    }

    #[test]
    fn test_render_predictions_panel_sorts_by_eta() {
        let predictions = vec![
            make_prediction(2, "slow", 120.0),
            make_prediction(1, "fast", 10.0),
            make_prediction(3, "mid", 60.0),
        ];
        let area = Rect::new(0, 0, 80, 6);
        let mut buf = Buffer::empty(area);
        render_predictions_panel(area, &mut buf, &predictions);

        // First data row (row 1, after top border) should show "fast" (lowest eta).
        let row1: String = (0..80).map(|x| buf[(x, 1u16)].symbol().to_string()).collect();
        assert!(
            row1.contains("fast"),
            "first prediction row should be sorted by eta (lowest first), got: '{row1}'"
        );
    }
}

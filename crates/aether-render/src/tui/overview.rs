//! Overview tab — sortable process table (F1) with detail panel.
//!
//! Renders all processes from the [`WorldGraph`] as a ratatui [`Table`] with
//! color-coded rows based on CPU load. Supports keyboard-driven sorting,
//! scrolling, row selection, and a detail panel (Enter/Esc).

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table, Widget, Wrap};

use aether_core::models::ProcessState;
use aether_core::WorldGraph;

use crate::palette::{self, Palette};

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
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        let (table_area, detail_area) = if self.detail_pid.is_some() {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        self.render_table(table_area, buf, world);

        if let (Some(pid), Some(panel_area)) = (self.detail_pid, detail_area) {
            render_detail_panel(panel_area, buf, world, pid);
        }
    }

    /// Render the process table into the given area.
    fn render_table(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
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

                let style = if self.selected_row == Some(global_idx) {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(fg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg)
                };

                Row::new(cols.iter().map(|c| c.as_str()).collect::<Vec<_>>()).style(style)
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
            format!("PID{}", sort_indicator(SortColumn::Pid)),
            format!("Name{}", sort_indicator(SortColumn::Name)),
            format!("CPU%{}", sort_indicator(SortColumn::Cpu)),
            format!("MEM{}", sort_indicator(SortColumn::Mem)),
            format!("State{}", sort_indicator(SortColumn::State)),
            format!("HP{}", sort_indicator(SortColumn::Hp)),
        ])
        .style(header_style);

        let widths = [
            Constraint::Percentage(10),
            Constraint::Percentage(30),
            Constraint::Percentage(12),
            Constraint::Percentage(18),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
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
}

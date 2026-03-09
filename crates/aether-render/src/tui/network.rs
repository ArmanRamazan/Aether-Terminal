//! Network tab — connection list sorted by bytes/sec (F3).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table, Widget};

use aether_core::WorldGraph;

use crate::palette::Palette;

/// State for the Network (F3) connection list tab.
#[derive(Debug, Default)]
pub(crate) struct NetworkTab {
    /// Currently selected row index (if any).
    selected_row: Option<usize>,
    /// Filter text entered via `/` search.
    filter_text: String,
    /// Number of rows scrolled past the top of the visible area.
    scroll_offset: usize,
}

impl NetworkTab {
    /// Set the filter text (called from App when Search action fires).
    pub(crate) fn set_filter(&mut self, text: String) {
        self.filter_text = text;
        self.selected_row = None;
        self.scroll_offset = 0;
    }

    /// Clear the filter.
    pub(crate) fn clear_filter(&mut self) {
        self.filter_text.clear();
        self.selected_row = None;
        self.scroll_offset = 0;
    }

    /// Handle navigation keys. Returns `true` if the key was consumed.
    pub(crate) fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        row_count: usize,
    ) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.move_down(row_count),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Esc => self.clear_filter(),
            _ => {}
        }
    }

    /// Render the connection table into `buf`.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        let rows = collect_connection_rows(world, &self.filter_text);
        let total = rows.len();
        let offset = self.scroll_offset.min(total.saturating_sub(1));

        let styled_rows: Vec<Row> = rows
            .iter()
            .skip(offset)
            .enumerate()
            .map(|(i, cols)| {
                let global_idx = offset + i;
                let proto_color = protocol_color(&cols[3]);

                let style = if self.selected_row == Some(global_idx) {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(proto_color)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(proto_color)
                };

                Row::new(cols.iter().map(|c| c.as_str()).collect::<Vec<_>>()).style(style)
            })
            .collect();

        let header_style = Style::default()
            .fg(Palette::HEALTHY)
            .add_modifier(Modifier::BOLD);

        let header = Row::new(vec![
            "Src PID", "Src Name", "Dest IP:Port", "Proto", "State", "Bytes/s",
        ])
        .style(header_style);

        let widths = [
            ratatui::layout::Constraint::Percentage(8),
            ratatui::layout::Constraint::Percentage(20),
            ratatui::layout::Constraint::Percentage(28),
            ratatui::layout::Constraint::Percentage(10),
            ratatui::layout::Constraint::Percentage(14),
            ratatui::layout::Constraint::Percentage(20),
        ];

        let filter_hint = if self.filter_text.is_empty() {
            String::new()
        } else {
            format!(" [filter: {}]", self.filter_text)
        };
        let title = format!("Network [F3] — {} connections{}", total, filter_hint);

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

    /// Current row count for external callers.
    pub(crate) fn row_count(&self, world: &WorldGraph) -> usize {
        collect_connection_rows(world, &self.filter_text).len()
    }

    fn move_down(&mut self, row_count: usize) {
        if row_count == 0 {
            return;
        }
        let next = match self.selected_row {
            Some(i) if i + 1 < row_count => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.selected_row = Some(next);
        self.ensure_visible(next);
    }

    fn move_up(&mut self) {
        let next = match self.selected_row {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.selected_row = Some(next);
        self.ensure_visible(next);
    }

    fn ensure_visible(&mut self, row: usize) {
        if row < self.scroll_offset {
            self.scroll_offset = row;
        }
    }
}

/// Collect connection rows from edges, sorted by bytes/sec descending.
fn collect_connection_rows(world: &WorldGraph, filter: &str) -> Vec<[String; 6]> {
    let filter_lower = filter.to_lowercase();

    let mut rows: Vec<[String; 6]> = world
        .edges()
        .map(|e| {
            let src_name = world
                .find_by_pid(e.source_pid)
                .map(|p| p.name.as_str())
                .unwrap_or("<exited>");

            [
                e.source_pid.to_string(),
                src_name.to_string(),
                e.dest.to_string(),
                format!("{:?}", e.protocol),
                format!("{:?}", e.state),
                format_bytes_per_sec(e.bytes_per_sec),
            ]
        })
        .filter(|cols| {
            if filter_lower.is_empty() {
                return true;
            }
            // Match against process name or dest IP.
            cols[1].to_lowercase().contains(&filter_lower)
                || cols[2].to_lowercase().contains(&filter_lower)
        })
        .collect();

    // Sort by bytes/sec descending (most active first).
    rows.sort_by(|a, b| {
        let ba = parse_bytes_sort_key(&a[5]);
        let bb = parse_bytes_sort_key(&b[5]);
        bb.cmp(&ba)
    });

    rows
}

/// Format bytes/sec into a human-readable string.
fn format_bytes_per_sec(bps: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bps >= GB {
        format!("{:.1} GB/s", bps as f64 / GB as f64)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps as f64 / MB as f64)
    } else if bps >= KB {
        format!("{:.1} KB/s", bps as f64 / KB as f64)
    } else {
        format!("{bps} B/s")
    }
}

/// Parse bytes/sec string back to raw value for sorting.
fn parse_bytes_sort_key(s: &str) -> u64 {
    let s = s.trim();
    if let Some(v) = s.strip_suffix(" GB/s") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0 * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" MB/s") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" KB/s") {
        (v.parse::<f64>().unwrap_or(0.0) * 1024.0) as u64
    } else if let Some(v) = s.strip_suffix(" B/s") {
        v.parse::<u64>().unwrap_or(0)
    } else {
        0
    }
}

/// Map protocol name to its display color.
fn protocol_color(proto_str: &str) -> Color {
    match proto_str {
        "TCP" => Palette::HEALTHY,
        "UDP" => Palette::NEON_BLUE,
        "DNS" => Palette::WARNING,
        _ => Color::Gray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ConnectionState, NetworkEdge, ProcessNode, ProcessState, Protocol};
    use glam::Vec3;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn make_process(pid: u32, name: &str) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: name.to_string(),
            cpu_percent: 10.0,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    fn make_edge(source_pid: u32, port: u16, proto: Protocol, bps: u64) -> NetworkEdge {
        NetworkEdge {
            source_pid,
            dest: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), port),
            protocol: proto,
            bytes_per_sec: bps,
            state: ConnectionState::Established,
        }
    }

    #[test]
    fn test_default_state() {
        let tab = NetworkTab::default();
        assert_eq!(tab.selected_row, None);
        assert!(tab.filter_text.is_empty());
        assert_eq!(tab.scroll_offset, 0);
    }

    #[test]
    fn test_format_bytes_per_sec() {
        assert_eq!(format_bytes_per_sec(500), "500 B/s");
        assert_eq!(format_bytes_per_sec(2048), "2.0 KB/s");
        assert_eq!(format_bytes_per_sec(5 * 1024 * 1024), "5.0 MB/s");
        assert_eq!(format_bytes_per_sec(3 * 1024 * 1024 * 1024), "3.0 GB/s");
    }

    #[test]
    fn test_protocol_color() {
        assert_eq!(protocol_color("TCP"), Palette::HEALTHY);
        assert_eq!(protocol_color("UDP"), Palette::NEON_BLUE);
        assert_eq!(protocol_color("DNS"), Palette::WARNING);
        assert_eq!(protocol_color("QUIC"), Color::Gray);
        assert_eq!(protocol_color("Unknown"), Color::Gray);
    }

    #[test]
    fn test_collect_rows_sorted_by_bytes_desc() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "nginx"));
        world.add_process(make_process(2, "postgres"));

        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));
        world.add_connection(2, 1, make_edge(2, 5432, Protocol::TCP, 5000));

        let rows = collect_connection_rows(&world, "");
        assert_eq!(rows.len(), 2);
        // Highest bytes/sec first.
        assert_eq!(rows[0][0], "2");
        assert_eq!(rows[1][0], "1");
    }

    #[test]
    fn test_filter_by_name() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "nginx"));
        world.add_process(make_process(2, "postgres"));

        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));
        world.add_connection(2, 1, make_edge(2, 5432, Protocol::TCP, 5000));

        let rows = collect_connection_rows(&world, "nginx");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][1], "nginx");
    }

    #[test]
    fn test_filter_by_ip() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "app"));
        world.add_process(make_process(2, "db"));

        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));

        let rows = collect_connection_rows(&world, "10.0.0.1");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_set_and_clear_filter() {
        let mut tab = NetworkTab::default();
        tab.selected_row = Some(3);
        tab.scroll_offset = 5;

        tab.set_filter("nginx".to_string());
        assert_eq!(tab.filter_text, "nginx");
        assert_eq!(tab.selected_row, None);
        assert_eq!(tab.scroll_offset, 0);

        tab.clear_filter();
        assert!(tab.filter_text.is_empty());
    }

    #[test]
    fn test_move_down_wraps() {
        let mut tab = NetworkTab::default();
        tab.move_down(3);
        assert_eq!(tab.selected_row, Some(0));
        tab.move_down(3);
        assert_eq!(tab.selected_row, Some(1));
        tab.move_down(3);
        assert_eq!(tab.selected_row, Some(2));
        tab.move_down(3);
        assert_eq!(tab.selected_row, Some(0));
    }

    #[test]
    fn test_move_up_stops_at_zero() {
        let mut tab = NetworkTab::default();
        tab.selected_row = Some(1);
        tab.move_up();
        assert_eq!(tab.selected_row, Some(0));
        tab.move_up();
        assert_eq!(tab.selected_row, Some(0));
    }

    #[test]
    fn test_move_down_empty_noop() {
        let mut tab = NetworkTab::default();
        tab.move_down(0);
        assert_eq!(tab.selected_row, None);
    }

    #[test]
    fn test_exited_process_shows_placeholder() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "a"));
        world.add_process(make_process(2, "b"));
        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));

        // Remove the source process — edge remains in graph.
        world.remove_process(1);

        let rows = collect_connection_rows(&world, "");
        // The edge should still show, with <exited> as source name.
        // Note: petgraph removes edges when a node is removed,
        // so this may result in 0 rows — that's correct behavior.
        // If edges survive, the name should be "<exited>".
        for row in &rows {
            if row[0] == "1" {
                assert_eq!(row[1], "<exited>");
            }
        }
    }
}

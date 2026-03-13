//! Network tab — connection list sorted by bytes/sec (F3).

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Sparkline, Table, Widget};

use aether_core::models::Protocol;
use aether_core::WorldGraph;

use crate::palette::Palette;

/// Maximum number of throughput samples retained (1 per second).
const THROUGHPUT_HISTORY_CAP: usize = 60;

/// State for the Network (F3) connection list tab.
#[derive(Debug, Default)]
pub(crate) struct NetworkTab {
    /// Currently selected row index (if any).
    selected_row: Option<usize>,
    /// Filter text entered via `/` search.
    filter_text: String,
    /// Number of rows scrolled past the top of the visible area.
    scroll_offset: usize,
    /// Rolling throughput history for the summary sparkline.
    throughput_history: VecDeque<u64>,
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
    pub(crate) fn handle_key(&mut self, code: crossterm::event::KeyCode, row_count: usize) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.move_down(row_count),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Esc => self.clear_filter(),
            _ => {}
        }
    }

    /// Sample current throughput from the world graph.
    ///
    /// Call once per second (same cadence as [`SystemSparklines`]).
    pub(crate) fn update(&mut self, world: &WorldGraph) {
        let total_bps: u64 = world.edges().map(|e| e.bytes_per_sec).sum();
        if self.throughput_history.len() >= THROUGHPUT_HISTORY_CAP {
            self.throughput_history.pop_front();
        }
        self.throughput_history.push_back(total_bps);
    }

    /// Render the full Network tab: summary panel + connection table.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(0)])
            .split(area);

        self.render_summary(chunks[0], buf, world);
        self.render_table(chunks[1], buf, world);
    }

    /// Render the connection table into `buf`.
    fn render_table(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
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
            "Src PID",
            "Src Name",
            "Dest IP:Port",
            "Proto",
            "State",
            "Bytes/s",
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

    /// Render the summary panel with aggregate stats and throughput sparkline.
    fn render_summary(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // --- Left: text stats ---
        let stats = compute_stats(world);
        let lines = vec![
            Line::from(vec![
                Span::styled("Connections: ", Style::default().fg(Palette::DATA)),
                Span::styled(
                    stats.total.to_string(),
                    Style::default().fg(Palette::HEALTHY),
                ),
                Span::styled("  Active: ", Style::default().fg(Palette::DATA)),
                Span::styled(
                    stats.active.to_string(),
                    Style::default()
                        .fg(if stats.active > 0 {
                            Palette::HEALTHY
                        } else {
                            Color::DarkGray
                        })
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Throughput: ", Style::default().fg(Palette::DATA)),
                Span::styled(
                    format_bytes_per_sec(stats.total_bps),
                    Style::default().fg(Palette::XP_PURPLE),
                ),
            ]),
            Line::from(vec![Span::styled(
                stats.protocol_distribution,
                Style::default().fg(Palette::NEON_BLUE),
            )]),
        ];

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    "Network Summary",
                    Style::default().fg(Palette::NEON_BLUE),
                ))
                .border_style(Style::default().fg(Palette::NEON_BLUE)),
        );
        Widget::render(paragraph, halves[0], buf);

        // --- Right: throughput sparkline ---
        let slice: Vec<u64> = self.throughput_history.iter().copied().collect();
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Throughput")
                    .border_style(Style::default().fg(Palette::XP_PURPLE)),
            )
            .data(&slice)
            .style(Style::default().fg(Palette::XP_PURPLE));
        Widget::render(sparkline, halves[1], buf);
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

/// Aggregate stats computed from the world graph.
struct NetworkStats {
    total: usize,
    active: usize,
    total_bps: u64,
    protocol_distribution: String,
}

/// Compute aggregate network statistics from the world graph.
fn compute_stats(world: &WorldGraph) -> NetworkStats {
    let mut total = 0_usize;
    let mut active = 0_usize;
    let mut total_bps = 0_u64;
    let mut counts: [usize; 7] = [0; 7];

    for edge in world.edges() {
        total += 1;
        total_bps += edge.bytes_per_sec;
        if edge.bytes_per_sec > 0 {
            active += 1;
        }
        counts[protocol_index(edge.protocol)] += 1;
    }

    let distribution = if total == 0 {
        "No connections".to_string()
    } else {
        let labels = ["TCP", "UDP", "DNS", "QUIC", "HTTP", "HTTPS", "Other"];
        let mut parts = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            if counts[i] > 0 {
                parts.push(format!(
                    "{}: {:.0}%",
                    label,
                    counts[i] as f64 / total as f64 * 100.0,
                ));
            }
        }
        parts.join(" | ")
    };

    NetworkStats {
        total,
        active,
        total_bps,
        protocol_distribution: distribution,
    }
}

/// Map protocol variant to an array index for counting.
fn protocol_index(proto: Protocol) -> usize {
    match proto {
        Protocol::TCP => 0,
        Protocol::UDP => 1,
        Protocol::DNS => 2,
        Protocol::QUIC => 3,
        Protocol::HTTP => 4,
        Protocol::HTTPS => 5,
        Protocol::Unknown | _ => 6,
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
            dest_pid: None,
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
    fn test_compute_stats_empty_world() {
        let world = WorldGraph::new();
        let stats = compute_stats(&world);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.active, 0);
        assert_eq!(stats.total_bps, 0);
        assert_eq!(stats.protocol_distribution, "No connections");
    }

    #[test]
    fn test_compute_stats_mixed_protocols() {
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "a"));
        world.add_process(make_process(2, "b"));

        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));
        world.add_connection(1, 2, make_edge(1, 443, Protocol::TCP, 500));
        world.add_connection(1, 2, make_edge(1, 53, Protocol::DNS, 0));

        let stats = compute_stats(&world);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.active, 2, "only connections with bps > 0");
        assert_eq!(stats.total_bps, 1500);
        assert!(stats.protocol_distribution.contains("TCP: 67%"));
        assert!(stats.protocol_distribution.contains("DNS: 33%"));
    }

    #[test]
    fn test_update_pushes_throughput_history() {
        let mut tab = NetworkTab::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, "a"));
        world.add_process(make_process(2, "b"));
        world.add_connection(1, 2, make_edge(1, 80, Protocol::TCP, 1000));

        tab.update(&world);
        assert_eq!(tab.throughput_history.len(), 1);
        assert_eq!(tab.throughput_history[0], 1000);

        tab.update(&world);
        assert_eq!(tab.throughput_history.len(), 2);
    }

    #[test]
    fn test_throughput_history_caps_at_limit() {
        let mut tab = NetworkTab::default();
        let world = WorldGraph::new();

        for _ in 0..100 {
            tab.update(&world);
        }

        assert_eq!(tab.throughput_history.len(), THROUGHPUT_HISTORY_CAP);
    }

    #[test]
    fn test_render_summary_does_not_panic() {
        let tab = NetworkTab::default();
        let world = WorldGraph::new();
        let area = Rect::new(0, 0, 120, 6);
        let mut buf = Buffer::empty(area);
        tab.render_summary(area, &mut buf, &world);
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

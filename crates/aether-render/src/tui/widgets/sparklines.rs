//! Rolling sparkline widgets for CPU, RAM, and Network metrics.
//!
//! Displays three horizontal sparklines (one per metric) that show 60-sample
//! history updated once per second from the [`WorldGraph`].

use std::collections::VecDeque;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Sparkline, Widget};

use aether_core::WorldGraph;

use crate::palette::Palette;

/// Maximum number of samples retained in each history buffer (1 per second).
const HISTORY_CAP: usize = 60;

/// Rolling sparkline state for CPU, RAM, and Network metrics.
///
/// Call [`update`](Self::update) once per second with the current [`WorldGraph`]
/// to push new samples, then [`render`](Self::render) each frame.
#[derive(Debug)]
pub(crate) struct SystemSparklines {
    /// Average CPU usage across all processes (0–100).
    cpu_history: VecDeque<u64>,
    /// Total RAM usage in megabytes.
    ram_history: VecDeque<u64>,
    /// Total network throughput in bytes per second.
    net_history: VecDeque<u64>,
}

impl Default for SystemSparklines {
    fn default() -> Self {
        Self {
            cpu_history: VecDeque::with_capacity(HISTORY_CAP),
            ram_history: VecDeque::with_capacity(HISTORY_CAP),
            net_history: VecDeque::with_capacity(HISTORY_CAP),
        }
    }
}

impl SystemSparklines {
    /// Sample current metrics from the world graph and push to history.
    ///
    /// Computes average CPU across all processes, total RAM (in MB), and
    /// total network bytes/sec from edges.
    pub(crate) fn update(&mut self, world: &WorldGraph) {
        let (cpu_sum, ram_sum, count) = world.processes().fold(
            (0.0_f32, 0_u64, 0_u32),
            |(cpu, ram, n), p| (cpu + p.cpu_percent, ram + p.mem_bytes, n + 1),
        );

        let avg_cpu = if count > 0 {
            (cpu_sum / count as f32) as u64
        } else {
            0
        };
        let ram_mb = ram_sum / (1024 * 1024);
        let net_bps: u64 = world.edges().map(|e| e.bytes_per_sec).sum();

        push_capped(&mut self.cpu_history, avg_cpu);
        push_capped(&mut self.ram_history, ram_mb);
        push_capped(&mut self.net_history, net_bps);
    }

    /// Render three sparklines in a horizontal row spanning `area`.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(area);

        render_one(&self.cpu_history, "CPU %", Palette::HEALTHY, chunks[0], buf);
        render_one(&self.ram_history, "RAM", Palette::NEON_BLUE, chunks[1], buf);
        render_one(&self.net_history, "NET", Palette::XP_PURPLE, chunks[2], buf);
    }
}

/// Render a single sparkline block.
fn render_one(
    data: &VecDeque<u64>,
    label: &str,
    color: ratatui::style::Color,
    area: Rect,
    buf: &mut Buffer,
) {
    let slice: Vec<u64> = data.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(label)
                .border_style(Style::default().fg(color)),
        )
        .data(&slice)
        .style(Style::default().fg(color));

    Widget::render(sparkline, area, buf);
}

/// Push a value into a capped deque, dropping the oldest sample if full.
fn push_capped(deque: &mut VecDeque<u64>, value: u64) {
    if deque.len() >= HISTORY_CAP {
        deque.pop_front();
    }
    deque.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState};
    use glam::Vec3;

    fn make_process(pid: u32, cpu: f32, mem: u64) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_update_pushes_correct_averages() {
        let mut sparklines = SystemSparklines::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 40.0, 2 * 1024 * 1024));
        world.add_process(make_process(2, 60.0, 4 * 1024 * 1024));

        sparklines.update(&world);

        assert_eq!(sparklines.cpu_history.len(), 1);
        assert_eq!(sparklines.cpu_history[0], 50); // avg(40, 60)
        assert_eq!(sparklines.ram_history[0], 6); // (2+4) MB
        assert_eq!(sparklines.net_history[0], 0); // no edges
    }

    #[test]
    fn test_update_empty_world_pushes_zeros() {
        let mut sparklines = SystemSparklines::default();
        let world = WorldGraph::new();

        sparklines.update(&world);

        assert_eq!(sparklines.cpu_history[0], 0);
        assert_eq!(sparklines.ram_history[0], 0);
        assert_eq!(sparklines.net_history[0], 0);
    }

    #[test]
    fn test_history_caps_at_60() {
        let mut sparklines = SystemSparklines::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 10.0, 1024 * 1024));

        for _ in 0..100 {
            sparklines.update(&world);
        }

        assert_eq!(sparklines.cpu_history.len(), HISTORY_CAP);
        assert_eq!(sparklines.ram_history.len(), HISTORY_CAP);
        assert_eq!(sparklines.net_history.len(), HISTORY_CAP);
    }

    #[test]
    fn test_render_does_not_panic() {
        let mut sparklines = SystemSparklines::default();
        let mut world = WorldGraph::new();
        world.add_process(make_process(1, 50.0, 1024 * 1024));
        sparklines.update(&world);

        let area = Rect::new(0, 0, 120, 5);
        let mut buf = Buffer::empty(area);
        sparklines.render(area, &mut buf);
    }

    #[test]
    fn test_push_capped_evicts_oldest() {
        let mut deque = VecDeque::with_capacity(HISTORY_CAP);
        for i in 0..HISTORY_CAP as u64 {
            push_capped(&mut deque, i);
        }
        assert_eq!(deque.len(), HISTORY_CAP);
        assert_eq!(deque[0], 0);

        push_capped(&mut deque, 999);
        assert_eq!(deque.len(), HISTORY_CAP);
        assert_eq!(deque[0], 1); // oldest evicted
        assert_eq!(*deque.back().unwrap(), 999);
    }
}

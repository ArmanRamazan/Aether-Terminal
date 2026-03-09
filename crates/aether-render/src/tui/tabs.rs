//! Tab bar and status bar rendering widgets.

use aether_core::WorldGraph;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::app::Tab;

/// Renders the tab bar showing all tabs with the active one highlighted.
///
/// Each tab is displayed as `[Fn Label]` with the active tab in cyan+bold.
pub(crate) fn render_tab_bar(area: Rect, buf: &mut Buffer, active: Tab) {
    let normal = Style::default().fg(Color::DarkGray);
    let highlight = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let mut x = area.x;
    for tab in Tab::ALL {
        if x >= area.right() {
            break;
        }
        let label = tab.label();
        let style = if tab == active { highlight } else { normal };

        let available = (area.right() - x) as usize;
        for (i, ch) in label.chars().enumerate() {
            if i >= available {
                break;
            }
            buf[(x + i as u16, area.y)].set_char(ch).set_style(style);
        }
        x += label.len() as u16 + 1; // +1 for spacing
    }
}

/// Renders the bottom status bar with live system statistics.
///
/// Format: `Aether Terminal v0.1 | Processes: N | CPU: X% | RAM: Y.YGB | Rank: Novice | XP: 0`
pub(crate) fn render_status_bar(area: Rect, buf: &mut Buffer, world: &WorldGraph) {
    let process_count = world.process_count();

    let avg_cpu = if process_count > 0 {
        let total: f32 = world.processes().map(|p| p.cpu_percent).sum();
        total / process_count as f32
    } else {
        0.0
    };

    let total_ram_bytes: u64 = world.processes().map(|p| p.mem_bytes).sum();
    let total_ram_gb = total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let text = format!(
        "Aether Terminal v0.1 | Processes: {} | CPU: {:.0}% | RAM: {:.1}GB | Rank: Novice | XP: 0",
        process_count, avg_cpu, total_ram_gb
    );

    let style = Style::default()
        .fg(Color::White)
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    // Fill the entire bar with background color.
    for x in area.x..area.right() {
        buf[(x, area.y)].set_style(style);
    }

    // Write text characters.
    let available = area.width as usize;
    for (i, ch) in text.chars().enumerate() {
        if i >= available {
            break;
        }
        buf[(area.x + i as u16, area.y)]
            .set_char(ch)
            .set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_render_tab_bar_highlights_active() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        render_tab_bar(area, &mut buf, Tab::World3D);

        // The active tab label should appear in the buffer.
        let content: String = (0..80)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("World 3D [F2]"));

        // Active tab cell should have cyan foreground.
        let world3d_start = content.find("World 3D [F2]").expect("tab label present");
        let cell = &buf[(world3d_start as u16, 0)];
        assert_eq!(cell.fg, Color::Cyan);
    }

    #[test]
    fn test_render_tab_bar_non_active_dimmed() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        render_tab_bar(area, &mut buf, Tab::Overview);

        let content: String = (0..80)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        // Network tab should be present but not highlighted.
        let net_start = content.find("Network [F3]").expect("tab label present");
        let cell = &buf[(net_start as u16, 0)];
        assert_eq!(cell.fg, Color::DarkGray);
    }

    #[test]
    fn test_render_status_bar_empty_world() {
        let area = Rect::new(0, 0, 100, 1);
        let mut buf = Buffer::empty(area);
        let world = WorldGraph::new();

        render_status_bar(area, &mut buf, &world);

        let content: String = (0..100)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("Processes: 0"));
        assert!(content.contains("CPU: 0%"));
    }

    #[test]
    fn test_render_status_bar_with_processes() {
        use aether_core::models::{ProcessNode, ProcessState};
        use glam::Vec3;

        let area = Rect::new(0, 0, 100, 1);
        let mut buf = Buffer::empty(area);
        let mut world = WorldGraph::new();

        world.add_process(ProcessNode {
            pid: 1,
            ppid: 0,
            name: "a".to_string(),
            cpu_percent: 50.0,
            mem_bytes: 1_073_741_824, // 1 GB
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        });
        world.add_process(ProcessNode {
            pid: 2,
            ppid: 0,
            name: "b".to_string(),
            cpu_percent: 30.0,
            mem_bytes: 1_073_741_824, // 1 GB
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        });

        render_status_bar(area, &mut buf, &world);

        let content: String = (0..100)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(content.contains("Processes: 2"));
        assert!(content.contains("CPU: 40%")); // avg of 50 and 30
        assert!(content.contains("RAM: 2.0GB"));
    }

    #[test]
    fn test_render_status_bar_background_fill() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        let world = WorldGraph::new();

        render_status_bar(area, &mut buf, &world);

        // Every cell should have the dark gray background.
        for x in 0..80 {
            assert_eq!(buf[(x, 0)].bg, Color::DarkGray);
        }
    }
}

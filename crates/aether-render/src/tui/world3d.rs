//! World 3D tab — interactive 3D process graph viewport (F2).
//!
//! Renders the process graph using [`SceneRenderer`] and displays it as
//! Braille characters with per-cell colors. Supports camera rotation,
//! zoom, auto-rotate, and node label overlays.

use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use aether_core::WorldGraph;

use crate::engine::projection::project_point;
use crate::engine::scene::SceneRenderer;
use crate::palette::Palette;

/// Rotation step per key press in radians.
const ROTATE_STEP: f32 = 0.1;

/// Zoom step per key press.
const ZOOM_STEP: f32 = 1.0;

/// Interactive 3D process graph viewport.
///
/// Owns a [`SceneRenderer`] and handles camera input. Renders the graph
/// as Braille characters with node label overlays.
pub(crate) struct World3DTab {
    scene: SceneRenderer,
    auto_rotate: bool,
    selected_pid: Option<u32>,
    last_width: u16,
    last_height: u16,
}

impl World3DTab {
    /// Create a new tab with a 1×1 scene (resized on first render).
    pub(crate) fn new() -> Self {
        Self {
            scene: SceneRenderer::new(1, 1),
            auto_rotate: false,
            selected_pid: None,
            last_width: 0,
            last_height: 0,
        }
    }

    /// Render the 3D viewport into the given area.
    pub(crate) fn render(&mut self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        if area.width < 2 || area.height < 2 {
            return;
        }

        // Resize scene if terminal area changed.
        let content_w = area.width;
        let content_h = area.height;
        if content_w != self.last_width || content_h != self.last_height {
            self.scene.resize(content_w as usize, content_h as usize);
            self.last_width = content_w;
            self.last_height = content_h;
        }

        // Auto-rotate camera.
        if self.auto_rotate {
            self.scene.camera_mut().rotate(0.02, 0.0);
        }

        let lines = self.scene.render(world);

        // Draw Braille lines into the ratatui buffer.
        for (row_idx, (line, colors)) in lines.iter().enumerate() {
            let y = area.y + row_idx as u16;
            if y >= area.bottom() {
                break;
            }
            for (col_idx, (ch, &color)) in line.chars().zip(colors.iter()).enumerate() {
                let x = area.x + col_idx as u16;
                if x >= area.right() {
                    break;
                }
                buf[(x, y)]
                    .set_char(ch)
                    .set_style(Style::default().fg(color));
            }
        }

        // Overlay node labels at projected positions.
        self.render_labels(area, buf, world);
    }

    /// Draw process name labels at projected node positions.
    fn render_labels(&self, area: Rect, buf: &mut Buffer, world: &WorldGraph) {
        let screen_w = area.width as u32;
        let screen_h = area.height as u32;
        if screen_w == 0 || screen_h == 0 {
            return;
        }

        let aspect = (screen_w as f32 * 2.0) / (screen_h as f32 * 4.0);
        let cam = self.scene.camera_ref();
        let view = cam.view_matrix();
        let proj = cam.projection_matrix(aspect);

        for node in world.processes() {
            let Some(pos) = self.scene.layout_position(node.pid) else {
                continue;
            };
            let Some(pt) = project_point(pos, &view, &proj, screen_w, screen_h) else {
                continue;
            };

            let label_x = pt.x as u16;
            let label_y = pt.y as u16;

            // Check bounds: label must fit within area.
            if label_y >= area.height || label_x >= area.width {
                continue;
            }

            let abs_x = area.x + label_x;
            let abs_y = area.y + label_y;
            let available = (area.right() - abs_x) as usize;
            let label = &node.name;

            if available < 2 || label.is_empty() {
                continue;
            }

            let style = Style::default().fg(Palette::DATA);
            for (i, ch) in label.chars().take(available).enumerate() {
                buf[(abs_x + i as u16, abs_y)].set_char(ch).set_style(style);
            }
        }
    }

    /// Handle a World3D-specific key. Returns `true` if the key was consumed.
    pub(crate) fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            // WASD rotation.
            KeyCode::Char('w') => self.scene.camera_mut().rotate(0.0, ROTATE_STEP),
            KeyCode::Char('s') => self.scene.camera_mut().rotate(0.0, -ROTATE_STEP),
            KeyCode::Char('a') => self.scene.camera_mut().rotate(-ROTATE_STEP, 0.0),
            KeyCode::Char('d') => self.scene.camera_mut().rotate(ROTATE_STEP, 0.0),
            // Zoom.
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.scene.camera_mut().zoom(-ZOOM_STEP);
            }
            KeyCode::Char('-') => self.scene.camera_mut().zoom(ZOOM_STEP),
            // Toggle auto-rotate.
            KeyCode::Char(' ') => self.auto_rotate = !self.auto_rotate,
            // Reset camera.
            KeyCode::Char('r') => {
                self.scene.camera_mut().reset();
                self.auto_rotate = false;
            }
            // Center on selected node.
            KeyCode::Char('c') => self.center_on_selected(),
            _ => return false,
        }
        true
    }

    /// Handle navigation direction (from hjkl/arrow keys).
    pub(crate) fn navigate(&mut self, dir: super::input::Direction) {
        use super::input::Direction;
        match dir {
            Direction::Up => self.scene.camera_mut().rotate(0.0, ROTATE_STEP),
            Direction::Down => self.scene.camera_mut().rotate(0.0, -ROTATE_STEP),
            Direction::Left => self.scene.camera_mut().rotate(-ROTATE_STEP, 0.0),
            Direction::Right => self.scene.camera_mut().rotate(ROTATE_STEP, 0.0),
        }
    }

    /// Center camera on the selected node, or on the centroid of all nodes.
    fn center_on_selected(&mut self) {
        if let Some(pid) = self.selected_pid {
            if let Some(pos) = self.scene.layout_position(pid) {
                self.scene.camera_mut().center = pos;
                return;
            }
        }
        // Fallback: center on all node positions.
        let positions: Vec<_> = self.scene.all_layout_positions();
        self.scene.camera_mut().auto_center(&positions);
    }
}

impl Default for World3DTab {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::camera::OrbitalCamera;
    use aether_core::models::{ProcessNode, ProcessState};
    use crossterm::event::KeyCode;
    use glam::Vec3;

    fn make_node(pid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 0,
            name: format!("proc_{pid}"),
            cpu_percent: 10.0,
            mem_bytes: 1024,
            state: ProcessState::Running,
            hp: 80.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_new_defaults() {
        let tab = World3DTab::new();
        assert!(!tab.auto_rotate);
        assert_eq!(tab.selected_pid, None);
    }

    #[test]
    fn test_handle_key_wasd_consumed() {
        let mut tab = World3DTab::new();
        assert!(tab.handle_key(KeyCode::Char('w')));
        assert!(tab.handle_key(KeyCode::Char('a')));
        assert!(tab.handle_key(KeyCode::Char('s')));
        assert!(tab.handle_key(KeyCode::Char('d')));
    }

    #[test]
    fn test_handle_key_zoom_consumed() {
        let mut tab = World3DTab::new();
        assert!(tab.handle_key(KeyCode::Char('+')));
        assert!(tab.handle_key(KeyCode::Char('=')));
        assert!(tab.handle_key(KeyCode::Char('-')));
    }

    #[test]
    fn test_handle_key_space_toggles_auto_rotate() {
        let mut tab = World3DTab::new();
        assert!(!tab.auto_rotate);
        tab.handle_key(KeyCode::Char(' '));
        assert!(tab.auto_rotate);
        tab.handle_key(KeyCode::Char(' '));
        assert!(!tab.auto_rotate);
    }

    #[test]
    fn test_handle_key_r_resets_camera() {
        let mut tab = World3DTab::new();
        tab.scene.camera_mut().rotate(1.0, 0.5);
        tab.auto_rotate = true;
        tab.handle_key(KeyCode::Char('r'));
        assert!(!tab.auto_rotate);
        let cam = tab.scene.camera_ref();
        assert!((cam.yaw - OrbitalCamera::default().yaw).abs() < 1e-6);
    }

    #[test]
    fn test_handle_key_unknown_not_consumed() {
        let mut tab = World3DTab::new();
        assert!(!tab.handle_key(KeyCode::Char('z')));
        assert!(!tab.handle_key(KeyCode::Char('q')));
    }

    #[test]
    fn test_navigate_rotates_camera() {
        use super::super::input::Direction;
        let mut tab = World3DTab::new();
        let initial_yaw = tab.scene.camera_ref().yaw;
        tab.navigate(Direction::Right);
        assert!(
            (tab.scene.camera_ref().yaw - (initial_yaw + ROTATE_STEP)).abs() < 1e-6,
            "right navigation should increase yaw"
        );
    }

    #[test]
    fn test_render_empty_graph_no_panic() {
        let mut tab = World3DTab::new();
        let graph = WorldGraph::new();
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &graph);
    }

    #[test]
    fn test_render_with_node_no_panic() {
        let mut tab = World3DTab::new();
        let mut graph = WorldGraph::new();
        graph.add_process(make_node(1));
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &graph);
    }

    #[test]
    fn test_resize_on_area_change() {
        let mut tab = World3DTab::new();
        let graph = WorldGraph::new();

        let area1 = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area1);
        tab.render(area1, &mut buf, &graph);
        assert_eq!(tab.last_width, 40);
        assert_eq!(tab.last_height, 20);

        let area2 = Rect::new(0, 0, 60, 30);
        let mut buf2 = Buffer::empty(area2);
        tab.render(area2, &mut buf2, &graph);
        assert_eq!(tab.last_width, 60);
        assert_eq!(tab.last_height, 30);
    }

    #[test]
    fn test_render_tiny_area_no_panic() {
        let mut tab = World3DTab::new();
        let graph = WorldGraph::new();
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        tab.render(area, &mut buf, &graph);
    }

    #[test]
    fn test_center_on_selected_no_selection_uses_centroid() {
        let mut tab = World3DTab::new();
        // Just verify it doesn't panic with no nodes.
        tab.center_on_selected();
    }
}

//! Scene render pipeline: orchestrates camera, layout, canvas, z-buffer,
//! and rasterizer into a complete 3D-to-Braille rendering pass.

use glam::Vec3;
use ratatui::style::Color;

use aether_core::graph::WorldGraph;
use aether_core::models::ProcessState;

use crate::braille::BrailleCanvas;
use crate::effects::{FlowEffect, PulseEffect};
use crate::engine::camera::OrbitalCamera;
use crate::engine::layout::ForceLayout;
use crate::engine::projection::{project_point, ScreenPoint};
use crate::engine::rasterizer::{draw_line, ZBuffer};
use crate::engine::shading::{shade_point, sphere_normal};
use crate::palette::{color_for_hp, Palette};

/// Light direction for Phong shading (normalized, pointing upper-right-forward).
const LIGHT_DIR: Vec3 = Vec3::new(0.4, 0.7, 0.5);

/// Minimum node radius in Braille pixels.
const MIN_RADIUS: f32 = 1.0;

/// Maximum node radius in Braille pixels.
const MAX_RADIUS: f32 = 5.0;

/// HP threshold below which a node is considered critical (20%).
const CRITICAL_HP_THRESHOLD: f32 = 20.0;

/// Extra radius in Braille pixels for the bloom outer circle on critical nodes.
const BLOOM_RADIUS_OFFSET: f32 = 3.0;

/// Zombie process flicker period in seconds (visible/invisible toggle).
const ZOMBIE_FLICKER_PERIOD: f32 = 0.5;

/// Average two colors channel-by-channel.
fn blend_colors(c1: Color, c2: Color) -> Color {
    match (c1, c2) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            Color::Rgb((r1 / 2) + (r2 / 2), (g1 / 2) + (g2 / 2), (b1 / 2) + (b2 / 2))
        }
        _ => c1,
    }
}

/// Dim a color by halving each RGB channel (simulated lower alpha for bloom).
fn dim_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
        other => other,
    }
}

/// Number of flow dots per edge for high-traffic connections (≥1 MB/s).
const HIGH_TRAFFIC_THRESHOLD: u64 = 1_000_000;

/// Orchestrates the full 3D render pipeline for a process graph.
///
/// Combines camera, force layout, Braille canvas, and z-buffer to produce
/// lines of Braille text with per-character colors for ratatui rendering.
pub struct SceneRenderer {
    camera: OrbitalCamera,
    layout: ForceLayout,
    canvas: BrailleCanvas,
    zbuffer: ZBuffer,
    pulse: PulseEffect,
    flow: FlowEffect,
}

impl SceneRenderer {
    /// Create a scene renderer for the given terminal cell dimensions.
    pub fn new(cell_width: usize, cell_height: usize) -> Self {
        let canvas = BrailleCanvas::new(cell_width, cell_height);
        let zbuffer = ZBuffer::new(canvas.pixel_width(), canvas.pixel_height());
        Self {
            camera: OrbitalCamera::default(),
            layout: ForceLayout::new(),
            canvas,
            zbuffer,
            pulse: PulseEffect::new(),
            flow: FlowEffect::new(),
        }
    }

    /// Resize the canvas and z-buffer for new terminal dimensions.
    pub fn resize(&mut self, cell_width: usize, cell_height: usize) {
        self.canvas = BrailleCanvas::new(cell_width, cell_height);
        self.zbuffer = ZBuffer::new(self.canvas.pixel_width(), self.canvas.pixel_height());
    }

    /// Render the process graph to Braille lines with per-character colors.
    ///
    /// `dt` is the elapsed time in seconds since the last frame, used to
    /// drive time-based effects like node pulsation.
    ///
    /// Performs a complete render pass: clear, layout sync, project, rasterize
    /// edges and nodes, apply shading, and convert to Braille output.
    pub fn render(&mut self, graph: &WorldGraph, dt: f32) -> Vec<(String, Vec<Color>)> {
        self.canvas.clear();
        self.zbuffer.clear();

        self.pulse.update(dt);
        self.flow.update(dt);
        self.layout.sync_with_graph(graph);
        self.layout.step(graph);

        // Project to cell space; rasterizer converts to Braille subpixels (×2, ×4).
        let screen_w = self.canvas.cell_width() as u32;
        let screen_h = self.canvas.cell_height() as u32;

        if screen_w == 0 || screen_h == 0 {
            return Vec::new();
        }

        // Aspect ratio uses Braille pixel dimensions (2×4 per cell) for correct proportions.
        let aspect = self.canvas.pixel_width() as f32 / self.canvas.pixel_height() as f32;
        let view = self.camera.view_matrix();
        let proj = self.camera.projection_matrix(aspect);

        self.render_edges(graph, &view, &proj, screen_w, screen_h);
        self.render_nodes(graph, &view, &proj, screen_w, screen_h);

        self.canvas.to_lines()
    }

    /// Mutable access to the camera for input handling.
    pub fn camera_mut(&mut self) -> &mut OrbitalCamera {
        &mut self.camera
    }

    /// Read-only access to the camera for projection queries.
    pub fn camera_ref(&self) -> &OrbitalCamera {
        &self.camera
    }

    /// Get a node's layout position by PID.
    pub fn layout_position(&self, pid: u32) -> Option<glam::Vec3> {
        self.layout.get_position(pid)
    }

    /// Collect all layout positions (for auto-centering).
    pub fn all_layout_positions(&self) -> Vec<glam::Vec3> {
        self.layout.all_positions()
    }

    /// Project a node position through the camera to screen space.
    fn project_node(
        &self,
        pid: u32,
        view: &glam::Mat4,
        proj: &glam::Mat4,
        screen_w: u32,
        screen_h: u32,
    ) -> Option<ScreenPoint> {
        let pos = self.layout.get_position(pid)?;
        project_point(pos, view, proj, screen_w, screen_h)
    }

    /// Rasterize all edges as lines between projected node positions.
    ///
    /// Edge color is the blend of source and destination node HP colors.
    /// Active edges (bytes_per_sec > 0) get animated flow dots.
    fn render_edges(
        &mut self,
        graph: &WorldGraph,
        view: &glam::Mat4,
        proj: &glam::Mat4,
        screen_w: u32,
        screen_h: u32,
    ) {
        for (src_pid, dst_pid, edge) in graph.edge_pairs_with_data() {
            let Some(p0) = self.project_node(src_pid, view, proj, screen_w, screen_h) else {
                continue;
            };
            let Some(p1) = self.project_node(dst_pid, view, proj, screen_w, screen_h) else {
                continue;
            };
            let src_hp = graph.find_by_pid(src_pid).map_or(50.0, |n| n.hp);
            let dst_hp = graph.find_by_pid(dst_pid).map_or(50.0, |n| n.hp);
            let edge_color = blend_colors(color_for_hp(src_hp), color_for_hp(dst_hp));
            draw_line(&mut self.canvas, &mut self.zbuffer, p0, p1, edge_color);

            if edge.bytes_per_sec > 0 {
                self.render_flow_dots(p0, p1, edge.bytes_per_sec);
            }
        }
    }

    /// Draw animated flow dots along an edge to visualize data transfer.
    ///
    /// High-traffic edges (≥1 MB/s) get 3 evenly spaced dots; others get 1.
    fn render_flow_dots(&mut self, p0: ScreenPoint, p1: ScreenPoint, bytes_per_sec: u64) {
        let base_t = self.flow.flow_dot_position(bytes_per_sec);
        let dot_count = if bytes_per_sec >= HIGH_TRAFFIC_THRESHOLD {
            3
        } else {
            1
        };
        let spacing = 1.0 / dot_count as f32;

        for i in 0..dot_count {
            let t = (base_t + i as f32 * spacing).fract();
            let bx = (p0.x + (p1.x - p0.x) * t) * 2.0;
            let by = (p0.y + (p1.y - p0.y) * t) * 4.0;
            let px = bx as usize;
            let py = by as usize;
            let depth = p0.depth + (p1.depth - p0.depth) * t;

            if px < self.canvas.pixel_width()
                && py < self.canvas.pixel_height()
                && self.zbuffer.test_and_set(px, py, depth)
            {
                self.canvas.set_pixel_colored(px, py, Palette::DATA);
            }
        }
    }

    /// Rasterize all nodes as shaded filled circles.
    ///
    /// Critical nodes (HP < 20%) get an outer bloom circle. Zombie processes
    /// flicker on/off every 500ms.
    fn render_nodes(
        &mut self,
        graph: &WorldGraph,
        view: &glam::Mat4,
        proj: &glam::Mat4,
        screen_w: u32,
        screen_h: u32,
    ) {
        let light = LIGHT_DIR.normalize();

        for node in graph.processes() {
            // Zombie flicker: skip rendering every other 500ms period.
            if node.state == ProcessState::Zombie {
                let period_index = (self.pulse.time() / ZOMBIE_FLICKER_PERIOD) as u32;
                if period_index % 2 == 1 {
                    continue;
                }
            }

            let Some(screen_pt) = self.project_node(node.pid, view, proj, screen_w, screen_h)
            else {
                continue;
            };

            let base_radius = depth_to_radius(screen_pt.depth);
            let radius = self.pulse.pulse_radius(base_radius, node.cpu_percent);
            let base_color = color_for_hp(node.hp);

            // Critical bloom: outer circle with dimmed color.
            if node.hp < CRITICAL_HP_THRESHOLD {
                let bloom_color = dim_color(Palette::CRITICAL);
                self.render_shaded_circle(screen_pt, radius + BLOOM_RADIUS_OFFSET, bloom_color, light);
            }

            self.render_shaded_circle(screen_pt, radius, base_color, light);
        }
    }

    /// Draw a filled circle with per-pixel Phong shading for sphere appearance.
    fn render_shaded_circle(
        &mut self,
        center: ScreenPoint,
        radius: f32,
        base_color: Color,
        light: Vec3,
    ) {
        // Braille subpixel coordinates.
        let cx = center.x * 2.0;
        let cy = center.y * 4.0;
        let r = radius;
        let depth = center.depth;

        let pw = self.canvas.pixel_width() as i32;
        let ph = self.canvas.pixel_height() as i32;
        let r_i = r as i32;

        let y_min = ((cy as i32) - r_i).max(0);
        let y_max = ((cy as i32) + r_i).min(ph - 1);
        let r_sq = r * r;

        for py in y_min..=y_max {
            let dy = py as f32 - cy;
            let dx_span = (r_sq - dy * dy).max(0.0).sqrt();
            let x_min = ((cx as i32) - dx_span as i32).max(0);
            let x_max = ((cx as i32) + dx_span as i32).min(pw - 1);

            for px in x_min..=x_max {
                let ux = px as usize;
                let uy = py as usize;
                if self.zbuffer.test_and_set(ux, uy, depth) {
                    let normal = sphere_normal(px as f32, py as f32, cx, cy, r);
                    let color = shade_point(normal, light, base_color);
                    self.canvas.set_pixel_colored(ux, uy, color);
                }
            }
        }
    }
}

/// Map depth to node radius: closer nodes appear larger.
///
/// Depth is in clip space (z/w). Lower depth = closer = larger radius.
/// Linear interpolation between `MAX_RADIUS` (closest) and `MIN_RADIUS` (farthest).
fn depth_to_radius(depth: f32) -> f32 {
    // Clip-space depth typically ranges 0.0 (near) to 1.0 (far).
    let t = depth.clamp(0.0, 1.0);
    MAX_RADIUS + (MIN_RADIUS - MAX_RADIUS) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::graph::WorldGraph;
    use aether_core::models::ProcessNode;

    fn make_node(pid: u32, hp: f32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 0,
            name: format!("proc_{pid}"),
            cpu_percent: 0.0,
            mem_bytes: 0,
            state: ProcessState::Running,
            hp,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    fn make_zombie(pid: u32) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 0,
            name: format!("zombie_{pid}"),
            cpu_percent: 0.0,
            mem_bytes: 0,
            state: ProcessState::Zombie,
            hp: 10.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[test]
    fn test_render_empty_graph_returns_empty() {
        let mut renderer = SceneRenderer::new(20, 10);
        let graph = WorldGraph::new();
        let lines = renderer.render(&graph, 0.0);
        // Empty graph should produce blank lines (all Braille blanks).
        for (line, _colors) in &lines {
            assert!(
                line.chars().all(|c| c == '\u{2800}'),
                "empty graph should produce blank Braille lines"
            );
        }
    }

    #[test]
    fn test_render_single_node_produces_output() {
        let mut renderer = SceneRenderer::new(40, 20);
        let mut graph = WorldGraph::new();
        graph.add_process(make_node(1, 80.0));

        // Sync layout and center camera on the node.
        renderer.layout.sync_with_graph(&graph);
        let pos = renderer
            .layout
            .get_position(1)
            .expect("node should exist in layout");
        renderer.camera_mut().center = pos;

        let lines = renderer.render(&graph, 0.0);
        assert!(!lines.is_empty(), "render should produce output lines");

        let has_content = lines
            .iter()
            .any(|(line, _)| line.chars().any(|c| c != '\u{2800}'));
        assert!(has_content, "single node should produce visible pixels");
    }

    #[test]
    fn test_resize_updates_dimensions() {
        let mut renderer = SceneRenderer::new(10, 5);
        assert_eq!(renderer.canvas.cell_width(), 10);
        assert_eq!(renderer.canvas.cell_height(), 5);

        renderer.resize(20, 10);
        assert_eq!(renderer.canvas.cell_width(), 20);
        assert_eq!(renderer.canvas.cell_height(), 10);
    }

    #[test]
    fn test_depth_to_radius_near_is_large() {
        let near = depth_to_radius(0.0);
        let far = depth_to_radius(1.0);
        assert!(
            near > far,
            "near radius {near} should be larger than far radius {far}"
        );
        assert!((near - MAX_RADIUS).abs() < f32::EPSILON);
        assert!((far - MIN_RADIUS).abs() < f32::EPSILON);
    }

    #[test]
    fn test_camera_mut_returns_mutable_ref() {
        let mut renderer = SceneRenderer::new(10, 5);
        renderer.camera_mut().rotate(0.1, 0.0);
        // Should not panic — just verify the API works.
    }

    #[test]
    fn test_render_zero_size_returns_empty() {
        let mut renderer = SceneRenderer::new(0, 0);
        let graph = WorldGraph::new();
        let lines = renderer.render(&graph, 0.0);
        assert!(lines.is_empty(), "zero-size canvas should return empty");
    }

    #[test]
    fn test_blend_colors_averages_channels() {
        let white = Color::Rgb(200, 100, 50);
        let black = Color::Rgb(100, 50, 20);
        let blended = blend_colors(white, black);
        assert_eq!(blended, Color::Rgb(150, 75, 35));
    }

    #[test]
    fn test_blend_colors_non_rgb_returns_first() {
        let result = blend_colors(Color::Red, Color::Blue);
        assert_eq!(result, Color::Red);
    }

    #[test]
    fn test_dim_color_halves_channels() {
        let color = Color::Rgb(200, 100, 50);
        assert_eq!(dim_color(color), Color::Rgb(100, 50, 25));
    }

    #[test]
    fn test_zombie_flicker_alternates_visibility() {
        let mut renderer = SceneRenderer::new(40, 20);
        let mut graph = WorldGraph::new();
        graph.add_process(make_zombie(1));

        renderer.layout.sync_with_graph(&graph);
        let pos = renderer.layout.get_position(1).expect("node in layout");
        renderer.camera_mut().center = pos;

        // At t=0, zombie is visible (period_index=0, even).
        let lines_visible = renderer.render(&graph, 0.0);
        let has_content_v = lines_visible
            .iter()
            .any(|(line, _)| line.chars().any(|c| c != '\u{2800}'));

        // Advance to t=0.6s → period_index=1 (odd), zombie hidden.
        let lines_hidden = renderer.render(&graph, 0.6);
        let has_content_h = lines_hidden
            .iter()
            .any(|(line, _)| line.chars().any(|c| c != '\u{2800}'));

        assert!(has_content_v, "zombie should be visible at t=0");
        assert!(!has_content_h, "zombie should be hidden at t=0.6s");
    }

    #[test]
    fn test_critical_node_renders_with_bloom() {
        let mut renderer = SceneRenderer::new(40, 20);
        let mut graph = WorldGraph::new();
        // HP=10 → critical, should trigger bloom.
        graph.add_process(make_node(1, 10.0));

        renderer.layout.sync_with_graph(&graph);
        let pos = renderer.layout.get_position(1).expect("node in layout");
        renderer.camera_mut().center = pos;

        let lines = renderer.render(&graph, 0.0);
        let has_content = lines
            .iter()
            .any(|(line, _)| line.chars().any(|c| c != '\u{2800}'));
        assert!(has_content, "critical node with bloom should produce visible pixels");
    }
}

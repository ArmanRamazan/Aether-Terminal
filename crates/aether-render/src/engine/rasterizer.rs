//! Rasterization primitives: z-buffer, Bresenham line, and circle drawing.

#![allow(dead_code)] // Items used by future scene renderer.

use ratatui::style::Color;

use crate::braille::BrailleCanvas;
use crate::engine::projection::ScreenPoint;

/// Depth buffer that tracks the closest fragment at each Braille pixel.
///
/// Resolution matches Braille subpixels: `term_width * 2` × `term_height * 4`.
pub(crate) struct ZBuffer {
    width: usize,
    height: usize,
    buffer: Vec<f32>,
}

impl ZBuffer {
    /// Create a z-buffer with all depths set to infinity.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            buffer: vec![f32::INFINITY; width * height],
        }
    }

    /// Test and update depth at `(x, y)`.
    ///
    /// Returns `true` if `depth` is closer than the stored value (pixel visible),
    /// updating the buffer. Returns `false` if the pixel is occluded.
    pub fn test_and_set(&mut self, x: usize, y: usize, depth: f32) -> bool {
        let Some(cell) = self.buffer.get_mut(y * self.width + x) else {
            return false;
        };
        if depth < *cell {
            *cell = depth;
            true
        } else {
            false
        }
    }

    /// Reset all depths to infinity.
    pub fn clear(&mut self) {
        self.buffer.fill(f32::INFINITY);
    }
}

/// Rasterize a line between two screen points using Bresenham's algorithm.
///
/// Coordinates are converted to Braille subpixel space (×2 horizontal, ×4 vertical).
/// Depth is linearly interpolated and tested against the z-buffer per pixel.
pub(crate) fn draw_line(
    canvas: &mut BrailleCanvas,
    zbuf: &mut ZBuffer,
    p0: ScreenPoint,
    p1: ScreenPoint,
    color: Color,
) {
    let px0 = (p0.x * 2.0) as i32;
    let py0 = (p0.y * 4.0) as i32;
    let px1 = (p1.x * 2.0) as i32;
    let py1 = (p1.y * 4.0) as i32;

    let dx = (px1 - px0).abs();
    let dy = -(py1 - py0).abs();
    let sx: i32 = if px0 < px1 { 1 } else { -1 };
    let sy: i32 = if py0 < py1 { 1 } else { -1 };
    let mut err = dx + dy;

    let total_steps = dx.max(-dy);

    let mut x = px0;
    let mut y = py0;
    let mut step = 0;

    loop {
        // Interpolate depth along the line.
        let t = if total_steps == 0 {
            0.0
        } else {
            step as f32 / total_steps as f32
        };
        let depth = p0.depth + (p1.depth - p0.depth) * t;

        if x >= 0 && y >= 0 {
            let ux = x as usize;
            let uy = y as usize;
            if ux < canvas.pixel_width()
                && uy < canvas.pixel_height()
                && zbuf.test_and_set(ux, uy, depth)
            {
                canvas.set_pixel_colored(ux, uy, color);
            }
        }

        if x == px1 && y == py1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
        step += 1;
    }
}

/// Rasterize a circle outline using the midpoint circle algorithm.
///
/// Coordinates are in screen space; converted to Braille subpixels internally.
/// All pixels use the center's depth for z-buffer testing.
pub(crate) fn draw_circle(
    canvas: &mut BrailleCanvas,
    zbuf: &mut ZBuffer,
    center: ScreenPoint,
    radius: f32,
    color: Color,
) {
    let cx = (center.x * 2.0) as i32;
    let cy = (center.y * 4.0) as i32;
    let r = radius as i32;
    let depth = center.depth;

    let pw = canvas.pixel_width() as i32;
    let ph = canvas.pixel_height() as i32;

    let mut x = r;
    let mut y = 0i32;
    let mut d = 1 - r;

    while x >= y {
        // Plot all 8 octants.
        for &(px, py) in &[
            (cx + x, cy + y),
            (cx - x, cy + y),
            (cx + x, cy - y),
            (cx - x, cy - y),
            (cx + y, cy + x),
            (cx - y, cy + x),
            (cx + y, cy - x),
            (cx - y, cy - x),
        ] {
            if px >= 0 && py >= 0 && px < pw && py < ph {
                let ux = px as usize;
                let uy = py as usize;
                if zbuf.test_and_set(ux, uy, depth) {
                    canvas.set_pixel_colored(ux, uy, color);
                }
            }
        }

        y += 1;
        if d <= 0 {
            d += 2 * y + 1;
        } else {
            x -= 1;
            d += 2 * (y - x) + 1;
        }
    }
}

/// Rasterize a filled circle using scanline fill.
///
/// For each row in the circle's bounding box, computes the horizontal span
/// from the circle equation and fills all pixels. Z-buffer tested per pixel.
pub(crate) fn draw_filled_circle(
    canvas: &mut BrailleCanvas,
    zbuf: &mut ZBuffer,
    center: ScreenPoint,
    radius: f32,
    color: Color,
) {
    let cx = (center.x * 2.0) as i32;
    let cy = (center.y * 4.0) as i32;
    let r = radius as i32;
    let depth = center.depth;

    let pw = canvas.pixel_width() as i32;
    let ph = canvas.pixel_height() as i32;

    let y_min = (cy - r).max(0);
    let y_max = (cy + r).min(ph - 1);

    let r_sq = radius * radius;

    for py in y_min..=y_max {
        let dy = py - cy;
        let dx = (r_sq - (dy as f32 * dy as f32)).sqrt() as i32;
        let x_min = (cx - dx).max(0);
        let x_max = (cx + dx).min(pw - 1);

        for px in x_min..=x_max {
            let ux = px as usize;
            let uy = py as usize;
            if zbuf.test_and_set(ux, uy, depth) {
                canvas.set_pixel_colored(ux, uy, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closer_pixel_overwrites_farther() {
        let mut zb = ZBuffer::new(4, 4);
        assert!(zb.test_and_set(1, 1, 1.0), "first write should succeed");
        assert!(zb.test_and_set(1, 1, 0.5), "closer depth should overwrite");
    }

    #[test]
    fn test_farther_pixel_rejected_after_closer() {
        let mut zb = ZBuffer::new(4, 4);
        assert!(zb.test_and_set(2, 2, 0.5));
        assert!(!zb.test_and_set(2, 2, 1.0), "farther depth should be rejected");
    }

    #[test]
    fn test_clear_resets_all_depths() {
        let mut zb = ZBuffer::new(4, 4);
        assert!(zb.test_and_set(0, 0, 0.1));
        zb.clear();
        assert!(zb.test_and_set(0, 0, 0.9), "after clear, any depth should succeed");
    }

    #[test]
    fn test_out_of_bounds_returns_false() {
        let mut zb = ZBuffer::new(2, 2);
        assert!(!zb.test_and_set(5, 5, 0.5), "out-of-bounds should return false");
    }

    /// Count raised dots across the entire canvas.
    fn count_pixels(canvas: &BrailleCanvas) -> usize {
        let mut count = 0;
        for y in 0..canvas.pixel_height() {
            for x in 0..canvas.pixel_width() {
                // A pixel is set if its bit is raised in the cell.
                let cx = x / 2;
                let cy = y / 4;
                let ch = canvas.cell_char(cx, cy);
                let bits = ch as u32 - 0x2800;
                let dx = x % 2;
                let dy = y % 4;
                let dot: u8 = match (dx, dy) {
                    (0, 0) => 0x01,
                    (0, 1) => 0x02,
                    (0, 2) => 0x04,
                    (0, 3) => 0x40,
                    (1, 0) => 0x08,
                    (1, 1) => 0x10,
                    (1, 2) => 0x20,
                    (1, 3) => 0x80,
                    _ => 0,
                };
                if bits & dot as u32 != 0 {
                    count += 1;
                }
            }
        }
        count
    }

    fn make_point(x: f32, y: f32, depth: f32) -> ScreenPoint {
        ScreenPoint { x, y, depth }
    }

    #[test]
    fn test_horizontal_line_pixels_at_expected_y() {
        // 10 cells wide, 5 cells tall → 20×20 pixel space
        let mut canvas = BrailleCanvas::new(10, 5);
        let mut zbuf = ZBuffer::new(canvas.pixel_width(), canvas.pixel_height());
        let color = Color::White;

        // Draw horizontal line at screen y=2 from x=1 to x=5
        // Braille y = 2*4 = 8
        let p0 = make_point(1.0, 2.0, 0.5);
        let p1 = make_point(5.0, 2.0, 0.5);
        draw_line(&mut canvas, &mut zbuf, p0, p1, color);

        let expected_py = 8usize; // 2.0 * 4
        let px_start = 2usize;   // 1.0 * 2
        let px_end = 10usize;    // 5.0 * 2

        // All pixels on the line should be at the expected y
        for px in px_start..=px_end {
            let cx = px / 2;
            let cy = expected_py / 4;
            let ch = canvas.cell_char(cx, cy);
            assert_ne!(
                ch as u32, 0x2800,
                "cell ({cx},{cy}) should have dots for horizontal line"
            );
        }
    }

    #[test]
    fn test_vertical_line_pixels_at_expected_x() {
        let mut canvas = BrailleCanvas::new(10, 10);
        let mut zbuf = ZBuffer::new(canvas.pixel_width(), canvas.pixel_height());
        let color = Color::White;

        // Draw vertical line at screen x=3 from y=1 to y=5
        // Braille x = 3*2 = 6
        let p0 = make_point(3.0, 1.0, 0.5);
        let p1 = make_point(3.0, 5.0, 0.5);
        draw_line(&mut canvas, &mut zbuf, p0, p1, color);

        let expected_px = 6usize; // 3.0 * 2
        let py_start = 4usize;   // 1.0 * 4
        let py_end = 20usize;    // 5.0 * 4

        // Check that pixels along the expected x column are set
        for py in py_start..=py_end {
            let cx = expected_px / 2;
            let cy = py / 4;
            let ch = canvas.cell_char(cx, cy);
            assert_ne!(
                ch as u32, 0x2800,
                "cell ({cx},{cy}) should have dots for vertical line at py={py}"
            );
        }
    }

    #[test]
    fn test_circle_pixels_at_expected_distance() {
        // 20 cells wide, 20 cells tall → 40×80 pixel space
        let mut canvas = BrailleCanvas::new(20, 20);
        let mut zbuf = ZBuffer::new(canvas.pixel_width(), canvas.pixel_height());
        let color = Color::White;

        // Center at screen (10, 10) → braille (20, 40), radius 8 pixels
        let center = make_point(10.0, 10.0, 0.5);
        let radius = 4.0; // 4 braille pixels
        draw_circle(&mut canvas, &mut zbuf, center, radius, color);

        let cx = (10.0 * 2.0) as i32; // 20
        let cy = (10.0 * 4.0) as i32; // 40

        let count = count_pixels(&canvas);
        assert!(count > 0, "circle should set some pixels");

        // Every set pixel should be approximately `radius` distance from center
        for y in 0..canvas.pixel_height() {
            for x in 0..canvas.pixel_width() {
                if is_pixel_set(&canvas, x, y) {
                    let dx = x as i32 - cx;
                    let dy = y as i32 - cy;
                    let dist = ((dx * dx + dy * dy) as f32).sqrt();
                    assert!(
                        (dist - radius).abs() < 1.5,
                        "pixel ({x},{y}) dist {dist:.1} should be ~{radius} from center"
                    );
                }
            }
        }
    }

    #[test]
    fn test_filled_circle_has_more_pixels_than_outline() {
        let mut canvas_outline = BrailleCanvas::new(20, 20);
        let mut zbuf_outline = ZBuffer::new(canvas_outline.pixel_width(), canvas_outline.pixel_height());
        let mut canvas_filled = BrailleCanvas::new(20, 20);
        let mut zbuf_filled = ZBuffer::new(canvas_filled.pixel_width(), canvas_filled.pixel_height());
        let color = Color::White;

        let center = make_point(10.0, 10.0, 0.5);
        let radius = 5.0;

        draw_circle(&mut canvas_outline, &mut zbuf_outline, center, radius, color);
        draw_filled_circle(&mut canvas_filled, &mut zbuf_filled, center, radius, color);

        let outline_count = count_pixels(&canvas_outline);
        let filled_count = count_pixels(&canvas_filled);

        assert!(
            filled_count > outline_count,
            "filled circle ({filled_count} px) should have more pixels than outline ({outline_count} px)"
        );
    }

    /// Check if a specific braille pixel is set.
    fn is_pixel_set(canvas: &BrailleCanvas, x: usize, y: usize) -> bool {
        let cx = x / 2;
        let cy = y / 4;
        let ch = canvas.cell_char(cx, cy);
        let bits = ch as u32 - 0x2800;
        let dx = x % 2;
        let dy = y % 4;
        let dot: u8 = match (dx, dy) {
            (0, 0) => 0x01,
            (0, 1) => 0x02,
            (0, 2) => 0x04,
            (0, 3) => 0x40,
            (1, 0) => 0x08,
            (1, 1) => 0x10,
            (1, 2) => 0x20,
            (1, 3) => 0x80,
            _ => 0,
        };
        bits & dot as u32 != 0
    }

    #[test]
    fn test_diagonal_line_pixel_count() {
        let mut canvas = BrailleCanvas::new(20, 20);
        let mut zbuf = ZBuffer::new(canvas.pixel_width(), canvas.pixel_height());
        let color = Color::White;

        // Diagonal from (1,1) to (8,8) in screen coords
        let p0 = make_point(1.0, 1.0, 0.0);
        let p1 = make_point(8.0, 8.0, 1.0);
        draw_line(&mut canvas, &mut zbuf, p0, p1, color);

        let count = count_pixels(&canvas);
        // Braille displacement: dx=14, dy=28 → Bresenham produces max(14,28)+1 = 29 pixels
        let dx = ((8.0 - 1.0) * 2.0) as usize; // 14
        let dy = ((8.0 - 1.0) * 4.0) as usize;  // 28
        let expected = dx.max(dy) + 1;           // 29
        assert_eq!(
            count, expected,
            "diagonal line should produce ~{expected} pixels, got {count}"
        );
    }
}

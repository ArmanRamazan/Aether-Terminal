//! Braille character canvas with 2×4 subpixel resolution.
//!
//! Each terminal cell maps to a 2-wide × 4-tall grid of dots, encoded as a
//! single Unicode Braille character (U+2800..U+28FF). This gives twice the
//! horizontal and four times the vertical resolution compared to plain text.

use ratatui::style::Color;

/// Bit offsets for the 8 dots within a Braille cell.
///
/// Standard Braille dot numbering mapped to `(dx, dy)` positions:
/// ```text
/// [0,0]=0x01  [1,0]=0x08
/// [0,1]=0x02  [1,1]=0x10
/// [0,2]=0x04  [1,2]=0x20
/// [0,3]=0x40  [1,3]=0x80
/// ```
const DOT_MAP: [[u8; 4]; 2] = [
    [0x01, 0x02, 0x04, 0x40], // dx=0
    [0x08, 0x10, 0x20, 0x80], // dx=1
];

/// Unicode code point for the blank Braille pattern (no dots raised).
const BRAILLE_BASE: u32 = 0x2800;

/// A pixel canvas backed by Unicode Braille characters.
///
/// The canvas stores one `u8` bitmask per terminal cell, where each bit
/// corresponds to one of the 8 Braille dots. An optional per-cell color
/// tracks the last color written to that cell.
pub struct BrailleCanvas {
    cell_width: usize,
    cell_height: usize,
    buffer: Vec<u8>,
    color_buffer: Vec<Color>,
}

impl BrailleCanvas {
    /// Create a blank canvas spanning `cell_width` × `cell_height` terminal cells.
    pub fn new(cell_width: usize, cell_height: usize) -> Self {
        let len = cell_width * cell_height;
        Self {
            cell_width,
            cell_height,
            buffer: vec![0; len],
            color_buffer: vec![Color::Reset; len],
        }
    }

    /// Width in terminal cells.
    pub fn cell_width(&self) -> usize {
        self.cell_width
    }

    /// Height in terminal cells.
    pub fn cell_height(&self) -> usize {
        self.cell_height
    }

    /// Pixel-space width (terminal columns × 2).
    pub fn pixel_width(&self) -> usize {
        self.cell_width * 2
    }

    /// Pixel-space height (terminal rows × 4).
    pub fn pixel_height(&self) -> usize {
        self.cell_height * 4
    }

    /// Raise a dot at pixel coordinates `(x, y)`.
    ///
    /// Out-of-bounds coordinates are silently ignored.
    #[allow(dead_code)]
    pub fn set_pixel(&mut self, x: usize, y: usize) {
        if let Some((idx, bit)) = self.pixel_to_cell(x, y) {
            self.buffer[idx] |= bit;
        }
    }

    /// Raise a dot at `(x, y)` and record its color.
    ///
    /// Out-of-bounds coordinates are silently ignored.
    pub fn set_pixel_colored(&mut self, x: usize, y: usize, color: Color) {
        if let Some((idx, bit)) = self.pixel_to_cell(x, y) {
            self.buffer[idx] |= bit;
            self.color_buffer[idx] = color;
        }
    }

    /// Clear a single dot at pixel coordinates `(x, y)`.
    ///
    /// Out-of-bounds coordinates are silently ignored.
    #[allow(dead_code)]
    pub fn clear_pixel(&mut self, x: usize, y: usize) {
        if let Some((idx, bit)) = self.pixel_to_cell(x, y) {
            self.buffer[idx] &= !bit;
        }
    }

    /// Reset all cells to blank.
    pub fn clear(&mut self) {
        self.buffer.fill(0);
        self.color_buffer.fill(Color::Reset);
    }

    /// Braille character for the cell at column `cx`, row `cy`.
    pub fn cell_char(&self, cx: usize, cy: usize) -> char {
        let idx = cy * self.cell_width + cx;
        // SAFETY: BRAILLE_BASE + 0..=255 is always a valid Unicode scalar.
        unsafe { char::from_u32_unchecked(BRAILLE_BASE + self.buffer[idx] as u32) }
    }

    /// Color assigned to the cell at column `cx`, row `cy`.
    pub fn cell_color(&self, cx: usize, cy: usize) -> Color {
        self.color_buffer[cy * self.cell_width + cx]
    }

    /// Convert the canvas to lines of Braille text with per-character colors.
    pub fn to_lines(&self) -> Vec<(String, Vec<Color>)> {
        let mut lines = Vec::with_capacity(self.cell_height);
        for cy in 0..self.cell_height {
            let mut text = String::with_capacity(self.cell_width * 3);
            let mut colors = Vec::with_capacity(self.cell_width);
            for cx in 0..self.cell_width {
                text.push(self.cell_char(cx, cy));
                colors.push(self.cell_color(cx, cy));
            }
            lines.push((text, colors));
        }
        lines
    }

    /// Map pixel `(x, y)` to a `(buffer_index, bit_mask)` pair.
    fn pixel_to_cell(&self, x: usize, y: usize) -> Option<(usize, u8)> {
        if x >= self.pixel_width() || y >= self.pixel_height() {
            return None;
        }
        let cx = x / 2;
        let cy = y / 4;
        let dx = x % 2;
        let dy = y % 4;
        Some((cy * self.cell_width + cx, DOT_MAP[dx][dy]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_canvas_all_blank_braille() {
        let canvas = BrailleCanvas::new(3, 2);
        for cy in 0..2 {
            for cx in 0..3 {
                assert_eq!(
                    canvas.cell_char(cx, cy),
                    '\u{2800}',
                    "cell ({cx},{cy}) should be blank braille"
                );
            }
        }
    }

    #[test]
    fn test_all_dots_set_gives_u28ff() {
        let mut canvas = BrailleCanvas::new(1, 1);
        for y in 0..4 {
            for x in 0..2 {
                canvas.set_pixel(x, y);
            }
        }
        assert_eq!(canvas.cell_char(0, 0), '\u{28FF}');
    }

    #[test]
    fn test_set_pixel_0_0_sets_bit_0x01() {
        let mut canvas = BrailleCanvas::new(1, 1);
        canvas.set_pixel(0, 0);
        assert_eq!(canvas.buffer[0], 0x01);
    }

    #[test]
    fn test_set_pixel_1_3_sets_bit_0x80() {
        let mut canvas = BrailleCanvas::new(1, 1);
        canvas.set_pixel(1, 3);
        assert_eq!(canvas.buffer[0], 0x80);
    }

    #[test]
    fn test_pixel_dimensions_match_cell_dimensions() {
        let canvas = BrailleCanvas::new(40, 10);
        assert_eq!(canvas.pixel_width(), 80, "pixel_width = cell_width * 2");
        assert_eq!(canvas.pixel_height(), 40, "pixel_height = cell_height * 4");
    }

    #[test]
    fn test_clear_pixel_removes_dot() {
        let mut canvas = BrailleCanvas::new(1, 1);
        canvas.set_pixel(0, 0);
        canvas.set_pixel(1, 0);
        canvas.clear_pixel(0, 0);
        assert_eq!(canvas.buffer[0], 0x08, "only (1,0) dot should remain");
    }

    #[test]
    fn test_clear_resets_all_cells() {
        let mut canvas = BrailleCanvas::new(2, 2);
        canvas.set_pixel(0, 0);
        canvas.set_pixel(2, 4);
        canvas.clear();
        assert!(canvas.buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_set_pixel_colored_stores_color() {
        let mut canvas = BrailleCanvas::new(1, 1);
        let color = Color::Rgb(0, 240, 255);
        canvas.set_pixel_colored(0, 0, color);
        assert_eq!(canvas.cell_color(0, 0), color);
        assert_eq!(canvas.buffer[0], 0x01);
    }

    #[test]
    fn test_out_of_bounds_ignored() {
        let mut canvas = BrailleCanvas::new(1, 1);
        canvas.set_pixel(2, 0); // x out of bounds
        canvas.set_pixel(0, 4); // y out of bounds
        assert_eq!(canvas.buffer[0], 0);
    }

    #[test]
    fn test_to_lines_produces_correct_output() {
        let mut canvas = BrailleCanvas::new(2, 1);
        canvas.set_pixel(0, 0); // cell (0,0) bit 0x01
        canvas.set_pixel(2, 0); // cell (1,0) bit 0x01
        let lines = canvas.to_lines();
        assert_eq!(lines.len(), 1);
        let (text, colors) = &lines[0];
        assert_eq!(text.chars().count(), 2);
        assert_eq!(text.chars().next().unwrap(), '\u{2801}');
        assert_eq!(text.chars().nth(1).unwrap(), '\u{2801}');
        assert_eq!(colors.len(), 2);
    }
}

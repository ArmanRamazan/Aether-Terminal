//! Z-buffer for depth testing at Braille pixel resolution.

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
}

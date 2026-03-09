//! 3D-to-screen projection pipeline.
//!
//! Transforms world-space coordinates through view → clip → NDC → screen space,
//! with near-plane clipping to discard points behind the camera.

use glam::{Mat4, Vec3, Vec4};

/// A projected point in screen coordinates with depth for z-buffering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    /// Horizontal screen coordinate (pixels, left = 0).
    pub x: f32,
    /// Vertical screen coordinate (pixels, top = 0).
    pub y: f32,
    /// Depth in clip space (clip.z / clip.w), for z-buffer ordering.
    pub depth: f32,
}

/// Project a 3D world-space point onto 2D screen coordinates.
///
/// Returns `None` if the point is behind the camera (clip.w ≤ 0).
pub fn project_point(
    point: Vec3,
    view: &Mat4,
    proj: &Mat4,
    screen_w: u32,
    screen_h: u32,
) -> Option<ScreenPoint> {
    let clip: Vec4 = *proj * (*view * Vec4::from((point, 1.0)));

    if clip.w <= 0.0 {
        return None;
    }

    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let depth = clip.z / clip.w;

    let x = (ndc_x + 1.0) * 0.5 * screen_w as f32;
    let y = (1.0 - ndc_y) * 0.5 * screen_h as f32;

    Some(ScreenPoint { x, y, depth })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn test_matrices() -> (Mat4, Mat4, u32, u32) {
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0), // eye
            Vec3::ZERO,               // target
            Vec3::Y,                  // up
        );
        let proj = Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_4, // 45° fov
            800.0 / 600.0,               // aspect
            0.1,                         // near
            100.0,                       // far
        );
        (view, proj, 800, 600)
    }

    #[test]
    fn test_center_point_projects_near_screen_center() {
        let (view, proj, w, h) = test_matrices();

        let result =
            project_point(Vec3::ZERO, &view, &proj, w, h).expect("origin should be visible");

        let half_w = w as f32 / 2.0;
        let half_h = h as f32 / 2.0;
        assert!(
            (result.x - half_w).abs() < 1.0,
            "x={} should be near {half_w}",
            result.x,
        );
        assert!(
            (result.y - half_h).abs() < 1.0,
            "y={} should be near {half_h}",
            result.y,
        );
    }

    #[test]
    fn test_behind_camera_returns_none() {
        let (view, proj, w, h) = test_matrices();

        // Camera at z=5 looking toward origin — z=10 is behind the camera.
        let result = project_point(Vec3::new(0.0, 0.0, 10.0), &view, &proj, w, h);
        assert!(result.is_none(), "point behind camera should be clipped");
    }

    #[test]
    fn test_right_point_projects_to_right_side() {
        let (view, proj, w, h) = test_matrices();

        let center = project_point(Vec3::ZERO, &view, &proj, w, h).expect("origin visible");
        let right = project_point(Vec3::new(2.0, 0.0, 0.0), &view, &proj, w, h)
            .expect("right point visible");

        assert!(
            right.x > center.x,
            "right point x={} should be > center x={}",
            right.x,
            center.x,
        );
    }
}

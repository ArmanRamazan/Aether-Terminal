//! Orbital camera for 3D viewport navigation.

use glam::{Mat4, Vec3};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_3};

/// Camera that orbits around a center point using spherical coordinates.
///
/// Controlled by yaw/pitch rotation and distance zoom. Produces view and
/// projection matrices for the 3D rendering pipeline.
pub struct OrbitalCamera {
    /// Point the camera orbits around.
    pub center: Vec3,
    /// Distance from center.
    pub distance: f32,
    /// Horizontal angle in radians.
    pub yaw: f32,
    /// Vertical angle in radians, clamped to (-PI/2, PI/2).
    pub pitch: f32,
    /// Field of view in radians.
    pub fov: f32,
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
}

const DEFAULT_DISTANCE: f32 = 10.0;
const DEFAULT_YAW: f32 = 0.0;
const DEFAULT_PITCH: f32 = 0.3;
const DEFAULT_FOV: f32 = FRAC_PI_3;
const DEFAULT_NEAR: f32 = 0.1;
const DEFAULT_FAR: f32 = 100.0;

const MIN_DISTANCE: f32 = 1.0;
const MAX_DISTANCE: f32 = 50.0;

/// Pitch limit slightly inside +-PI/2 to avoid gimbal lock.
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;

impl OrbitalCamera {
    /// Create a camera with default orbital parameters.
    pub fn new() -> Self {
        Self {
            center: Vec3::ZERO,
            distance: DEFAULT_DISTANCE,
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
            fov: DEFAULT_FOV,
            near: DEFAULT_NEAR,
            far: DEFAULT_FAR,
        }
    }

    /// Compute camera world position from spherical coordinates.
    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.center + Vec3::new(x, y, z)
    }

    /// View matrix (world → camera space) using look-at from position to center.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.center, Vec3::Y)
    }

    /// Perspective projection matrix for the given aspect ratio.
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    /// Rotate the camera by delta yaw and pitch. Pitch is clamped to avoid gimbal lock.
    pub fn rotate(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Zoom by changing distance. Clamped to [1.0, 50.0].
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Set center to the centroid of the given points. No-op if empty.
    pub fn auto_center(&mut self, points: &[Vec3]) {
        if points.is_empty() {
            return;
        }
        let sum: Vec3 = points.iter().copied().sum();
        self.center = sum / points.len() as f32;
    }

    /// Restore all parameters to defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for OrbitalCamera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_matrix_default_is_valid() {
        let cam = OrbitalCamera::new();
        let view = cam.view_matrix();
        // No column should be all zeros or contain NaN
        for col in 0..4 {
            let c = view.col(col);
            assert!(
                !c.x.is_nan() && !c.y.is_nan() && !c.z.is_nan() && !c.w.is_nan(),
                "view matrix column {col} contains NaN"
            );
        }
        assert_ne!(view, Mat4::ZERO, "view matrix should not be zero");
    }

    #[test]
    fn test_rotate_changes_yaw_and_pitch() {
        let mut cam = OrbitalCamera::new();
        let (orig_yaw, orig_pitch) = (cam.yaw, cam.pitch);

        cam.rotate(0.5, 0.2);

        assert!(
            (cam.yaw - (orig_yaw + 0.5)).abs() < 1e-6,
            "yaw should increase by 0.5"
        );
        assert!(
            (cam.pitch - (orig_pitch + 0.2)).abs() < 1e-6,
            "pitch should increase by 0.2"
        );
    }

    #[test]
    fn test_rotate_clamps_pitch() {
        let mut cam = OrbitalCamera::new();
        cam.rotate(0.0, 100.0);
        assert!(
            cam.pitch <= PITCH_LIMIT,
            "pitch should be clamped to PITCH_LIMIT"
        );

        cam.rotate(0.0, -200.0);
        assert!(
            cam.pitch >= -PITCH_LIMIT,
            "pitch should be clamped to -PITCH_LIMIT"
        );
    }

    #[test]
    fn test_zoom_clamps_to_range() {
        let mut cam = OrbitalCamera::new();

        cam.zoom(-100.0);
        assert!(
            (cam.distance - MIN_DISTANCE).abs() < 1e-6,
            "distance should clamp to minimum"
        );

        cam.zoom(200.0);
        assert!(
            (cam.distance - MAX_DISTANCE).abs() < 1e-6,
            "distance should clamp to maximum"
        );
    }

    #[test]
    fn test_position_spherical_coordinates() {
        let mut cam = OrbitalCamera::new();
        cam.center = Vec3::ZERO;
        cam.distance = 10.0;
        cam.yaw = 0.0;
        cam.pitch = 0.0;

        let pos = cam.position();
        // At yaw=0, pitch=0: position should be (0, 0, distance)
        assert!((pos.x).abs() < 1e-5, "x should be ~0 at yaw=0, pitch=0");
        assert!((pos.y).abs() < 1e-5, "y should be ~0 at pitch=0");
        assert!(
            (pos.z - 10.0).abs() < 1e-5,
            "z should be ~distance at yaw=0, pitch=0"
        );
    }

    #[test]
    fn test_auto_center_moves_to_centroid() {
        let mut cam = OrbitalCamera::new();
        let points = [Vec3::new(2.0, 4.0, 6.0), Vec3::new(4.0, 6.0, 8.0)];
        cam.auto_center(&points);

        let expected = Vec3::new(3.0, 5.0, 7.0);
        assert!(
            (cam.center - expected).length() < 1e-5,
            "center should be centroid of points"
        );
    }

    #[test]
    fn test_auto_center_noop_on_empty() {
        let mut cam = OrbitalCamera::new();
        let orig = cam.center;
        cam.auto_center(&[]);
        assert_eq!(
            cam.center, orig,
            "center should not change for empty points"
        );
    }

    #[test]
    fn test_reset_restores_defaults() {
        let mut cam = OrbitalCamera::new();
        cam.yaw = 1.0;
        cam.pitch = 0.5;
        cam.distance = 30.0;
        cam.center = Vec3::new(1.0, 2.0, 3.0);

        cam.reset();

        assert_eq!(cam.yaw, DEFAULT_YAW);
        assert_eq!(cam.pitch, DEFAULT_PITCH);
        assert_eq!(cam.distance, DEFAULT_DISTANCE);
        assert_eq!(cam.center, Vec3::ZERO);
    }
}

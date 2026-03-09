//! Phong-like shading: ambient + diffuse lighting for 3D nodes.

use glam::Vec3;
use ratatui::style::Color;

/// Shade a surface point using ambient + Lambertian diffuse lighting.
///
/// `normal` and `light_dir` should be normalized. Brightness is applied
/// to each RGB channel of `base_color`, clamped to 255.
pub fn shade_point(normal: Vec3, light_dir: Vec3, base_color: Color) -> Color {
    let ambient = 0.3_f32;
    let diffuse = 0.7 * normal.dot(light_dir).max(0.0);
    let brightness = ambient + diffuse;

    let (r, g, b) = match base_color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    };

    let apply = |c: u8| ((c as f32 * brightness).round() as u32).min(255) as u8;

    Color::Rgb(apply(r), apply(g), apply(b))
}

/// Compute the surface normal for a point on a sphere.
///
/// Given a pixel position and the sphere's center/radius, returns
/// the outward-facing normal as if the circle were a 3D sphere.
pub fn sphere_normal(
    pixel_x: f32,
    pixel_y: f32,
    center_x: f32,
    center_y: f32,
    radius: f32,
) -> Vec3 {
    let dx = (pixel_x - center_x) / radius;
    let dy = (pixel_y - center_y) / radius;
    let dz = (1.0 - dx * dx - dy * dy).max(0.0).sqrt();

    Vec3::new(dx, dy, dz).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Color = Color::Rgb(255, 255, 255);

    #[test]
    fn test_facing_light_near_full_brightness() {
        let normal = Vec3::Z;
        let light_dir = Vec3::Z;
        let result = shade_point(normal, light_dir, WHITE);

        // ambient 0.3 + diffuse 0.7 * 1.0 = 1.0 → full brightness
        let Color::Rgb(r, g, b) = result else {
            panic!("expected Rgb");
        };
        assert_eq!(r, 255, "facing light should produce full red");
        assert_eq!(g, 255, "facing light should produce full green");
        assert_eq!(b, 255, "facing light should produce full blue");
    }

    #[test]
    fn test_perpendicular_to_light_ambient_only() {
        let normal = Vec3::X; // perpendicular to Z light
        let light_dir = Vec3::Z;
        let result = shade_point(normal, light_dir, WHITE);

        // dot(X, Z) = 0 → ambient only: 0.3 * 255 ≈ 77
        let Color::Rgb(r, _, _) = result else {
            panic!("expected Rgb");
        };
        let expected = (255.0_f32 * 0.3).round() as u8;
        assert_eq!(r, expected, "perpendicular should yield ~30% brightness");
    }

    #[test]
    fn test_behind_light_ambient_only() {
        let normal = Vec3::NEG_Z; // facing away from light
        let light_dir = Vec3::Z;
        let result = shade_point(normal, light_dir, WHITE);

        // dot(-Z, Z) = -1, clamped to 0 → ambient only
        let Color::Rgb(r, _, _) = result else {
            panic!("expected Rgb");
        };
        let expected = (255.0_f32 * 0.3).round() as u8;
        assert_eq!(r, expected, "behind light should yield ambient only");
    }

    #[test]
    fn test_shade_preserves_color_ratios() {
        let color = Color::Rgb(200, 100, 50);
        let result = shade_point(Vec3::Z, Vec3::Z, color);

        // Full brightness (1.0) → same values
        let Color::Rgb(r, g, b) = result else {
            panic!("expected Rgb");
        };
        assert_eq!(r, 200);
        assert_eq!(g, 100);
        assert_eq!(b, 50);
    }

    #[test]
    fn test_sphere_normal_at_center_points_outward() {
        let n = sphere_normal(5.0, 5.0, 5.0, 5.0, 10.0);

        // At center: dx=0, dy=0, dz=1 → normal points straight out
        assert!((n.x).abs() < 1e-5, "center normal x should be ~0");
        assert!((n.y).abs() < 1e-5, "center normal y should be ~0");
        assert!((n.z - 1.0).abs() < 1e-5, "center normal z should be ~1");
    }

    #[test]
    fn test_sphere_normal_at_edge_is_horizontal() {
        let n = sphere_normal(15.0, 5.0, 5.0, 5.0, 10.0);

        // At right edge: dx=1, dy=0, dz=0 → normal points right
        assert!((n.x - 1.0).abs() < 1e-5, "edge normal x should be ~1");
        assert!((n.z).abs() < 1e-5, "edge normal z should be ~0");
    }
}

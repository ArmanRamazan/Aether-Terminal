//! Cyberpunk color palette inspired by Blade Runner / Ghost in the Shell.
//!
//! ALL widget colors must come from this module — never hardcode `Color::Rgb`
//! elsewhere in the render crate.

use ratatui::style::Color;

/// Central color palette for the Aether Terminal UI.
///
/// Deep-space backgrounds with neon accent colors that map to system health
/// levels. Every widget in `tui/` must reference these constants instead of
/// constructing ad-hoc `Color::Rgb` values.
pub struct Palette;

impl Palette {
    /// Deep Space background — `#050A0E`.
    pub const BG: Color = Color::Rgb(5, 10, 14);

    /// Electric Cyan for healthy / nominal state — `#00F0FF`.
    pub const HEALTHY: Color = Color::Rgb(0, 240, 255);

    /// Neon Blue for moderate load — `#0080FF`.
    pub const NEON_BLUE: Color = Color::Rgb(0, 128, 255);

    /// Neon Yellow for warning state — `#FCEE09`.
    pub const WARNING: Color = Color::Rgb(252, 238, 9);

    /// Cherry Red for critical state — `#FF003C`.
    pub const CRITICAL: Color = Color::Rgb(255, 0, 60);

    /// Pure White for data text — `#FAFAFA`.
    pub const DATA: Color = Color::Rgb(250, 250, 250);

    /// Neon Purple for XP / gamification accents — `#BF00FF`.
    pub const XP_PURPLE: Color = Color::Rgb(191, 0, 255);
}

/// Returns a color that represents the given CPU/memory load percentage.
///
/// | Range       | Color       |
/// |-------------|-------------|
/// | 0 – 49 %    | `HEALTHY`   |
/// | 50 – 74 %   | `NEON_BLUE` |
/// | 75 – 89 %   | `WARNING`   |
/// | 90 – 100 %  | `CRITICAL`  |
pub fn color_for_load(percent: f32) -> Color {
    match percent {
        p if p < 50.0 => Palette::HEALTHY,
        p if p < 75.0 => Palette::NEON_BLUE,
        p if p < 90.0 => Palette::WARNING,
        _ => Palette::CRITICAL,
    }
}

/// Returns a color that represents the given HP value (0.0 – 100.0).
///
/// | Range       | Color      |
/// |-------------|------------|
/// | > 50        | `HEALTHY`  |
/// | 20 – 50     | `WARNING`  |
/// | < 20        | `CRITICAL` |
pub fn color_for_hp(hp: f32) -> Color {
    match hp {
        h if h > 50.0 => Palette::HEALTHY,
        h if h > 20.0 => Palette::WARNING,
        _ => Palette::CRITICAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_for_load_healthy_below_50() {
        assert_eq!(color_for_load(0.0), Palette::HEALTHY);
        assert_eq!(color_for_load(49.9), Palette::HEALTHY);
    }

    #[test]
    fn test_color_for_load_neon_blue_50_to_74() {
        assert_eq!(color_for_load(50.0), Palette::NEON_BLUE);
        assert_eq!(color_for_load(74.9), Palette::NEON_BLUE);
    }

    #[test]
    fn test_color_for_load_warning_75_to_89() {
        assert_eq!(color_for_load(75.0), Palette::WARNING);
        assert_eq!(color_for_load(89.9), Palette::WARNING);
    }

    #[test]
    fn test_color_for_load_critical_90_and_above() {
        assert_eq!(color_for_load(90.0), Palette::CRITICAL);
        assert_eq!(color_for_load(100.0), Palette::CRITICAL);
    }

    #[test]
    fn test_color_for_hp_healthy_above_50() {
        assert_eq!(color_for_hp(100.0), Palette::HEALTHY);
        assert_eq!(color_for_hp(50.1), Palette::HEALTHY);
    }

    #[test]
    fn test_color_for_hp_warning_20_to_50() {
        assert_eq!(color_for_hp(50.0), Palette::WARNING);
        assert_eq!(color_for_hp(20.1), Palette::WARNING);
    }

    #[test]
    fn test_color_for_hp_critical_below_20() {
        assert_eq!(color_for_hp(20.0), Palette::CRITICAL);
        assert_eq!(color_for_hp(0.0), Palette::CRITICAL);
    }
}

//! Cyberpunk color palette and TOML theme loading.
//!
//! ALL widget colors must come from this module — never hardcode `Color::Rgb`
//! elsewhere in the render crate.

use std::path::Path;

use ratatui::style::Color;
use serde::Deserialize;

use crate::RenderError;

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

// ── Theme system ───────────────────────────────────────────────────────

/// Runtime theme loaded from a TOML file.
///
/// Maps to the same semantic roles as [`Palette`] constants, but values
/// are determined at runtime from a theme file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub bg: Color,
    pub healthy: Color,
    pub neon_blue: Color,
    pub warning: Color,
    pub critical: Color,
    pub data: Color,
    pub xp_purple: Color,
}

impl Default for Theme {
    /// Returns the built-in cyberpunk theme matching [`Palette`] constants.
    fn default() -> Self {
        Self {
            bg: Palette::BG,
            healthy: Palette::HEALTHY,
            neon_blue: Palette::NEON_BLUE,
            warning: Palette::WARNING,
            critical: Palette::CRITICAL,
            data: Palette::DATA,
            xp_purple: Palette::XP_PURPLE,
        }
    }
}

/// Raw TOML structure for deserialization.
#[derive(Deserialize)]
struct ThemeFile {
    colors: ThemeColors,
}

#[derive(Deserialize)]
struct ThemeColors {
    bg: String,
    healthy: String,
    neon_blue: String,
    warning: String,
    critical: String,
    data: String,
    xp_purple: String,
}

/// Parse a `#RRGGBB` hex string into a ratatui `Color::Rgb`.
fn parse_hex_color(hex: &str) -> Result<Color, RenderError> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return Err(RenderError::Theme(format!(
            "invalid hex color length: expected 6 chars, got {}",
            hex.len()
        )));
    }
    let r = u8::from_str_radix(&hex[0..2], 16)
        .map_err(|e| RenderError::Theme(format!("bad red component: {e}")))?;
    let g = u8::from_str_radix(&hex[2..4], 16)
        .map_err(|e| RenderError::Theme(format!("bad green component: {e}")))?;
    let b = u8::from_str_radix(&hex[4..6], 16)
        .map_err(|e| RenderError::Theme(format!("bad blue component: {e}")))?;
    Ok(Color::Rgb(r, g, b))
}

/// Load a [`Theme`] from a TOML file at the given path.
///
/// Falls back to the default cyberpunk theme if the file does not exist.
/// Returns an error only for malformed files (bad TOML, invalid hex).
pub fn load_from_file(path: &Path) -> Result<Theme, RenderError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Theme::default()),
        Err(e) => return Err(RenderError::Io(e)),
    };

    let file: ThemeFile = toml::from_str(&contents)
        .map_err(|e| RenderError::Theme(format!("TOML parse error: {e}")))?;

    let c = &file.colors;
    Ok(Theme {
        bg: parse_hex_color(&c.bg)?,
        healthy: parse_hex_color(&c.healthy)?,
        neon_blue: parse_hex_color(&c.neon_blue)?,
        warning: parse_hex_color(&c.warning)?,
        critical: parse_hex_color(&c.critical)?,
        data: parse_hex_color(&c.data)?,
        xp_purple: parse_hex_color(&c.xp_purple)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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

    #[test]
    fn test_hex_color_parsing_valid() {
        assert_eq!(parse_hex_color("#FF0000").unwrap(), Color::Rgb(255, 0, 0));
        assert_eq!(parse_hex_color("#00FF00").unwrap(), Color::Rgb(0, 255, 0));
        assert_eq!(parse_hex_color("#0000FF").unwrap(), Color::Rgb(0, 0, 255));
        assert_eq!(parse_hex_color("#050A0E").unwrap(), Color::Rgb(5, 10, 14));
        // Without # prefix
        assert_eq!(parse_hex_color("FAFAFA").unwrap(), Color::Rgb(250, 250, 250));
    }

    #[test]
    fn test_hex_color_parsing_invalid_length() {
        assert!(parse_hex_color("#FFF").is_err(), "3-char hex should fail");
        assert!(parse_hex_color("#GGGGGG").is_err(), "non-hex chars should fail");
    }

    #[test]
    fn test_theme_default_matches_palette() {
        let theme = Theme::default();
        assert_eq!(theme.bg, Palette::BG);
        assert_eq!(theme.healthy, Palette::HEALTHY);
        assert_eq!(theme.critical, Palette::CRITICAL);
    }

    #[test]
    fn test_theme_fallback_on_missing_file() {
        let theme = load_from_file(Path::new("/nonexistent/theme.toml")).unwrap();
        assert_eq!(theme, Theme::default(), "missing file should return default theme");
    }

    #[test]
    fn test_load_from_file_parses_toml() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(
            tmp,
            r##"
[colors]
bg        = "#000000"
healthy   = "#00FF41"
neon_blue = "#008F11"
warning   = "#CCFF00"
critical  = "#FF0000"
data      = "#00FF41"
xp_purple = "#39FF14"
"##
        )
        .unwrap();

        let theme = load_from_file(tmp.path()).unwrap();
        assert_eq!(theme.bg, Color::Rgb(0, 0, 0));
        assert_eq!(theme.healthy, Color::Rgb(0, 255, 65));
        assert_eq!(theme.critical, Color::Rgb(255, 0, 0));
    }
}

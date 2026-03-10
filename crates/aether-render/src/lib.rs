//! TUI framework and 3D software rasterizer for Aether Terminal.
//!
//! Two layers: a ratatui-based TUI with tabbed views, and a custom 3D engine
//! that renders the process graph using Braille characters for 2x4 subpixel density.

pub mod braille;
pub mod effects;
pub mod engine;
pub mod palette;
pub mod tui;

/// Errors produced by the render crate.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Terminal I/O failure (crossterm / ratatui).
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Theme file parsing or validation error.
    #[error("theme error: {0}")]
    Theme(String),
}

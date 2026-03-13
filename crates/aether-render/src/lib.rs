//! TUI framework and 3D software rasterizer for Aether Terminal.
//!
//! Two layers: a ratatui-based TUI with tabbed views, and a custom 3D engine
//! that renders the process graph using Braille characters for 2x4 subpixel density.

pub(crate) mod braille;
pub(crate) mod effects;
pub(crate) mod engine;
pub(crate) mod palette;
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

/// Prediction data for rendering in Overview table and 3D scene.
///
/// Populated by the binary crate from `PredictedAnomaly` events.
/// Decoupled from `aether-predict` to respect hexagonal architecture.
#[derive(Debug, Clone)]
pub struct PredictionDisplay {
    /// Process ID.
    pub pid: u32,
    /// Process name.
    pub process_name: String,
    /// Short anomaly type label (e.g. "OOM", "CPU Spike").
    pub anomaly_label: String,
    /// Confidence score in [0.0, 1.0].
    pub confidence: f32,
    /// Estimated seconds until the anomaly manifests.
    pub eta_seconds: f32,
}

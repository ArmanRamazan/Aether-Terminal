//! Error types for the prediction engine.

/// Errors produced by the prediction subsystem.
#[derive(Debug, thiserror::Error)]
pub enum PredictError {
    /// ONNX model loading or validation failure.
    #[error("model error: {0}")]
    Model(String),

    /// Inference execution failure.
    #[error("inference error: {0}")]
    Inference(String),

    /// Feature extraction failure.
    #[error("feature extraction error: {0}")]
    Feature(String),
}

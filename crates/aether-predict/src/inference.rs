//! ONNX model loading and inference via tract.
//!
//! Wraps tract-onnx to load optimized ONNX models and run inference
//! on flat f32 tensors. Provides [`AnomalyDetector`] (autoencoder MSE)
//! and [`CpuForecaster`] (LSTM prediction with confidence).

use std::path::Path;

use tract_onnx::prelude::*;

use crate::error::PredictError;

/// Wrapper around a tract ONNX model for running inference.
pub struct OnnxModel {
    model: TypedRunnableModel<TypedModel>,
    input_shape: Vec<usize>,
}

impl OnnxModel {
    /// Load an ONNX model from disk, optimize it, and prepare for inference.
    pub fn load(path: &Path) -> Result<Self, PredictError> {
        let typed = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| PredictError::Model(e.to_string()))?
            .into_typed()
            .map_err(|e| PredictError::Model(e.to_string()))?;

        let input_shape = typed
            .input_fact(0)
            .map_err(|e| PredictError::Model(e.to_string()))?
            .shape
            .as_concrete()
            .ok_or_else(|| PredictError::Model("dynamic input shape not supported".into()))?
            .to_vec();

        let model = typed
            .into_optimized()
            .map_err(|e| PredictError::Model(e.to_string()))?
            .into_runnable()
            .map_err(|e| PredictError::Model(e.to_string()))?;

        Ok(Self { model, input_shape })
    }

    /// Run inference on a flat f32 slice, reshaped to the model's expected input.
    pub fn predict(&self, input: &[f32]) -> Result<Vec<f32>, PredictError> {
        let expected_len: usize = self.input_shape.iter().product();
        if input.len() != expected_len {
            return Err(PredictError::Inference(format!(
                "expected {} input values for shape {:?}, got {}",
                expected_len,
                self.input_shape,
                input.len()
            )));
        }

        let array = tract_ndarray::ArrayD::from_shape_vec(
            tract_ndarray::IxDyn(&self.input_shape),
            input.to_vec(),
        )
        .map_err(|e| PredictError::Inference(e.to_string()))?;
        let tensor: Tensor = array.into();

        let result = self
            .model
            .run(tvec!(tensor.into()))
            .map_err(|e| PredictError::Inference(e.to_string()))?;

        let output = result[0]
            .to_array_view::<f32>()
            .map_err(|e| PredictError::Inference(e.to_string()))?;

        Ok(output.iter().copied().collect())
    }

    /// Expected number of input elements.
    pub fn input_len(&self) -> usize {
        self.input_shape.iter().product()
    }
}

/// Autoencoder-based anomaly detector.
///
/// Computes anomaly score as MSE reconstruction error: high error indicates anomaly.
pub struct AnomalyDetector {
    model: OnnxModel,
}

impl AnomalyDetector {
    /// Load an autoencoder ONNX model from disk.
    pub fn load(path: &Path) -> Result<Self, PredictError> {
        Ok(Self {
            model: OnnxModel::load(path)?,
        })
    }

    /// Compute anomaly score as MSE between input and its reconstruction.
    pub fn detect(&self, input: &[f32]) -> Result<f32, PredictError> {
        let reconstructed = self.model.predict(input)?;
        if input.len() != reconstructed.len() {
            return Err(PredictError::Inference(format!(
                "reconstruction length mismatch: input={}, output={}",
                input.len(),
                reconstructed.len()
            )));
        }
        let mse = input
            .iter()
            .zip(&reconstructed)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            / input.len() as f32;
        Ok(mse)
    }
}

/// LSTM-based CPU usage forecaster.
///
/// Predicts future CPU usage with a confidence score.
pub struct CpuForecaster {
    model: OnnxModel,
}

impl CpuForecaster {
    /// Load a CPU forecast ONNX model from disk.
    pub fn load(path: &Path) -> Result<Self, PredictError> {
        Ok(Self {
            model: OnnxModel::load(path)?,
        })
    }

    /// Forecast CPU usage. Returns (predicted_cpu, confidence).
    pub fn forecast(&self, input: &[f32]) -> Result<(f32, f32), PredictError> {
        let output = self.model.predict(input)?;
        if output.len() < 2 {
            return Err(PredictError::Inference(format!(
                "expected at least 2 output values, got {}",
                output.len()
            )));
        }
        Ok((output[0], output[1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an identity model: output = input (passthrough).
    fn identity_typed_model(shape: &[usize]) -> TypedModel {
        let mut model = TypedModel::default();
        let source = model
            .add_source("input", f32::fact(shape))
            .expect("add source");
        model.set_output_outlets(&[source]).expect("set output");
        model
    }

    /// Build a model that adds 0.5 to every element: output = input + 0.5.
    fn offset_typed_model(shape: &[usize]) -> TypedModel {
        let mut model = TypedModel::default();
        let source = model
            .add_source("input", f32::fact(shape))
            .expect("add source");

        let total: usize = shape.iter().product();
        let offset_data = vec![0.5f32; total];
        let offset_tensor: Tensor =
            tract_ndarray::ArrayD::from_shape_vec(tract_ndarray::IxDyn(shape), offset_data)
                .expect("build offset tensor")
                .into();
        let offset = model
            .add_const("offset", offset_tensor.into_arc_tensor())
            .expect("add const");

        let sum = model
            .wire_node(
                "add",
                tract_onnx::tract_core::ops::math::add(),
                &[source, offset],
            )
            .expect("wire add");
        model.set_output_outlets(&sum).expect("set output");
        model
    }

    fn onnx_from_typed(typed: TypedModel) -> OnnxModel {
        let input_shape = typed
            .input_fact(0)
            .expect("input fact")
            .shape
            .as_concrete()
            .expect("concrete shape")
            .to_vec();
        let model = typed
            .into_optimized()
            .expect("optimize")
            .into_runnable()
            .expect("runnable");
        OnnxModel { model, input_shape }
    }

    #[test]
    fn test_predict_wrong_input_shape_fails() {
        let model = onnx_from_typed(identity_typed_model(&[1, 9]));
        let wrong_input = vec![1.0f32; 5];
        let result = model.predict(&wrong_input);
        assert!(result.is_err(), "wrong input shape should produce an error");
    }

    #[test]
    fn test_anomaly_detection_produces_positive_score() {
        let detector = AnomalyDetector {
            model: onnx_from_typed(offset_typed_model(&[1, 9])),
        };
        let input = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        let score = detector.detect(&input).expect("detect should succeed");
        assert!(score > 0.0, "anomaly score should be positive, got {score}");
    }
}

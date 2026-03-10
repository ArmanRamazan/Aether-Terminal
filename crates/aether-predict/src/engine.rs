//! Async prediction engine orchestrating feature extraction, windowing, and inference.
//!
//! Receives a shared [`WorldGraph`], extracts per-process features on a timer,
//! feeds them through a sliding window, and emits [`PredictedAnomaly`] events
//! for processes whose anomaly score exceeds the configured threshold.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use aether_core::graph::WorldGraph;

use crate::features::{FeatureExtractor, FeatureVector};
use crate::models::{ConfidenceScorer, PredictedAnomaly};
use crate::window::SlidingWindow;

#[cfg(feature = "predict")]
use crate::inference::{AnomalyDetector, CpuForecaster};

/// Configuration for the prediction engine.
#[derive(Debug, Clone)]
pub struct PredictConfig {
    /// Interval between inference ticks.
    pub inference_interval: Duration,
    /// Number of top-variance processes to run inference on per tick.
    pub top_n: usize,
    /// Minimum confidence to emit a prediction.
    pub confidence_threshold: f32,
    /// Directory containing ONNX model files.
    pub model_path: PathBuf,
}

impl Default for PredictConfig {
    fn default() -> Self {
        Self {
            inference_interval: Duration::from_secs(5),
            top_n: 20,
            confidence_threshold: 0.7,
            model_path: PathBuf::from("models"),
        }
    }
}

/// Async prediction engine: extracts features, maintains sliding windows,
/// runs ONNX inference, and emits [`PredictedAnomaly`] events.
#[allow(dead_code)] // prediction_tx + scorer used only with `predict` feature
pub struct PredictEngine {
    extractor: FeatureExtractor,
    window: SlidingWindow,
    #[cfg(feature = "predict")]
    anomaly_detector: Option<AnomalyDetector>,
    #[cfg(feature = "predict")]
    forecaster: Option<CpuForecaster>,
    prediction_tx: mpsc::Sender<PredictedAnomaly>,
    config: PredictConfig,
    scorer: ConfidenceScorer,
}

impl PredictEngine {
    /// Creates a new engine, attempting to load ONNX models from `config.model_path`.
    ///
    /// Missing models are logged as warnings; the engine degrades gracefully
    /// by skipping inference when no models are available.
    pub fn new(config: PredictConfig, prediction_tx: mpsc::Sender<PredictedAnomaly>) -> Self {
        #[cfg(feature = "predict")]
        let anomaly_detector = load_model(
            &config.model_path.join("anomaly_detector.onnx"),
            "anomaly detector",
            AnomalyDetector::load,
        );

        #[cfg(feature = "predict")]
        let forecaster = load_model(
            &config.model_path.join("cpu_forecast.onnx"),
            "CPU forecaster",
            CpuForecaster::load,
        );

        Self {
            extractor: FeatureExtractor::new(),
            window: SlidingWindow::default(),
            #[cfg(feature = "predict")]
            anomaly_detector,
            #[cfg(feature = "predict")]
            forecaster,
            prediction_tx,
            config,
            scorer: ConfidenceScorer::new(),
        }
    }

    /// Runs the prediction loop until cancelled.
    ///
    /// Reads the shared [`WorldGraph`] on each tick, extracts features,
    /// updates sliding windows, and runs inference on the top-N processes
    /// ranked by feature-vector variance.
    pub async fn run(&mut self, world: Arc<RwLock<WorldGraph>>, cancel: CancellationToken) {
        let mut interval = tokio::time::interval(self.config.inference_interval);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    self.process_tick(&world).await;
                }
            }
        }
    }

    /// Processes a single tick: extract, window, select, infer.
    async fn process_tick(&mut self, world: &Arc<RwLock<WorldGraph>>) {
        let (features, names) = {
            let guard = world.read().await;
            let features = self.extractor.extract(&guard);
            let names: HashMap<u32, String> =
                guard.processes().map(|p| (p.pid, p.name.clone())).collect();
            (features, names)
        };

        if features.is_empty() {
            return;
        }

        let alive_pids: Vec<u32> = features.keys().copied().collect();
        for (&pid, fv) in &features {
            self.window.push(pid, *fv);
        }
        self.window.cleanup(&alive_pids);

        let top_pids = select_top_n(&features, self.config.top_n);
        self.run_inference(&top_pids, &names).await;
    }

    /// Runs anomaly detection on selected processes and emits predictions.
    #[cfg(feature = "predict")]
    async fn run_inference(&self, pids: &[u32], names: &HashMap<u32, String>) {
        let detector = match &self.anomaly_detector {
            Some(d) => d,
            None => return,
        };

        for &pid in pids {
            let tensor = match self.window.to_tensor(pid) {
                Some(t) => t,
                None => continue,
            };

            let score = match detector.detect(&tensor) {
                Ok(s) => s,
                Err(e) => {
                    tracing::debug!("inference failed for pid {pid}: {e}");
                    continue;
                }
            };

            // Use anomaly score uniformly across types; ConfidenceScorer
            // picks the type whose per-type threshold is exceeded.
            let scores = [score; 5];
            if let Some((anomaly_type, confidence)) = self.scorer.classify(&scores) {
                if confidence < self.config.confidence_threshold {
                    continue;
                }
                let name = names.get(&pid).cloned().unwrap_or_default();
                let anomaly = PredictedAnomaly {
                    pid,
                    process_name: name,
                    anomaly_type,
                    confidence,
                    eta_seconds: 60.0,
                    recommended_action: anomaly_type.recommended_action().to_string(),
                };
                let _ = self.prediction_tx.send(anomaly).await;
            }
        }
    }

    /// No-op inference when the `predict` feature is disabled.
    #[cfg(not(feature = "predict"))]
    async fn run_inference(&self, _pids: &[u32], _names: &HashMap<u32, String>) {}
}

/// Loads a model from disk, logging a warning on failure and returning `None`.
#[cfg(feature = "predict")]
fn load_model<T>(
    path: &std::path::Path,
    label: &str,
    loader: fn(&std::path::Path) -> Result<T, crate::error::PredictError>,
) -> Option<T> {
    match loader(path) {
        Ok(m) => {
            tracing::info!("{label} loaded from {}", path.display());
            Some(m)
        }
        Err(e) => {
            tracing::warn!("{label} not loaded: {e}");
            None
        }
    }
}

/// Selects up to `n` PIDs with the highest feature-vector variance.
pub(crate) fn select_top_n(features: &HashMap<u32, FeatureVector>, n: usize) -> Vec<u32> {
    let mut scored: Vec<(u32, f32)> = features
        .iter()
        .map(|(&pid, fv)| (pid, variance(fv)))
        .collect();
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(n).map(|(pid, _)| pid).collect()
}

/// Variance of a feature vector across its 9 dimensions.
fn variance(fv: &FeatureVector) -> f32 {
    let n = fv.len() as f32;
    let mean = fv.iter().sum::<f32>() / n;
    fv.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / n
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState};
    use glam::Vec3;
    use std::time::Duration;

    fn make_process(pid: u32, cpu: f32, mem: u64) -> ProcessNode {
        ProcessNode {
            pid,
            ppid: 1,
            name: format!("proc-{pid}"),
            cpu_percent: cpu,
            mem_bytes: mem,
            state: ProcessState::Running,
            hp: 100.0,
            xp: 0,
            position_3d: Vec3::ZERO,
        }
    }

    #[tokio::test]
    async fn test_engine_processes_world_state() {
        let (tx, mut rx) = mpsc::channel(16);
        let config = PredictConfig {
            inference_interval: Duration::from_millis(50),
            ..Default::default()
        };
        let mut engine = PredictEngine::new(config, tx);

        let world = Arc::new(RwLock::new(WorldGraph::new()));
        {
            let mut w = world.write().await;
            w.add_process(make_process(1, 50.0, 1024));
            w.add_process(make_process(2, 80.0, 2048));
        }

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            engine.run(world, cancel_clone).await;
        });

        // Let the engine process at least two ticks.
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancel.cancel();
        handle.await.expect("engine task should not panic");

        // Without ONNX models loaded, no predictions are emitted,
        // but the engine should process world state without errors.
        assert!(
            rx.try_recv().is_err(),
            "no predictions expected without models"
        );
    }

    #[test]
    fn test_top_n_selects_highest_variance() {
        let mut features = HashMap::new();
        // Zero variance: all dimensions identical.
        features.insert(1, [0.5; 9]);
        // High variance: alternating 0 and 1.
        features.insert(2, [0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
        // Medium variance: slight spread.
        features.insert(3, [0.4, 0.6, 0.4, 0.6, 0.4, 0.6, 0.4, 0.6, 0.4]);

        let top = select_top_n(&features, 2);

        assert_eq!(top.len(), 2, "should return exactly 2 pids");
        assert_eq!(top[0], 2, "highest variance pid should be first");
        assert_eq!(top[1], 3, "second highest variance pid should be second");

        // Requesting more than available returns all.
        let all = select_top_n(&features, 100);
        assert_eq!(all.len(), 3, "should return all pids when n > count");
    }

    #[test]
    fn test_predictions_above_threshold_only() {
        let scorer = ConfidenceScorer::new();
        let config_threshold = 0.7;

        // All scores well below every per-type threshold → no classification.
        let low_scores = [0.3; 5];
        assert!(
            scorer.classify(&low_scores).is_none(),
            "low scores should not trigger any anomaly type"
        );

        // Score above CpuSpike threshold (0.70) but below OomRisk (0.75).
        let mid_scores = [0.72; 5];
        let result = scorer.classify(&mid_scores);
        assert!(result.is_some(), "0.72 exceeds CpuSpike threshold 0.70");
        let (_, confidence) = result.expect("classified");
        assert!(
            confidence >= config_threshold,
            "confidence {confidence} should meet config threshold {config_threshold}"
        );

        // Score above all per-type thresholds → highest-confidence match.
        let high_scores = [0.95; 5];
        let (_, confidence) = scorer.classify(&high_scores).expect("should classify");
        assert!(
            confidence >= config_threshold,
            "high confidence should exceed threshold"
        );
    }

    #[tokio::test]
    async fn test_works_without_model_graceful_degradation() {
        let (tx, mut rx) = mpsc::channel(16);
        // Point to a nonexistent model directory.
        let config = PredictConfig {
            inference_interval: Duration::from_millis(50),
            model_path: PathBuf::from("/nonexistent/path"),
            ..Default::default()
        };
        let mut engine = PredictEngine::new(config, tx);

        let world = Arc::new(RwLock::new(WorldGraph::new()));
        {
            let mut w = world.write().await;
            w.add_process(make_process(1, 90.0, 4096));
            w.add_process(make_process(2, 10.0, 512));
            w.add_process(make_process(3, 50.0, 2048));
        }

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            engine.run(world, cancel_clone).await;
        });

        // Run several ticks without models — should not panic or emit.
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();
        handle
            .await
            .expect("engine should run gracefully without models");

        assert!(
            rx.try_recv().is_err(),
            "no predictions should be emitted without loaded models"
        );
    }
}

# aether-predict

## Purpose
On-device predictive AI engine. Runs ONNX models via `tract` (pure-Rust, no Python/libtorch) to detect anomalies and forecast resource exhaustion. Feature-gated behind `#[cfg(feature = "predict")]`. Receives WorldState snapshots, extracts feature vectors, runs inference on a sliding window, emits PredictedAnomaly events.

## Modules
- `engine.rs` — PredictEngine: main loop, receives WorldState every 5s, orchestrates feature extraction → inference → emission
- `features.rs` — FeatureExtractor: converts WorldState into per-process feature vectors
- `window.rs` — SlidingWindow: ring buffer of feature snapshots (60 samples = 5 minutes at 5s interval)
- `models/mod.rs` — model loading, ONNX session management
- `models/anomaly.rs` — AnomalyDetector: autoencoder, reconstruction error = anomaly score
- `models/forecast.rs` — CpuForecaster: LSTM/Transformer, predicts CPU 60s ahead
- `types.rs` — PredictedAnomaly { pid, anomaly_type, confidence, eta_seconds, recommended_action }, AnomalyType enum
- `lib.rs` — re-exports, public API

## Feature Vector (per process, per tick)
```
[cpu_pct, mem_bytes, mem_delta, fd_count, thread_count,
 net_bytes_in, net_bytes_out, syscall_rate, io_wait_pct]
```

## Pipeline
```
WorldState (every 5s) → FeatureExtractor → SlidingWindow (60 samples)
                                                  ↓
                                           ONNX Inference (tract)
                                                  ↓
                                           PredictedAnomaly → mpsc → Core / Render
```

## Models (models/ directory at workspace root)
- `anomaly_detector.onnx` — Autoencoder, reconstruction error threshold = anomaly
- `cpu_forecast.onnx` — Time-series model, predicts CPU usage 60s ahead

## Rules
- ALL inference behind `#[cfg(feature = "predict")]` — crate must compile (as no-op) without the feature
- NEVER use Python, PyTorch, or TensorFlow — tract-onnx only (pure-Rust)
- Streaming inference: process sliding window each tick, no batch accumulation
- Inference must complete within 100ms per tick (5s interval gives ample headroom)
- SlidingWindow is a fixed-size ring buffer — no heap growth over time
- Feature extraction must be allocation-free in steady state (pre-allocated vectors)
- NEVER depend on other aether-* crates except aether-core
- Models are loaded from disk at startup — no network downloads
- If model file is missing, log warning and disable predictions (don't crash)

## Testing
```bash
# Unit tests (mock model, synthetic features)
cargo test -p aether-predict

# Integration tests with real ONNX models
cargo test -p aether-predict --features predict
```
Unit tests must use synthetic feature vectors and mock inference — never require actual ONNX model files.

## Key Dependencies
- tract-onnx (ONNX model loading and inference)
- tract-core (tensor operations)
- tokio (async task, mpsc channels)
- aether-core (WorldState, PredictedAnomaly, traits)

# ADR-006: On-device predictive AI via tract-onnx

**Status:** Accepted
**Date:** 2026-03-08

## Context

The current MCP integration sends data to external LLMs for analysis. This is reactive — anomalies are detected after they happen. We want to predict OOM, CPU spikes, and resource exhaustion before they occur, without external API calls.

## Decision

Build `aether-predict` crate with `tract-onnx` (pure-Rust ONNX runtime) for on-device inference. Pre-trained models shipped as `.onnx` files. Feature extraction from WorldState stream.

## Rationale

- **tract over onnxruntime-rs**: tract is pure Rust, no C++ runtime dependency, smaller binary. Sufficient for small models (autoencoder, LSTM).
- **On-device over cloud**: Zero latency, no API costs, works offline, no data leaves the machine.
- **ONNX format**: Train models in PyTorch/TensorFlow, export to ONNX, run in Rust. Decouples training from inference.
- **Connects to autograd-engine**: Models can be trained with the sibling project, demonstrating full ML pipeline.

## Technical Approach

- Feature extraction: sliding window of 60 samples (5 min at 5s intervals)
- Feature vector per process: `[cpu, mem, mem_delta, fd_count, threads, net_in, net_out, syscall_rate, io_wait]`
- Models:
  - `anomaly_detector.onnx` — Autoencoder, high reconstruction error = anomaly
  - `cpu_forecast.onnx` — LSTM predicting CPU 60s ahead
- Inference runs every 5s in a dedicated tokio task
- Results sent via `mpsc<PredictedAnomaly>` to core and render

## Consequences

- Models must be pre-trained offline (training not in scope for this project)
- tract adds ~1.5MB to binary
- Feature-gated: `#[cfg(feature = "predict")]` to keep lean builds possible
- Initial models can be simple (threshold-based) with ONNX upgrade path

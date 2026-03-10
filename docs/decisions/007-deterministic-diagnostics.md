# ADR-007: Deterministic diagnostic engine over ML-based analysis

**Status:** Accepted
**Date:** 2026-03-10

## Context

PoC used tract-onnx (aether-predict) for anomaly detection and CPU forecasting. However, no trained models exist, and ML-based analysis is a black box — users can't understand why an alert fired. The system needs real diagnostic value for MVP.

## Decision

Build a deterministic diagnostic engine (aether-analyze) as the primary analysis layer. Rule-based with stateful analyzers: trend detection (linear regression), capacity planning, correlation analysis, and z-score anomaly detection. ML (aether-predict) remains as optional secondary layer.

## Rationale

- **Explainable**: Every diagnostic shows exactly which rule fired, what values triggered it, and what the trend is. No black box.
- **Testable**: Deterministic rules have deterministic outputs. 100% unit-testable.
- **Zero dependencies**: No ONNX models needed. Works out of the box.
- **Composable**: Builtin rules + user JIT rules (aether-script) + optional ML predictions all feed into same Diagnostic output.
- **Production-proven**: Prometheus alerting rules follow the same pattern. Familiar to SREs.

## Consequences

- Won't detect novel anomaly patterns that rules don't cover (ML advantage)
- Need to maintain 30+ rules with sensible defaults
- Users may want to customize thresholds — expose via config later
- ML can be added as another analyzer feeding into the same pipeline

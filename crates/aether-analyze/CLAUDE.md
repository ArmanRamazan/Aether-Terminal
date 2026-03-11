# aether-analyze

## Purpose
Deterministic diagnostic engine with layered analysis. Collects system metrics, applies trend/capacity/correlation/anomaly analyzers, evaluates rules, and produces diagnostics with recommendations. No ML — all analysis is deterministic.

## Modules
- `error.rs` — AnalyzeError enum (thiserror)
- `collectors/` — (future) procfs, cgroup, perf data collectors
- `analyzers/` — (future) trend, capacity, correlation, anomaly analyzers
- `rules/` — (future) engine, builtin rules, rule types
- `recommendations/` — (future) diagnostic generator
- `store.rs` — (future) MetricStore for time-series metric storage
- `engine.rs` — (future) AnalyzeEngine orchestrating the full pipeline

## Key Types
- `AnalyzeEngine` — orchestrates collectors → analyzers → rules → recommendations
- `MetricStore` — in-memory time-series storage with windowed queries
- `RuleEngine` — evaluates rules against analyzer output, produces RuleFinding
- `TrendAnalyzer` — detects trends (rising, falling, stable) in metric series
- `CapacityAnalyzer` — projects resource exhaustion timelines

## Data Flow
```
Collectors → MetricStore → Analyzers → RuleEngine → RecommendationGenerator → Diagnostic
```

## Rules
- All analysis must be deterministic — no randomness, no ML inference
- Rules produce RuleFinding → generator produces Diagnostic (from aether-core)
- Depends only on aether-core, never on other aether-* crates
- MetricStore keeps bounded history (configurable window)

## Testing
```bash
cargo test -p aether-analyze
```

## Key Dependencies
- aether-core (path dependency)
- tokio (async runtime)
- thiserror (error types)
- tracing (instrumentation)
- serde (serialization)

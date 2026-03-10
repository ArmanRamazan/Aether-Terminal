# ADR-008: Bidirectional Prometheus integration

**Status:** Accepted
**Date:** 2026-03-10

## Context

Aether-Terminal runs on a single host but needs to integrate with existing monitoring infrastructure (Prometheus + Grafana) and eventually support cluster-wide views.

## Decision

Build aether-metrics crate with two capabilities: Prometheus exporter (/metrics endpoint) and PromQL consumer (HTTP client). Both optional via CLI flags.

## Rationale

- **Exporter**: Teams already have Grafana dashboards. Exposing /metrics lets them add Aether data without changing workflows. Standard Prometheus text format (OpenMetrics).
- **Consumer**: Reading from Prometheus gives cluster visibility without building a custom agent protocol. PromQL is battle-tested for metric queries.
- **Cluster-ready**: Consumer produces TimeSeries — same format as local data. Analyzers work identically on local and remote data.
- **axum reuse**: SSE transport already uses axum. Adding /metrics is one more route.

## Consequences

- reqwest dependency for consumer (HTTP client)
- Need to handle Prometheus being down gracefully (consumer errors don't crash Aether)
- /metrics endpoint must not leak sensitive data (PIDs, process names are acceptable for monitoring)
- Polling interval must be configurable (default 15s matches Prometheus scrape interval)

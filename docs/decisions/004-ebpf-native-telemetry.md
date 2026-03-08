# ADR-004: Native eBPF telemetry engine

**Status:** Accepted
**Date:** 2026-03-08
**Supersedes:** ADR-001 (sysinfo as primary, eBPF as future addition)

## Context

ADR-001 chose sysinfo for MVP. Now the project aims for 10/10 implementation complexity. eBPF provides kernel-level observability: every syscall, every TCP connection, every fork/exec — at 100K+ events/sec with zero-copy ring buffers.

## Decision

Build a dedicated `aether-ebpf` crate using `aya` (pure-Rust eBPF loader). sysinfo remains as crossplatform fallback. eBPF is the primary telemetry source on Linux.

## Rationale

- `aya` is pure Rust — no libbpf-sys/C dependency, aligns with project philosophy
- Ring buffer provides zero-copy event streaming into tokio channels
- Demonstrates kernel-level systems programming (rare in portfolios)
- Per-syscall granularity enables predictive AI with richer features
- sysinfo fallback preserves macOS/Windows development workflow

## Technical Approach

- BPF programs written in C, compiled with `aya-bpf` toolchain
- Ring buffer (per-CPU, 256KB) for high-throughput event delivery
- Feature-gated: `#[cfg(feature = "ebpf")]`
- Requires `CAP_BPF` or root on Linux
- Falls back to sysinfo transparently when eBPF unavailable

## Consequences

- Linux-only for full telemetry (acceptable — production servers are Linux)
- Requires BPF-capable kernel (5.8+)
- `unsafe` code limited to eBPF FFI boundary, fully documented
- Additional CI complexity: eBPF tests need privileged runners

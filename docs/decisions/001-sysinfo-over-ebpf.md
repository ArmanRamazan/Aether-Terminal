# ADR-001: sysinfo over eBPF for initial release

**Status:** Accepted
**Date:** 2026-03-08

## Context
System metrics can be collected via eBPF (kernel-level, Linux-only, requires root) or sysinfo crate (user-space, crossplatform, no privileges).

## Decision
Use `sysinfo` as primary SystemProbe implementation. eBPF is future feature-gated addition.

## Rationale
- Development on WSL2 — eBPF support unreliable
- sysinfo works on Linux/macOS/Windows immediately
- Can demonstrate hexagonal architecture by swapping implementations later
- eBPF adds complexity that could delay portfolio delivery

## Consequences
- Lower granularity than eBPF (no kernel-level events)
- Process tree updates limited to polling interval (1Hz)
- Network data limited to interface-level (no per-connection tracking without libpcap)

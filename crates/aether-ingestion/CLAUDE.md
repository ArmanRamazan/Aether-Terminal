# aether-ingestion

## Purpose
System data collection. Implements `SystemProbe` trait from aether-core. Primary implementation: `SysinfoProbe` (crossplatform). Future: `EbpfProbe` (Linux only, feature-gated).

## Modules
- `sysinfo_probe.rs` — SysinfoProbe implementing SystemProbe trait
- `pipeline.rs` — IngestionPipeline with dual-tick async tasks
- `dpi.rs` — (future) Deep Packet Inspection via etherparse
- `ebpf_probe.rs` — (future) eBPF-based probe, behind `#[cfg(feature = "ebpf")]`

## Rules
- SysinfoProbe must work on Linux, macOS, and Windows (WSL2)
- Refresh sysinfo::System only on snapshot() call, NOT on construction
- fast_tick: 100ms (10Hz for MVP). Sends MetricsUpdate
- slow_tick: 1000ms (1Hz). Sends TopologyChange (full process tree rebuild)
- Pipeline uses CancellationToken for graceful shutdown
- Memory: do NOT store history here. That's the render crate's job.
- NEVER do heavy computation in fast_tick — just read cached sysinfo data

## Testing
```bash
cargo test -p aether-ingestion
```
Test that snapshot() returns real process data (pid > 0, name not empty).

## Key Dependencies
- aether-core (path dependency)
- sysinfo
- tokio (runtime, channels, time)
- tokio-util (CancellationToken)

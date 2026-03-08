# aether-ebpf

## Purpose
eBPF telemetry engine. Loads BPF programs into the Linux kernel via `aya` (pure-Rust, no libbpf-sys), reads events from ring buffers, and bridges them into the core event pipeline. Feature-gated behind `#[cfg(feature = "ebpf")]`. Falls back to sysinfo on non-Linux or when unavailable.

## Modules
- `loader.rs` — BpfLoader: loads compiled BPF bytecode, attaches probes, initializes ring buffers
- `ring_buffer.rs` — RingBufReader: async reader that polls per-CPU ring buffers via `aya::maps::RingBuf`, deserializes into typed events
- `probes/mod.rs` — probe configuration and attachment logic
- `probes/process.rs` — tracepoint/sched_process_fork, tracepoint/sched_process_exit
- `probes/network.rs` — kprobe/tcp_connect, kprobe/tcp_close, tracepoint/net_dev_xmit
- `probes/syscall.rs` — raw_tracepoint/sys_enter with configurable syscall filter
- `events.rs` — RawKernelEvent, ProcessFork, ProcessExit, TcpConnect, TcpClose, SyscallEvent structs
- `bridge.rs` — converts RawKernelEvent → aether-core SystemEvent, sends via mpsc channel
- `lib.rs` — re-exports, EbpfEngine top-level struct

## BPF Programs (bpf/ directory at workspace root)
- `process_monitor.bpf.c` — fork/exit tracepoints
- `net_monitor.bpf.c` — TCP connect/close kprobes, net_dev_xmit tracepoint
- `syscall_monitor.bpf.c` — raw tracepoint sys_enter with configurable filter

## Rules
- ALL kernel interaction is behind `#[cfg(feature = "ebpf")]` — crate must compile (as no-op) without the feature
- NEVER use libbpf-sys or libbpf-rs — aya only (pure-Rust)
- Ring buffer reads must be zero-copy: deserialize directly into Rust structs, no intermediate allocation
- Target throughput: 100K events/sec from ring buffer
- Per-CPU ring buffer size: 256KB (configurable)
- Event structs shared between BPF C code and Rust must match exactly (repr(C), packed)
- NEVER depend on other aether-* crates except aether-core
- BPF programs are pre-compiled bytecode — do NOT require clang/llvm at runtime
- Graceful degradation: if BPF loading fails (no root, no CAP_BPF), log warning and return

## Ring Buffer Protocol
```
Kernel → Ring Buffer (per-CPU, 256KB) → aya::maps::RingBuf → tokio mpsc → Core
```

## Event Types
```
ProcessFork { parent_pid, child_pid, timestamp_ns }
ProcessExit { pid, exit_code, timestamp_ns }
TcpConnect { pid, src, dst, timestamp_ns }
TcpClose { pid, src, dst, bytes_sent, bytes_recv, duration_ns }
SyscallEvent { pid, syscall_nr, latency_ns }
```

## Testing
```bash
# Unit tests (no kernel required, mock ring buffer)
cargo test -p aether-ebpf

# Integration tests (Linux only, requires root/CAP_BPF)
sudo cargo test -p aether-ebpf --features ebpf
```
Mock the ring buffer reader for unit tests — never require root for `cargo test`.

## Key Dependencies
- aya (BPF loader, map access, program attachment)
- aya-obj (BPF object file parsing)
- tokio (async ring buffer polling, mpsc channels)
- aether-core (SystemEvent, traits)

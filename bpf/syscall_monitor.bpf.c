// SPDX-License-Identifier: GPL-2.0
// Syscall monitor — raw tracepoint sys_enter with PID filtering.
//
// This BPF program attaches to the raw_tracepoint/sys_enter hook, emitting
// events into a ring buffer for userspace consumption via aya. Only traces
// PIDs present in the target_pids map (empty map = no events).
// Not compiled automatically — requires clang/llvm toolchain.

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

/// Event emitted on syscall entry. Must match Rust SyscallEvent layout (16 bytes).
struct syscall_event {
    __u32 pid;
    __u32 syscall_nr;
    __u64 timestamp_ns;
};

/// Ring buffer map for syscall events (256 KB).
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} syscall_events SEC(".maps");

/// PID filter: only trace PIDs present in this map. Empty map = no events.
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 1024);
    __type(key, __u32);
    __type(value, __u8);
} target_pids SEC(".maps");

SEC("raw_tracepoint/sys_enter")
int handle_sys_enter(struct bpf_raw_tracepoint_args *ctx)
{
    struct syscall_event *event;
    __u64 pid_tgid;
    __u32 pid;

    pid_tgid = bpf_get_current_pid_tgid();
    pid = pid_tgid >> 32;

    // Only trace PIDs in the filter map.
    if (!bpf_map_lookup_elem(&target_pids, &pid))
        return 0;

    event = bpf_ringbuf_reserve(&syscall_events, sizeof(*event), 0);
    if (!event)
        return 0;

    event->pid = pid;
    event->syscall_nr = (__u32)ctx->args[1];
    event->timestamp_ns = bpf_ktime_get_ns();

    bpf_ringbuf_submit(event, 0);
    return 0;
}

char LICENSE[] SEC("license") = "GPL";

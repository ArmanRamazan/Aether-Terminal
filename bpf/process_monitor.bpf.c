// SPDX-License-Identifier: GPL-2.0
// Process lifecycle monitor — fork and exit tracepoints.
//
// This BPF program attaches to sched_process_fork and sched_process_exit
// tracepoints, emitting events into a ring buffer for userspace consumption
// via aya. Not compiled automatically — requires clang/llvm toolchain.

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

/// Event emitted on process fork. Must match Rust ProcessForkEvent layout.
struct process_fork_event {
    __u32 parent_pid;
    __u32 child_pid;
    __u64 timestamp_ns;
};

/// Event emitted on process exit. Must match Rust ProcessExitEvent layout.
struct process_exit_event {
    __u32 pid;
    __s32 exit_code;
    __u64 timestamp_ns;
};

/// Ring buffer map for fork events (256 KB).
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} fork_events SEC(".maps");

/// Ring buffer map for exit events (256 KB).
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} exit_events SEC(".maps");

SEC("tracepoint/sched/sched_process_fork")
int handle_fork(struct trace_event_raw_sched_process_fork *ctx)
{
    struct process_fork_event *event;

    event = bpf_ringbuf_reserve(&fork_events, sizeof(*event), 0);
    if (!event)
        return 0;

    event->parent_pid = BPF_CORE_READ(ctx, parent_pid);
    event->child_pid = BPF_CORE_READ(ctx, child_pid);
    event->timestamp_ns = bpf_ktime_get_ns();

    bpf_ringbuf_submit(event, 0);
    return 0;
}

SEC("tracepoint/sched/sched_process_exit")
int handle_exit(struct trace_event_raw_sched_process_template *ctx)
{
    struct process_exit_event *event;
    struct task_struct *task;

    event = bpf_ringbuf_reserve(&exit_events, sizeof(*event), 0);
    if (!event)
        return 0;

    task = (struct task_struct *)bpf_get_current_task();

    event->pid = BPF_CORE_READ(ctx, pid);
    event->exit_code = BPF_CORE_READ(task, exit_code) >> 8;
    event->timestamp_ns = bpf_ktime_get_ns();

    bpf_ringbuf_submit(event, 0);
    return 0;
}

char LICENSE[] SEC("license") = "GPL";

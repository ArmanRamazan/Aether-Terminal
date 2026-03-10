// SPDX-License-Identifier: GPL-2.0
// Network connection monitor — TCP connect and close kprobes.
//
// This BPF program attaches to tcp_v4_connect and tcp_close kernel functions,
// emitting events into ring buffers for userspace consumption via aya.
// Not compiled automatically — requires clang/llvm toolchain.

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>

/// Event emitted on TCP connect. Must match Rust TcpConnectEvent layout (24 bytes).
struct tcp_connect_event {
    __u32 pid;
    __u32 saddr;
    __u32 daddr;
    __u16 sport;
    __u16 dport;
    __u64 timestamp_ns;
};

/// Event emitted on TCP close. Must match Rust TcpCloseEvent layout (40 bytes).
struct tcp_close_event {
    __u32 pid;
    __u32 saddr;
    __u32 daddr;
    __u16 sport;
    __u16 dport;
    __u64 bytes_sent;
    __u64 bytes_recv;
    __u64 duration_ns;
};

/// Ring buffer map for TCP connect events (256 KB).
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} connect_events SEC(".maps");

/// Ring buffer map for TCP close events (256 KB).
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024);
} close_events SEC(".maps");

/// Hash map tracking connection start timestamps for duration calculation.
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 65536);
    __type(key, __u64);
    __type(value, __u64);
} conn_start SEC(".maps");

SEC("kprobe/tcp_v4_connect")
int handle_tcp_connect(struct pt_regs *ctx)
{
    struct tcp_connect_event *event;
    struct sock *sk;
    __u64 pid_tgid;
    __u64 ts;

    pid_tgid = bpf_get_current_pid_tgid();
    sk = (struct sock *)PT_REGS_PARM1(ctx);

    event = bpf_ringbuf_reserve(&connect_events, sizeof(*event), 0);
    if (!event)
        return 0;

    ts = bpf_ktime_get_ns();

    event->pid = pid_tgid >> 32;
    event->saddr = BPF_CORE_READ(sk, __sk_common.skc_rcv_saddr);
    event->daddr = BPF_CORE_READ(sk, __sk_common.skc_daddr);
    event->sport = BPF_CORE_READ(sk, __sk_common.skc_num);
    event->dport = __bpf_ntohs(BPF_CORE_READ(sk, __sk_common.skc_dport));
    event->timestamp_ns = ts;

    bpf_ringbuf_submit(event, 0);

    // Store connection start time keyed by sock pointer for duration tracking.
    __u64 sk_ptr = (__u64)sk;
    bpf_map_update_elem(&conn_start, &sk_ptr, &ts, BPF_ANY);

    return 0;
}

SEC("kprobe/tcp_close")
int handle_tcp_close(struct pt_regs *ctx)
{
    struct tcp_close_event *event;
    struct sock *sk;
    struct tcp_sock *tp;
    __u64 pid_tgid;
    __u64 *start_ts;
    __u64 sk_ptr;

    pid_tgid = bpf_get_current_pid_tgid();
    sk = (struct sock *)PT_REGS_PARM1(ctx);
    tp = (struct tcp_sock *)sk;

    event = bpf_ringbuf_reserve(&close_events, sizeof(*event), 0);
    if (!event)
        return 0;

    event->pid = pid_tgid >> 32;
    event->saddr = BPF_CORE_READ(sk, __sk_common.skc_rcv_saddr);
    event->daddr = BPF_CORE_READ(sk, __sk_common.skc_daddr);
    event->sport = BPF_CORE_READ(sk, __sk_common.skc_num);
    event->dport = __bpf_ntohs(BPF_CORE_READ(sk, __sk_common.skc_dport));
    event->bytes_sent = BPF_CORE_READ(tp, bytes_sent);
    event->bytes_recv = BPF_CORE_READ(tp, bytes_received);

    // Calculate connection duration from stored start timestamp.
    sk_ptr = (__u64)sk;
    start_ts = bpf_map_lookup_elem(&conn_start, &sk_ptr);
    if (start_ts) {
        event->duration_ns = bpf_ktime_get_ns() - *start_ts;
        bpf_map_delete_elem(&conn_start, &sk_ptr);
    } else {
        event->duration_ns = 0;
    }

    bpf_ringbuf_submit(event, 0);
    return 0;
}

char LICENSE[] SEC("license") = "GPL";

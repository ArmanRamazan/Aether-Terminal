//! Raw kernel event types matching BPF C struct layouts.
//!
//! These structs use `#[repr(C)]` to match the exact memory layout of events
//! written by BPF programs into ring buffers.

/// Event emitted by `sched_process_fork` tracepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ProcessForkEvent {
    /// PID of the parent process.
    pub parent_pid: u32,
    /// PID of the newly created child process.
    pub child_pid: u32,
    /// Kernel timestamp in nanoseconds (from `bpf_ktime_get_ns`).
    pub timestamp_ns: u64,
}

/// Event emitted by `sched_process_exit` tracepoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ProcessExitEvent {
    /// PID of the exiting process.
    pub pid: u32,
    /// Process exit code.
    pub exit_code: i32,
    /// Kernel timestamp in nanoseconds (from `bpf_ktime_get_ns`).
    pub timestamp_ns: u64,
}

/// Event emitted by `kprobe/tcp_v4_connect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TcpConnectEvent {
    /// PID of the connecting process.
    pub pid: u32,
    /// Source IPv4 address (network byte order).
    pub saddr: u32,
    /// Destination IPv4 address (network byte order).
    pub daddr: u32,
    /// Source port (network byte order).
    pub sport: u16,
    /// Destination port (network byte order).
    pub dport: u16,
    /// Kernel timestamp in nanoseconds.
    pub timestamp_ns: u64,
}

/// Event emitted by `kprobe/tcp_close`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TcpCloseEvent {
    /// PID of the closing process.
    pub pid: u32,
    /// Source IPv4 address (network byte order).
    pub saddr: u32,
    /// Destination IPv4 address (network byte order).
    pub daddr: u32,
    /// Source port (network byte order).
    pub sport: u16,
    /// Destination port (network byte order).
    pub dport: u16,
    /// Bytes sent over the connection lifetime.
    pub bytes_sent: u64,
    /// Bytes received over the connection lifetime.
    pub bytes_recv: u64,
    /// Connection duration in nanoseconds (connect to close).
    pub duration_ns: u64,
}

/// Event emitted by `raw_tracepoint/sys_enter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SyscallEvent {
    /// PID of the calling process.
    pub pid: u32,
    /// Syscall number (architecture-specific).
    pub syscall_nr: u32,
    /// Kernel timestamp in nanoseconds.
    pub timestamp_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_fork_event_repr_c_layout() {
        assert_eq!(
            mem::size_of::<ProcessForkEvent>(),
            16,
            "fork event should be 4+4+8 = 16 bytes"
        );
    }

    #[test]
    fn test_exit_event_repr_c_layout() {
        assert_eq!(
            mem::size_of::<ProcessExitEvent>(),
            16,
            "exit event should be 4+4+8 = 16 bytes"
        );
    }

    #[test]
    fn test_fork_event_fields() {
        let event = ProcessForkEvent {
            parent_pid: 1,
            child_pid: 42,
            timestamp_ns: 123_456_789,
        };
        assert_eq!(event.parent_pid, 1);
        assert_eq!(event.child_pid, 42);
        assert_eq!(event.timestamp_ns, 123_456_789);
    }

    #[test]
    fn test_exit_event_fields() {
        let event = ProcessExitEvent {
            pid: 42,
            exit_code: -9,
            timestamp_ns: 987_654_321,
        };
        assert_eq!(event.pid, 42);
        assert_eq!(event.exit_code, -9);
        assert_eq!(event.timestamp_ns, 987_654_321);
    }

    #[test]
    fn test_tcp_connect_event_repr_c_layout() {
        assert_eq!(
            mem::size_of::<TcpConnectEvent>(),
            24,
            "tcp connect event should be 4+4+4+2+2+8 = 24 bytes"
        );
    }

    #[test]
    fn test_tcp_close_event_repr_c_layout() {
        assert_eq!(
            mem::size_of::<TcpCloseEvent>(),
            40,
            "tcp close event should be 4+4+4+2+2+8+8+8 = 40 bytes"
        );
    }

    #[test]
    fn test_syscall_event_repr_c_layout() {
        assert_eq!(
            mem::size_of::<SyscallEvent>(),
            16,
            "syscall event should be 4+4+8 = 16 bytes"
        );
    }

    #[test]
    fn test_tcp_connect_event_fields() {
        let event = TcpConnectEvent {
            pid: 10,
            saddr: 0x7F000001,
            daddr: 0xC0A80001,
            sport: 12345,
            dport: 80,
            timestamp_ns: 111_222_333,
        };
        assert_eq!(event.pid, 10);
        assert_eq!(event.saddr, 0x7F000001);
        assert_eq!(event.daddr, 0xC0A80001);
        assert_eq!(event.sport, 12345);
        assert_eq!(event.dport, 80);
        assert_eq!(event.timestamp_ns, 111_222_333);
    }

    #[test]
    fn test_tcp_close_event_fields() {
        let event = TcpCloseEvent {
            pid: 20,
            saddr: 0x7F000001,
            daddr: 0xC0A80001,
            sport: 54321,
            dport: 443,
            bytes_sent: 1024,
            bytes_recv: 2048,
            duration_ns: 500_000_000,
        };
        assert_eq!(event.pid, 20);
        assert_eq!(event.bytes_sent, 1024);
        assert_eq!(event.bytes_recv, 2048);
        assert_eq!(event.duration_ns, 500_000_000);
    }

    #[test]
    fn test_syscall_event_fields() {
        let event = SyscallEvent {
            pid: 30,
            syscall_nr: 1,
            timestamp_ns: 444_555_666,
        };
        assert_eq!(event.pid, 30);
        assert_eq!(event.syscall_nr, 1);
        assert_eq!(event.timestamp_ns, 444_555_666);
    }
}

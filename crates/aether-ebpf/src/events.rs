//! Raw kernel event types matching BPF C struct layouts.
//!
//! These structs use `#[repr(C)]` to match the exact memory layout of events
//! written by BPF programs into ring buffers.

use std::mem;

use crate::error::EbpfError;

/// Event type discriminants for ring buffer dispatch.
pub const EVENT_TYPE_FORK: u32 = 1;
/// Process exit event type.
pub const EVENT_TYPE_EXIT: u32 = 2;
/// TCP connect event type.
pub const EVENT_TYPE_TCP_CONNECT: u32 = 3;
/// TCP close event type.
pub const EVENT_TYPE_TCP_CLOSE: u32 = 4;
/// Syscall event type.
pub const EVENT_TYPE_SYSCALL: u32 = 5;

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

/// Raw kernel event deserialized from a BPF ring buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum RawKernelEvent {
    /// Process fork detected.
    Fork(ProcessForkEvent),
    /// Process exit detected.
    Exit(ProcessExitEvent),
    /// TCP connection initiated.
    TcpConnect(TcpConnectEvent),
    /// TCP connection closed.
    TcpClose(TcpCloseEvent),
    /// Syscall entered.
    Syscall(SyscallEvent),
}

/// Parse raw bytes from a ring buffer into a typed kernel event.
///
/// # Safety
///
/// `data` must contain a valid `#[repr(C)]` struct matching the given `event_type`.
/// The BPF ring buffer guarantees this when the BPF programs and Rust structs are
/// in sync. Callers must ensure `data` came from a trusted BPF ring buffer.
pub unsafe fn parse_event(event_type: u32, data: &[u8]) -> Result<RawKernelEvent, EbpfError> {
    match event_type {
        EVENT_TYPE_FORK => parse_repr_c::<ProcessForkEvent>(event_type, data)
            .map(RawKernelEvent::Fork),
        EVENT_TYPE_EXIT => parse_repr_c::<ProcessExitEvent>(event_type, data)
            .map(RawKernelEvent::Exit),
        EVENT_TYPE_TCP_CONNECT => parse_repr_c::<TcpConnectEvent>(event_type, data)
            .map(RawKernelEvent::TcpConnect),
        EVENT_TYPE_TCP_CLOSE => parse_repr_c::<TcpCloseEvent>(event_type, data)
            .map(RawKernelEvent::TcpClose),
        EVENT_TYPE_SYSCALL => parse_repr_c::<SyscallEvent>(event_type, data)
            .map(RawKernelEvent::Syscall),
        _ => Err(EbpfError::ParseError {
            event_type,
            data_len: data.len(),
            reason: "unknown event type".into(),
        }),
    }
}

/// Parse a `#[repr(C)]` struct from raw bytes with size validation.
///
/// # Safety
///
/// Same requirements as [`parse_event`] — `data` must be a valid representation
/// of `T` from a trusted BPF ring buffer.
unsafe fn parse_repr_c<T: Copy>(event_type: u32, data: &[u8]) -> Result<T, EbpfError> {
    let expected = mem::size_of::<T>();
    if data.len() < expected {
        return Err(EbpfError::ParseError {
            event_type,
            data_len: data.len(),
            reason: format!("expected at least {expected} bytes"),
        });
    }
    // SAFETY: T is repr(C) + Copy, buffer size verified above.
    // read_unaligned handles potential misalignment from the ring buffer.
    Ok(std::ptr::read_unaligned(data.as_ptr().cast::<T>()))
}

/// Convert a null-terminated BPF `comm` field to a Rust string.
///
/// BPF `comm` fields are fixed-size `[u8; 16]` arrays, null-padded.
/// Returns the portion before the first null byte as a UTF-8 string,
/// replacing invalid bytes with the Unicode replacement character.
pub fn comm_to_string(comm: &[u8; 16]) -> String {
    let len = comm.iter().position(|&b| b == 0).unwrap_or(16);
    String::from_utf8_lossy(&comm[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_sizes_match_c_layout() {
        assert_eq!(
            mem::size_of::<ProcessForkEvent>(),
            16,
            "fork: 4+4+8 = 16 bytes"
        );
        assert_eq!(
            mem::size_of::<ProcessExitEvent>(),
            16,
            "exit: 4+4+8 = 16 bytes"
        );
        assert_eq!(
            mem::size_of::<TcpConnectEvent>(),
            24,
            "tcp_connect: 4+4+4+2+2+8 = 24 bytes"
        );
        assert_eq!(
            mem::size_of::<TcpCloseEvent>(),
            40,
            "tcp_close: 4+4+4+2+2+8+8+8 = 40 bytes"
        );
        assert_eq!(
            mem::size_of::<SyscallEvent>(),
            16,
            "syscall: 4+4+8 = 16 bytes"
        );
    }

    #[test]
    fn test_parse_process_fork_event() {
        let event = ProcessForkEvent {
            parent_pid: 1,
            child_pid: 42,
            timestamp_ns: 123_456_789,
        };
        // SAFETY: ProcessForkEvent is repr(C), Copy. Transmuting to bytes is safe.
        let bytes: [u8; mem::size_of::<ProcessForkEvent>()] = unsafe { mem::transmute(event) };

        // SAFETY: bytes are a valid ProcessForkEvent representation.
        let parsed = unsafe { parse_event(EVENT_TYPE_FORK, &bytes) }
            .expect("should parse valid fork bytes");

        match parsed {
            RawKernelEvent::Fork(f) => {
                assert_eq!(f.parent_pid, 1);
                assert_eq!(f.child_pid, 42);
                assert_eq!(f.timestamp_ns, 123_456_789);
            }
            other => panic!("expected Fork, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_process_exit_event() {
        let event = ProcessExitEvent {
            pid: 100,
            exit_code: -9,
            timestamp_ns: 987_654_321,
        };
        let bytes: [u8; mem::size_of::<ProcessExitEvent>()] = unsafe { mem::transmute(event) };

        // SAFETY: bytes are a valid ProcessExitEvent representation.
        let parsed = unsafe { parse_event(EVENT_TYPE_EXIT, &bytes) }
            .expect("should parse valid exit bytes");

        match parsed {
            RawKernelEvent::Exit(e) => {
                assert_eq!(e.pid, 100);
                assert_eq!(e.exit_code, -9);
            }
            other => panic!("expected Exit, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_tcp_connect_event() {
        let event = TcpConnectEvent {
            pid: 5,
            saddr: 0x7F000001,
            daddr: 0xC0A80001,
            sport: 12345,
            dport: 80,
            timestamp_ns: 999,
        };
        let bytes: [u8; mem::size_of::<TcpConnectEvent>()] = unsafe { mem::transmute(event) };

        // SAFETY: bytes are a valid TcpConnectEvent representation.
        let parsed = unsafe { parse_event(EVENT_TYPE_TCP_CONNECT, &bytes) }
            .expect("should parse tcp connect");

        assert!(matches!(parsed, RawKernelEvent::TcpConnect(c) if c.dport == 80));
    }

    #[test]
    fn test_parse_tcp_close_event() {
        let event = TcpCloseEvent {
            pid: 10,
            saddr: 0x7F000001,
            daddr: 0xC0A80001,
            sport: 54321,
            dport: 443,
            bytes_sent: 1024,
            bytes_recv: 2048,
            duration_ns: 500_000,
        };
        let bytes: [u8; mem::size_of::<TcpCloseEvent>()] = unsafe { mem::transmute(event) };

        // SAFETY: bytes are a valid TcpCloseEvent representation.
        let parsed = unsafe { parse_event(EVENT_TYPE_TCP_CLOSE, &bytes) }
            .expect("should parse tcp close");

        assert!(matches!(parsed, RawKernelEvent::TcpClose(c) if c.bytes_sent == 1024));
    }

    #[test]
    fn test_parse_syscall_event() {
        let event = SyscallEvent {
            pid: 42,
            syscall_nr: 1,
            timestamp_ns: 777,
        };
        let bytes: [u8; mem::size_of::<SyscallEvent>()] = unsafe { mem::transmute(event) };

        // SAFETY: bytes are a valid SyscallEvent representation.
        let parsed = unsafe { parse_event(EVENT_TYPE_SYSCALL, &bytes) }
            .expect("should parse syscall");

        assert!(matches!(parsed, RawKernelEvent::Syscall(s) if s.syscall_nr == 1));
    }

    #[test]
    fn test_parse_event_short_buffer_returns_error() {
        let bytes = [0u8; 4];
        // SAFETY: testing error path with short buffer.
        let result = unsafe { parse_event(EVENT_TYPE_FORK, &bytes) };
        assert!(result.is_err(), "short buffer should return error");
    }

    #[test]
    fn test_parse_event_unknown_type_returns_error() {
        let bytes = [0u8; 16];
        // SAFETY: testing error path with unknown event type.
        let result = unsafe { parse_event(999, &bytes) };
        assert!(result.is_err(), "unknown event type should return error");
    }

    #[test]
    fn test_comm_to_string_handles_null_terminator() {
        let mut comm = [0u8; 16];
        comm[0] = b'b';
        comm[1] = b'a';
        comm[2] = b's';
        comm[3] = b'h';
        // bytes 4..15 are null

        assert_eq!(comm_to_string(&comm), "bash");
    }

    #[test]
    fn test_comm_to_string_full_buffer_no_null() {
        let comm = [b'a'; 16];
        assert_eq!(comm_to_string(&comm), "aaaaaaaaaaaaaaaa");
    }

    #[test]
    fn test_comm_to_string_empty() {
        let comm = [0u8; 16];
        assert_eq!(comm_to_string(&comm), "");
    }
}

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
}

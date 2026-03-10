//! Async ring buffer reader for BPF events.
//!
//! Reads raw bytes from aya `RingBuf` maps and deserializes them into
//! typed kernel events. Uses `tokio::io::unix::AsyncFd` for non-blocking poll.

use std::mem;

use aya::maps::ring_buf::RingBuf;
use aya::maps::MapData;
use aya::Ebpf;
use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

use crate::error::EbpfError;
use crate::events::{ProcessExitEvent, ProcessForkEvent};

/// Raw kernel event deserialized from a BPF ring buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum RawKernelEvent {
    /// Process fork detected.
    Fork(ProcessForkEvent),
    /// Process exit detected.
    Exit(ProcessExitEvent),
}

/// Async reader for BPF ring buffer maps.
///
/// Owns two `RingBuf` maps (fork and exit events) and provides async
/// polling via `tokio::io::unix::AsyncFd`.
pub struct RingBufferReader {
    fork_buf: AsyncFd<RingBuf<MapData>>,
    exit_buf: AsyncFd<RingBuf<MapData>>,
}

impl RingBufferReader {
    /// Create a reader by taking ownership of the `fork_events` and `exit_events` maps.
    pub fn new(bpf: &mut Ebpf) -> Result<Self, EbpfError> {
        let fork_map = bpf
            .take_map("fork_events")
            .ok_or_else(|| EbpfError::MapError("map 'fork_events' not found".into()))?;
        let exit_map = bpf
            .take_map("exit_events")
            .ok_or_else(|| EbpfError::MapError("map 'exit_events' not found".into()))?;

        let fork_ring = RingBuf::try_from(fork_map)
            .map_err(|e| EbpfError::MapError(format!("fork_events: {e}")))?;
        let exit_ring = RingBuf::try_from(exit_map)
            .map_err(|e| EbpfError::MapError(format!("exit_events: {e}")))?;

        let fork_buf = AsyncFd::with_interest(fork_ring, Interest::READABLE)
            .map_err(|e| EbpfError::MapError(format!("AsyncFd fork_events: {e}")))?;
        let exit_buf = AsyncFd::with_interest(exit_ring, Interest::READABLE)
            .map_err(|e| EbpfError::MapError(format!("AsyncFd exit_events: {e}")))?;

        Ok(Self {
            fork_buf,
            exit_buf,
        })
    }

    /// Poll both ring buffers and return all available events.
    ///
    /// Waits until at least one buffer is readable, then drains both.
    /// Returns an empty vec only on spurious wakeups.
    pub async fn poll(&mut self) -> Result<Vec<RawKernelEvent>, EbpfError> {
        // Wait for either buffer to become readable.
        tokio::select! {
            guard = self.fork_buf.readable_mut() => {
                let mut guard = guard.map_err(|e| EbpfError::MapError(format!("poll fork: {e}")))?;
                let events = drain_fork(guard.get_inner_mut());
                guard.clear_ready();
                // Also drain exit buffer opportunistically (non-blocking).
                let mut exit_events = try_drain_exit(&mut self.exit_buf);
                let mut all = events;
                all.append(&mut exit_events);
                Ok(all)
            }
            guard = self.exit_buf.readable_mut() => {
                let mut guard = guard.map_err(|e| EbpfError::MapError(format!("poll exit: {e}")))?;
                let events = drain_exit(guard.get_inner_mut());
                guard.clear_ready();
                // Also drain fork buffer opportunistically (non-blocking).
                let fork_events = try_drain_fork(&mut self.fork_buf);
                let mut all = fork_events;
                all.append(&mut events.into_iter().collect());
                Ok(all)
            }
        }
    }
}

/// Drain all available fork events from the ring buffer.
fn drain_fork(ring: &mut RingBuf<MapData>) -> Vec<RawKernelEvent> {
    let mut events = Vec::new();
    while let Some(item) = ring.next() {
        if let Some(event) = parse_fork(&item) {
            events.push(RawKernelEvent::Fork(event));
        }
    }
    events
}

/// Drain all available exit events from the ring buffer.
fn drain_exit(ring: &mut RingBuf<MapData>) -> Vec<RawKernelEvent> {
    let mut events = Vec::new();
    while let Some(item) = ring.next() {
        if let Some(event) = parse_exit(&item) {
            events.push(RawKernelEvent::Exit(event));
        }
    }
    events
}

/// Try to drain fork events without awaiting readability.
fn try_drain_fork(fd: &mut AsyncFd<RingBuf<MapData>>) -> Vec<RawKernelEvent> {
    drain_fork(fd.get_mut())
}

/// Try to drain exit events without awaiting readability.
fn try_drain_exit(fd: &mut AsyncFd<RingBuf<MapData>>) -> Vec<RawKernelEvent> {
    drain_exit(fd.get_mut())
}

/// Parse raw bytes into a `ProcessForkEvent`. Returns `None` on size mismatch.
fn parse_fork(bytes: &[u8]) -> Option<ProcessForkEvent> {
    if bytes.len() < mem::size_of::<ProcessForkEvent>() {
        return None;
    }
    // SAFETY: ProcessForkEvent is repr(C), Copy, and we verified the buffer size.
    // The BPF ring buffer guarantees aligned writes matching our struct layout.
    Some(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<ProcessForkEvent>()) })
}

/// Parse raw bytes into a `ProcessExitEvent`. Returns `None` on size mismatch.
fn parse_exit(bytes: &[u8]) -> Option<ProcessExitEvent> {
    if bytes.len() < mem::size_of::<ProcessExitEvent>() {
        return None;
    }
    // SAFETY: ProcessExitEvent is repr(C), Copy, and we verified the buffer size.
    // The BPF ring buffer guarantees aligned writes matching our struct layout.
    Some(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<ProcessExitEvent>()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fork_valid_bytes() {
        let event = ProcessForkEvent {
            parent_pid: 1,
            child_pid: 42,
            timestamp_ns: 123_456_789,
        };
        // SAFETY: ProcessForkEvent is repr(C), Copy. Transmuting to bytes is safe.
        let bytes: [u8; mem::size_of::<ProcessForkEvent>()] =
            unsafe { mem::transmute(event) };

        let parsed = parse_fork(&bytes).expect("should parse valid fork bytes");
        assert_eq!(parsed.parent_pid, 1);
        assert_eq!(parsed.child_pid, 42);
        assert_eq!(parsed.timestamp_ns, 123_456_789);
    }

    #[test]
    fn test_parse_exit_valid_bytes() {
        let event = ProcessExitEvent {
            pid: 100,
            exit_code: -9,
            timestamp_ns: 987_654_321,
        };
        let bytes: [u8; mem::size_of::<ProcessExitEvent>()] =
            unsafe { mem::transmute(event) };

        let parsed = parse_exit(&bytes).expect("should parse valid exit bytes");
        assert_eq!(parsed.pid, 100);
        assert_eq!(parsed.exit_code, -9);
        assert_eq!(parsed.timestamp_ns, 987_654_321);
    }

    #[test]
    fn test_parse_fork_short_bytes_returns_none() {
        let bytes = [0u8; 4]; // too short
        assert!(parse_fork(&bytes).is_none(), "short buffer should return None");
    }

    #[test]
    fn test_parse_exit_short_bytes_returns_none() {
        let bytes = [0u8; 8]; // too short (need 16)
        assert!(parse_exit(&bytes).is_none(), "short buffer should return None");
    }

    #[test]
    fn test_raw_kernel_event_variants() {
        let fork = RawKernelEvent::Fork(ProcessForkEvent {
            parent_pid: 1,
            child_pid: 2,
            timestamp_ns: 0,
        });
        let exit = RawKernelEvent::Exit(ProcessExitEvent {
            pid: 1,
            exit_code: 0,
            timestamp_ns: 0,
        });
        // Verify enum variants are distinguishable.
        assert!(matches!(fork, RawKernelEvent::Fork(_)));
        assert!(matches!(exit, RawKernelEvent::Exit(_)));
    }
}

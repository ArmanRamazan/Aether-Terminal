//! Translates raw eBPF kernel events into core [`SystemEvent`]s.
//!
//! Reads [`RawKernelEvent`] from a channel (fed by `aether-ebpf` ring buffer reader)
//! and emits the corresponding [`SystemEvent`] variants for the rest of the pipeline.

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_ebpf::events::RawKernelEvent;

use crate::error::IngestionError;

/// Bridge between raw eBPF kernel events and the core event pipeline.
pub struct EbpfBridge {
    event_rx: mpsc::Receiver<RawKernelEvent>,
    event_tx: mpsc::Sender<SystemEvent>,
}

impl EbpfBridge {
    /// Create a new bridge with the given kernel event receiver and system event sender.
    pub fn new(
        event_rx: mpsc::Receiver<RawKernelEvent>,
        event_tx: mpsc::Sender<SystemEvent>,
    ) -> Self {
        Self { event_rx, event_tx }
    }

    /// Run the bridge loop, translating kernel events until cancellation or channel close.
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<(), IngestionError> {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                recv = self.event_rx.recv() => {
                    let Some(raw) = recv else {
                        tracing::debug!("eBPF event channel closed, bridge shutting down");
                        return Ok(());
                    };
                    self.translate(raw).await?;
                }
            }
        }
    }

    /// Convert a single raw kernel event to a system event and send it.
    async fn translate(&self, raw: RawKernelEvent) -> Result<(), IngestionError> {
        let event = match raw {
            RawKernelEvent::Fork(fork) => SystemEvent::ProcessCreated {
                pid: fork.child_pid,
                name: format!("<fork:{}>", fork.child_pid),
            },
            RawKernelEvent::Exit(exit) => SystemEvent::ProcessExited { pid: exit.pid },
            RawKernelEvent::TcpConnect(_)
            | RawKernelEvent::TcpClose(_)
            | RawKernelEvent::Syscall(_) => SystemEvent::TopologyChange,
        };

        self.event_tx
            .send(event)
            .await
            .map_err(|_| IngestionError::ChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_ebpf::events::{ProcessExitEvent, ProcessForkEvent};

    #[tokio::test]
    async fn test_bridge_converts_process_fork_to_created() {
        let (raw_tx, raw_rx) = mpsc::channel(16);
        let (sys_tx, mut sys_rx) = mpsc::channel(16);

        let mut bridge = EbpfBridge::new(raw_rx, sys_tx);
        let cancel = CancellationToken::new();

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { bridge.run(cancel_clone).await });

        raw_tx
            .send(RawKernelEvent::Fork(ProcessForkEvent {
                parent_pid: 1,
                child_pid: 42,
                timestamp_ns: 100,
            }))
            .await
            .expect("send should succeed");

        let event = sys_rx.recv().await.expect("should receive event");
        match event {
            SystemEvent::ProcessCreated { pid, name } => {
                assert_eq!(pid, 42, "child pid should be forwarded");
                assert!(name.contains("42"), "name should contain child pid");
            }
            other => panic!("expected ProcessCreated, got {other:?}"),
        }

        cancel.cancel();
        handle.await.expect("task should not panic").expect("bridge should shut down cleanly");
    }

    #[tokio::test]
    async fn test_bridge_converts_process_exit_to_exited() {
        let (raw_tx, raw_rx) = mpsc::channel(16);
        let (sys_tx, mut sys_rx) = mpsc::channel(16);

        let mut bridge = EbpfBridge::new(raw_rx, sys_tx);
        let cancel = CancellationToken::new();

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { bridge.run(cancel_clone).await });

        raw_tx
            .send(RawKernelEvent::Exit(ProcessExitEvent {
                pid: 99,
                exit_code: -1,
                timestamp_ns: 200,
            }))
            .await
            .expect("send should succeed");

        let event = sys_rx.recv().await.expect("should receive event");
        match event {
            SystemEvent::ProcessExited { pid } => {
                assert_eq!(pid, 99, "exiting pid should be forwarded");
            }
            other => panic!("expected ProcessExited, got {other:?}"),
        }

        cancel.cancel();
        handle.await.expect("task should not panic").expect("bridge should shut down cleanly");
    }
}

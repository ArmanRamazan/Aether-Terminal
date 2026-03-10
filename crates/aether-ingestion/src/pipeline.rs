//! Dual-tick async ingestion pipeline with optional eBPF hybrid mode.
//!
//! Runs two concurrent intervals:
//! - **fast tick** (100 ms): calls [`SystemProbe::snapshot()`] and sends [`SystemEvent::MetricsUpdate`]
//! - **slow tick** (1 s): sends [`SystemEvent::TopologyChange`]
//!
//! In [`PipelineMode::Hybrid`], an [`EbpfBridge`] is spawned alongside the sysinfo
//! ticks, merging kernel events via `tokio::select!`. If the bridge fails or its
//! input channel closes, the pipeline falls back to sysinfo-only automatically.
//!
//! Graceful shutdown via [`CancellationToken`](tokio_util::sync::CancellationToken).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_core::traits::SystemProbe;

use crate::ebpf_bridge::EbpfBridge;
use crate::error::IngestionError;

const FAST_TICK: Duration = Duration::from_millis(100);
const SLOW_TICK: Duration = Duration::from_secs(1);

/// Pipeline operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    /// Sysinfo polling only (default).
    Sysinfo,
    /// eBPF events merged with sysinfo polling.
    Hybrid,
}

/// Asynchronous pipeline that periodically probes system metrics and emits events.
///
/// Generic over `P` to accept any [`SystemProbe`] implementation without
/// requiring trait-object overhead (the trait uses RPITIT and is not object-safe).
pub struct IngestionPipeline<P> {
    probe: Arc<P>,
    event_tx: mpsc::Sender<SystemEvent>,
    mode: PipelineMode,
    bridge: Option<EbpfBridge>,
}

impl<P: SystemProbe> IngestionPipeline<P> {
    /// Create a new pipeline with the given probe and event sender.
    pub fn new(probe: Arc<P>, event_tx: mpsc::Sender<SystemEvent>) -> Self {
        Self {
            probe,
            event_tx,
            mode: PipelineMode::Sysinfo,
            bridge: None,
        }
    }

    /// Enable hybrid mode with an eBPF bridge for kernel event translation.
    ///
    /// The bridge's `event_tx` should write to the same channel as this pipeline's
    /// `event_tx` so that both sysinfo and eBPF events are merged on the receiver side.
    pub fn with_ebpf(mut self, bridge: EbpfBridge) -> Self {
        self.mode = PipelineMode::Hybrid;
        self.bridge = Some(bridge);
        self
    }

    /// Return the current pipeline mode.
    pub fn mode(&self) -> PipelineMode {
        self.mode
    }

    /// Run the pipeline until the cancellation token is triggered.
    ///
    /// In hybrid mode, spawns the eBPF bridge as a separate task and monitors it
    /// via `tokio::select!`. If the bridge stops (channel close or error), the
    /// pipeline continues in sysinfo-only mode.
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<(), IngestionError> {
        let mut fast = tokio::time::interval(FAST_TICK);
        let mut slow = tokio::time::interval(SLOW_TICK);

        match self.bridge.take() {
            None => loop {
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(()),
                    _ = fast.tick() => self.handle_fast_tick().await?,
                    _ = slow.tick() => self.handle_slow_tick().await?,
                }
            },
            Some(mut bridge) => {
                let bridge_cancel = cancel.clone();
                let handle = tokio::spawn(async move { bridge.run(bridge_cancel).await });
                tokio::pin!(handle);
                let mut bridge_active = true;

                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        _ = fast.tick() => self.handle_fast_tick().await?,
                        _ = slow.tick() => self.handle_slow_tick().await?,
                        result = &mut handle, if bridge_active => {
                            bridge_active = false;
                            match result {
                                Ok(Ok(())) => tracing::info!("eBPF bridge completed, continuing sysinfo-only"),
                                Ok(Err(e)) => tracing::warn!("eBPF bridge failed: {e}, falling back to sysinfo"),
                                Err(e) => tracing::warn!("eBPF bridge panicked: {e}, falling back to sysinfo"),
                            }
                        }
                    }
                }
            }
        }
    }

    /// Fast tick: snapshot metrics and send [`SystemEvent::MetricsUpdate`].
    async fn handle_fast_tick(&self) -> Result<(), IngestionError> {
        let snapshot = match self.probe.snapshot().await {
            Ok(snap) => snap,
            Err(e) => {
                tracing::warn!("probe snapshot failed: {e}");
                return Ok(());
            }
        };

        self.event_tx
            .send(SystemEvent::MetricsUpdate { snapshot })
            .await
            .map_err(|_| IngestionError::ChannelClosed)
    }

    /// Slow tick: signal a topology change.
    async fn handle_slow_tick(&self) -> Result<(), IngestionError> {
        self.event_tx
            .send(SystemEvent::TopologyChange)
            .await
            .map_err(|_| IngestionError::ChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::models::{ProcessNode, ProcessState, SystemSnapshot};
    use aether_ebpf::events::{ProcessForkEvent, RawKernelEvent};
    use glam::Vec3;
    use std::time::SystemTime;

    /// A deterministic mock probe that always returns a fixed snapshot.
    struct MockProbe;

    impl SystemProbe for MockProbe {
        async fn snapshot(&self) -> Result<SystemSnapshot, aether_core::error::CoreError> {
            Ok(SystemSnapshot {
                processes: vec![ProcessNode {
                    pid: 1,
                    ppid: 0,
                    name: "mock".to_string(),
                    cpu_percent: 5.0,
                    mem_bytes: 2048,
                    state: ProcessState::Running,
                    hp: 100.0,
                    xp: 0,
                    position_3d: Vec3::ZERO,
                }],
                edges: vec![],
                timestamp: SystemTime::now(),
            })
        }
    }

    #[tokio::test]
    async fn test_pipeline_sends_events_within_timeout() {
        let (tx, mut rx) = mpsc::channel(64);
        let mut pipeline = IngestionPipeline::new(Arc::new(MockProbe), tx);
        let cancel = CancellationToken::new();

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { pipeline.run(cancel_clone).await });

        let event = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("should receive event within 500ms")
            .expect("channel should not be closed");

        assert!(
            matches!(
                event,
                SystemEvent::MetricsUpdate { .. } | SystemEvent::TopologyChange
            ),
            "first event should be MetricsUpdate or TopologyChange"
        );

        cancel.cancel();
        let result = handle.await.expect("task should not panic");
        assert!(result.is_ok(), "pipeline should shut down cleanly");
    }

    #[tokio::test]
    async fn test_pipeline_stops_on_cancel() {
        let (tx, _rx) = mpsc::channel(64);
        let mut pipeline = IngestionPipeline::new(Arc::new(MockProbe), tx);
        let cancel = CancellationToken::new();

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { pipeline.run(cancel_clone).await });

        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("pipeline should stop within 500ms")
            .expect("task should not panic");

        assert!(result.is_ok(), "pipeline should return Ok on cancellation");
    }

    #[tokio::test]
    async fn test_hybrid_mode_merges_event_sources() {
        let (event_tx, mut event_rx) = mpsc::channel(64);
        let (raw_tx, raw_rx) = mpsc::channel(16);

        let bridge = EbpfBridge::new(raw_rx, event_tx.clone());
        let mut pipeline =
            IngestionPipeline::new(Arc::new(MockProbe), event_tx).with_ebpf(bridge);

        assert_eq!(pipeline.mode(), PipelineMode::Hybrid);

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { pipeline.run(cancel_clone).await });

        // Send an eBPF event through the bridge
        raw_tx
            .send(RawKernelEvent::Fork(ProcessForkEvent {
                parent_pid: 1,
                child_pid: 42,
                timestamp_ns: 100,
            }))
            .await
            .expect("send raw event should succeed");

        // Collect events — expect both sysinfo and eBPF-sourced events
        let mut saw_metrics = false;
        let mut saw_process_created = false;

        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await {
                Ok(Some(SystemEvent::MetricsUpdate { .. })) => saw_metrics = true,
                Ok(Some(SystemEvent::ProcessCreated { pid, .. })) if pid == 42 => {
                    saw_process_created = true;
                }
                _ => {}
            }
            if saw_metrics && saw_process_created {
                break;
            }
        }

        cancel.cancel();
        handle.await.expect("task should not panic").expect("pipeline should shut down cleanly");

        assert!(saw_metrics, "should receive MetricsUpdate from sysinfo");
        assert!(saw_process_created, "should receive ProcessCreated from eBPF bridge");
    }

    #[tokio::test]
    async fn test_fallback_to_sysinfo_on_ebpf_failure() {
        let (event_tx, mut event_rx) = mpsc::channel(64);
        let (raw_tx, raw_rx) = mpsc::channel(16);

        let bridge = EbpfBridge::new(raw_rx, event_tx.clone());
        let mut pipeline =
            IngestionPipeline::new(Arc::new(MockProbe), event_tx).with_ebpf(bridge);

        // Close the raw event channel immediately — bridge will detect EOF and stop
        drop(raw_tx);

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { pipeline.run(cancel_clone).await });

        // Pipeline should still produce sysinfo events despite bridge failure
        let event = tokio::time::timeout(Duration::from_millis(500), event_rx.recv())
            .await
            .expect("should receive sysinfo event after bridge failure")
            .expect("channel should not be closed");

        assert!(
            matches!(
                event,
                SystemEvent::MetricsUpdate { .. } | SystemEvent::TopologyChange
            ),
            "should still receive sysinfo events after eBPF fallback"
        );

        cancel.cancel();
        handle.await.expect("task should not panic").expect("pipeline should shut down cleanly");
    }
}

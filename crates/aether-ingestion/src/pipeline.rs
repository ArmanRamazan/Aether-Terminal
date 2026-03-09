//! Dual-tick async ingestion pipeline.
//!
//! Runs two concurrent intervals:
//! - **fast tick** (100 ms): calls [`SystemProbe::snapshot()`] and sends [`SystemEvent::MetricsUpdate`]
//! - **slow tick** (1 s): sends [`SystemEvent::TopologyChange`]
//!
//! Graceful shutdown via [`CancellationToken`](tokio_util::sync::CancellationToken).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use aether_core::events::SystemEvent;
use aether_core::traits::SystemProbe;

use crate::error::IngestionError;

const FAST_TICK: Duration = Duration::from_millis(100);
const SLOW_TICK: Duration = Duration::from_secs(1);

/// Asynchronous pipeline that periodically probes system metrics and emits events.
///
/// Generic over `P` to accept any [`SystemProbe`] implementation without
/// requiring trait-object overhead (the trait uses RPITIT and is not object-safe).
pub struct IngestionPipeline<P> {
    probe: Arc<P>,
    event_tx: mpsc::Sender<SystemEvent>,
}

impl<P: SystemProbe> IngestionPipeline<P> {
    /// Create a new pipeline with the given probe and event sender.
    pub fn new(probe: Arc<P>, event_tx: mpsc::Sender<SystemEvent>) -> Self {
        Self { probe, event_tx }
    }

    /// Run the dual-tick loop until the cancellation token is triggered.
    ///
    /// Returns `Ok(())` on graceful shutdown, or [`IngestionError::ChannelClosed`]
    /// if the event receiver is dropped.
    pub async fn run(&self, cancel: CancellationToken) -> Result<(), IngestionError> {
        let mut fast = tokio::time::interval(FAST_TICK);
        let mut slow = tokio::time::interval(SLOW_TICK);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Ok(());
                }
                _ = fast.tick() => {
                    self.handle_fast_tick().await?;
                }
                _ = slow.tick() => {
                    self.handle_slow_tick().await?;
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
    use glam::Vec3;
    use std::time::SystemTime;

    /// A deterministic mock probe that always returns a fixed snapshot.
    struct MockProbe;

    impl SystemProbe for MockProbe {
        async fn snapshot(
            &self,
        ) -> Result<SystemSnapshot, Box<dyn std::error::Error + Send + Sync>> {
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
        let pipeline = IngestionPipeline::new(Arc::new(MockProbe), tx);
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
        let pipeline = IngestionPipeline::new(Arc::new(MockProbe), tx);
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
}

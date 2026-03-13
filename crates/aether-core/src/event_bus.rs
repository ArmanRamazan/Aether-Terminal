//! EventBus trait and in-process implementation.
//!
//! Abstracts event publishing/subscribing for cross-project integration.
//! Current implementation uses `tokio::sync::broadcast`; future implementations
//! may use gRPC streaming (Phase 2).

use crate::events::IntegrationEvent;
use tokio::sync::broadcast;

/// Port for publishing and subscribing to integration events.
///
/// Implemented by [`InProcessEventBus`] (broadcast channel) now,
/// and by a gRPC streaming adapter in Phase 2.
pub trait EventBus: Send + Sync {
    /// Publish an event to all subscribers.
    fn publish(&self, event: IntegrationEvent)
        -> impl std::future::Future<Output = ()> + Send;

    /// Create a new subscription receiver.
    fn subscribe(&self) -> broadcast::Receiver<IntegrationEvent>;
}

/// In-process EventBus backed by a tokio broadcast channel.
pub struct InProcessEventBus {
    sender: broadcast::Sender<IntegrationEvent>,
}

impl InProcessEventBus {
    /// Create a new bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl EventBus for InProcessEventBus {
    async fn publish(&self, event: IntegrationEvent) {
        // Ignore send error — it means no active receivers.
        let _ = self.sender.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<IntegrationEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = InProcessEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(IntegrationEvent::DiagnosticCreated {
            diagnostic_id: 1,
            severity: "high".to_string(),
            summary: "CPU spike".to_string(),
        })
        .await;

        let event = rx.recv().await.unwrap();
        match event {
            IntegrationEvent::DiagnosticCreated {
                diagnostic_id,
                severity,
                summary,
            } => {
                assert_eq!(diagnostic_id, 1);
                assert_eq!(severity, "high");
                assert_eq!(summary, "CPU spike");
            }
            _ => panic!("expected DiagnosticCreated"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = InProcessEventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(IntegrationEvent::TargetDiscovered {
            name: "web-server".to_string(),
            kind: "pod".to_string(),
        })
        .await;

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();

        match (&e1, &e2) {
            (
                IntegrationEvent::TargetDiscovered { name: n1, kind: k1 },
                IntegrationEvent::TargetDiscovered { name: n2, kind: k2 },
            ) => {
                assert_eq!(n1, "web-server");
                assert_eq!(k1, "pod");
                assert_eq!(n2, "web-server");
                assert_eq!(k2, "pod");
            }
            _ => panic!("expected TargetDiscovered from both subscribers"),
        }
    }
}

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aether_core::models::Diagnostic;
use aether_core::traits::OutputSink;

/// Routes diagnostics to registered output sinks with severity filtering and dedup.
pub struct OutputPipeline {
    sinks: Vec<Box<dyn OutputSink>>,
    dedup_window: Duration,
    last_sent: Mutex<HashMap<(String, String), Instant>>,
}

impl OutputPipeline {
    /// Create a new pipeline with the given sinks and dedup window.
    pub fn new(sinks: Vec<Box<dyn OutputSink>>, dedup_window: Duration) -> Self {
        Self {
            sinks,
            dedup_window,
            last_sent: Mutex::new(HashMap::new()),
        }
    }

    /// Dispatch a diagnostic to all matching sinks.
    ///
    /// Filters by severity and dedup window. Sends to each matching sink
    /// concurrently. Errors are logged but do not propagate.
    pub async fn dispatch(&self, diagnostic: &Diagnostic) {
        let dedup_key = (
            format!("{:?}", diagnostic.target),
            format!("{:?}", diagnostic.category),
        );

        // Check dedup window
        {
            let mut last = match self.last_sent.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            let now = Instant::now();
            if let Some(prev) = last.get(&dedup_key) {
                if now.duration_since(*prev) < self.dedup_window {
                    return;
                }
            }
            last.insert(dedup_key, now);
        }

        for sink in &self.sinks {
            if diagnostic.severity >= sink.min_severity() {
                if let Err(e) = sink.send(diagnostic).await {
                    tracing::warn!(sink = sink.name(), "output dispatch error: {e}");
                }
            }
        }
    }

    /// Number of registered sinks.
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::metrics::HostId;
    use aether_core::models::{
        DiagCategory, DiagTarget, Evidence, Recommendation, RecommendedAction, Severity, Urgency,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct MockSink {
        name: &'static str,
        min_severity: Severity,
        send_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl OutputSink for MockSink {
        async fn send(
            &self,
            _diagnostic: &Diagnostic,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.send_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn name(&self) -> &str {
            self.name
        }
        fn min_severity(&self) -> Severity {
            self.min_severity
        }
    }

    fn make_diag(severity: Severity) -> Diagnostic {
        Diagnostic {
            id: 1,
            host: HostId::new("test"),
            target: DiagTarget::Process {
                pid: 42,
                name: "test-proc".into(),
            },
            severity,
            category: DiagCategory::CpuSpike,
            summary: "test".into(),
            evidence: vec![Evidence {
                metric: "cpu".into(),
                current: 90.0,
                threshold: 80.0,
                trend: None,
                context: "test".into(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::NoAction {
                    reason: "test".into(),
                },
                reason: "test".into(),
                urgency: Urgency::Informational,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[tokio::test]
    async fn test_pipeline_filters_by_severity() {
        let count = Arc::new(AtomicUsize::new(0));
        let pipeline = OutputPipeline::new(
            vec![Box::new(MockSink {
                name: "warn-only",
                min_severity: Severity::Warning,
                send_count: Arc::clone(&count),
            })],
            Duration::from_secs(0),
        );

        // Info should be filtered out
        pipeline.dispatch(&make_diag(Severity::Info)).await;
        assert_eq!(count.load(Ordering::Relaxed), 0);

        // Warning should pass
        pipeline.dispatch(&make_diag(Severity::Warning)).await;
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_dedup_within_window() {
        let count = Arc::new(AtomicUsize::new(0));
        let pipeline = OutputPipeline::new(
            vec![Box::new(MockSink {
                name: "all",
                min_severity: Severity::Info,
                send_count: Arc::clone(&count),
            })],
            Duration::from_secs(60),
        );

        pipeline.dispatch(&make_diag(Severity::Critical)).await;
        pipeline.dispatch(&make_diag(Severity::Critical)).await;
        assert_eq!(count.load(Ordering::Relaxed), 1, "second dispatch should be deduped");
    }

    #[tokio::test]
    async fn test_dedup_after_window() {
        let count = Arc::new(AtomicUsize::new(0));
        let pipeline = OutputPipeline::new(
            vec![Box::new(MockSink {
                name: "all",
                min_severity: Severity::Info,
                send_count: Arc::clone(&count),
            })],
            Duration::from_millis(10),
        );

        pipeline.dispatch(&make_diag(Severity::Critical)).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        pipeline.dispatch(&make_diag(Severity::Critical)).await;
        assert_eq!(count.load(Ordering::Relaxed), 2, "should send after window expires");
    }
}

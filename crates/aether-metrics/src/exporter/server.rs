use std::sync::{Arc, RwLock};

use axum::{routing::get, Router};
use tokio_util::sync::CancellationToken;
use tracing::info;

use aether_core::{Diagnostic, Severity, WorldGraph};

use super::encode::encode_openmetrics;
use super::registry::{LabelSet, MetricDesc, MetricRegistry, MetricType};

/// Prometheus-compatible metrics exporter with HTTP server.
#[derive(Clone)]
pub struct MetricsExporter {
    registry: Arc<RwLock<MetricRegistry>>,
}

impl Default for MetricsExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsExporter {
    /// Create exporter with default metric descriptions registered.
    pub fn new() -> Self {
        let mut registry = MetricRegistry::default();
        register_defaults(&mut registry);
        Self {
            registry: Arc::new(RwLock::new(registry)),
        }
    }

    /// Update metrics from current world state and diagnostics.
    pub fn update_from_world(&self, world: &WorldGraph, diagnostics: &[Diagnostic]) {
        let mut reg = self.registry.write().expect("registry lock poisoned");

        for proc in world.processes() {
            let mut labels = LabelSet::new();
            labels.insert("pid".into(), proc.pid.to_string());
            labels.insert("name".into(), proc.name.clone());

            reg.set_gauge(
                "aether_process_cpu_percent",
                labels.clone(),
                f64::from(proc.cpu_percent),
            );
            reg.set_gauge(
                "aether_process_memory_bytes",
                labels.clone(),
                proc.mem_bytes as f64,
            );
            reg.set_gauge("aether_process_hp", labels, f64::from(proc.hp));
        }

        let mut info_count = 0.0_f64;
        let mut warn_count = 0.0_f64;
        let mut crit_count = 0.0_f64;

        for diag in diagnostics {
            match diag.severity {
                Severity::Info => info_count += 1.0,
                Severity::Warning => warn_count += 1.0,
                Severity::Critical => crit_count += 1.0,
            }
        }

        let mut info_labels = LabelSet::new();
        info_labels.insert("severity".into(), "info".into());
        reg.set_gauge("aether_diagnostics_active", info_labels, info_count);

        let mut warn_labels = LabelSet::new();
        warn_labels.insert("severity".into(), "warning".into());
        reg.set_gauge("aether_diagnostics_active", warn_labels, warn_count);

        let mut crit_labels = LabelSet::new();
        crit_labels.insert("severity".into(), "critical".into());
        reg.set_gauge("aether_diagnostics_active", crit_labels, crit_count);
    }

    /// Start HTTP server with /metrics and /health endpoints.
    pub async fn serve(
        self,
        port: u16,
        cancel: CancellationToken,
    ) -> Result<(), crate::error::MetricsError> {
        let registry = self.registry;

        let metrics_registry = Arc::clone(&registry);
        let metrics_handler = move || {
            let reg = Arc::clone(&metrics_registry);
            async move {
                let snapshot = reg.read().expect("registry lock poisoned").snapshot();
                let body = encode_openmetrics(&snapshot);
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    body,
                )
            }
        };

        let health_handler = || async { axum::Json(serde_json::json!({ "status": "ok" })) };

        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler));

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .map_err(|e| crate::error::MetricsError::Server(e.to_string()))?;

        info!(port, "metrics server listening");

        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .map_err(|e| crate::error::MetricsError::Server(e.to_string()))
    }
}

fn register_defaults(registry: &mut MetricRegistry) {
    let defaults = [
        (
            "aether_process_cpu_percent",
            "CPU usage percentage per process",
            MetricType::Gauge,
        ),
        (
            "aether_process_memory_bytes",
            "Memory usage in bytes per process",
            MetricType::Gauge,
        ),
        (
            "aether_process_hp",
            "Health points per process",
            MetricType::Gauge,
        ),
        (
            "aether_host_cpu_percent",
            "Host-level CPU usage percentage",
            MetricType::Gauge,
        ),
        (
            "aether_host_memory_used_bytes",
            "Host memory used in bytes",
            MetricType::Gauge,
        ),
        (
            "aether_host_load_avg_1m",
            "Host 1-minute load average",
            MetricType::Gauge,
        ),
        (
            "aether_diagnostics_active",
            "Active diagnostics count by severity",
            MetricType::Gauge,
        ),
        (
            "aether_analyze_evaluations_total",
            "Total number of analysis evaluations",
            MetricType::Counter,
        ),
    ];

    for (name, help, metric_type) in defaults {
        registry.register(MetricDesc {
            name: name.into(),
            help: help.into(),
            metric_type,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_core::{
        DiagCategory, DiagTarget, Evidence, Recommendation, RecommendedAction, Urgency,
    };
    use axum::body::Body;
    use http_body_util::BodyExt;
    use std::time::Instant;
    use tower::ServiceExt;

    fn make_diagnostic(severity: Severity) -> Diagnostic {
        Diagnostic {
            id: 1,
            host: aether_core::HostId("test".into()),
            target: DiagTarget::Host(aether_core::HostId("test".into())),
            severity,
            category: DiagCategory::CpuSpike,
            summary: "test diagnostic".into(),
            evidence: vec![Evidence {
                metric: "cpu".into(),
                current: 90.0,
                threshold: 80.0,
                trend: None,
                context: String::new(),
            }],
            recommendation: Recommendation {
                action: RecommendedAction::Investigate {
                    what: String::new(),
                },
                reason: String::new(),
                urgency: Urgency::Informational,
                auto_executable: false,
            },
            detected_at: Instant::now(),
            resolved_at: None,
        }
    }

    #[test]
    fn test_update_populates_gauges() {
        let exporter = MetricsExporter::new();
        let mut world = WorldGraph::default();

        world.add_process(aether_core::ProcessNode {
            pid: 42,
            ppid: 1,
            name: "test_proc".into(),
            cpu_percent: 55.5,
            mem_bytes: 1024,
            state: aether_core::ProcessState::Running,
            hp: 80.0,
            xp: 0,
            position_3d: glam::Vec3::ZERO,
        });

        let diagnostics = vec![
            make_diagnostic(Severity::Warning),
            make_diagnostic(Severity::Critical),
        ];

        exporter.update_from_world(&world, &diagnostics);

        let reg = exporter.registry.read().expect("lock");
        let snap = reg.snapshot();

        let cpu_family = snap
            .iter()
            .find(|f| f.desc.name == "aether_process_cpu_percent")
            .expect("cpu metric");
        assert_eq!(
            cpu_family.samples.len(),
            1,
            "should have one process sample"
        );
        assert!(
            (cpu_family.samples[0].1 - 55.5).abs() < f64::EPSILON,
            "cpu value"
        );

        let diag_family = snap
            .iter()
            .find(|f| f.desc.name == "aether_diagnostics_active")
            .expect("diag metric");
        assert_eq!(
            diag_family.samples.len(),
            3,
            "should have 3 severity labels"
        );
    }

    #[tokio::test]
    async fn test_metrics_endpoint_content_type() {
        let exporter = MetricsExporter::new();
        let registry = Arc::clone(&exporter.registry);

        let metrics_registry = registry;
        let metrics_handler = move || {
            let reg = Arc::clone(&metrics_registry);
            async move {
                let snapshot = reg.read().expect("registry lock poisoned").snapshot();
                let body = encode_openmetrics(&snapshot);
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    body,
                )
            }
        };

        let app = Router::new().route("/metrics", get(metrics_handler));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), 200);
        let content_type = response
            .headers()
            .get("content-type")
            .expect("content-type header");
        assert!(
            content_type
                .to_str()
                .expect("header str")
                .starts_with("text/plain"),
            "content type should be text/plain"
        );
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let health_handler = || async { axum::Json(serde_json::json!({ "status": "ok" })) };
        let app = Router::new().route("/health", get(health_handler));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), 200);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["status"], "ok");
    }
}

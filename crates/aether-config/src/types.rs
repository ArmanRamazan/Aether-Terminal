//! Configuration types for Aether Terminal.
//!
//! Supports TOML and YAML formats with `${ENV_VAR}` interpolation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level configuration for Aether Terminal.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AetherConfig {
    /// Service discovery settings.
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    /// Explicitly configured monitoring targets.
    #[serde(default, alias = "target")]
    pub targets: Vec<TargetConfig>,
    /// Diagnostic threshold overrides.
    #[serde(default)]
    pub thresholds: ThresholdConfig,
    /// Output/notification channels.
    #[serde(default)]
    pub output: OutputConfig,
    /// API server settings.
    #[serde(default)]
    pub api: ApiConfig,
    /// Prometheus scrape settings.
    #[serde(default)]
    pub scrape: ScrapeConfig,
    /// Active probe settings.
    #[serde(default)]
    pub probe: ProbeConfig,
}


/// Service discovery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable auto-discovery of services.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Ports to scan for known services.
    #[serde(default = "default_scan_ports")]
    pub scan_ports: Vec<u16>,
    /// Enable Kubernetes API discovery.
    #[serde(default)]
    pub kubernetes: bool,
    /// Kubernetes namespace to scan.
    #[serde(default)]
    pub kubernetes_namespace: Option<String>,
    /// Kubernetes label selector filter.
    #[serde(default)]
    pub kubernetes_label_selector: Option<String>,
    /// Discovery interval in seconds.
    #[serde(default = "default_discovery_interval")]
    pub interval_seconds: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scan_ports: default_scan_ports(),
            kubernetes: false,
            kubernetes_namespace: None,
            kubernetes_label_selector: None,
            interval_seconds: 60,
        }
    }
}

/// A monitored target (service, container, pod).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    /// Human-readable name.
    pub name: String,
    /// Target kind: "service", "container", "pod".
    #[serde(default)]
    pub kind: Option<String>,
    /// Prometheus metrics URL.
    #[serde(default)]
    pub prometheus: Option<String>,
    /// Health check endpoint URL.
    #[serde(default)]
    pub health: Option<String>,
    /// TCP probe endpoint (host:port).
    #[serde(default)]
    pub probe_tcp: Option<String>,
    /// Log file path glob.
    #[serde(default)]
    pub logs: Option<String>,
    /// Arbitrary labels for filtering.
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// Diagnostic threshold configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    /// Throughput drop threshold (%).
    #[serde(default = "default_throughput_drop")]
    pub throughput_drop_percent: f64,
    /// P99 latency threshold (ms).
    #[serde(default = "default_latency_p99")]
    pub latency_p99_ms: f64,
    /// Connection pool usage threshold (%).
    #[serde(default = "default_conn_pool")]
    pub connection_pool_percent: f64,
    /// Error rate threshold (%).
    #[serde(default = "default_error_rate")]
    pub error_rate_percent: f64,
    /// TLS certificate expiry warning (days).
    #[serde(default = "default_tls_expiry")]
    pub tls_expiry_days: u32,
    /// Health check timeout (ms).
    #[serde(default = "default_health_timeout")]
    pub health_check_timeout_ms: u64,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            throughput_drop_percent: 30.0,
            latency_p99_ms: 500.0,
            connection_pool_percent: 80.0,
            error_rate_percent: 5.0,
            tls_expiry_days: 30,
            health_check_timeout_ms: 5000,
        }
    }
}

/// Output/notification channel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Slack webhook output.
    #[serde(default)]
    pub slack: Option<WebhookConfig>,
    /// Discord webhook output.
    #[serde(default)]
    pub discord: Option<WebhookConfig>,
    /// Telegram bot output.
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    /// Stdout output.
    #[serde(default)]
    pub stdout: Option<StdoutConfig>,
    /// File output.
    #[serde(default)]
    pub file: Option<FileConfig>,
}

/// Webhook configuration for Slack/Discord.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL.
    pub webhook_url: String,
    /// Minimum severity to send ("info", "warning", "critical").
    #[serde(default = "default_severity_warning")]
    pub severity: String,
}

/// Telegram bot configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot API token.
    pub bot_token: String,
    /// Target chat ID.
    pub chat_id: String,
    /// Minimum severity to send.
    #[serde(default = "default_severity_warning")]
    pub severity: String,
}

/// Stdout output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutConfig {
    /// Enable stdout output.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Output format ("json" or "text").
    #[serde(default = "default_format_json")]
    pub format: String,
    /// Minimum severity to print.
    #[serde(default = "default_severity_info")]
    pub severity: String,
}

/// File output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    /// Output file path.
    pub path: String,
    /// Minimum severity to write.
    #[serde(default = "default_severity_info")]
    pub severity: String,
    /// Max file size before rotation (MB).
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u64,
}

/// gRPC API configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiConfig {
    /// gRPC server settings.
    #[serde(default)]
    pub grpc: Option<GrpcConfig>,
    /// Event streaming settings.
    #[serde(default)]
    pub events: Option<EventsConfig>,
}

/// gRPC server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// Enable gRPC server.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Bind address (e.g. "0.0.0.0:50051").
    #[serde(default = "default_grpc_bind")]
    pub bind: String,
}

/// Event broadcast configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsConfig {
    /// Enable event broadcasting.
    #[serde(default = "default_true")]
    pub broadcast: bool,
}

/// Prometheus scrape configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeConfig {
    /// Scrape interval in seconds.
    #[serde(default = "default_scrape_interval")]
    pub interval_seconds: u64,
    /// Scrape timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

impl Default for ScrapeConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 15,
            timeout_seconds: 5,
        }
    }
}

/// Active probe configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    /// Probe interval in seconds.
    #[serde(default = "default_probe_interval")]
    pub interval_seconds: u64,
    /// Probe timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            timeout_seconds: 5,
        }
    }
}

// --- Default value functions for serde ---

fn default_true() -> bool {
    true
}

fn default_scan_ports() -> Vec<u16> {
    vec![5432, 6379, 8080, 9090, 9200, 27017]
}

fn default_discovery_interval() -> u64 {
    60
}

fn default_throughput_drop() -> f64 {
    30.0
}

fn default_latency_p99() -> f64 {
    500.0
}

fn default_conn_pool() -> f64 {
    80.0
}

fn default_error_rate() -> f64 {
    5.0
}

fn default_tls_expiry() -> u32 {
    30
}

fn default_health_timeout() -> u64 {
    5000
}

fn default_severity_warning() -> String {
    "warning".to_owned()
}

fn default_severity_info() -> String {
    "info".to_owned()
}

fn default_format_json() -> String {
    "json".to_owned()
}

fn default_max_size_mb() -> u64 {
    100
}

fn default_grpc_bind() -> String {
    "0.0.0.0:50051".to_owned()
}

fn default_scrape_interval() -> u64 {
    15
}

fn default_probe_interval() -> u64 {
    30
}

fn default_timeout() -> u64 {
    5
}

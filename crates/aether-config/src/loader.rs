//! Config file loading with format auto-detection and env interpolation.

use std::path::Path;

use regex::Regex;

use crate::error::ConfigError;
use crate::types::AetherConfig;

/// Load configuration from a file path.
///
/// Auto-detects format by extension: `.toml` for TOML, `.yaml`/`.yml` for YAML.
/// Interpolates `${ENV_VAR}` patterns before parsing.
pub fn load(path: &Path) -> Result<AetherConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)?;
    let interpolated = interpolate_env(&raw);

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let config = match ext {
        "toml" => toml::from_str::<AetherConfig>(&interpolated)?,
        "yaml" | "yml" => serde_yaml::from_str::<AetherConfig>(&interpolated)?,
        other => {
            return Err(ConfigError::UnsupportedFormat {
                extension: other.to_owned(),
            });
        }
    };

    validate(&config)?;
    Ok(config)
}

/// Load configuration from a raw string with explicit format.
///
/// Format must be `"toml"`, `"yaml"`, or `"yml"`.
pub fn load_str(content: &str, format: &str) -> Result<AetherConfig, ConfigError> {
    let interpolated = interpolate_env(content);

    let config = match format {
        "toml" => toml::from_str::<AetherConfig>(&interpolated)?,
        "yaml" | "yml" => serde_yaml::from_str::<AetherConfig>(&interpolated)?,
        other => {
            return Err(ConfigError::UnsupportedFormat {
                extension: other.to_owned(),
            });
        }
    };

    validate(&config)?;
    Ok(config)
}

/// Replace `${VAR_NAME}` patterns with environment variable values.
///
/// Unset variables are replaced with an empty string.
fn interpolate_env(input: &str) -> String {
    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("valid regex");
    re.replace_all(input, |caps: &regex::Captures| {
        let var = &caps[1];
        std::env::var(var).unwrap_or_default()
    })
    .into_owned()
}

/// Validate config after parsing.
fn validate(config: &AetherConfig) -> Result<(), ConfigError> {
    for (i, target) in config.targets.iter().enumerate() {
        if target.name.trim().is_empty() {
            return Err(ConfigError::Validation {
                message: format!("target[{i}] has an empty name"),
            });
        }
    }

    for port in &config.discovery.scan_ports {
        if *port == 0 {
            return Err(ConfigError::Validation {
                message: "discovery.scan_ports contains port 0".to_owned(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_toml() {
        let toml_str = r#"
[discovery]
enabled = false
scan_ports = [8080, 9090]
interval_seconds = 30

[[targets]]
name = "api-server"
kind = "service"
prometheus = "http://localhost:9090/metrics"

[thresholds]
throughput_drop_percent = 25.0
latency_p99_ms = 200.0

[scrape]
interval_seconds = 10
timeout_seconds = 3
"#;
        let config = load_str(toml_str, "toml").expect("should parse TOML");
        assert!(!config.discovery.enabled, "discovery should be disabled");
        assert_eq!(config.discovery.scan_ports, vec![8080, 9090]);
        assert_eq!(config.discovery.interval_seconds, 30);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].name, "api-server");
        assert_eq!(
            config.targets[0].prometheus.as_deref(),
            Some("http://localhost:9090/metrics")
        );
        assert!((config.thresholds.throughput_drop_percent - 25.0).abs() < f64::EPSILON);
        assert_eq!(config.scrape.interval_seconds, 10);
    }

    #[test]
    fn test_load_yaml() {
        let yaml_str = r#"
discovery:
  enabled: true
  scan_ports: [5432, 6379]
targets:
  - name: postgresql
    kind: service
    probe_tcp: "localhost:5432"
thresholds:
  error_rate_percent: 10.0
"#;
        let config = load_str(yaml_str, "yaml").expect("should parse YAML");
        assert!(config.discovery.enabled);
        assert_eq!(config.discovery.scan_ports, vec![5432, 6379]);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].name, "postgresql");
        assert_eq!(
            config.targets[0].probe_tcp.as_deref(),
            Some("localhost:5432")
        );
        assert!((config.thresholds.error_rate_percent - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_defaults() {
        let config = AetherConfig::default();
        assert!(config.discovery.enabled);
        assert_eq!(
            config.discovery.scan_ports,
            vec![5432, 6379, 8080, 9090, 9200, 27017]
        );
        assert_eq!(config.discovery.interval_seconds, 60);
        assert!(config.targets.is_empty());
        assert!((config.thresholds.throughput_drop_percent - 30.0).abs() < f64::EPSILON);
        assert!((config.thresholds.latency_p99_ms - 500.0).abs() < f64::EPSILON);
        assert!((config.thresholds.connection_pool_percent - 80.0).abs() < f64::EPSILON);
        assert!((config.thresholds.error_rate_percent - 5.0).abs() < f64::EPSILON);
        assert_eq!(config.thresholds.tls_expiry_days, 30);
        assert_eq!(config.thresholds.health_check_timeout_ms, 5000);
        assert_eq!(config.scrape.interval_seconds, 15);
        assert_eq!(config.scrape.timeout_seconds, 5);
        assert_eq!(config.probe.interval_seconds, 30);
        assert_eq!(config.probe.timeout_seconds, 5);
    }

    #[test]
    fn test_env_interpolation() {
        std::env::set_var("AETHER_TEST_WEBHOOK", "https://hooks.example.com/test");
        let toml_str = r#"
[output.slack]
webhook_url = "${AETHER_TEST_WEBHOOK}"
severity = "critical"
"#;
        let config = load_str(toml_str, "toml").expect("should parse with env interpolation");
        let slack = config.output.slack.expect("slack should be present");
        assert_eq!(slack.webhook_url, "https://hooks.example.com/test");
        assert_eq!(slack.severity, "critical");
        std::env::remove_var("AETHER_TEST_WEBHOOK");
    }

    #[test]
    fn test_invalid_format() {
        let dir = std::env::temp_dir();
        let path = dir.join("aether-config-test.json");
        {
            let mut f = std::fs::File::create(&path).expect("create temp file");
            f.write_all(b"{}").expect("write temp file");
        }
        let result = load(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::UnsupportedFormat { .. }),
            "expected UnsupportedFormat, got: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_validation_empty_target_name() {
        let toml_str = r#"
[[targets]]
name = "  "
"#;
        let result = load_str(toml_str, "toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Validation { .. }),
            "expected Validation error, got: {err}"
        );
    }

    #[test]
    fn test_validation_port_zero() {
        let toml_str = r#"
[discovery]
scan_ports = [8080, 0]
"#;
        let result = load_str(toml_str, "toml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::Validation { .. }),
            "expected Validation error, got: {err}"
        );
    }

    #[test]
    fn test_load_file_toml() {
        let dir = std::env::temp_dir();
        let path = dir.join("aether-config-test.toml");
        {
            let mut f = std::fs::File::create(&path).expect("create temp file");
            f.write_all(
                br#"
[discovery]
enabled = false

[[targets]]
name = "redis"
probe_tcp = "localhost:6379"
"#,
            )
            .expect("write temp file");
        }
        let config = load(&path).expect("should load TOML file");
        assert!(!config.discovery.enabled);
        assert_eq!(config.targets[0].name, "redis");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_empty_config_uses_defaults() {
        let config = load_str("", "toml").expect("empty TOML should use defaults");
        assert!(config.discovery.enabled);
        assert!(config.targets.is_empty());
    }
}

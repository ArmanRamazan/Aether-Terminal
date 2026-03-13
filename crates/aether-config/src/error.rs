//! Error types for config loading and validation.

/// Errors that can occur when loading or validating configuration.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Failed to read the config file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing failed.
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// YAML parsing failed.
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// File extension is not .toml, .yaml, or .yml.
    #[error("unsupported config format: {extension}")]
    UnsupportedFormat {
        /// The file extension encountered.
        extension: String,
    },

    /// Config validation failed.
    #[error("validation error: {message}")]
    Validation {
        /// Description of the validation failure.
        message: String,
    },
}

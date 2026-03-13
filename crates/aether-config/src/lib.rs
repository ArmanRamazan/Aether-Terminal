//! Configuration loading for Aether Terminal.
//!
//! Supports TOML and YAML with auto-detection by file extension.
//! Environment variable interpolation via `${VAR_NAME}` syntax.

pub mod error;
pub mod loader;
pub mod types;

pub use error::ConfigError;
pub use loader::load;
pub use types::AetherConfig;

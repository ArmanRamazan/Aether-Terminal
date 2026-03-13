# aether-config

## Purpose
Configuration loading crate. Parses `aether.toml` or `aether.yaml`, auto-detecting format by extension. Supports `${ENV_VAR}` interpolation and validation.

## Modules
- `types.rs` — AetherConfig and all nested config structs with serde defaults
- `loader.rs` — `load(path)` and `load_str(content, format)` with env interpolation
- `error.rs` — ConfigError enum (Io, TomlParse, YamlParse, UnsupportedFormat, Validation)

## Key Types
- `AetherConfig` — top-level config (discovery, targets, thresholds, output, api, scrape, probe)
- `TargetConfig` — monitored service definition
- `ThresholdConfig` — diagnostic thresholds with sensible defaults
- `OutputConfig` — Slack/Discord/Telegram/stdout/file notification channels

## Strict Rules
- Depends ONLY on aether-core (hexagonal architecture)
- All config types: Debug, Clone, Serialize, Deserialize
- All types with sensible defaults implement Default
- `pub(crate)` by default, `pub` only for cross-crate API
- No `.unwrap()` in production code
- Validation runs after parsing, before returning config

## Testing
```bash
cargo test -p aether-config
```
- Tests cover TOML, YAML, defaults, env interpolation, invalid format, validation

## Key Dependencies
- serde + toml + serde_yaml (parsing)
- thiserror (errors)
- regex (env interpolation)

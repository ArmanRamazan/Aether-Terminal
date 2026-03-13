# aether-output

## Purpose
Output pipeline crate. Routes diagnostic alerts to external notification channels (Slack, Discord, Telegram) and local outputs (stdout, file) with severity filtering and deduplication.

## Modules
- `pipeline.rs` — OutputPipeline: severity routing, dedup, concurrent dispatch
- `slack.rs` — SlackSink: Slack Incoming Webhook with Block Kit formatting
- `discord.rs` — DiscordSink: Discord Webhook with embed formatting
- `telegram.rs` — TelegramSink: Telegram Bot API with Markdown
- `stdout.rs` — StdoutSink: stdout output (JSON or human-readable text)
- `file.rs` — FileSink: JSON-line file append with size-based rotation
- `error.rs` — OutputError enum

## Strict Rules
- Depends ONLY on aether-core (hexagonal architecture)
- All sinks implement `OutputSink` trait from aether-core
- No `.unwrap()` in production code
- Webhook errors are logged, never propagated to caller
- Dedup key: (target, category) tuple
- `pub(crate)` by default, `pub` only for cross-crate API

## Testing
```bash
cargo test -p aether-output
```

## Key Dependencies
- aether-core (traits, types)
- reqwest (webhook HTTP calls)
- tokio (async I/O, timers)
- serde_json (serialization)
- async-trait
- thiserror

# Aether-Terminal

## Project Overview

Aether-Terminal — кинематографический 3D TUI системный монитор на Rust с eBPF-телеметрией, предиктивным AI, JIT-компилируемым DSL и интеграцией MCP для ИИ-агентов. Процессы отображаются как узлы 3D-графа с RPG-механикой (HP, XP, ранги).

**Роль в портфолио**: Инновационный showcase уровня 10/10 по сложности реализации — компилятор, eBPF, ML inference, 3D рендер, всё на Rust.

## Tech Stack

- **Language**: Rust (edition 2021, MSRV 1.75+)
- **Async runtime**: tokio (full features)
- **TUI**: ratatui + crossterm
- **3D math**: glam (Vec3, Mat4, projections)
- **Graph**: petgraph (StableGraph)
- **eBPF**: aya (pure-Rust BPF loader, Linux only, feature-gated)
- **System metrics**: sysinfo (crossplatform fallback)
- **ML inference**: tract-onnx (pure-Rust ONNX runtime, feature-gated)
- **JIT compiler**: cranelift-codegen + cranelift-frontend (rule DSL)
- **DSL parser**: logos (lexer) + hand-written recursive descent parser
- **MCP**: rmcp (Rust MCP SDK), axum (SSE transport)
- **Storage**: rusqlite (bundled SQLite)
- **CLI**: clap (derive)
- **Logging**: tracing + tracing-subscriber

## Architecture

Cargo workspace с 9 crates. Hexagonal architecture — все crates зависят от `aether-core`, но НЕ друг от друга. Общение через traits и tokio channels.

```
aether-terminal    (bin) → orchestrates all crates
aether-core        (lib) → types, traits, graph, events
aether-ebpf        (lib) → eBPF loader, ring buffer, kernel probes
aether-ingestion   (lib) → sysinfo fallback + eBPF bridge, pipeline
aether-predict     (lib) → ONNX inference, anomaly prediction
aether-script      (lib) → DSL lexer/parser/AST, Cranelift JIT compiler
aether-render      (lib) → TUI + 3D rasterizer
aether-mcp         (lib) → MCP server (stdio + SSE)
aether-gamification(lib) → HP, XP, achievements, SQLite
```

Design doc: `docs/plans/2026-03-08-aether-terminal-design.md`

## Code Style

- **Formatting**: `cargo fmt` (rustfmt defaults)
- **Linting**: `cargo clippy -- -D warnings`
- **Error handling**: `thiserror` for library crates, `anyhow` in binary crate only
- **Naming**: snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE for constants
- **Visibility**: `pub(crate)` by default, `pub` only for cross-crate API
- **Tests**: inline `#[cfg(test)] mod tests` per file, integration tests in `tests/` dir
- **Comments**: doc-comments (`///`) on all public items. No commented-out code.
- **Imports**: group by std → external → crate. Use `use` at top of file.
- **Unsafe**: FORBIDDEN unless for eBPF FFI (`aether-ebpf`) or Cranelift JIT function calls (`aether-script`). Document every `unsafe` block with safety invariant.

## Build & Test Commands

```bash
cargo check --workspace          # fast type check
cargo build --workspace          # full build
cargo test --workspace           # all tests
cargo test -p aether-core        # single crate tests
cargo clippy --workspace         # lint
cargo fmt --check                # format check
cargo run -p aether-terminal     # run the app
```

## Git Conventions

- Branch naming: `feat/<name>`, `fix/<name>`, `refactor/<name>`
- Commit format: `type(scope): description` (e.g. `feat(core): add WorldGraph`)
- Types: feat, fix, refactor, test, docs, chore
- Scopes: core, ingestion, render, mcp, gamification, workspace, orchestrator
- **NEVER add `Co-Authored-By` lines** — all commits must be authored solely by the project owner. This applies to all agents, orchestrators, and manual commits without exception.

## Orchestrator

Automated sprint execution via `tools/orchestrator/`:

```bash
cd tools/orchestrator
python main.py tasks/<sprint>.yaml          # run
python main.py tasks/<sprint>.yaml --dry-run # preview
python main.py --resume                      # resume
python main.py --status                      # check progress
```

Sprint YAML format: see `tools/orchestrator/tasks/ms1-workspace-setup.yaml`

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| eBPF loader | aya (pure Rust) | No C deps, aligns with Rust-only philosophy |
| Metrics fallback | sysinfo | WSL2 dev, crossplatform when eBPF unavailable |
| 3D | Custom software rasterizer | Portfolio showcase, Braille 2x4 density |
| MCP transport | stdio + SSE dual mode | Claude Desktop compat + realtime Arbiter |
| Graph library | petgraph::StableGraph | Stable indices survive node removal |
| ML inference | tract-onnx (pure Rust) | No C++ runtime, train in PyTorch → run in Rust |
| JIT compiler | Cranelift | Pure Rust, ms-fast compilation, designed for JIT |
| DSL parser | Hand-written recursive descent | Demonstrates compiler skills, no generator deps |
| Gamification | Light RPG first, full later | Professional tool first, fun second |

## Crate-Specific Context

Each crate has its own CLAUDE.md with module-specific rules:
- `crates/aether-core/CLAUDE.md`
- `crates/aether-ebpf/CLAUDE.md`
- `crates/aether-ingestion/CLAUDE.md`
- `crates/aether-predict/CLAUDE.md`
- `crates/aether-script/CLAUDE.md`
- `crates/aether-render/CLAUDE.md`
- `crates/aether-mcp/CLAUDE.md`
- `crates/aether-gamification/CLAUDE.md`

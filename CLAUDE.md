# Aether-Terminal

## Project Overview

Aether-Terminal — production diagnostics platform с тремя уровнями интеллекта (Rules → ML → AI Agent). Наблюдает за инфраструктурой, детектит проблемы детерминистически, предсказывает аномалии через ML, позволяет AI-агентам управлять через MCP. Процессы отображаются как узлы 3D-графа с RPG-механикой (HP, XP, ранги).

**Роль в экосистеме**: Observer-слой для self-healing платформы из 5 проектов (Aether-Terminal, K8s Autoscaler, Service Graph, Auto-Fix Agent, Custom Orchestrator). Предоставляет API и Event Bus для внешних сервисов.

**Роль в портфолио**: Инновационный showcase — компилятор, eBPF, ML inference, 3D рендер, gRPC API, всё на Rust.

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
- **gRPC**: tonic + prost (machine-to-machine API)
- **Web backend**: axum (REST + WebSocket)
- **Web frontend**: React 18 + TypeScript + Vite + recharts
- **Diagnostics**: deterministic rule engine (30+ rules), trend/capacity/correlation analyzers
- **Monitoring**: Prometheus text format export + PromQL consumer + active scraper
- **Config**: serde + toml + serde_yaml (TOML/YAML auto-detect)
- **Storage**: rusqlite (bundled SQLite)
- **CLI**: clap (derive)
- **Logging**: tracing + tracing-subscriber

## Architecture

Cargo workspace с 17 crates (12 existing + 5 planned). Hexagonal architecture — все crates зависят от `aether-core`, но НЕ друг от друга. Общение через traits и tokio channels.

```
EXISTING (12):
aether-terminal    (bin) → orchestrates all crates
aether-core        (lib) → types, traits, graph, events, event bus
aether-ebpf        (lib) → eBPF loader, ring buffer, kernel probes
aether-ingestion   (lib) → sysinfo fallback + eBPF bridge, pipeline
aether-predict     (lib) → ONNX inference, anomaly prediction
aether-script      (lib) → DSL lexer/parser/AST, Cranelift JIT compiler
aether-render      (lib) → TUI + 3D rasterizer
aether-mcp         (lib) → MCP server (stdio + SSE)
aether-gamification(lib) → HP, XP, achievements, SQLite
aether-analyze     (lib) → deterministic diagnostics, 30+ rules, analyzers
aether-metrics     (lib) → Prometheus exporter + consumer + scraper
aether-web         (lib) → React SPA + axum backend (REST + WebSocket)

PLANNED (5, Phase 2):
aether-config      (lib) → TOML/YAML config, validation, env interpolation
aether-discovery   (lib) → auto-discovery (ports, K8s API, known patterns)
aether-prober      (lib) → HTTP health, TCP latency, DNS, TLS checks
aether-output      (lib) → Slack, Discord, Telegram webhooks + stdout/file
aether-api         (lib) → gRPC server (tonic), event bus streaming
```

Design docs:
- `docs/plans/2026-03-13-global-vision.md` — 5-project ecosystem
- `docs/plans/2026-03-13-roadmap.md` — Phase 0-3 implementation roadmap
- `docs/plans/2026-03-13-v1-production-diagnostics-design.md` — v1.0 detailed spec

## Architecture Principles

**Hexagonal Architecture (Ports & Adapters)**
- `aether-core` — центр: доменные типы, traits (ports), события. Нулевые зависимости на внешние crates кроме `petgraph`, `glam`, `serde`.
- Все остальные crates — адаптеры: реализуют traits из core. **НИКОГДА** не зависят друг от друга, только от core.
- Связь между crates — исключительно через `tokio::sync` каналы (`mpsc`, `broadcast`) и `Arc<RwLock<T>>`.
- Dependency rule: зависимости направлены ВНУТРЬ (к core), никогда наружу. `aether-terminal` (bin) — единственный crate, который знает обо всех адаптерах.

**YAGNI — You Aren't Gonna Need It**
- Реализуй только то, что требуется текущей задачей. Никаких "а вдруг пригодится".
- Не добавляй конфигурационных параметров, feature flags, абстрактных фабрик "на будущее".
- Если функционал не описан в задаче — его не существует. Не угадывай требования.
- Builder pattern, generic-и, trait objects — только когда реально есть >1 реализация СЕЙЧАС.

**KISS — Keep It Simple, Stupid**
- Предпочитай простой линейный код вложенным абстракциям.
- Конкретные типы лучше trait objects, пока нет реальной нужды в динамическом диспатче.
- `match` лучше цепочки `if let`. Прямой вызов лучше indirect dispatch.
- Если решение помещается в одну функцию — не создавай struct + impl + trait.
- Плоская структура модулей: не делай `mod foo { mod bar { mod baz } }` если `foo.rs` достаточно.

**DRY — Don't Repeat Yourself (но с умом)**
- Извлекай общий код в функцию/метод только когда паттерн повторяется **3+ раза** и **семантически идентичен**.
- Два похожих блока кода ≠ дублирование, если они меняются по разным причинам.
- Не абстрагируй ради красоты. Копипаста лучше неправильной абстракции.
- Общие типы — в `aether-core`. Утилиты уровня crate — в `utils.rs` того же crate.

**Single Responsibility**
- Каждый файл — одна ответственность. `graph.rs` = WorldGraph, `events.rs` = события, `pipeline.rs` = pipeline.
- Каждый struct — одна роль. Не смешивай состояние, конфигурацию и логику в одном struct.
- Функции до ~50 строк. Если длиннее — разбей на вспомогательные с говорящими именами.

**Dependency Injection**
- Конструкторы принимают зависимости явно: `fn new(probe: Arc<dyn SystemProbe>, tx: mpsc::Sender<SystemEvent>)`.
- Никаких глобальных `static mut`, `lazy_static` с мутабельным состоянием, синглтонов.
- Тестируемость: любой struct можно создать в тесте с mock-зависимостями.

**API Stability (для library crates)**
- `pub(crate)` по умолчанию. `pub` — ТОЛЬКО для cross-crate API.
- `lib.rs` — ТОЛЬКО `pub mod` + `pub use` re-exports. Нулевая логика.
- Не дублируй публичные методы (одна функция = один способ вызова).
- Не экспортируй internal types (HashMap keys, builder internals).
- `format!("{:?}")` ЗАПРЕЩЁН для API сериализации — используй `Display` impl или `Serialize`.
- Каждый тип в JSON API должен иметь стабильный контракт через Serialize/Deserialize.

**Additive Architecture**
- Новый код ДОПОЛНЯЕТ существующий, не переписывает.
- Новый crate = реализация trait из core. Ноль изменений в существующих crate-ах.
- Trait-ы в core определяют контракт заранее (DataSource, OutputSink, ServiceDiscovery, EventBus).
- `aether-terminal` (bin) — единственное место где wiring меняется при добавлении crate-ов.

## Rust Best Practices

### Ownership & Borrowing
- Передавай `&self` для read, `&mut self` для write. Избегай `Clone` без необходимости.
- `Arc<T>` — для shared ownership через async boundaries. `Rc<T>` запрещён (не Send).
- `Arc<RwLock<T>>` — для shared mutable state (WorldGraph). `Arc<Mutex<T>>` — для простых случаев.
- Возвращай `impl Iterator` вместо `Vec` когда вызывающий может быть ленивым.
- Используй `Cow<'_, str>` только при реальной нужде избежать аллокации, не "на всякий случай".

### Error Handling
- **Library crates**: `thiserror` с конкретными enum вариантами. Каждый crate — свой `Error` тип.
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum CoreError {
      #[error("process {pid} not found")]
      ProcessNotFound { pid: u32 },
      #[error("graph operation failed: {0}")]
      GraphError(String),
  }
  ```
- **Binary crate**: `anyhow::Result` в main и интеграционном коде.
- **ЗАПРЕЩЕНО**: `.unwrap()` в production code. Допустимо ТОЛЬКО в тестах и `const` инициализации.
- **ЗАПРЕЩЕНО**: `.expect()` в HTTP/WebSocket handlers — poisoned lock не должен паниковать сервер. Возвращай HTTP 500.
- `.expect("инвариант")` — допустимо ТОЛЬКО когда паника = баг в нашей логике (не I/O, не user input, не lock poisoning).
- `?` operator — основной способ проброса ошибок. Не оборачивай в `.map_err()` без добавления контекста.
- В web handlers: используй `WebError` enum с `IntoResponse` — НИКОГДА прямой StatusCode return.

### Type System
- Используй newtype pattern для доменных значений: `struct Pid(u32)`, `struct Hp(f32)` — когда это улучшает читаемость API.
- `enum` > bool параметры. `fn set_mode(mode: Mode)` >> `fn set_mode(is_fast: bool)`.
- Derive порядок: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]` — от самого базового.
- `#[must_use]` на функциях, возвращающих Result или значимые результаты.
- `#[non_exhaustive]` на **ВСЕХ** public enums в aether-core — обязательно для semver safety.
- Все `match` на `#[non_exhaustive]` enums из других crates **ОБЯЗАНЫ** иметь `_ =>` wildcard.

### Naming Conventions
- **Файлы**: `snake_case.rs` — одно слово или два через `_`. Никаких длинных имён типа `system_probe_implementation.rs`.
- **Типы**: `PascalCase`. Без суффиксов `Manager`, `Handler`, `Helper` — называй по роли: `Pipeline`, `Engine`, `Canvas`.
- **Функции**: `snake_case`. Начинай с глагола: `add_process`, `render_frame`, `parse_rule`.
- **Константы**: `SCREAMING_SNAKE`. Группируй в `impl` блоки или модули.
- **Модули**: совпадают с главным типом файла. `graph.rs` → `pub struct WorldGraph`.
- **Trait-ы**: прилагательное или существительное: `SystemProbe`, `Storage`, `Renderable`. Без `I` prefix.

### Async Code
- `async fn` — только когда функция реально делает I/O или await-ит.
- Не делай функцию async если она просто вычисляет значение синхронно.
- `tokio::spawn` — для долгоживущих задач (pipeline, engine). Не спавнь таск для одного await.
- `tokio::select!` — для multiplexing каналов с cancellation. Всегда включай cancel branch.
- `CancellationToken` (из `tokio-util`) — для graceful shutdown вместо raw channels.

### Testing
- Inline тесты: `#[cfg(test)] mod tests { use super::*; ... }` в конце каждого файла.
- Каждый тест — одно утверждение (логически). Имя = `test_<что_проверяем>_<ожидание>`.
  ```rust
  #[test]
  fn test_add_process_increases_count() { ... }
  #[test]
  fn test_remove_nonexistent_returns_false() { ... }
  ```
- Используй `assert_eq!`, `assert!`, `assert_matches!` с описанием: `assert_eq!(count, 1, "after adding one process")`.
- Для async тестов: `#[tokio::test]`. Для тестов с таймаутом: `tokio::time::timeout`.
- Тестовые утилиты и фикстуры — в `#[cfg(test)]` блоке, не в production коде.
- **Не** мокай то, что можно создать напрямую. Реальный `WorldGraph` в тестах лучше мока.

### Documentation
- `///` doc-comment на каждом `pub` item. Одна строка — достаточно для очевидных вещей.
- Первая строка — краткое описание (без "This function..."). Просто что делает.
  ```rust
  /// Add a process node to the graph. Returns its index.
  pub fn add_process(&mut self, node: ProcessNode) -> NodeIndex { ... }
  ```
- `//` inline комментарии — только для неочевидной логики, хаков, бизнес-правил.
- **НЕ** комментируй очевидное: `// increment counter` перед `counter += 1` — запрещено.
- `// TODO:` — допустимо с issue reference или описанием "зачем потом".
- Никакого закомментированного кода. Dead code → удаляй.

### Module Structure Per Crate
```
crates/aether-<name>/
├── Cargo.toml
├── CLAUDE.md            # crate-specific context
└── src/
    ├── lib.rs           # pub mod declarations, re-exports
    ├── <module>.rs      # один файл = один модуль = один ключевой struct
    ├── error.rs         # crate Error enum (thiserror)
    └── tests/           # integration tests (optional)
```
- `lib.rs` — только `pub mod` и `pub use` re-exports. Нулевая логика.
- Не создавай подпапки (`src/tui/widgets/`) пока в модуле < 3 файлов.
- Когда модуль растёт до 3+ файлов — конвертируй `module.rs` → `module/mod.rs` + подфайлы.

### Performance Mindset
- Не оптимизируй преждевременно, но и не пессимизируй.
- `&str` вместо `String` в параметрах. `Into<String>` для конструкторов если нужен owned.
- `Vec::with_capacity(n)` когда размер известен заранее.
- `HashMap`/`HashSet` — дефолтный hasher ок. `FxHashMap` только после профилирования.
- Hot path (60fps render loop): минимум аллокаций, переиспользуй буферы.
- Cold path (инициализация, конфиг): читаемость важнее наносекунд.

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
| Diagnostics | Deterministic rules first | Predictable, no ML required for base functionality |
| Config format | TOML + YAML dual | TOML for local dev, YAML for K8s — auto-detect by extension |
| Integration API | gRPC (tonic) | Machine-to-machine for future ecosystem projects |
| Event bus | broadcast channel → gRPC stream | In-process first, network-capable later |
| Webhooks | Slack/Discord/Telegram builtin | Most popular, covers 90% of teams |
| Service discovery | Port scan + K8s API | Auto-discovery for wow-effect, config for precision |

## Scopes for Commits

- core, ingestion, render, mcp, gamification, ebpf, predict, script
- analyze, metrics, web, config, discovery, prober, output, api
- workspace, orchestrator, ci, docker, docs

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
- `crates/aether-analyze/CLAUDE.md`
- `crates/aether-metrics/CLAUDE.md`
- `crates/aether-web/CLAUDE.md`
- `crates/aether-terminal/CLAUDE.md`

# Contributing to Aether-Terminal

Thank you for your interest in contributing to Aether-Terminal! This guide will help you get started.

## Development Environment Setup

### Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **Node.js** 18+ and **npm** (for the web frontend in `crates/aether-web/frontend/`)
- **Git**

### Building

```bash
# Clone the repository
git clone https://github.com/AquamarineIndigo/Aether-Terminal.git
cd Aether-Terminal

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run the application
cargo run -p aether-terminal
```

### Frontend (React)

```bash
cd crates/aether-web/frontend
npm install
npm run dev    # development server
npm run build  # production build
```

## Code Style

All code must pass formatting and linting checks before merge:

```bash
cargo fmt --check        # formatting
cargo clippy --workspace # linting (must have zero warnings)
```

- **Rust**: follow `rustfmt` defaults, `clippy` with `-D warnings`
- **TypeScript/React**: Vite + TypeScript strict mode

See [CLAUDE.md](./CLAUDE.md) for detailed coding conventions.

## Commit Format

We use conventional commits:

```
type(scope): description
```

**Types**: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

**Scopes**: `core`, `ingestion`, `render`, `mcp`, `gamification`, `ebpf`, `predict`, `script`, `analyze`, `metrics`, `web`, `config`, `discovery`, `prober`, `output`, `api`, `workspace`, `orchestrator`, `ci`, `docker`, `docs`

**Examples**:
```
feat(core): add process dependency tracking
fix(render): prevent panic on zero-size terminal
docs(web): update REST API documentation
```

## Pull Request Process

1. **Fork** the repository and create a branch from `main`:
   ```bash
   git checkout -b feat/my-feature
   ```

2. **Make your changes** following the code style guidelines above.

3. **Test** your changes:
   ```bash
   cargo test --workspace
   cargo clippy --workspace
   cargo fmt --check
   ```

4. **Commit** using the conventional commit format.

5. **Push** your branch and open a Pull Request against `main`.

6. Fill out the PR template and wait for review.

## Architecture

Aether-Terminal uses a hexagonal architecture with a Cargo workspace of 12+ crates. All crates depend on `aether-core` but never on each other.

For a detailed overview, see [docs/architecture.md](./docs/architecture.md).

Key principles:
- **Hexagonal**: all crates are adapters around `aether-core`
- **Additive**: new features supplement existing code, no rewrites
- **Three intelligence levels**: Rules → ML → AI Agent

## Where to Start

Look for issues labeled [`good first issue`](https://github.com/AquamarineIndigo/Aether-Terminal/labels/good%20first%20issue) — these are scoped, well-described tasks suitable for newcomers.

If you're unsure about anything, open an issue or discussion to ask before starting work.

## Reporting Bugs

Use the [bug report template](https://github.com/AquamarineIndigo/Aether-Terminal/issues/new?template=bug_report.md) when filing bugs.

## Requesting Features

Use the [feature request template](https://github.com/AquamarineIndigo/Aether-Terminal/issues/new?template=feature_request.md) for new ideas.

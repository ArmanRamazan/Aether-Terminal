# ADR-005: JIT-compiled rule DSL via Cranelift

**Status:** Accepted
**Date:** 2026-03-08

## Context

Currently all monitoring logic is hardcoded. Users cannot define custom alerts or reactions without modifying Rust source code. We need a way for users to write rules that are expressive, fast, and hot-reloadable.

## Decision

Build a custom domain-specific language (Aether DSL) with a full compilation pipeline: lexer (logos) -> parser (recursive descent) -> type checker -> Cranelift JIT codegen. Rules compile to native machine code.

## Rationale

- **Cranelift over LLVM**: Cranelift is pure Rust, compiles in milliseconds (vs seconds for LLVM), designed for JIT. Perfect for hot-reload.
- **Custom DSL over embedded scripting (Lua/Rhai)**: Custom DSL provides type safety, domain-specific syntax (`when ... then ...`), and compiles to native code instead of interpretation. Portfolio differentiator.
- **JIT over AOT**: Rules must be reloadable at runtime without restarting the monitor. Atomic swap of compiled function pointers achieves this.

## Technical Approach

```
rule <name> {
  when <condition>
  then <action>
}
```

- Lexer: `logos` crate for zero-copy tokenization
- Parser: hand-written recursive descent (no parser generators — demonstrates compiler skills)
- Type system: `Process`, `System`, `Duration`, `Percentage`, numeric types
- Codegen: Cranelift `Function` → native code via `JITModule`
- Hot-reload: file watcher (notify) or SIGHUP → recompile → `Arc::swap`

## Consequences

- Adds ~3K LOC for the compiler pipeline
- Cranelift is a non-trivial dependency (~2MB compile time)
- Error messages must be user-friendly (source locations, suggestions)
- Security: DSL must NOT allow arbitrary system calls — only predefined actions

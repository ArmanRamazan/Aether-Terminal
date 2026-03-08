# aether-script

## Purpose
JIT-compiled rule DSL engine. Parses `.aether` rule files through a custom lexer (logos) and recursive-descent parser, type-checks the AST, compiles to native x86_64/aarch64 via Cranelift, and evaluates rules against WorldState each tick. Supports hot-reload via file watcher or SIGHUP.

## Modules
- `lexer.rs` — Token enum with logos derive macros, lexer entry point
- `parser.rs` — recursive-descent parser: tokens → AST
- `ast.rs` — Rule, WhenClause, ThenClause, Expr, BinaryOp, Duration, Severity, etc.
- `typechecker.rs` — validates AST: field existence, type compatibility, duration constraints
- `codegen.rs` — Cranelift IR generator: typed AST → CLIF → native function pointers
- `compiler.rs` — CompilerPipeline: orchestrates lex → parse → typecheck → codegen
- `runtime.rs` — CompiledRuleSet: holds compiled function pointers, evaluate(WorldState) → Vec<RuleAction>
- `hotreload.rs` — file watcher (notify crate) + SIGHUP handler → recompile → atomic swap via Arc<RwLock>
- `actions.rs` — RuleAction enum: Alert, Kill, Log, Metric
- `lib.rs` — re-exports, ScriptEngine top-level struct

## Compilation Pipeline
```
.aether file → Lexer (logos) → Token stream
                                    ↓
                              Parser (recursive descent) → AST
                                    ↓
                              Type Checker → Typed AST
                                    ↓
                              Cranelift IR Generator → CLIF
                                    ↓
                              Cranelift Codegen → Native x86_64/aarch64
                                    ↓
                              CompiledRuleSet (function pointers)
```

## Runtime Evaluation
```
WorldState → CompiledRuleSet.evaluate() → Vec<RuleAction>

RuleAction = Alert { message, severity }
           | Kill { pid }
           | Log { message }
           | Metric { name, value }
```

## Hot-Reload
```
File watcher (notify) or SIGHUP → recompile → atomic swap via Arc<RwLock<CompiledRuleSet>>
```

## DSL Syntax Example
```
rule memory_leak {
  when process.mem_growth > 5% for 60s
  then alert "memory leak detected" severity warning
}
```

## Rules
- Lexer MUST use logos — no hand-written character-by-character lexer
- Parser MUST be hand-written recursive descent — no parser generators (pest, lalrpop, nom)
- Codegen MUST use cranelift-codegen + cranelift-frontend — no LLVM, no interpreter
- Hot-reload swap MUST be atomic: Arc<RwLock<CompiledRuleSet>>, never partially compiled state
- Compilation errors must produce clear diagnostics with line/column numbers
- NEVER depend on other aether-* crates except aether-core
- Rule evaluation must be safe: compiled code runs in-process but cannot access arbitrary memory
- Rule files at workspace root: `rules/*.aether`
- SIGHUP reloads all rules; file watcher reloads changed files only

## Testing
```bash
# Unit tests (lexer, parser, typechecker, codegen independently)
cargo test -p aether-script

# Integration tests (compile + evaluate against mock WorldState)
cargo test -p aether-script
```
Test each pipeline stage independently. Lexer tests: token sequences. Parser tests: AST shapes. Codegen tests: evaluate compiled rules against known WorldState, assert correct RuleActions.

## Key Dependencies
- logos (lexer generator)
- cranelift-codegen (native code generation)
- cranelift-frontend (IR builder)
- cranelift-module + cranelift-jit (JIT compilation)
- notify (file watcher for hot-reload)
- tokio (async task, signal handling)
- aether-core (WorldState, RuleAction, traits)

# Architecture Overview

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     aether-terminal (bin)                    │
│                                                             │
│  ┌──────────┐   ┌──────────┐   ┌────────┐   ┌───────────┐ │
│  │ Ingestion│   │  Render  │   │  MCP   │   │Gamification│ │
│  │ Pipeline │   │  Engine  │   │ Server │   │  Engine    │ │
│  └────┬─────┘   └────┬─────┘   └───┬────┘   └─────┬─────┘ │
│       │              │             │               │       │
│       └──────┬───────┴─────┬───────┴───────────────┘       │
│              │             │                               │
│        ┌─────▼─────────────▼─────┐                         │
│        │     aether-core         │                         │
│        │  WorldGraph + Channels  │                         │
│        └─────────────────────────┘                         │
└─────────────────────────────────────────────────────────────┘
```

## Data Flow

```
OS Kernel / sysinfo
       │
       ▼ (10Hz fast_tick, 1Hz slow_tick)
  IngestionPipeline ──mpsc<SystemEvent>──→ Core (WorldGraph updater)
       │                                        │
       │                                   broadcast<WorldState>
       │                                        │
       │                          ┌─────────────┼────────────────┐
       │                          ▼             ▼                ▼
       │                     RenderEngine   McpServer    GamificationEngine
       │                          │             │                │
       │                     Terminal      AI Agent          SQLite
       │                     (TUI+3D)    (JSON-RPC)       (HP/XP/Rank)
       │                          ▲             │                │
       │                          │             ▼                │
       │                          └──── ArbiterQueue ────────────┘
       │                                (approve/deny)
```

## Channel Architecture

| Channel | Type | From | To | Payload |
|---------|------|------|----|---------|
| system_events | `mpsc` | IngestionPipeline | Core Updater | SystemEvent |
| world_state | `broadcast` | Core Updater | Render, MCP, Game | WorldState snapshot |
| agent_actions | `mpsc` | MCP Server | Arbiter Queue | AgentAction |
| game_events | `mpsc` | Core Updater | Gamification Engine | GameEvent |
| arbiter_feedback | `mpsc` | Render (UI) | Arbiter Queue | Approve/Deny |

## Crate Dependency Graph

```
aether-terminal
  ├── aether-core
  ├── aether-ingestion  → aether-core
  ├── aether-render     → aether-core
  ├── aether-mcp        → aether-core
  └── aether-gamification → aether-core
```

Rule: library crates NEVER depend on each other. Only on aether-core. The binary crate wires them together.

## Thread/Task Model

```
Main Thread:
  └── tokio runtime
        ├── task: IngestionPipeline.fast_tick (10Hz)
        ├── task: IngestionPipeline.slow_tick (1Hz)
        ├── task: Core WorldGraph updater (receives SystemEvent, broadcasts WorldState)
        ├── task: GamificationEngine (receives GameEvent, updates HP/XP)
        ├── task: McpServer (stdio OR http, based on CLI flags)
        └── blocking: TUI render loop (crossterm + ratatui, 60fps)
```

## Key Design Patterns

1. **Hexagonal Architecture**: Core defines traits (ports), crates implement (adapters)
2. **Event Sourcing**: All state changes flow through typed events
3. **Shared Nothing**: Crates communicate only via channels and Arc<RwLock<WorldGraph>>
4. **Graceful Degradation**: --no-3d falls back to 2D, --no-game disables gamification
5. **Feature Gating**: eBPF behind `#[cfg(feature = "ebpf")]`

# Aether-Terminal Web UI Design

**Date**: 2026-03-11
**Status**: Approved
**Priority**: Before MVP diagnostic engine (Phase 0)

---

## Decision

Add Web UI as parallel interface to TUI. Launched via `--web [PORT]` flag. TUI and Web run simultaneously, both reading from the same `Arc<RwLock<WorldGraph>>`.

**Tech stack**: Rust backend (axum) + React/TypeScript frontend + react-three-fiber (Three.js) for 3D.

**Single binary**: Vite builds static assets → rust-embed bundles into Rust binary. One `cargo build` = one executable with everything.

---

## Architecture

```
aether-terminal (bin)
  │
  ├── TUI (crossterm/ratatui) — existing, always available
  │
  └── aether-web (lib) — NEW, --web flag
        ├── axum HTTP server
        │     ├── GET /*           → embedded static files (React build)
        │     ├── GET /api/*       → REST endpoints (JSON)
        │     └── GET /ws          → WebSocket (real-time push)
        │
        └── frontend/ (React/TypeScript)
              ├── 3D Graph         → react-three-fiber + drei
              ├── Process Table    → sortable, filterable
              ├── Stats Dashboard  → recharts sparklines
              ├── Network View     → connection list
              └── Arbiter Panel    → approve/deny actions
```

## New Crate: aether-web

```
crates/aether-web/
├── Cargo.toml          — axum, tokio, serde_json, rust-embed, tower-http
├── CLAUDE.md
├── build.rs            — runs `npm run build` in frontend/ before cargo build
├── src/
│   ├── lib.rs
│   ├── error.rs        — WebError enum (thiserror)
│   ├── server.rs       — WebServer: axum router setup, serves static + API + WS
│   ├── api.rs          — REST handlers: processes, graph, stats, connections
│   ├── ws.rs           — WebSocket: push WorldState, receive actions
│   └── state.rs        — SharedState: Arc refs to WorldGraph, ArbiterQueue, etc.
└── frontend/
    ├── package.json
    ├── vite.config.ts
    ├── tsconfig.json
    ├── index.html
    └── src/
        ├── main.tsx
        ├── App.tsx             — router + layout
        ├── types/index.ts      — TypeScript types mirroring Rust models
        ├── hooks/
        │   ├── useWorldState.ts    — WebSocket subscription → zustand store
        │   └── useApi.ts           — REST fetch helpers
        ├── stores/
        │   └── worldStore.ts       — zustand: processes, connections, stats
        ├── pages/
        │   ├── OverviewPage.tsx    — process table + stats bar + sparklines
        │   ├── Graph3DPage.tsx     — full 3D graph view
        │   ├── NetworkPage.tsx     — connections table
        │   └── ArbiterPage.tsx     — action queue
        └── components/
            ├── Layout.tsx          — sidebar nav + top stats bar + main area
            ├── Sidebar.tsx         — navigation: Overview, 3D Graph, Network, Arbiter
            ├── StatsBar.tsx        — top bar: CPU, MEM, process count, uptime
            ├── ProcessTable.tsx    — sortable table with HP/XP columns
            ├── ProcessDetail.tsx   — side panel on row click
            ├── SparklineChart.tsx  — recharts mini line chart
            ├── Graph3D.tsx         — react-three-fiber scene
            ├── ProcessNode.tsx     — 3D sphere: size=mem, color=hp, pulse=cpu
            ├── ConnectionEdge.tsx  — 3D line: width=bandwidth, particles=flow
            ├── NetworkTable.tsx    — connections list
            └── ArbiterQueue.tsx    — pending actions with approve/deny buttons
```

## WebSocket Protocol

### Server → Client (push every 500ms)

```typescript
interface WorldUpdate {
  type: "world_state";
  data: {
    processes: Process[];
    connections: Connection[];
    stats: SystemStats;
    timestamp: number;
  };
}

interface Process {
  pid: number;
  ppid: number;
  name: string;
  cpu_percent: number;
  mem_bytes: number;
  state: "running" | "sleeping" | "zombie" | "stopped";
  hp: number;
  xp: number;
  position: [number, number, number];  // 3D coordinates from layout engine
}

interface Connection {
  from_pid: number;
  to_pid: number;
  protocol: string;
  bytes_per_sec: number;
}

interface SystemStats {
  process_count: number;
  total_cpu_percent: number;
  total_memory_bytes: number;
  total_memory_used: number;
  uptime_seconds: number;
}
```

### Client → Server (actions)

```typescript
interface ArbiterAction {
  type: "arbiter_action";
  action: "approve" | "deny";
  action_id: string;
}

interface SelectProcess {
  type: "select_process";
  pid: number;
}
```

## REST API

```
GET /api/processes          → Process[] (full list)
GET /api/processes/:pid     → Process + connections + details
GET /api/connections        → Connection[] (all)
GET /api/stats              → SystemStats
GET /api/arbiter/pending    → PendingAction[]
POST /api/arbiter/:id       → { action: "approve" | "deny" }
```

## 3D Graph Design

### Visual Mapping

| Process Property | 3D Representation |
|-----------------|-------------------|
| Memory usage | Node sphere size (0.3 – 2.0 radius) |
| HP (health) | Color gradient: green (100) → yellow (50) → red (0) |
| CPU percent | Pulsation speed/intensity |
| State: zombie | Grey, no pulse, dissolve particles |
| Critical diag | Red glow (bloom effect) |
| Warning diag | Yellow ring outline |

### Edge Visual

| Connection Property | 3D Representation |
|-------------------|-------------------|
| bytes_per_sec | Line thickness |
| Protocol | Color: TCP=blue, UDP=green, HTTP=cyan |
| Active data flow | Animated particles along edge |

### Camera & Interaction

- OrbitControls: left-drag rotate, right-drag pan, scroll zoom
- Click node: select, show detail panel on side
- Hover: tooltip with name, PID, CPU, MEM
- Double-click: zoom to node

### Effects (drei + postprocessing)

- Bloom: glow on critical/high-CPU nodes
- Billboard text: process name labels always face camera
- Grid helper: subtle ground grid for spatial reference

## CLI Flag

```
--web [PORT]    Start web dashboard alongside TUI (default: 8080)
```

## Build Pipeline

### Development
```bash
cd crates/aether-web/frontend
npm install
npm run dev          # Vite dev server on :5173, proxies /api and /ws to Rust

# In parallel:
cargo run -p aether-terminal -- --web 8080
```

### Production
```bash
cd crates/aether-web/frontend
npm run build        # outputs to frontend/dist/

# build.rs detects frontend/dist/ and embeds via rust-embed
cargo build --release -p aether-terminal
# Single binary, serves embedded frontend
```

## Dependencies

### Rust (aether-web/Cargo.toml)
- axum + axum-extra (WebSocket)
- tower-http (CORS, static files)
- rust-embed (embed frontend build)
- serde_json
- tokio
- tracing

### Frontend (package.json)
- react, react-dom
- @react-three/fiber, @react-three/drei, three
- @react-three/postprocessing
- zustand (state management)
- recharts (charts)
- vite, typescript

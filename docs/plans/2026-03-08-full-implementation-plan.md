# Aether-Terminal: Full Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a cinematic 3D TUI system monitor in Rust with eBPF telemetry, predictive AI, JIT-compiled rule DSL, MCP integration, and RPG gamification.

**Architecture:** 9-crate cargo workspace (hexagonal). 8 milestones, ~25 sprints, ~95 tasks.

**Tech Stack:** Rust, tokio, ratatui, glam, petgraph, aya, tract-onnx, cranelift, logos, sysinfo, rmcp, rusqlite

---

## Milestone Map

> **Progress** (updated 2026-03-10): MS1-MS4 complete, MS5 partial, MS6-MS8 pending.

```
MS1: Foundation (core + ingestion)     ████████████████████ DONE
MS2: TUI Shell (render basics)         ████████████████████ DONE
MS3: 3D Engine (rasterizer)            ████████████████████ DONE
MS4: AI Bridge (MCP)                   ████████████████████ DONE
MS5: Gamification & Polish             ██████████████░░░░░░ 70% (HP/XP/persistence done, polish pending)
MS6: eBPF Telemetry Engine             ░░░░░░░░░░░░░░░░░░░░ TODO
MS7: JIT-Compiled Rule DSL             ░░░░░░░░░░░░░░░░░░░░ TODO
MS8: Predictive AI Engine              ░░░░░░░░░░░░░░░░░░░░ TODO
```

Each milestone has checkpoints that produce a working binary. You can demo at any checkpoint.

### Sprint YAML Status

Completed sprints (in `tools/orchestrator/tasks/done/`):
- MS1: ms1-workspace-setup, ms1-ingestion, ms1-ingestion-sysinfo
- MS2: ms2-tui-framework, ms2-overview-tab, ms2-vim-navigation, ms2-network-tab
- MS3: ms3-3d-math, ms3-rasterizer, ms3-graph-layout, ms3-scene-renderer, ms3-visual-effects
- MS4: ms4-mcp-server, ms4-mcp-tools, ms4-arbiter
- MS5: ms5-hp-xp, ms5-persistence

Remaining sprints (in `tools/orchestrator/tasks/`):
- MS5: ms5-polish
- MS6: ms6-ebpf-foundation, ms6-ebpf-integration
- MS7: ms7-lexer-parser, ms7-type-checker, ms7-cranelift-jit, ms7-hot-reload
- MS8: ms8-features, ms8-inference, ms8-predict-integration

---

# MILESTONE 1: Foundation

**Goal:** Core types + system data collection. `cargo run` prints live process data.

**Duration:** 3 sprints, ~12 tasks

## Sprint 1.1: Workspace & Core Types

### Task 1.1.1: Initialize cargo workspace
```
Files: Cargo.toml, crates/*/Cargo.toml, crates/*/src/lib.rs, crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo check --workspace
```
- Create root `Cargo.toml` with workspace members
- Create 6 crates: aether-terminal (bin), aether-core, aether-ingestion, aether-render, aether-mcp, aether-gamification (all lib)
- Each lib crate: minimal `lib.rs` with purpose comment
- Binary crate: `main.rs` prints "Aether Terminal v0.1.0"
- Set up dependencies per design doc
- **Commit:** `feat(workspace): initialize cargo workspace with 6 crates`

### Task 1.1.2: Core data models
```
Files: crates/aether-core/src/models.rs
Agent: rust-engineer
Test: cargo test -p aether-core
Depends: 1.1.1
```
- `ProcessNode` — pid, ppid, name, cpu_percent, mem_bytes, state, hp, xp, position_3d (Vec3)
- `ProcessState` enum — Running, Sleeping, Zombie, Stopped
- `NetworkEdge` — source_pid, dest (SocketAddr), protocol, bytes_per_sec, state
- `Protocol` enum — TCP, UDP, DNS, QUIC, HTTP, HTTPS, Unknown
- `ConnectionState` enum — Established, Listen, TimeWait, CloseWait
- `SystemSnapshot` — processes: Vec<ProcessNode>, edges: Vec<NetworkEdge>, timestamp
- All types: `#[derive(Debug, Clone, Serialize, Deserialize)]`
- Tests: construction, serialization round-trip
- **Commit:** `feat(core): define process and network data models`

### Task 1.1.3: World graph
```
Files: crates/aether-core/src/graph.rs
Agent: rust-engineer
Test: cargo test -p aether-core
Depends: 1.1.2
```
- `WorldGraph` struct wrapping `petgraph::StableGraph<ProcessNode, NetworkEdge>`
- Internal `HashMap<u32, NodeIndex>` for O(1) pid → node lookup
- Methods:
  - `new() -> Self`
  - `add_process(node: ProcessNode) -> NodeIndex`
  - `remove_process(pid: u32) -> bool`
  - `update_process(pid: u32, f: impl FnOnce(&mut ProcessNode))`
  - `add_connection(from_pid: u32, to_pid: u32, edge: NetworkEdge) -> Option<EdgeIndex>`
  - `find_by_pid(pid: u32) -> Option<&ProcessNode>`
  - `find_by_pid_mut(pid: u32) -> Option<&mut ProcessNode>`
  - `processes() -> impl Iterator<Item = &ProcessNode>`
  - `edges() -> impl Iterator<Item = &NetworkEdge>`
  - `process_count() -> usize`
  - `edge_count() -> usize`
  - `apply_snapshot(snapshot: &SystemSnapshot)` — sync graph with new data
- Tests: add/remove/find/update/apply_snapshot (5+ tests)
- **Commit:** `feat(core): implement WorldGraph with petgraph`

### Task 1.1.4: Events and trait definitions
```
Files: crates/aether-core/src/events.rs, crates/aether-core/src/traits.rs, crates/aether-core/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-core
Depends: 1.1.3
```
- **events.rs:**
  - `SystemEvent` enum: ProcessCreated{pid, name}, ProcessExited{pid}, MetricsUpdate{snapshot}, TopologyChange
  - `GameEvent` enum: HpChanged{pid, delta, new_hp}, XpEarned{amount, reason}, AchievementUnlocked{id, name}
  - `AgentAction` enum: KillProcess{pid}, RestartService{name}, Inspect{pid}, CustomScript{command}
  - All: `#[derive(Debug, Clone)]`
- **traits.rs:**
  - `trait SystemProbe: Send + Sync + 'static` — `async fn snapshot(&self) -> Result<SystemSnapshot>`, `fn subscribe(&self) -> broadcast::Receiver<SystemEvent>`
  - `trait Storage: Send + Sync + 'static` — `async fn save_session(&self, session: &GameSession) -> Result<()>`, `async fn load_rankings(&self) -> Result<Vec<Ranking>>`
  - `GameSession`, `Ranking` structs
- **lib.rs:** Re-export all modules: `pub mod models, graph, events, traits`
- **Commit:** `feat(core): add event types and hexagonal trait ports`

**Checkpoint 1.1:** `cargo test -p aether-core` — all core types and graph working. Foundation exists.

---

## Sprint 1.2: System Data Collection

### Task 1.2.1: SysinfoProbe — process snapshot
```
Files: crates/aether-ingestion/src/sysinfo_probe.rs, crates/aether-ingestion/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-ingestion
Depends: 1.1.4
```
- `SysinfoProbe` struct implementing `SystemProbe` trait
- Internal `sysinfo::System` refreshed on each `snapshot()` call
- Maps `sysinfo::Process` → `ProcessNode`:
  - pid, ppid from sysinfo
  - name from process name
  - cpu_percent from cpu_usage()
  - mem_bytes from memory()
  - state: map sysinfo ProcessStatus → our ProcessState
  - hp: 100.0 (initial), xp: 0
  - position_3d: Vec3::ZERO (layout assigns later)
- Network edges: use `sysinfo::Networks` for interface-level data (simplified)
- `subscribe()`: creates broadcast channel, spawns tokio task that sends MetricsUpdate every 1s
- Tests: snapshot returns non-empty processes, subscribe receives events
- **Commit:** `feat(ingestion): implement SysinfoProbe for cross-platform metrics`

### Task 1.2.2: Dual-tick async pipeline
```
Files: crates/aether-ingestion/src/pipeline.rs
Agent: rust-engineer
Test: cargo test -p aether-ingestion
Depends: 1.2.1
```
- `IngestionPipeline` struct:
  - Takes `Arc<dyn SystemProbe>` and `mpsc::Sender<SystemEvent>`
  - `async fn run(&self)` — spawns two tokio tasks:
    - `fast_tick`: every 100ms (10Hz for MVP, 60Hz later), sends MetricsUpdate
    - `slow_tick`: every 1000ms, sends TopologyChange
  - `fn stop(&self)` — cancellation via `CancellationToken`
- Uses `tokio::select!` for graceful shutdown
- Tests: pipeline starts/stops cleanly, events arrive within expected interval
- **Commit:** `feat(ingestion): add dual-tick async pipeline`

### Task 1.2.3: Integration — main.rs prints live data
```
Files: crates/aether-terminal/src/main.rs
Agent: rust-engineer
Test: cargo run -p aether-terminal (manual verify: prints process list)
Depends: 1.2.2
```
- Wire up: SysinfoProbe → IngestionPipeline → mpsc channel → main loop
- Main loop: receive SystemEvent, print process count, top 5 by CPU
- Format: `[PID 1234] firefox (CPU: 15.2%, MEM: 512MB, HP: 100)`
- Run for 5 seconds then exit (temporary, for testing)
- **Commit:** `feat(terminal): wire ingestion to main and print live data`

**CHECKPOINT MS1:** `cargo run -p aether-terminal` prints live process data to console. Core foundation works.

---

# MILESTONE 2: TUI Shell

**Goal:** Full TUI with tabs, process table, sparklines. No 3D yet — that's MS3.

**Duration:** 4 sprints, ~16 tasks

## Sprint 2.1: Basic TUI Framework

### Task 2.1.1: App struct and event loop
```
Files: crates/aether-render/src/tui/app.rs, crates/aether-render/src/tui/mod.rs, crates/aether-render/src/lib.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 1.2.3
```
- `App` struct:
  - `current_tab: Tab` enum (Overview, World3D, Network, Arbiter)
  - `world: Arc<RwLock<WorldGraph>>` (shared with ingestion)
  - `should_quit: bool`
  - `tick_rate: Duration` (16ms = 60fps)
- `run(&mut self, terminal: &mut Terminal)` — main loop:
  - Poll crossterm events (key press, resize)
  - Receive world state updates via channel
  - Render frame
  - `tokio::time::interval` for tick
- Key handling: q/Ctrl-C quit, F1-F4 switch tabs, hjkl navigation
- **Commit:** `feat(render): add TUI app struct with event loop`

### Task 2.1.2: Tab system and layout
```
Files: crates/aether-render/src/tui/app.rs (modify), crates/aether-render/src/tui/tabs.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.1.1
```
- Top bar: tab names with highlight on active `[F1 Overview] [F2 3D World] [F3 Network] [F4 Arbiter]`
- Bottom status bar: `Aether Terminal v0.1 | Processes: 142 | CPU: 23% | RAM: 4.2GB | Rank: Novice | XP: 0`
- Center area: delegated to active tab's render function
- Color scheme: Deep Space background (#050A0E), Electric Cyan text (#00F0FF)
- **Commit:** `feat(render): add tab system with status bar`

### Task 2.1.3: Palette and theme system
```
Files: crates/aether-render/src/palette.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 2.1.2
```
- `Palette` struct with named colors:
  ```rust
  pub const BG: Color = Color::Rgb(5, 10, 14);        // #050A0E
  pub const HEALTHY: Color = Color::Rgb(0, 240, 255);  // #00F0FF
  pub const NEON_BLUE: Color = Color::Rgb(0, 128, 255);// #0080FF
  pub const WARNING: Color = Color::Rgb(252, 238, 9);  // #FCEE09
  pub const CRITICAL: Color = Color::Rgb(255, 0, 60);  // #FF003C
  pub const DATA: Color = Color::Rgb(250, 250, 250);   // #FAFAFA
  pub const XP_PURPLE: Color = Color::Rgb(191, 0, 255);// #BF00FF
  ```
- `fn color_for_load(percent: f32) -> Color` — gradient HEALTHY→NEON_BLUE→WARNING→CRITICAL
- `fn color_for_hp(hp: f32) -> Color` — HEALTHY if >50, WARNING if >20, CRITICAL if ≤20
- Tests: color_for_load returns correct color at boundaries
- **Commit:** `feat(render): add cyberpunk color palette`

### Task 2.1.4: Wire TUI into main.rs
```
Files: crates/aether-terminal/src/main.rs (rewrite)
Agent: rust-engineer
Test: cargo run -p aether-terminal (manual: TUI appears with tabs)
Depends: 2.1.3
```
- Replace println loop with TUI:
  - Initialize crossterm raw mode + alternate screen
  - Create shared `Arc<RwLock<WorldGraph>>`
  - Spawn ingestion pipeline in background tokio task
  - Spawn graph updater task: receives SystemEvent → updates WorldGraph
  - Run App::run() in main thread
  - Cleanup on exit: restore terminal
- CLI args via clap:
  - `--no-3d` flag (for MS3, no-op for now)
  - `--no-game` flag
  - `--theme` flag
  - `--log-level` flag
- **Commit:** `feat(terminal): wire TUI app with live data pipeline`

**Checkpoint 2.1:** TUI opens, tabs switch with F1-F4, status bar shows live CPU/RAM. Cyberpunk colors.

---

## Sprint 2.2: Overview Tab (F1)

### Task 2.2.1: Process table widget
```
Files: crates/aether-render/src/tui/overview.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.1.4
```
- Sortable table: PID, Name, CPU%, MEM, State, HP
- Columns auto-resize to terminal width
- Color-coded rows by load (palette::color_for_load)
- Scroll with j/k, select with Enter
- Header row styled with HEALTHY color
- **Commit:** `feat(render): add process table widget in Overview tab`

### Task 2.2.2: Sparkline widgets
```
Files: crates/aether-render/src/tui/widgets/sparklines.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.2.1
```
- CPU sparkline: rolling 60-sample history (1 per second)
- RAM sparkline: same
- Network throughput sparkline
- Use ratatui's built-in Sparkline widget with custom colors
- Layout: 3 sparklines in a row above the process table
- **Commit:** `feat(render): add CPU/RAM/Network sparkline widgets`

### Task 2.2.3: Process detail panel
```
Files: crates/aether-render/src/tui/overview.rs (modify)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.2.2
```
- When process selected (Enter): right panel slides out showing:
  - Full process info (pid, ppid, user, command, start time)
  - Open connections list
  - HP bar (colored)
  - CPU/MEM history sparklines for this process
- Press Esc to close panel
- **Commit:** `feat(render): add process detail panel`

**Checkpoint 2.2:** Overview tab shows process table with sparklines. Select process → detail panel.

---

## Sprint 2.3: Network Tab (F3)

### Task 2.3.1: Connection list view
```
Files: crates/aether-render/src/tui/network.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.2.3
```
- Table: Source PID, Source Name, Dest IP:Port, Protocol, State, Bytes/s
- Color by protocol: TCP=Cyan, UDP=Blue, DNS=Yellow, Unknown=Gray
- Sort by bytes/sec descending (most active first)
- Filter input: type to filter by process name or IP
- **Commit:** `feat(render): add network connection list view`

### Task 2.3.2: Network statistics panel
```
Files: crates/aether-render/src/tui/network.rs (modify)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.3.1
```
- Summary panel at top:
  - Total connections, active transfers
  - Bytes in/out per second (sparkline)
  - Protocol distribution (bar chart: TCP 60%, UDP 30%, DNS 10%)
- **Commit:** `feat(render): add network statistics panel`

**Checkpoint 2.3:** Network tab shows live connections with filtering and stats.

---

## Sprint 2.4: Vim Navigation & Input

### Task 2.4.1: Command mode
```
Files: crates/aether-render/src/tui/input.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 2.3.2
```
- Input modes: Normal, Command (`:` prefix), Search (`/` prefix)
- Normal: hjkl navigate, q quit, F1-F4 tabs, Enter select, Esc back
- Command: `:kill <pid>`, `:sort <column>`, `:theme <name>`, `:q` quit
- Search: `/text` filters current view, n/N next/prev match
- Status bar shows current mode and input buffer
- **Commit:** `feat(render): add Vim-style input modes`

### Task 2.4.2: Help overlay
```
Files: crates/aether-render/src/tui/help.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 2.4.1
```
- Press `?` → floating help overlay with keybinding list
- Styled as semi-transparent popup over current tab
- Press any key to dismiss
- **Commit:** `feat(render): add help overlay`

**CHECKPOINT MS2:** Full TUI with Overview (process table + sparklines), Network tab, Vim navigation, command mode. **Demoable product.**

---

# MILESTONE 3: 3D Engine

**Goal:** Software 3D rasterizer rendering process graph in World3D tab (F2).

**Duration:** 5 sprints, ~18 tasks

## Sprint 3.1: Math Foundation

### Task 3.1.1: Camera system
```
Files: crates/aether-render/src/engine/camera.rs, crates/aether-render/src/engine/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: MS2 complete
```
- `OrbitalCamera`:
  - `center: Vec3` — point camera orbits around
  - `distance: f32` — distance from center
  - `yaw: f32, pitch: f32` — angles in radians
  - `fov: f32` — field of view (default: 60°)
  - `near: f32, far: f32` — clipping planes
- Methods:
  - `view_matrix() -> Mat4` — lookAt matrix from glam
  - `projection_matrix(aspect: f32) -> Mat4` — perspective projection
  - `rotate(dyaw: f32, dpitch: f32)` — orbital rotation
  - `zoom(delta: f32)` — change distance
  - `position() -> Vec3` — current camera world position
  - `auto_center(points: &[Vec3])` — center on center-of-mass
- Tests: view_matrix at known position, rotation changes yaw/pitch, zoom clamps
- **Commit:** `feat(engine): implement orbital camera system`

### Task 3.1.2: Projection pipeline
```
Files: crates/aether-render/src/engine/projection.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.1.1
```
- `project_point(point: Vec3, view: &Mat4, proj: &Mat4, screen_w: u32, screen_h: u32) -> Option<ScreenPoint>`
- `ScreenPoint { x: f32, y: f32, depth: f32 }` — screen coordinates + depth for z-buffer
- Handles clipping: returns None if behind camera (z < near)
- Handles NDC → screen coordinate transform
- Tests: point at center projects to screen center, point behind camera returns None
- **Commit:** `feat(engine): add 3D to screen projection pipeline`

### Task 3.1.3: Braille coordinate system
```
Files: crates/aether-render/src/braille.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.1.2
```
- Braille mapping: each terminal cell = 2x4 dot grid = 8 bits
- `BrailleCanvas`:
  - `width: usize, height: usize` — in terminal cells
  - Internal buffer: `Vec<u8>` — pixel_w × pixel_h (cell_w*2 × cell_h*4)
  - `set_pixel(x: usize, y: usize)` — set a dot
  - `clear_pixel(x: usize, y: usize)`
  - `clear()` — reset all
  - `to_string() -> String` — convert buffer to Braille Unicode characters (U+2800 base)
  - `cell_at(cx: usize, cy: usize) -> char` — get Braille char for one cell
- Braille offset map (standard Braille dot numbering):
  ```
  [0,0]=0x01  [1,0]=0x08
  [0,1]=0x02  [1,1]=0x10
  [0,2]=0x04  [1,2]=0x20
  [0,3]=0x40  [1,3]=0x80
  ```
- Tests: set_pixel creates correct Braille char, all-dots-set = U+28FF, empty = U+2800
- **Commit:** `feat(render): implement Braille canvas with 2x4 subpixel mapping`

**Checkpoint 3.1:** Math foundation complete. Camera, projection, Braille canvas all tested.

---

## Sprint 3.2: Rasterizer

### Task 3.2.1: Z-buffer
```
Files: crates/aether-render/src/engine/rasterizer.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.1.3
```
- `ZBuffer`:
  - `width: usize, height: usize` — in Braille pixels (term_w*2 × term_h*4)
  - `buffer: Vec<f32>` — depth values, init to f32::INFINITY
  - `test_and_set(x: usize, y: usize, depth: f32) -> bool` — true if pixel should be drawn
  - `clear()`
- Tests: closer pixel overwrites farther, same depth rejected
- **Commit:** `feat(engine): add z-buffer for depth testing`

### Task 3.2.2: Line rasterizer (Bresenham)
```
Files: crates/aether-render/src/engine/rasterizer.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.2.1
```
- `draw_line(canvas: &mut BrailleCanvas, zbuf: &mut ZBuffer, p0: ScreenPoint, p1: ScreenPoint, color: Color)`
- Bresenham's line algorithm adapted for Braille subpixel space
- Depth interpolation along line for z-buffer test per pixel
- Tests: horizontal line, vertical line, diagonal line produce correct pixels
- **Commit:** `feat(engine): add Bresenham line rasterizer`

### Task 3.2.3: Circle rasterizer (for nodes)
```
Files: crates/aether-render/src/engine/rasterizer.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.2.2
```
- `draw_circle(canvas: &mut BrailleCanvas, zbuf: &mut ZBuffer, center: ScreenPoint, radius: f32, color: Color)`
- Midpoint circle algorithm in Braille space
- Filled circle variant: `draw_filled_circle` with scanline fill
- Tests: circle at known position has correct bounding pixels
- **Commit:** `feat(engine): add circle rasterizer for process nodes`

### Task 3.2.4: Shading (Phong-like)
```
Files: crates/aether-render/src/engine/shading.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.2.3
```
- `shade_point(normal: Vec3, light_dir: Vec3, base_color: Color) -> Color`
- Ambient: 0.3 * base_color
- Diffuse: 0.7 * max(dot(normal, light_dir), 0) * base_color
- Light direction: fixed at camera position (headlight)
- For sphere nodes: normal = normalize(pixel_pos_3d - center)
- Simplified: map brightness to character density (for ASCII mode too)
- Tests: facing light = full brightness, perpendicular = ambient only
- **Commit:** `feat(engine): add Phong-like shading for 3D nodes`

**Checkpoint 3.2:** Rasterizer draws lines and shaded circles in Braille. Z-buffer works.

---

## Sprint 3.3: Graph Layout

### Task 3.3.1: Force-directed layout (Fruchterman-Reingold 3D)
```
Files: crates/aether-render/src/engine/layout.rs
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.2.4
```
- `ForceLayout`:
  - `positions: HashMap<u32, Vec3>` — pid → 3D position
  - `velocities: HashMap<u32, Vec3>`
  - `temperature: f32` — cooling schedule
  - `k: f32` — optimal distance (sqrt(volume / node_count))
- `step(graph: &WorldGraph)` — one iteration:
  - Repulsive force between all node pairs: `k² / distance` along direction
  - Attractive force along edges: `distance² / k` toward neighbor
  - Apply velocity with damping
  - Reduce temperature
- `initial_placement(pids: &[u32])` — random sphere distribution
- Run 50 iterations on new graph, then 1 incremental step per frame
- Tests: two connected nodes converge, disconnected nodes repel
- **Commit:** `feat(engine): implement 3D force-directed graph layout`

### Task 3.3.2: Layout integration with WorldGraph
```
Files: crates/aether-render/src/engine/layout.rs (modify), crates/aether-core/src/graph.rs (modify)
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 3.3.1
```
- `WorldGraph::update_positions(layout: &ForceLayout)` — copy layout positions into ProcessNode.position_3d
- `ForceLayout::sync_with_graph(graph: &WorldGraph)` — add new nodes, remove dead ones
- New processes get position near parent (ppid) + random jitter
- **Commit:** `feat(engine): integrate force layout with world graph`

**Checkpoint 3.3:** Graph layout positions processes in 3D space. New processes appear near parents.

---

## Sprint 3.4: Scene Renderer

### Task 3.4.1: Scene render pipeline
```
Files: crates/aether-render/src/engine/scene.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 3.3.2
```
- `SceneRenderer`:
  - `camera: OrbitalCamera`
  - `layout: ForceLayout`
  - `canvas: BrailleCanvas`
  - `zbuffer: ZBuffer`
  - `color_buffer: Vec<Color>` — per-cell color (terminal cell resolution)
- `render(graph: &WorldGraph, area: Rect) -> Vec<(String, Color)>`:
  1. Clear canvas and z-buffer
  2. Update layout (1 step)
  3. For each edge: project endpoints, draw_line
  4. For each node: project center, draw_filled_circle with shading
  5. Convert canvas to Braille strings with colors
  6. Return lines for ratatui rendering
- **Commit:** `feat(engine): implement scene render pipeline`

### Task 3.4.2: World3D tab widget (F2)
```
Files: crates/aether-render/src/tui/world3d.rs
Agent: rust-engineer
Test: cargo run -p aether-terminal (manual: F2 shows 3D graph)
Depends: 3.4.1
```
- Custom ratatui widget wrapping SceneRenderer
- Mouse/keyboard camera controls:
  - Arrow keys / WASD: rotate camera
  - +/-: zoom in/out
  - Space: auto-rotate toggle
  - R: reset camera to default
  - C: center on selected node
- Node labels: show process name next to projected position (if space allows)
- **Commit:** `feat(render): add World3D tab with interactive camera`

### Task 3.4.3: Node interaction in 3D
```
Files: crates/aether-render/src/tui/world3d.rs (modify)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 3.4.2
```
- Click/Enter on node: highlight it (thicker circle, brighter color)
- Selected node: show info panel (like Overview detail panel)
- Tab between nodes with Tab key
- Nearest-node selection: find closest projected node to cursor
- **Commit:** `feat(render): add node selection and interaction in 3D view`

**Checkpoint 3.4:** 3D graph visible in terminal! Nodes = circles, edges = lines. Camera rotates. Nodes selectable.

---

## Sprint 3.5: Visual Effects

### Task 3.5.1: Node pulsation
```
Files: crates/aether-render/src/effects.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 3.4.3
```
- `PulseEffect`:
  - Nodes with high CPU load pulse (radius oscillates sinusoidally)
  - Amplitude proportional to cpu_percent (0% = no pulse, 100% = ±30% radius)
  - Frequency: 1Hz base, increases with load
- Apply in SceneRenderer before drawing each node
- **Commit:** `feat(render): add CPU load pulsation effect`

### Task 3.5.2: Color-coded health visualization
```
Files: crates/aether-render/src/engine/scene.rs (modify)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 3.5.1
```
- Node color = palette::color_for_hp(node.hp)
- Edge color = blend of source and dest node colors
- Critical nodes (HP < 20%): render double circle (bloom-like)
- Zombie processes: render as flickering (alternate visible/invisible)
- **Commit:** `feat(render): add health-based color visualization`

### Task 3.5.3: Edge data flow animation
```
Files: crates/aether-render/src/effects.rs (extend)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 3.5.2
```
- `FlowEffect`:
  - Bright dots travel along edges in direction of data flow
  - Speed proportional to bytes_per_sec
  - Use time-based offset to animate dots along Bresenham line
- Apply in SceneRenderer when drawing edges with active transfers
- **Commit:** `feat(render): add data flow animation on edges`

**CHECKPOINT MS3:** 3D graph with force-directed layout, pulsating nodes, flowing edges, camera controls. **The "wow" factor.** Fully demoable.

---

# MILESTONE 4: AI Bridge (MCP)

**Goal:** MCP server exposing system topology to AI agents. Arbiter Mode for human-in-the-loop.

**Duration:** 4 sprints, ~14 tasks

## Sprint 4.1: MCP Server Core

### Task 4.1.1: JSON-RPC server skeleton
```
Files: crates/aether-mcp/src/server.rs, crates/aether-mcp/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: MS3 complete
```
- `McpServer`:
  - `graph: Arc<RwLock<WorldGraph>>`
  - `action_tx: mpsc::Sender<AgentAction>` (to core for Arbiter)
- JSON-RPC 2.0 method dispatch:
  - Parse request → match method name → call handler → return result
  - Error handling: MethodNotFound, InvalidParams, InternalError
- Server info response: name="aether-terminal", version="0.1.0"
- Tests: parse valid request, dispatch to handler, error on unknown method
- **Commit:** `feat(mcp): implement JSON-RPC server skeleton`

### Task 4.1.2: Stdio transport
```
Files: crates/aether-mcp/src/transport/stdio.rs, crates/aether-mcp/src/transport/mod.rs
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.1.1
```
- `StdioTransport`:
  - Read JSON-RPC from stdin (line-delimited)
  - Write JSON-RPC to stdout
  - Async: tokio::io::stdin/stdout
- Activated with `aether --mcp-stdio`
- When active: TUI is disabled (stdin conflict), only MCP runs
- Tests: mock stdin with request, verify stdout response
- **Commit:** `feat(mcp): add stdio transport for Claude Desktop`

### Task 4.1.3: SSE transport
```
Files: crates/aether-mcp/src/transport/sse.rs
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.1.2
```
- `SseTransport`:
  - Axum HTTP server on configurable port (default 3000)
  - POST `/mcp` — JSON-RPC request → response
  - GET `/mcp/sse` — Server-Sent Events stream for push notifications
  - GET `/health` — health check
- Runs alongside TUI (different thread)
- Activated with `aether --mcp-sse <port>`
- Tests: HTTP request returns valid JSON-RPC response
- **Commit:** `feat(mcp): add SSE/HTTP transport for realtime AI connection`

**Checkpoint 4.1:** MCP server responds to JSON-RPC requests via stdio and HTTP.

---

## Sprint 4.2: MCP Tools

### Task 4.2.1: get_system_topology tool
```
Files: crates/aether-mcp/src/tools.rs
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.1.3
```
- Returns JSON: `{ processes: [...], connections: [...], summary: { total_processes, total_connections, avg_cpu, avg_memory } }`
- Reads from `Arc<RwLock<WorldGraph>>`
- Limits response: top 50 processes by CPU, all connections for those
- **Commit:** `feat(mcp): implement get_system_topology tool`

### Task 4.2.2: inspect_process tool
```
Files: crates/aether-mcp/src/tools.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.2.1
```
- Input: `{ pid: u32 }`
- Returns: full ProcessNode data + connections + HP/XP + recommendations
- Recommendations logic: if CPU > 80% suggest investigation, if HP < 30% suggest kill
- Error if pid not found
- **Commit:** `feat(mcp): implement inspect_process tool`

### Task 4.2.3: list_anomalies tool
```
Files: crates/aether-mcp/src/tools.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.2.2
```
- Returns processes where: HP < 50, state == Zombie, CPU > 90%, or memory growing > 5%/min
- Sorted by severity (HP ascending)
- Each anomaly includes: pid, name, reason, severity (critical/warning), suggested_action
- **Commit:** `feat(mcp): implement list_anomalies tool`

### Task 4.2.4: execute_action tool
```
Files: crates/aether-mcp/src/tools.rs (extend), crates/aether-mcp/src/arbiter.rs
Agent: rust-engineer
Test: cargo test -p aether-mcp
Depends: 4.2.3
```
- Input: `{ action: "kill"|"restart"|"nice", pid: u32 }`
- Does NOT execute immediately — pushes to Arbiter queue
- Returns: `{ status: "pending_approval", action_id: "uuid" }`
- `ArbiterQueue`:
  - `pending: Vec<PendingAction>` with id, action, pid, requested_at
  - `approve(id) -> Result<()>` — executes action
  - `deny(id) -> Result<()>` — removes from queue
- **Commit:** `feat(mcp): implement execute_action with Arbiter queue`

**Checkpoint 4.2:** All MCP tools work. AI can query topology, inspect processes, get anomalies, request actions.

---

## Sprint 4.3: Arbiter Mode UI

### Task 4.3.1: Arbiter tab (F4)
```
Files: crates/aether-render/src/tui/arbiter.rs
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 4.2.4
```
- Pending actions list: `[1] Claude: kill PID 1234 (nginx-worker) — 30s ago [Y/N/I]`
- Action history: approved/denied actions log
- Stats: total approved, denied, pending count
- Key bindings: Y approve, N deny, I inspect (jumps to 3D and centers on node)
- **Commit:** `feat(render): implement Arbiter tab with action approval UI`

### Task 4.3.2: MCP integration in main.rs
```
Files: crates/aether-terminal/src/main.rs (modify)
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --mcp-sse 3000 (manual: curl test)
Depends: 4.3.1
```
- Wire MCP server into main orchestration:
  - Share WorldGraph with MCP via Arc<RwLock>
  - Share ArbiterQueue between MCP and render
  - CLI flags: --mcp-stdio, --mcp-sse, --mcp-test
- When --mcp-stdio: run MCP only (no TUI)
- When --mcp-sse: run both TUI and MCP HTTP server
- **Commit:** `feat(terminal): integrate MCP server with main app`

**CHECKPOINT MS4:** AI agents can connect via MCP, query system, request actions. Arbiter Mode lets user approve/deny. **Full agentic integration.**

---

# MILESTONE 5: Gamification & Polish

**Goal:** RPG mechanics, achievements, visual polish. Release-ready.

**Duration:** 3 sprints, ~12 tasks

## Sprint 5.1: HP/XP System

### Task 5.1.1: HP calculation engine
```
Files: crates/aether-gamification/src/hp.rs, crates/aether-gamification/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-gamification
Depends: MS4 complete
```
- `HpEngine`:
  - `calculate_hp_delta(node: &ProcessNode, prev_snapshot: &ProcessNode, dt_secs: f32) -> f32`
  - Rules:
    - Memory growth > 5%/min: -1.0 * dt_secs
    - CPU > 90%: -2.0 * dt_secs
    - Zombie state: set to 0 immediately
    - Healthy (no anomalies): +0.5 * dt_secs (regeneration, cap at 100)
  - `apply_to_graph(graph: &mut WorldGraph, prev_graph: &WorldGraph, dt_secs: f32)`
- Tests: memory leak reduces HP, zombie = 0 HP, healthy regenerates
- **Commit:** `feat(gamification): implement HP calculation engine`

### Task 5.1.2: XP and ranking system
```
Files: crates/aether-gamification/src/xp.rs
Agent: rust-engineer
Test: cargo test -p aether-gamification
Depends: 5.1.1
```
- `XpTracker`:
  - `total_xp: u32`
  - `add_xp(amount: u32, reason: &str)` — emits GameEvent::XpEarned
  - `current_rank() -> Rank` — based on total_xp thresholds
  - `xp_to_next_rank() -> u32`
- `Rank` enum: Novice(0), Operator(100), Engineer(500), Architect(2000), AetherLord(10000)
- XP sources:
  - +1 per minute uptime (accumulated)
  - +50 per Arbiter-approved action
  - +10 per zombie kill
  - +5 per anomaly auto-resolved
- Tests: rank transitions at thresholds, XP accumulates correctly
- **Commit:** `feat(gamification): implement XP tracking and ranks`

### Task 5.1.3: Achievements system
```
Files: crates/aether-gamification/src/achievements.rs
Agent: rust-engineer
Test: cargo test -p aether-gamification
Depends: 5.1.2
```
- `AchievementTracker`:
  - `definitions: Vec<AchievementDef>` — id, name, description, condition
  - `unlocked: HashSet<String>` — achievement ids
  - `check(state: &GameState) -> Vec<AchievementDef>` — newly unlocked
- Achievements:
  - "first_blood": kills > 0
  - "uptime_champion": uptime > 24h without anomalies
  - "network_oracle": dpi_analyses > 100
  - "zombie_hunter": zombie_kills > 50
  - "ai_whisperer": arbiter_approvals > 100
- Tests: achievement unlocks at threshold, no double-unlock
- **Commit:** `feat(gamification): implement achievement tracking`

**Checkpoint 5.1:** HP/XP/Achievements calculated. Processes have health, user gains XP.

---

## Sprint 5.2: SQLite Persistence

### Task 5.2.1: Database schema and storage
```
Files: crates/aether-gamification/src/storage.rs
Agent: rust-engineer
Test: cargo test -p aether-gamification
Depends: 5.1.3
```
- `SqliteStorage` implementing `Storage` trait
- Tables:
  ```sql
  CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    total_xp INTEGER NOT NULL DEFAULT 0,
    rank TEXT NOT NULL DEFAULT 'Novice',
    uptime_secs INTEGER NOT NULL DEFAULT 0
  );
  CREATE TABLE achievements (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    unlocked_at TEXT NOT NULL,
    session_id INTEGER REFERENCES sessions(id)
  );
  CREATE TABLE action_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    action TEXT NOT NULL,
    pid INTEGER,
    source TEXT NOT NULL,  -- 'user' or 'ai_agent'
    approved BOOLEAN,
    session_id INTEGER REFERENCES sessions(id)
  );
  ```
- Methods: save_session, load_rankings, log_action, get_achievements
- DB path: `~/.aether-terminal/data.db`
- Tests: save and load round-trip, rankings sorted correctly
- **Commit:** `feat(gamification): add SQLite persistence`

### Task 5.2.2: Wire gamification into main
```
Files: crates/aether-terminal/src/main.rs (modify)
Agent: rust-engineer
Test: cargo run -p aether-terminal (manual: XP shown in status bar)
Depends: 5.2.1
```
- Spawn gamification task: receives events, updates HP/XP, checks achievements
- Status bar: shows rank, XP, XP to next rank
- Achievement popup: notification when achievement unlocks (timed overlay)
- Session tracking: save on exit, load previous rank on start
- **Commit:** `feat(terminal): wire gamification with persistence`

**Checkpoint 5.2:** XP shown in status bar, achievements persist across sessions, rank progresses.

---

## Sprint 5.3: Final Polish

### Task 5.3.1: Process death animation
```
Files: crates/aether-render/src/effects.rs (extend)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 5.2.2
```
- When process exits or is killed:
  - Node "dissolves": characters scatter outward over 500ms
  - Color fades from node color → dark
  - Edges connected to node fade simultaneously
- Implementation: mark dying nodes in WorldGraph, animate over N frames
- **Commit:** `feat(render): add process death dissolve animation`

### Task 5.3.2: Startup animation
```
Files: crates/aether-render/src/effects.rs (extend)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 5.3.1
```
- On launch: "matrix rain" fills screen for 1 second
- Title "AETHER TERMINAL" types out character by character
- Subtitle with version and rank
- Fades to main UI
- **Commit:** `feat(render): add startup animation sequence`

### Task 5.3.3: Theme system
```
Files: assets/themes/cyberpunk.toml, assets/themes/matrix.toml, crates/aether-render/src/palette.rs (modify)
Agent: rust-engineer
Test: cargo test -p aether-render
Depends: 5.3.2
```
- TOML theme files:
  ```toml
  [colors]
  background = "#050A0E"
  healthy = "#00F0FF"
  warning = "#FCEE09"
  critical = "#FF003C"
  data = "#FAFAFA"
  accent = "#BF00FF"
  ```
- `ThemeLoader` reads TOML, overrides Palette defaults
- `--theme matrix` flag loads green-on-black Matrix theme
- **Commit:** `feat(render): add TOML-based theme system`

### Task 5.3.4: Final CLI and README
```
Files: crates/aether-terminal/src/main.rs (finalize), README.md
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --help
Depends: 5.3.3
```
- Complete CLI with all flags from design doc
- README with:
  - GIF placeholder (record with asciinema later)
  - Installation instructions (cargo install)
  - Usage examples
  - Architecture diagram (text)
  - Feature list
  - License
- **Commit:** `docs: add comprehensive README and finalize CLI`

**CHECKPOINT MS5:** Core product complete. 3D visualization, MCP integration, gamification, themes, animations. **Demoable at 62%.**

---

---

# MILESTONE 6: eBPF Telemetry Engine

**Goal:** Kernel-level event capture via eBPF. Ring buffer with zero-copy reads. sysinfo becomes fallback.

**Duration:** 3 sprints, ~12 tasks

## Sprint 6.1: eBPF Foundation

### Task 6.1.1: eBPF crate setup and BPF program compilation
```
Files: crates/aether-ebpf/Cargo.toml, crates/aether-ebpf/src/lib.rs, bpf/process_monitor.bpf.c
Agent: rust-engineer
Test: cargo check -p aether-ebpf (Linux only)
Depends: MS1 complete
```
- Create `aether-ebpf` crate with `aya` dependency
- Write `process_monitor.bpf.c`: tracepoint/sched_process_fork, sched_process_exit
- BPF program outputs `ProcessFork` and `ProcessExit` structs to ring buffer
- Compile BPF with `aya-bpf` toolchain (include compiled bytes in binary)
- Feature-gated: entire crate behind `#[cfg(feature = "ebpf")]`
- **Commit:** `feat(ebpf): initialize eBPF crate with process monitor BPF program`

### Task 6.1.2: BPF loader and ring buffer reader
```
Files: crates/aether-ebpf/src/loader.rs, crates/aether-ebpf/src/ring_buffer.rs
Agent: rust-engineer
Test: sudo cargo test -p aether-ebpf (requires root)
Depends: 6.1.1
```
- `BpfLoader`:
  - `load_and_attach(program_bytes: &[u8]) -> Result<AttachedProgram>`
  - Handles BTF, map creation, program verification
  - Error handling: permission denied, kernel version too old, BTF missing
- `RingBufferReader`:
  - `new(map: &Map) -> Self`
  - `async fn poll(&mut self) -> Result<RawKernelEvent>` — zero-copy read
  - Converts raw bytes → typed Rust structs
- Tests: load BPF program, read at least one fork event
- **Commit:** `feat(ebpf): implement BPF loader and ring buffer reader`

### Task 6.1.3: Network and syscall BPF programs
```
Files: bpf/net_monitor.bpf.c, bpf/syscall_monitor.bpf.c, crates/aether-ebpf/src/probes.rs
Agent: rust-engineer
Test: sudo cargo test -p aether-ebpf
Depends: 6.1.2
```
- `net_monitor.bpf.c`: kprobe/tcp_connect, kprobe/tcp_close → TcpConnect, TcpClose events
- `syscall_monitor.bpf.c`: raw_tracepoint/sys_enter with configurable syscall filter
- `probes.rs`: ProbeManager that attaches/detaches multiple BPF programs
- Per-CPU ring buffers (256KB each) for throughput
- **Commit:** `feat(ebpf): add network and syscall BPF programs`

### Task 6.1.4: Event types and C struct mapping
```
Files: crates/aether-ebpf/src/events.rs
Agent: rust-engineer
Test: cargo test -p aether-ebpf
Depends: 6.1.3
```
- `RawKernelEvent` enum:
  - ProcessFork { parent_pid, child_pid, comm: [u8; 16], timestamp_ns }
  - ProcessExit { pid, exit_code, runtime_ns, timestamp_ns }
  - TcpConnect { pid, src: SockAddrIn, dst: SockAddrIn, timestamp_ns }
  - TcpClose { pid, src, dst, bytes_sent, bytes_recv, duration_ns }
  - SyscallEvent { pid, syscall_nr, latency_ns, timestamp_ns }
- `#[repr(C)]` structs matching BPF program output, `unsafe` byte casting with safety docs
- Tests: struct sizes match C layout, parse sample bytes
- **Commit:** `feat(ebpf): define kernel event types with C ABI compatibility`

**Checkpoint 6.1:** eBPF programs load and capture fork/exit, TCP, syscall events.

---

## Sprint 6.2: eBPF Integration

### Task 6.2.1: eBPF bridge in ingestion
```
Files: crates/aether-ingestion/src/ebpf_bridge.rs
Agent: rust-engineer
Test: cargo test -p aether-ingestion
Depends: 6.1.4
```
- `EbpfBridge`:
  - Receives `mpsc<RawKernelEvent>` from eBPF ring buffer reader
  - Converts to `SystemEvent`:
    - ProcessFork → SystemEvent::ProcessCreated
    - ProcessExit → SystemEvent::ProcessExited
    - TcpConnect → SystemEvent::ConnectionOpened
    - TcpClose → SystemEvent::ConnectionClosed
  - Gap-fills with sysinfo for data not available via eBPF (disk I/O, total RAM)
- Tests: convert sample eBPF events to SystemEvents
- **Commit:** `feat(ingestion): add eBPF bridge for kernel event translation`

### Task 6.2.2: Hybrid pipeline (eBPF + sysinfo)
```
Files: crates/aether-ingestion/src/pipeline.rs (modify)
Agent: rust-engineer
Test: cargo test -p aether-ingestion
Depends: 6.2.1
```
- IngestionPipeline now supports two modes:
  - `Mode::Sysinfo`: original polling (fallback)
  - `Mode::Ebpf`: eBPF as primary + sysinfo gap-filling
- Auto-detect: try eBPF → if fails, fall back to sysinfo with warning log
- Merge streams: eBPF events (real-time) + sysinfo (1Hz gap-fill) → unified mpsc
- **Commit:** `feat(ingestion): add hybrid eBPF/sysinfo pipeline with auto-fallback`

### Task 6.2.3: Wire eBPF into main.rs
```
Files: crates/aether-terminal/src/main.rs (modify)
Agent: rust-engineer
Test: sudo cargo run -p aether-terminal -- --ebpf
Depends: 6.2.2
```
- CLI flag: `--ebpf` enables eBPF mode
- Spawn eBPF ring buffer reader as tokio task
- Connect to ingestion pipeline via mpsc channel
- Graceful fallback if eBPF initialization fails
- **Commit:** `feat(terminal): integrate eBPF telemetry with --ebpf flag`

**CHECKPOINT MS6:** eBPF captures kernel events at 100K+ evt/sec. Seamless fallback to sysinfo. **Deep systems programming showcase.**

---

# MILESTONE 7: JIT-Compiled Rule DSL

**Goal:** Custom Aether DSL with lexer, parser, type checker, and Cranelift JIT compilation. Hot-reload.

**Duration:** 4 sprints, ~15 tasks

## Sprint 7.1: Lexer and Parser

### Task 7.1.1: Lexer (logos)
```
Files: crates/aether-script/Cargo.toml, crates/aether-script/src/lexer.rs, crates/aether-script/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: MS1 complete
```
- Token types:
  - Keywords: `rule`, `when`, `then`, `and`, `or`, `not`, `for`, `after`, `unless`, `severity`
  - Literals: integer, float, percentage (e.g. `90%`), duration (e.g. `30s`, `5m`), string
  - Identifiers: `process.cpu`, `process.state`, `system.load`
  - Operators: `>`, `<`, `>=`, `<=`, `==`, `!=`
  - Actions: `alert`, `action`, `log`, `kill`, `restart`
  - Punctuation: `{`, `}`, newline
- Use `logos` derive macro for zero-copy tokenization
- Span tracking for error messages (line:column)
- Tests: tokenize sample rule files, error on invalid tokens
- **Commit:** `feat(script): implement lexer with logos`

### Task 7.1.2: AST types
```
Files: crates/aether-script/src/ast.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.1.1
```
- AST nodes:
  ```rust
  struct RuleFile { rules: Vec<Rule> }
  struct Rule { name: String, condition: Condition, actions: Vec<Action>, span: Span }
  enum Condition {
      Comparison { left: Expr, op: CmpOp, right: Expr },
      Duration { condition: Box<Condition>, duration: Duration },
      And(Box<Condition>, Box<Condition>),
      Or(Box<Condition>, Box<Condition>),
      Not(Box<Condition>),
  }
  enum Expr {
      FieldAccess { object: String, field: String },
      Literal(Literal),
  }
  enum Literal { Int(i64), Float(f64), Percent(f64), Duration(Duration), String(String), Ident(String) }
  enum Action {
      Alert { message: Expr, severity: Severity },
      Kill { pid_expr: Option<Expr> },
      Log { message: Expr },
      ActionWithDelay { action: Box<Action>, delay: Duration, unless_condition: Option<Condition> },
  }
  ```
- Tests: construct and display AST nodes
- **Commit:** `feat(script): define AST types for Aether DSL`

### Task 7.1.3: Recursive descent parser
```
Files: crates/aether-script/src/parser.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.1.2
```
- Hand-written recursive descent (no parser generators):
  - `parse_file()` → Vec<Rule>
  - `parse_rule()` → Rule
  - `parse_condition()` → Condition (with precedence: `not` > `and` > `or`)
  - `parse_comparison()` → Condition::Comparison
  - `parse_action()` → Action
  - `parse_expr()` → Expr
- Error recovery: skip to next `rule` keyword on parse error
- Descriptive error messages with source location: `error[E001]: expected 'when', found 'then' at line 3, col 5`
- Tests: parse all example rule files from rules/ directory
- **Commit:** `feat(script): implement recursive descent parser`

**Checkpoint 7.1:** Lexer and parser produce correct AST from .aether files.

---

## Sprint 7.2: Type Checker

### Task 7.2.1: Type system
```
Files: crates/aether-script/src/types.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.1.3
```
- Types:
  - `Process` — has fields: cpu (Percent), mem_bytes (Int), mem_growth (Percent), state (ProcessState), name (String), parent (String), pid (Int)
  - `System` — has fields: load (Percent), total_mem (Int), process_count (Int)
  - `Percent`, `Duration`, `Int`, `Float`, `String`, `Bool`, `ProcessState`
- `TypeChecker`:
  - Resolve field accesses: `process.cpu` → Percent
  - Check comparison types: Percent can compare with Percent or Float
  - Check action parameters: `alert` requires String message, valid severity
  - Duration contexts: `for 30s` requires inner condition to be Bool
- Errors: type mismatch, unknown field, invalid comparison
- Tests: valid rules type-check, invalid rules produce descriptive errors
- **Commit:** `feat(script): implement type checker for Aether DSL`

**Checkpoint 7.2:** Rules are type-checked. Invalid rules produce clear error messages.

---

## Sprint 7.3: Cranelift JIT Codegen

### Task 7.3.1: Cranelift IR generation
```
Files: crates/aether-script/src/codegen.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.2.1
```
- `CodeGenerator`:
  - Input: Typed AST
  - Output: Cranelift `Function` per rule
  - WorldState is passed as a pointer to the JIT function
  - Field access compiled to struct offset loads
  - Comparisons compiled to icmp/fcmp instructions
  - `and`/`or` compiled to conditional branches
  - `for <duration>` compiled to counter check (external state)
- Function signature: `fn(state: *const WorldStateFFI) -> RuleResult`
- Tests: generate IR for simple rules, verify with Cranelift verifier
- **Commit:** `feat(script): implement Cranelift IR generation from typed AST`

### Task 7.3.2: JIT compilation and execution
```
Files: crates/aether-script/src/codegen.rs (extend), crates/aether-script/src/runtime.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.3.1
```
- `JitCompiler`:
  - Uses `cranelift_jit::JITModule` to compile IR → native code
  - Returns function pointer: `unsafe fn(*const WorldStateFFI) -> RuleResult`
- `CompiledRuleSet`:
  - `rules: Vec<CompiledRule>` (name + function pointer + metadata)
  - `fn evaluate(&self, state: &WorldState) -> Vec<RuleAction>`
  - Converts WorldState → FFI-safe WorldStateFFI, calls each rule, collects actions
- Safety: all `unsafe` blocks documented with invariants
- Tests: compile and execute rule, verify correct RuleAction output
- **Commit:** `feat(script): implement JIT compilation and rule execution`

### Task 7.3.3: Duration tracking (stateful rules)
```
Files: crates/aether-script/src/runtime.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.3.2
```
- `for <duration>` rules need state between evaluations:
  - `DurationTracker`: HashMap<(rule_id, pid), Instant>
  - On each evaluation: if condition true, start/update timer; if false, reset
  - Fire action only when timer exceeds duration
- `unless recovered` condition: reset timer if recovery condition met
- Tests: rule fires after duration, resets on recovery
- **Commit:** `feat(script): add duration tracking for stateful rules`

**Checkpoint 7.3:** Rules compile to native code and execute correctly with duration tracking.

---

## Sprint 7.4: Hot-Reload and Integration

### Task 7.4.1: File watcher and hot-reload
```
Files: crates/aether-script/src/hot_reload.rs
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.3.3
```
- `HotReloader`:
  - Watches `.aether` files via `notify` crate
  - On file change: lex → parse → type-check → compile → swap
  - Atomic swap: `Arc<ArcSwap<CompiledRuleSet>>`
  - Also triggered by SIGHUP signal
  - Error handling: on compilation failure, keep old rules, log error
- Tests: modify file → verify new rules loaded, compilation error → old rules preserved
- **Commit:** `feat(script): implement hot-reload with file watcher`

### Task 7.4.2: ScriptEngine tokio task
```
Files: crates/aether-script/src/lib.rs (extend)
Agent: rust-engineer
Test: cargo test -p aether-script
Depends: 7.4.1
```
- `ScriptEngine`:
  - Receives `broadcast<WorldState>`
  - On each tick: evaluate all compiled rules against current state
  - Send resulting `RuleAction`s via `mpsc` to Arbiter/Core
  - DurationTracker persists between ticks
- **Commit:** `feat(script): implement ScriptEngine as tokio task`

### Task 7.4.3: Wire script engine into main.rs + Rules tab (F5)
```
Files: crates/aether-terminal/src/main.rs (modify), crates/aether-render/src/tui/rules.rs
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --rules rules/default.aether
Depends: 7.4.2
```
- CLI flag: `--rules <PATH>` loads rule files
- Spawn ScriptEngine and HotReloader as tokio tasks
- New tab F5 (Rules): shows active rules, match counts, last fired time, errors
- Rule actions feed into ArbiterQueue (same as MCP actions)
- **Commit:** `feat(terminal): integrate JIT rule engine with Rules tab`

**CHECKPOINT MS7:** Custom DSL compiles to native code via Cranelift. Hot-reload works. Rules visible in UI. **Compiler engineering showcase.**

---

# MILESTONE 8: Predictive AI Engine

**Goal:** On-device ONNX inference for anomaly prediction. Feature extraction from WorldState stream.

**Duration:** 3 sprints, ~10 tasks

## Sprint 8.1: Feature Extraction

### Task 8.1.1: Feature vector definition and extraction
```
Files: crates/aether-predict/Cargo.toml, crates/aether-predict/src/features.rs, crates/aether-predict/src/lib.rs
Agent: rust-engineer
Test: cargo test -p aether-predict
Depends: MS1 complete
```
- `FeatureExtractor`:
  - Input: `&WorldState`
  - Output: `HashMap<u32, FeatureVector>` (per pid)
  - `FeatureVector`: `[f32; 9]` = [cpu_pct, mem_bytes_normalized, mem_delta, fd_count, thread_count, net_bytes_in, net_bytes_out, syscall_rate, io_wait_pct]
  - Normalization: min-max scaling based on running statistics
- Tests: extract features from sample WorldState, verify dimensions
- **Commit:** `feat(predict): implement per-process feature extraction`

### Task 8.1.2: Sliding window buffer
```
Files: crates/aether-predict/src/window.rs
Agent: rust-engineer
Test: cargo test -p aether-predict
Depends: 8.1.1
```
- `SlidingWindow`:
  - Per-process circular buffer of FeatureVectors
  - Capacity: 60 samples (= 5 minutes at 5s intervals)
  - `push(pid: u32, features: FeatureVector)`
  - `get_window(pid: u32) -> Option<&[FeatureVector]>` — returns contiguous slice
  - `to_tensor(pid: u32) -> Option<Tensor>` — converts to tract-compatible tensor [1, 60, 9]
  - Handles process creation (partial windows) and exit (cleanup)
- Tests: push 60 samples → full window, push 61 → oldest dropped
- **Commit:** `feat(predict): implement sliding window buffer for time-series`

**Checkpoint 8.1:** Features extracted from WorldState, buffered in sliding windows.

---

## Sprint 8.2: ONNX Inference

### Task 8.2.1: Model loading and inference
```
Files: crates/aether-predict/src/inference.rs
Agent: rust-engineer
Test: cargo test -p aether-predict
Depends: 8.1.2
```
- `OnnxModel`:
  - `load(path: &Path) -> Result<Self>` — loads ONNX model via tract
  - `predict(input: &Tensor) -> Result<Tensor>` — runs inference
  - Input shape: [1, 60, 9] (batch, timesteps, features)
  - Output: depends on model type
- `AnomalyDetector` (wraps autoencoder model):
  - Computes reconstruction error
  - Anomaly if error > threshold (configurable)
- `CpuForecaster` (wraps LSTM model):
  - Predicts CPU 60 seconds ahead
  - Returns predicted value + confidence interval
- Feature-gated: `#[cfg(feature = "predict")]`
- Tests: load dummy ONNX model, verify output shape
- **Commit:** `feat(predict): implement ONNX model loading and inference via tract`

### Task 8.2.2: Prediction types and confidence scoring
```
Files: crates/aether-predict/src/models.rs
Agent: rust-engineer
Test: cargo test -p aether-predict
Depends: 8.2.1
```
- `PredictedAnomaly`:
  ```rust
  struct PredictedAnomaly {
      pid: u32,
      process_name: String,
      anomaly_type: AnomalyType,
      confidence: f32,      // 0.0-1.0
      eta_seconds: u32,     // predicted time until event
      recommended_action: String,
      timestamp: Instant,
  }
  enum AnomalyType { OomRisk, CpuSpike, MemoryLeak, Deadlock, DiskExhaustion }
  ```
- Confidence calibration: map reconstruction error → probability
- Threshold configuration: per anomaly type, configurable via CLI or config file
- Tests: construct predictions, verify serialization
- **Commit:** `feat(predict): define prediction types and confidence scoring`

**Checkpoint 8.2:** Models load and run inference. Predictions typed and scored.

---

## Sprint 8.3: PredictEngine Integration

### Task 8.3.1: PredictEngine tokio task
```
Files: crates/aether-predict/src/engine.rs
Agent: rust-engineer
Test: cargo test -p aether-predict
Depends: 8.2.2
```
- `PredictEngine`:
  - Receives `broadcast<WorldState>` every tick
  - Every 5 seconds: extract features → update windows → run inference on top-N processes
  - Top-N selection: processes with highest recent variance (avoid wasting inference on idle)
  - Sends `PredictedAnomaly` via `mpsc` to core and render
  - Configurable: inference interval, top-N count, confidence threshold
- Tests: engine processes sample world states, produces predictions
- **Commit:** `feat(predict): implement PredictEngine as async task`

### Task 8.3.2: Prediction visualization in render
```
Files: crates/aether-render/src/tui/overview.rs (modify), crates/aether-render/src/engine/scene.rs (modify)
Agent: rust-engineer
Test: cargo check -p aether-render
Depends: 8.3.1
```
- Overview tab: new "Predictions" section showing upcoming anomalies
  - Sorted by eta_seconds ascending (most imminent first)
  - Color: Neon Orange (#FF6600) for predicted, distinct from actual anomalies
- 3D view: predicted-anomaly nodes get pulsing orange outline (preview of danger)
- Process detail panel: shows prediction if available (type, confidence, ETA)
- **Commit:** `feat(render): add prediction visualization in Overview and 3D tabs`

### Task 8.3.3: Wire predictions into main.rs + MCP tool
```
Files: crates/aether-terminal/src/main.rs (modify), crates/aether-mcp/src/tools.rs (extend)
Agent: rust-engineer
Test: cargo run -p aether-terminal -- --predict
Depends: 8.3.2
```
- CLI flag: `--predict` enables predictive AI
- Spawn PredictEngine as tokio task
- New MCP tool: `predict_anomalies` — returns current predictions as JSON
- Predictions feed into gamification: +20 XP per accurate prediction (confirmed within ETA)
- **Commit:** `feat(terminal): integrate predictive AI with --predict flag and MCP tool`

**CHECKPOINT MS8 / FINAL:** On-device ML predicts anomalies before they happen. Visualized in TUI and available via MCP. **Full 10/10 system complete.**

---

## Sprint Summary

| Sprint | Tasks | Focus | Checkpoint |
|--------|-------|-------|------------|
| 1.1 | 4 | Workspace + core types | Core types tested |
| 1.2 | 3 | Ingestion pipeline | Live data prints |
| 2.1 | 4 | TUI framework | Tabs + status bar |
| 2.2 | 3 | Overview tab | Process table + sparklines |
| 2.3 | 2 | Network tab | Connection list |
| 2.4 | 2 | Vim navigation | Command mode + help |
| 3.1 | 3 | 3D math | Camera + projection + Braille |
| 3.2 | 4 | Rasterizer | Z-buffer + lines + circles + shading |
| 3.3 | 2 | Graph layout | Force-directed 3D |
| 3.4 | 3 | Scene renderer | 3D in terminal! |
| 3.5 | 3 | Visual effects | Pulse + health colors + flow |
| 4.1 | 3 | MCP server | JSON-RPC + transports |
| 4.2 | 4 | MCP tools | Topology + inspect + actions |
| 4.3 | 2 | Arbiter UI | Approval tab + integration |
| 5.1 | 3 | HP/XP/Achievements | RPG mechanics |
| 5.2 | 2 | SQLite persistence | Session tracking |
| 5.3 | 4 | Polish | Animations + themes + README |
| 6.1 | 4 | eBPF foundation | BPF programs + loader + events |
| 6.2 | 3 | eBPF integration | Bridge + hybrid pipeline + main |
| 7.1 | 3 | Lexer + parser | Tokenization + AST |
| 7.2 | 1 | Type checker | Type safety for DSL |
| 7.3 | 3 | Cranelift JIT | IR gen + compilation + duration |
| 7.4 | 3 | Hot-reload + integration | File watcher + engine + Rules tab |
| 8.1 | 2 | Feature extraction | Features + sliding window |
| 8.2 | 2 | ONNX inference | Model loading + predictions |
| 8.3 | 3 | Predict integration | Engine + visualization + MCP |
| **Total** | **75** | | |

## Orchestrator Sprint Files Needed

```
tasks/ms1-workspace-setup.yaml     (Sprint 1.1: 4 tasks)
tasks/ms1-ingestion.yaml           (Sprint 1.2: 3 tasks)
tasks/ms2-tui-framework.yaml       (Sprint 2.1: 4 tasks)
tasks/ms2-overview-tab.yaml        (Sprint 2.2: 3 tasks)
tasks/ms2-network-tab.yaml         (Sprint 2.3: 2 tasks)
tasks/ms2-vim-navigation.yaml      (Sprint 2.4: 2 tasks)
tasks/ms3-3d-math.yaml             (Sprint 3.1: 3 tasks)
tasks/ms3-rasterizer.yaml          (Sprint 3.2: 4 tasks)
tasks/ms3-graph-layout.yaml        (Sprint 3.3: 2 tasks)
tasks/ms3-scene-renderer.yaml      (Sprint 3.4: 3 tasks)
tasks/ms3-visual-effects.yaml      (Sprint 3.5: 3 tasks)
tasks/ms4-mcp-server.yaml          (Sprint 4.1: 3 tasks)
tasks/ms4-mcp-tools.yaml           (Sprint 4.2: 4 tasks)
tasks/ms4-arbiter.yaml             (Sprint 4.3: 2 tasks)
tasks/ms5-hp-xp.yaml               (Sprint 5.1: 3 tasks)
tasks/ms5-persistence.yaml         (Sprint 5.2: 2 tasks)
tasks/ms5-polish.yaml              (Sprint 5.3: 4 tasks)
tasks/ms6-ebpf-foundation.yaml    (Sprint 6.1: 4 tasks)
tasks/ms6-ebpf-integration.yaml   (Sprint 6.2: 3 tasks)
tasks/ms7-lexer-parser.yaml        (Sprint 7.1: 3 tasks)
tasks/ms7-type-checker.yaml        (Sprint 7.2: 1 task)
tasks/ms7-cranelift-jit.yaml       (Sprint 7.3: 3 tasks)
tasks/ms7-hot-reload.yaml          (Sprint 7.4: 3 tasks)
tasks/ms8-features.yaml            (Sprint 8.1: 2 tasks)
tasks/ms8-inference.yaml           (Sprint 8.2: 2 tasks)
tasks/ms8-predict-integration.yaml (Sprint 8.3: 3 tasks)
```

# aether-render

## Purpose
TUI framework + 3D software rasterizer. This is the largest and most complex crate. Two layers: TUI (ratatui widgets) and Engine (3D math + rendering).

## Module Structure
```
src/
├── lib.rs
├── palette.rs        — color constants, theme loading
├── braille.rs        — BrailleCanvas (2x4 subpixel mapping)
├── effects.rs        — visual effects (pulse, dissolve, flow)
├── tui/
│   ├── mod.rs
│   ├── app.rs        — App struct, event loop, tab routing
│   ├── tabs.rs       — tab bar widget
│   ├── overview.rs   — F1: process table + sparklines
│   ├── world3d.rs    — F2: 3D viewport widget
│   ├── network.rs    — F3: connection list
│   ├── arbiter.rs    — F4: AI action approval
│   ├── help.rs       — ? overlay
│   └── input.rs      — Vim-style input modes
└── engine/
    ├── mod.rs
    ├── camera.rs      — OrbitalCamera
    ├── projection.rs  — 3D→screen projection
    ├── rasterizer.rs  — z-buffer, Bresenham line, circle fill
    ├── shading.rs     — Phong ambient+diffuse
    ├── layout.rs      — ForceLayout (Fruchterman-Reingold 3D)
    └── scene.rs       — SceneRenderer (orchestrates rendering)
```

## Rules
- Engine code must be PURE MATH — no ratatui dependency in engine/
- TUI code uses ratatui widgets and calls engine for 3D content
- BrailleCanvas is the bridge: engine writes to it, TUI reads from it
- ALL colors come from palette.rs — never hardcode Color::Rgb in widgets
- 60fps target: SceneRenderer.render() must complete in <16ms for 100 nodes
- Camera state is mutable, but WorldGraph is read-only (Arc<RwLock>)
- Force layout: 50 initial iterations, then 1 incremental step per frame
- Z-buffer resolution: term_width*2 × term_height*4 (Braille subpixels)

## Performance
- Use dirty flags: only re-render 3D when graph or camera changes
- Reuse BrailleCanvas buffer (clear, don't reallocate)
- Skip offscreen nodes (frustum culling)
- Limit label rendering to top N nearest nodes

## Testing
```bash
cargo test -p aether-render
```
Engine tests: camera matrices, projection, Braille encoding, layout convergence.
TUI tests: minimal (mostly visual, tested manually).

## Key Dependencies
- aether-core (path dependency)
- ratatui, crossterm
- glam (Vec3, Mat4, Quat)
- tachyonfx (optional, for advanced effects)

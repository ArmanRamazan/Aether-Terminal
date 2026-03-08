# ADR-002: Custom software rasterizer over existing TUI 3D libraries

**Status:** Accepted
**Date:** 2026-03-08

## Context
3D visualization in terminal can be done with existing libraries (ratatui-3d, if exists) or custom software rasterizer.

## Decision
Build custom software rasterizer using glam for math and Braille characters for output.

## Rationale
- No mature terminal 3D library exists in Rust ecosystem
- Custom rasterizer is the primary portfolio showcase (demonstrates deep Rust + graphics knowledge)
- Braille characters provide 2x4 subpixel resolution (8 dots per cell)
- Full control over rendering pipeline enables future effects (bloom, trails, dissolve)

## Consequences
- Significant implementation effort (~18 tasks across 5 sprints)
- Must handle edge cases: terminal resize, minimum size, fallback modes
- Performance critical: must render 100+ nodes at 60fps in Braille
- Three fallback modes needed: Braille → HalfBlock → ASCII

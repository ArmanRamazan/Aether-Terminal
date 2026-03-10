//! Visual effects for 3D node rendering.
//!
//! CPU-load-driven pulsation makes active processes "breathe" —
//! their radius oscillates sinusoidally proportional to load.
//! Death dissolve fades removed processes out over 500ms.
//! Startup animation plays a cinematic boot sequence on launch.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use glam::Vec3;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::palette::Palette;

/// Sinusoidal pulsation effect driven by CPU load.
///
/// Nodes with higher CPU usage pulse faster and with greater amplitude,
/// creating a "breathing" visual cue for system activity.
pub struct PulseEffect {
    /// Accumulated time in seconds.
    time: f32,
}

impl PulseEffect {
    /// Create a new pulse effect at time zero.
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    /// Advance the effect clock by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Current accumulated time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Compute pulsed radius for a node given its base radius and CPU load.
    ///
    /// - Amplitude: 0–30% of base radius, proportional to `cpu_percent`.
    /// - Frequency: 1–3 Hz, proportional to `cpu_percent`.
    /// - At 0% CPU the radius is unchanged.
    pub fn pulse_radius(&self, base_radius: f32, cpu_percent: f32) -> f32 {
        let load = cpu_percent / 100.0;
        let amplitude = load * 0.3 * base_radius;
        let frequency = 1.0 + load * 2.0;
        base_radius + amplitude * (self.time * frequency * std::f32::consts::TAU).sin()
    }
}

impl Default for PulseEffect {
    fn default() -> Self {
        Self::new()
    }
}

/// Animated flow dots traveling along edges to visualize data transfer.
///
/// Speed is proportional to `bytes_per_sec` — idle edges stay still,
/// busy edges show fast-moving dots.
pub struct FlowEffect {
    /// Accumulated time in seconds.
    time: f32,
}

impl FlowEffect {
    /// Create a new flow effect at time zero.
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    /// Advance the effect clock by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }

    /// Position of a flow dot along an edge as 0.0..1.0.
    ///
    /// Speed scales linearly with throughput, capped at 10 MB/s.
    pub fn flow_dot_position(&self, bytes_per_sec: u64) -> f32 {
        let cap = 10_000_000_u64;
        let speed = 0.3 + bytes_per_sec.min(cap) as f32 / cap as f32;
        (self.time * speed).fract()
    }
}

impl Default for FlowEffect {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-node state for the death dissolve animation.
pub(crate) struct DeathState {
    start_time: Instant,
    /// World-space position at the moment of death.
    pub(crate) original_position: Vec3,
    duration: Duration,
}

/// Dissolve animation for removed processes.
///
/// When a PID disappears from the graph, it is registered here and rendered
/// with scattered pixel offsets and color fade for `duration` before being
/// fully removed.
pub(crate) struct DeathEffect {
    dying_nodes: HashMap<u32, DeathState>,
}

impl DeathEffect {
    /// Create an empty death effect with no dying nodes.
    pub(crate) fn new() -> Self {
        Self {
            dying_nodes: HashMap::new(),
        }
    }

    /// Register a process as dying at the given world position.
    pub(crate) fn mark_dying(&mut self, pid: u32, position: Vec3) {
        self.dying_nodes.entry(pid).or_insert(DeathState {
            start_time: Instant::now(),
            original_position: position,
            duration: Duration::from_millis(500),
        });
    }

    /// Remove completed animations. Call once per frame.
    pub(crate) fn update(&mut self) {
        let now = Instant::now();
        self.dying_nodes
            .retain(|_, state| now.duration_since(state.start_time) < state.duration);
    }

    /// Animation progress for a dying node: `0.0` (just died) to `1.0` (fully gone).
    ///
    /// Returns `None` if the pid is not dying.
    pub(crate) fn is_dying(&self, pid: u32) -> Option<f32> {
        let state = self.dying_nodes.get(&pid)?;
        let elapsed = state.start_time.elapsed().as_secs_f32();
        let total = state.duration.as_secs_f32();
        Some((elapsed / total).min(1.0))
    }

    /// Iterate over all currently dying nodes.
    pub(crate) fn dying_pids(&self) -> impl Iterator<Item = (u32, &DeathState)> {
        self.dying_nodes.iter().map(|(&pid, state)| (pid, state))
    }
}

impl Default for DeathEffect {
    fn default() -> Self {
        Self::new()
    }
}

/// Current phase of the startup boot sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupPhase {
    /// Green characters rain down in random columns.
    MatrixRain,
    /// "AETHER TERMINAL" types out character by character.
    TitleType,
    /// Main UI fades in over the animation.
    FadeIn,
    /// Animation complete — normal rendering.
    Done,
}

/// Cinematic startup animation sequence.
///
/// Plays a 3-second boot sequence: matrix rain → title typewriter → fade-in.
/// Call [`update`](Self::update) each frame and [`render`](Self::render) while
/// [`is_done`](Self::is_done) returns false.
pub struct StartupAnimation {
    phase: StartupPhase,
    elapsed: f32,
    /// Deterministic seed per column for reproducible rain patterns.
    column_seeds: Vec<u32>,
}

impl StartupAnimation {
    /// Phase transition times in seconds.
    const RAIN_END: f32 = 1.0;
    const TYPE_END: f32 = 2.5;
    const FADE_END: f32 = 3.0;

    /// Title text displayed during the typewriter phase.
    const TITLE: &'static str = "AETHER TERMINAL";

    /// Create a new startup animation beginning at the MatrixRain phase.
    pub fn new() -> Self {
        Self {
            phase: StartupPhase::MatrixRain,
            elapsed: 0.0,
            column_seeds: Vec::new(),
        }
    }

    /// Whether the animation has finished.
    pub fn is_done(&self) -> bool {
        self.phase == StartupPhase::Done
    }

    /// Current animation phase.
    pub fn phase(&self) -> StartupPhase {
        self.phase
    }

    /// Advance the animation by `dt` seconds. Returns `true` when complete.
    pub fn update(&mut self, dt: f32) -> bool {
        if self.phase == StartupPhase::Done {
            return true;
        }
        self.elapsed += dt;
        self.phase = match self.elapsed {
            t if t < Self::RAIN_END => StartupPhase::MatrixRain,
            t if t < Self::TYPE_END => StartupPhase::TitleType,
            t if t < Self::FADE_END => StartupPhase::FadeIn,
            _ => StartupPhase::Done,
        };
        self.phase == StartupPhase::Done
    }

    /// Render the current animation frame into the buffer.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        match self.phase {
            StartupPhase::MatrixRain => self.render_rain(area, buf),
            StartupPhase::TitleType => self.render_title(area, buf),
            StartupPhase::FadeIn => self.render_fade(area, buf),
            StartupPhase::Done => {}
        }
    }

    /// Fill the area with falling green characters.
    fn render_rain(&mut self, area: Rect, buf: &mut Buffer) {
        let width = area.width as usize;
        let height = area.height as usize;

        // Lazily initialize column seeds.
        if self.column_seeds.len() != width {
            self.column_seeds = (0..width).map(|i| (i as u32).wrapping_mul(2654435761)).collect();
        }

        // Progress within the rain phase (0.0..1.0).
        let progress = (self.elapsed / Self::RAIN_END).min(1.0);
        // Brightness fades toward end of rain phase.
        let brightness = 1.0 - progress * 0.3;

        let green = scale_color(Palette::HEALTHY, brightness);
        let dim_green = scale_color(Palette::HEALTHY, brightness * 0.4);

        fill_bg(area, buf);

        for col in 0..width {
            let seed = self.column_seeds[col];
            // Each column has a "head" position that falls at varying speed.
            let speed = 0.5 + (seed % 100) as f32 / 100.0;
            let head = (self.elapsed * speed * height as f32) as usize;

            for row in 0..height {
                if row > head {
                    continue;
                }
                let cell_hash = simple_hash(seed, row as u32);
                let ch = rain_char(cell_hash);
                let dist_from_head = head.saturating_sub(row);
                let style = if dist_from_head < 2 {
                    Style::default().fg(green).bg(Palette::BG)
                } else {
                    Style::default().fg(dim_green).bg(Palette::BG)
                };
                let x = area.x + col as u16;
                let y = area.y + row as u16;
                if x < area.right() && y < area.bottom() {
                    buf[(x, y)].set_char(ch).set_style(style);
                }
            }
        }
    }

    /// Render the typewriter title centered in the area.
    fn render_title(&self, area: Rect, buf: &mut Buffer) {
        let phase_elapsed = self.elapsed - Self::RAIN_END;
        let phase_duration = Self::TYPE_END - Self::RAIN_END;
        let progress = (phase_elapsed / phase_duration).min(1.0);

        fill_bg(area, buf);

        let chars_to_show = ((Self::TITLE.len() as f32 * progress).ceil() as usize)
            .min(Self::TITLE.len());
        let visible = &Self::TITLE[..chars_to_show];

        let cx = area.x + area.width.saturating_sub(Self::TITLE.len() as u16) / 2;
        let cy = area.y + area.height / 2;

        if cy >= area.bottom() {
            return;
        }

        for (i, ch) in visible.chars().enumerate() {
            let x = cx + i as u16;
            if x >= area.right() {
                break;
            }
            let is_cursor = i + 1 == chars_to_show && chars_to_show < Self::TITLE.len();
            let style = if is_cursor {
                Style::default().fg(Palette::BG).bg(Palette::HEALTHY)
            } else {
                Style::default().fg(Palette::HEALTHY).bg(Palette::BG)
            };
            buf[(x, cy)].set_char(ch).set_style(style);
        }

        // Subtitle line
        let sub = "v0.1.0 — initializing...";
        if cy + 2 < area.bottom() && area.width as usize >= sub.len() {
            let sx = area.x + area.width.saturating_sub(sub.len() as u16) / 2;
            let sub_alpha = (phase_elapsed - 0.5).max(0.0) / (phase_duration - 0.5);
            let sub_color = scale_color(Palette::NEON_BLUE, sub_alpha.min(1.0));
            for (i, ch) in sub.chars().enumerate() {
                let x = sx + i as u16;
                if x >= area.right() {
                    break;
                }
                buf[(x, cy + 2)]
                    .set_char(ch)
                    .set_style(Style::default().fg(sub_color).bg(Palette::BG));
            }
        }
    }

    /// Render progressively transparent overlay (simulated with dimming).
    fn render_fade(&self, area: Rect, buf: &mut Buffer) {
        let phase_elapsed = self.elapsed - Self::TYPE_END;
        let phase_duration = Self::FADE_END - Self::TYPE_END;
        let progress = (phase_elapsed / phase_duration).min(1.0);

        // Dim overlay: fills with BG at decreasing opacity (simulated by
        // leaving more cells untouched as progress increases).
        let threshold = 1.0 - progress;
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let cell_val = simple_hash(x as u32, y as u32) % 100;
                if (cell_val as f32 / 100.0) < threshold {
                    buf[(x, y)]
                        .set_char(' ')
                        .set_style(Style::default().bg(Palette::BG));
                }
            }
        }
    }
}

impl Default for StartupAnimation {
    fn default() -> Self {
        Self::new()
    }
}

/// Fill an area with the background color.
fn fill_bg(area: Rect, buf: &mut Buffer) {
    let bg_style = Style::default().bg(Palette::BG);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buf[(x, y)].set_char(' ').set_style(bg_style);
        }
    }
}

/// Simple deterministic hash for reproducible pseudo-random patterns.
fn simple_hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(2654435761).wrapping_add(b.wrapping_mul(340573321));
    h ^= h >> 16;
    h = h.wrapping_mul(2246822519);
    h ^= h >> 13;
    h
}

/// Pick a character for the matrix rain from a hash value.
fn rain_char(hash: u32) -> char {
    const CHARS: &[u8] = b"0123456789ABCDEFabcdef@#$%&*+=<>{}[]|/\\~";
    CHARS[(hash as usize) % CHARS.len()] as char
}

/// Scale an RGB color's brightness by a factor (0.0 = black, 1.0 = unchanged).
fn scale_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let f = factor.clamp(0.0, 1.0);
            Color::Rgb(
                (r as f32 * f) as u8,
                (g as f32 * f) as u8,
                (b as f32 * f) as u8,
            )
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_radius_zero_cpu_returns_base() {
        let effect = PulseEffect::new();
        let result = effect.pulse_radius(3.0, 0.0);
        assert!(
            (result - 3.0).abs() < f32::EPSILON,
            "0% CPU should return base radius, got {result}"
        );
    }

    #[test]
    fn test_pulse_radius_stays_within_bounds() {
        let mut effect = PulseEffect::new();
        let base = 4.0;
        let cpu = 100.0;
        // Sample many time steps to find min/max.
        let mut min_r = f32::MAX;
        let mut max_r = f32::MIN;
        for _ in 0..1000 {
            effect.update(0.001);
            let r = effect.pulse_radius(base, cpu);
            min_r = min_r.min(r);
            max_r = max_r.max(r);
        }
        let amplitude = 0.3 * base; // 100% CPU → 30% amplitude
        assert!(
            min_r >= base - amplitude - 0.01,
            "min radius {min_r} below expected {}",
            base - amplitude
        );
        assert!(
            max_r <= base + amplitude + 0.01,
            "max radius {max_r} above expected {}",
            base + amplitude
        );
    }

    #[test]
    fn test_update_advances_time() {
        let mut effect = PulseEffect::new();
        effect.update(0.5);
        effect.update(0.3);
        assert!(
            (effect.time - 0.8).abs() < f32::EPSILON,
            "time should accumulate to 0.8, got {}",
            effect.time
        );
    }

    #[test]
    fn test_pulse_frequency_increases_with_cpu() {
        // At higher CPU, the period should be shorter (higher frequency).
        // Compare half-period crossing for 50% vs 100% CPU.
        let base = 3.0;

        let find_first_peak_time = |cpu: f32| -> f32 {
            let mut effect = PulseEffect::new();
            let mut prev = effect.pulse_radius(base, cpu);
            for i in 1..10000 {
                effect.update(0.0001);
                let r = effect.pulse_radius(base, cpu);
                if r < prev {
                    return i as f32 * 0.0001;
                }
                prev = r;
            }
            f32::MAX
        };

        let peak_50 = find_first_peak_time(50.0);
        let peak_100 = find_first_peak_time(100.0);
        assert!(
            peak_100 < peak_50,
            "100% CPU peak at {peak_100}s should be earlier than 50% at {peak_50}s"
        );
    }

    #[test]
    fn test_flow_dot_position_in_unit_range() {
        let mut effect = FlowEffect::new();
        effect.update(1.5);
        let pos = effect.flow_dot_position(5_000_000);
        assert!(
            (0.0..1.0).contains(&pos),
            "position {pos} should be in 0.0..1.0"
        );
    }

    #[test]
    fn test_flow_dot_faster_with_more_traffic() {
        let mut low = FlowEffect::new();
        let mut high = FlowEffect::new();
        low.update(0.5);
        high.update(0.5);
        let pos_low = low.flow_dot_position(1_000);
        let pos_high = high.flow_dot_position(10_000_000);
        // Higher traffic → higher speed → position advances more per unit time.
        assert!(
            pos_high > pos_low || (pos_high - pos_low).abs() < 0.01,
            "high traffic pos {pos_high} should >= low traffic pos {pos_low}"
        );
    }

    #[test]
    fn test_flow_dot_caps_at_max_bytes() {
        let mut effect = FlowEffect::new();
        effect.update(2.0);
        let at_cap = effect.flow_dot_position(10_000_000);
        let above_cap = effect.flow_dot_position(100_000_000);
        assert!(
            (at_cap - above_cap).abs() < f32::EPSILON,
            "above-cap {above_cap} should equal at-cap {at_cap}"
        );
    }

    #[test]
    fn test_death_mark_dying_returns_progress() {
        let mut effect = DeathEffect::new();
        effect.mark_dying(42, Vec3::ZERO);
        let progress = effect.is_dying(42);
        assert!(progress.is_some(), "marked pid should be dying");
        let p = progress.expect("checked above");
        assert!(p >= 0.0 && p <= 1.0, "progress {p} should be in 0.0..1.0");
    }

    #[test]
    fn test_death_unknown_pid_returns_none() {
        let effect = DeathEffect::new();
        assert!(
            effect.is_dying(999).is_none(),
            "unknown pid should not be dying"
        );
    }

    #[test]
    fn test_death_mark_dying_idempotent() {
        let mut effect = DeathEffect::new();
        effect.mark_dying(1, Vec3::new(1.0, 2.0, 3.0));
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Second call should not reset the timer.
        effect.mark_dying(1, Vec3::new(9.0, 9.0, 9.0));
        let state = &effect.dying_nodes[&1];
        assert!(
            (state.original_position - Vec3::new(1.0, 2.0, 3.0)).length() < f32::EPSILON,
            "second mark_dying should not overwrite original position"
        );
    }

    #[test]
    fn test_death_update_removes_expired() {
        let mut effect = DeathEffect::new();
        effect.dying_nodes.insert(
            1,
            DeathState {
                start_time: Instant::now() - Duration::from_secs(1),
                original_position: Vec3::ZERO,
                duration: Duration::from_millis(500),
            },
        );
        effect.update();
        assert!(
            effect.is_dying(1).is_none(),
            "expired animation should be removed"
        );
    }

    #[test]
    fn test_death_dying_pids_iterates_all() {
        let mut effect = DeathEffect::new();
        effect.mark_dying(1, Vec3::X);
        effect.mark_dying(2, Vec3::Y);
        let pids: Vec<u32> = effect.dying_pids().map(|(pid, _)| pid).collect();
        assert_eq!(pids.len(), 2, "should have 2 dying pids");
        assert!(pids.contains(&1));
        assert!(pids.contains(&2));
    }

    #[test]
    fn test_startup_initial_phase_is_matrix_rain() {
        let anim = StartupAnimation::new();
        assert_eq!(anim.phase(), StartupPhase::MatrixRain);
        assert!(!anim.is_done());
    }

    #[test]
    fn test_startup_transitions_through_phases() {
        let mut anim = StartupAnimation::new();

        // Still in MatrixRain at 0.5s
        anim.update(0.5);
        assert_eq!(anim.phase(), StartupPhase::MatrixRain);

        // TitleType at 1.0s
        anim.update(0.5);
        assert_eq!(anim.phase(), StartupPhase::TitleType);

        // Still TitleType at 2.0s
        anim.update(1.0);
        assert_eq!(anim.phase(), StartupPhase::TitleType);

        // FadeIn at 2.5s
        anim.update(0.5);
        assert_eq!(anim.phase(), StartupPhase::FadeIn);

        // Done at 3.0s
        let done = anim.update(0.5);
        assert!(done, "update should return true when done");
        assert_eq!(anim.phase(), StartupPhase::Done);
        assert!(anim.is_done());
    }

    #[test]
    fn test_startup_update_returns_true_when_done() {
        let mut anim = StartupAnimation::new();
        assert!(!anim.update(0.5));
        assert!(!anim.update(0.5));
        assert!(!anim.update(1.0));
        assert!(!anim.update(0.5));
        assert!(anim.update(0.5));
        // Subsequent calls remain true.
        assert!(anim.update(0.1));
    }

    #[test]
    fn test_startup_render_does_not_panic() {
        let mut anim = StartupAnimation::new();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        // Render each phase.
        anim.render(area, &mut buf);

        anim.update(1.0);
        anim.render(area, &mut buf);

        anim.update(1.5);
        anim.render(area, &mut buf);

        anim.update(0.5);
        anim.render(area, &mut buf); // Done phase — no-op
    }

    #[test]
    fn test_startup_render_rain_fills_bg() {
        let mut anim = StartupAnimation::new();
        anim.update(0.5);
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        anim.render(area, &mut buf);

        // At least some cells should have non-space characters (rain).
        let non_space = (0..40)
            .flat_map(|x| (0..10).map(move |y| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(non_space > 0, "rain should produce visible characters");
    }

    #[test]
    fn test_startup_title_appears_during_type_phase() {
        let mut anim = StartupAnimation::new();
        anim.update(2.0); // mid-TitleType phase
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        anim.render(area, &mut buf);

        // The title row (center) should contain "AETHER" substring.
        let cy = 12; // area.height / 2
        let row_text: String = (0..80).map(|x| buf[(x, cy)].symbol().to_string()).collect();
        assert!(
            row_text.contains("AETHER"),
            "title row should contain 'AETHER', got: '{row_text}'"
        );
    }

    #[test]
    fn test_scale_color_black_at_zero() {
        let c = scale_color(Color::Rgb(100, 200, 50), 0.0);
        assert_eq!(c, Color::Rgb(0, 0, 0));
    }

    #[test]
    fn test_scale_color_unchanged_at_one() {
        let c = scale_color(Color::Rgb(100, 200, 50), 1.0);
        assert_eq!(c, Color::Rgb(100, 200, 50));
    }

    #[test]
    fn test_rain_char_deterministic() {
        let a = rain_char(42);
        let b = rain_char(42);
        assert_eq!(a, b, "same hash should produce same character");
    }
}

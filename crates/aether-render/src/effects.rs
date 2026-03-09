//! Visual effects for 3D node rendering.
//!
//! CPU-load-driven pulsation makes active processes "breathe" —
//! their radius oscillates sinusoidally proportional to load.

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
            "min radius {min_r} below expected {}", base - amplitude
        );
        assert!(
            max_r <= base + amplitude + 0.01,
            "max radius {max_r} above expected {}", base + amplitude
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
        assert!((0.0..1.0).contains(&pos), "position {pos} should be in 0.0..1.0");
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
}

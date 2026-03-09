//! XP tracking and rank progression for the user.

use aether_core::events::GameEvent;

/// User rank determined by accumulated XP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rank {
    Novice,
    Operator,
    Engineer,
    Architect,
    AetherLord,
}

impl Rank {
    /// XP threshold required to reach this rank.
    #[must_use]
    pub fn threshold(&self) -> u32 {
        match self {
            Self::Novice => 0,
            Self::Operator => 100,
            Self::Engineer => 500,
            Self::Architect => 2_000,
            Self::AetherLord => 10_000,
        }
    }

    /// Determine rank from total XP.
    #[must_use]
    pub fn from_xp(xp: u32) -> Self {
        if xp >= 10_000 {
            Self::AetherLord
        } else if xp >= 2_000 {
            Self::Architect
        } else if xp >= 500 {
            Self::Engineer
        } else if xp >= 100 {
            Self::Operator
        } else {
            Self::Novice
        }
    }

    /// Human-readable rank name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::Novice => "Novice",
            Self::Operator => "Operator",
            Self::Engineer => "Engineer",
            Self::Architect => "Architect",
            Self::AetherLord => "Aether Lord",
        }
    }
}

/// Tracks accumulated XP and emits gamification events.
#[derive(Default)]
pub struct XpTracker {
    total_xp: u32,
    pending_events: Vec<GameEvent>,
    /// Fractional seconds accumulated toward the next uptime XP tick.
    uptime_accumulator: f32,
}

impl XpTracker {
    /// Create a tracker starting at 0 XP.
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_xp: 0,
            pending_events: Vec::new(),
            uptime_accumulator: 0.0,
        }
    }

    /// Award XP and emit an XpEarned event.
    pub fn add_xp(&mut self, amount: u32, reason: &str) {
        self.total_xp += amount;
        self.pending_events.push(GameEvent::XpEarned {
            amount,
            reason: reason.to_string(),
        });
    }

    /// Current rank based on total XP.
    #[must_use]
    pub fn current_rank(&self) -> Rank {
        Rank::from_xp(self.total_xp)
    }

    /// XP remaining until the next rank. Returns 0 if already at max rank.
    #[must_use]
    pub fn xp_to_next_rank(&self) -> u32 {
        let next = match self.current_rank() {
            Rank::Novice => Rank::Operator.threshold(),
            Rank::Operator => Rank::Engineer.threshold(),
            Rank::Engineer => Rank::Architect.threshold(),
            Rank::Architect => Rank::AetherLord.threshold(),
            Rank::AetherLord => return 0,
        };
        next.saturating_sub(self.total_xp)
    }

    /// Total accumulated XP.
    #[must_use]
    pub fn total_xp(&self) -> u32 {
        self.total_xp
    }

    /// Accumulate uptime and award +1 XP per 60 seconds.
    pub fn tick(&mut self, dt_secs: f32) {
        self.uptime_accumulator += dt_secs;
        while self.uptime_accumulator >= 60.0 {
            self.uptime_accumulator -= 60.0;
            self.add_xp(1, "uptime");
        }
    }

    /// Award +50 XP for an Arbiter-approved action.
    pub fn on_action_approved(&mut self) {
        self.add_xp(50, "action approved");
    }

    /// Award +10 XP for killing a zombie process.
    pub fn on_zombie_killed(&mut self) {
        self.add_xp(10, "zombie killed");
    }

    /// Drain all pending game events.
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rank_from_xp_novice() {
        assert_eq!(Rank::from_xp(0), Rank::Novice);
        assert_eq!(Rank::from_xp(99), Rank::Novice);
    }

    #[test]
    fn test_rank_from_xp_operator() {
        assert_eq!(Rank::from_xp(100), Rank::Operator);
        assert_eq!(Rank::from_xp(499), Rank::Operator);
    }

    #[test]
    fn test_rank_from_xp_engineer() {
        assert_eq!(Rank::from_xp(500), Rank::Engineer);
        assert_eq!(Rank::from_xp(1999), Rank::Engineer);
    }

    #[test]
    fn test_rank_from_xp_architect() {
        assert_eq!(Rank::from_xp(2000), Rank::Architect);
        assert_eq!(Rank::from_xp(9999), Rank::Architect);
    }

    #[test]
    fn test_rank_from_xp_aether_lord() {
        assert_eq!(Rank::from_xp(10000), Rank::AetherLord);
    }

    #[test]
    fn test_xp_accumulates() {
        let mut tracker = XpTracker::new();
        tracker.add_xp(50, "test");
        tracker.add_xp(30, "test");
        assert_eq!(tracker.total_xp(), 80);
    }

    #[test]
    fn test_xp_to_next_rank_from_zero() {
        let tracker = XpTracker::new();
        assert_eq!(tracker.xp_to_next_rank(), 100, "Novice needs 100 XP for Operator");
    }

    #[test]
    fn test_xp_to_next_rank_partial() {
        let mut tracker = XpTracker::new();
        tracker.add_xp(60, "test");
        assert_eq!(tracker.xp_to_next_rank(), 40, "60 XP Novice needs 40 more for Operator");
    }

    #[test]
    fn test_xp_to_next_rank_at_max() {
        let mut tracker = XpTracker::new();
        tracker.add_xp(10000, "test");
        assert_eq!(tracker.xp_to_next_rank(), 0, "AetherLord has no next rank");
    }

    #[test]
    fn test_tick_awards_uptime_xp() {
        let mut tracker = XpTracker::new();
        // 120 seconds = 2 uptime XP ticks.
        tracker.tick(120.0);
        assert_eq!(tracker.total_xp(), 2, "120s should award 2 uptime XP");
    }

    #[test]
    fn test_tick_accumulates_fractional() {
        let mut tracker = XpTracker::new();
        tracker.tick(30.0);
        assert_eq!(tracker.total_xp(), 0, "30s is not enough for uptime XP");
        tracker.tick(30.0);
        assert_eq!(tracker.total_xp(), 1, "60s total should award 1 uptime XP");
    }

    #[test]
    fn test_on_action_approved_awards_50() {
        let mut tracker = XpTracker::new();
        tracker.on_action_approved();
        assert_eq!(tracker.total_xp(), 50);
    }

    #[test]
    fn test_on_zombie_killed_awards_10() {
        let mut tracker = XpTracker::new();
        tracker.on_zombie_killed();
        assert_eq!(tracker.total_xp(), 10);
    }

    #[test]
    fn test_drain_events_returns_and_clears() {
        let mut tracker = XpTracker::new();
        tracker.add_xp(10, "test");
        tracker.add_xp(20, "test2");

        let events = tracker.drain_events();
        assert_eq!(events.len(), 2, "should have 2 events");
        assert!(tracker.drain_events().is_empty(), "should be empty after drain");
    }

    #[test]
    fn test_rank_display_name() {
        assert_eq!(Rank::Novice.display_name(), "Novice");
        assert_eq!(Rank::AetherLord.display_name(), "Aether Lord");
    }

    #[test]
    fn test_rank_threshold() {
        assert_eq!(Rank::Novice.threshold(), 0);
        assert_eq!(Rank::Operator.threshold(), 100);
        assert_eq!(Rank::Engineer.threshold(), 500);
        assert_eq!(Rank::Architect.threshold(), 2000);
        assert_eq!(Rank::AetherLord.threshold(), 10000);
    }
}

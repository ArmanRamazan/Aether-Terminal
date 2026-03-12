//! Achievement definitions and unlock tracking.

use std::collections::HashSet;
use std::time::Duration;

use aether_core::events::GameEvent;

/// Definition of a single achievement.
#[derive(Debug, Clone)]
pub struct AchievementDef {
    /// Unique identifier (e.g. "first_blood").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Describes how to unlock.
    pub description: String,
}

/// Snapshot of game state used for checking achievement conditions.
pub struct GameState {
    pub kills: u32,
    pub uptime_hours: f32,
    pub zombie_kills: u32,
    pub arbiter_approvals: u32,
    pub dpi_analyses: u32,
    pub diagnostic_resolved_count: u32,
    pub critical_resolved_count: u32,
    pub time_without_critical: Duration,
}

/// Tracks achievement definitions and which ones have been unlocked.
pub struct AchievementTracker {
    definitions: Vec<AchievementDef>,
    unlocked: HashSet<String>,
    pending_events: Vec<GameEvent>,
}

impl AchievementTracker {
    /// Create a tracker with the default set of 5 achievements.
    #[must_use]
    pub fn new() -> Self {
        let definitions = vec![
            AchievementDef {
                id: "first_blood".into(),
                name: "First Blood".into(),
                description: "Kill your first process".into(),
            },
            AchievementDef {
                id: "uptime_champion".into(),
                name: "Uptime Champion".into(),
                description: "24h without anomalies".into(),
            },
            AchievementDef {
                id: "network_oracle".into(),
                name: "Network Oracle".into(),
                description: "Analyze 100 network flows".into(),
            },
            AchievementDef {
                id: "zombie_hunter".into(),
                name: "Zombie Hunter".into(),
                description: "Kill 50 zombie processes".into(),
            },
            AchievementDef {
                id: "ai_whisperer".into(),
                name: "AI Whisperer".into(),
                description: "Approve 100 AI agent actions".into(),
            },
            AchievementDef {
                id: "first_responder".into(),
                name: "First Responder".into(),
                description: "Resolve your first diagnostic".into(),
            },
            AchievementDef {
                id: "firefighter".into(),
                name: "Firefighter".into(),
                description: "Resolve 10 Critical diagnostics".into(),
            },
            AchievementDef {
                id: "stability_master".into(),
                name: "Stability Master".into(),
                description: "Run 1 hour with 0 active Critical diagnostics".into(),
            },
        ];

        Self {
            definitions,
            unlocked: HashSet::new(),
            pending_events: Vec::new(),
        }
    }

    /// Check game state against all achievement conditions.
    /// Returns newly unlocked achievements and emits `AchievementUnlocked` events.
    pub fn check(&mut self, state: &GameState) -> Vec<AchievementDef> {
        let mut newly_unlocked = Vec::new();

        for def in &self.definitions {
            if self.unlocked.contains(&def.id) {
                continue;
            }

            let met = match def.id.as_str() {
                "first_blood" => state.kills > 0,
                "uptime_champion" => state.uptime_hours > 24.0,
                "network_oracle" => state.dpi_analyses > 100,
                "zombie_hunter" => state.zombie_kills > 50,
                "ai_whisperer" => state.arbiter_approvals > 100,
                "first_responder" => state.diagnostic_resolved_count > 0,
                "firefighter" => state.critical_resolved_count >= 10,
                "stability_master" => state.time_without_critical >= Duration::from_secs(3600),
                _ => false,
            };

            if met {
                self.unlocked.insert(def.id.clone());
                self.pending_events.push(GameEvent::AchievementUnlocked {
                    id: def.id.clone(),
                    name: def.name.clone(),
                });
                newly_unlocked.push(def.clone());
            }
        }

        newly_unlocked
    }

    /// Whether a specific achievement has been unlocked.
    #[must_use]
    pub fn is_unlocked(&self, id: &str) -> bool {
        self.unlocked.contains(id)
    }

    /// Number of achievements unlocked so far.
    #[must_use]
    pub fn unlocked_count(&self) -> usize {
        self.unlocked.len()
    }

    /// Total number of defined achievements.
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.definitions.len()
    }

    /// Drain all pending game events.
    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

impl Default for AchievementTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state() -> GameState {
        GameState {
            kills: 0,
            uptime_hours: 0.0,
            zombie_kills: 0,
            arbiter_approvals: 0,
            dpi_analyses: 0,
            diagnostic_resolved_count: 0,
            critical_resolved_count: 0,
            time_without_critical: Duration::ZERO,
        }
    }

    #[test]
    fn test_initial_count_is_eight() {
        let tracker = AchievementTracker::new();
        assert_eq!(tracker.total_count(), 8);
        assert_eq!(tracker.unlocked_count(), 0);
    }

    #[test]
    fn test_first_blood_unlocks_at_threshold() {
        let mut tracker = AchievementTracker::new();
        let mut state = base_state();

        let unlocked = tracker.check(&state);
        assert!(unlocked.is_empty(), "no kills means no unlock");

        state.kills = 1;
        let unlocked = tracker.check(&state);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(unlocked[0].id, "first_blood");
        assert!(tracker.is_unlocked("first_blood"));
    }

    #[test]
    fn test_no_double_unlock() {
        let mut tracker = AchievementTracker::new();
        let state = GameState { kills: 1, ..base_state() };

        let first = tracker.check(&state);
        assert_eq!(first.len(), 1, "first check unlocks");

        let second = tracker.check(&state);
        assert!(second.is_empty(), "second check must not re-trigger");

        assert_eq!(tracker.unlocked_count(), 1);
    }

    #[test]
    fn test_uptime_champion_unlocks() {
        let mut tracker = AchievementTracker::new();
        let state = GameState { uptime_hours: 25.0, ..base_state() };

        let unlocked = tracker.check(&state);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(unlocked[0].id, "uptime_champion");
    }

    #[test]
    fn test_multiple_achievements_at_once() {
        let mut tracker = AchievementTracker::new();
        let state = GameState {
            kills: 5,
            uptime_hours: 30.0,
            zombie_kills: 100,
            arbiter_approvals: 200,
            dpi_analyses: 200,
            diagnostic_resolved_count: 50,
            critical_resolved_count: 10,
            time_without_critical: Duration::from_secs(7200),
        };

        let unlocked = tracker.check(&state);
        assert_eq!(unlocked.len(), 8, "all achievements should unlock");
        assert_eq!(tracker.unlocked_count(), 8);
    }

    #[test]
    fn test_check_emits_events() {
        let mut tracker = AchievementTracker::new();
        let state = GameState { kills: 1, ..base_state() };

        tracker.check(&state);
        let events = tracker.drain_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            GameEvent::AchievementUnlocked { id, name } => {
                assert_eq!(id, "first_blood");
                assert_eq!(name, "First Blood");
            }
            _ => panic!("expected AchievementUnlocked event"),
        }
    }

    #[test]
    fn test_first_responder_achievement_unlocks() {
        let mut tracker = AchievementTracker::new();
        let state = GameState {
            diagnostic_resolved_count: 1,
            ..base_state()
        };

        let unlocked = tracker.check(&state);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(unlocked[0].id, "first_responder");
        assert!(tracker.is_unlocked("first_responder"));
    }

    #[test]
    fn test_firefighter_achievement_unlocks() {
        let mut tracker = AchievementTracker::new();
        let state = GameState {
            diagnostic_resolved_count: 10,
            critical_resolved_count: 10,
            ..base_state()
        };

        let unlocked = tracker.check(&state);
        assert!(unlocked.iter().any(|a| a.id == "firefighter"));
        assert!(tracker.is_unlocked("firefighter"));
    }

    #[test]
    fn test_stability_master_achievement_unlocks() {
        let mut tracker = AchievementTracker::new();
        let state = GameState {
            time_without_critical: Duration::from_secs(3600),
            ..base_state()
        };

        let unlocked = tracker.check(&state);
        assert_eq!(unlocked.len(), 1);
        assert_eq!(unlocked[0].id, "stability_master");
        assert!(tracker.is_unlocked("stability_master"));
    }

    #[test]
    fn test_drain_events_clears() {
        let mut tracker = AchievementTracker::new();
        let state = GameState { kills: 1, ..base_state() };

        tracker.check(&state);
        tracker.drain_events();
        assert!(tracker.drain_events().is_empty(), "should be empty after drain");
    }
}

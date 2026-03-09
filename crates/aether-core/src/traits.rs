//! Hexagonal port traits for dependency inversion.
//!
//! These traits define the boundaries between crates:
//! - [`SystemProbe`] — implemented by `aether-ingestion` (sysinfo) and `aether-ebpf`
//! - [`Storage`] — implemented by `aether-gamification` (SQLite)

use crate::models::SystemSnapshot;

/// A session record for gamification persistence.
#[derive(Debug, Clone)]
pub struct GameSession {
    pub id: u64,
    pub started_at: String,
    pub total_xp: u32,
    pub rank: String,
    pub uptime_secs: u64,
}

/// A ranking entry loaded from storage.
#[derive(Debug, Clone)]
pub struct Ranking {
    pub session_id: u64,
    pub total_xp: u32,
    pub rank: String,
}

/// Port for system metrics collection.
///
/// Implemented by `SysinfoProbe` (crossplatform) and `EbpfProbe` (Linux eBPF).
pub trait SystemProbe: Send + Sync + 'static {
    /// Collect a point-in-time snapshot of all processes and connections.
    fn snapshot(
        &self,
    ) -> impl std::future::Future<Output = Result<SystemSnapshot, Box<dyn std::error::Error + Send + Sync>>>
           + Send;
}

/// Port for gamification data persistence.
///
/// Implemented by `SqliteStorage` in the `aether-gamification` crate.
pub trait Storage: Send + Sync + 'static {
    /// Persist a game session record.
    fn save_session(
        &self,
        session: &GameSession,
    ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
           + Send;

    /// Load the ranking leaderboard.
    fn load_rankings(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Ranking>, Box<dyn std::error::Error + Send + Sync>>>
           + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_session_construction() {
        let session = GameSession {
            id: 1,
            started_at: "2026-03-09T12:00:00Z".to_string(),
            total_xp: 500,
            rank: "Silver".to_string(),
            uptime_secs: 3600,
        };
        let cloned = session.clone();
        assert_eq!(cloned.id, 1);
        assert_eq!(cloned.total_xp, 500);
        assert_eq!(cloned.rank, "Silver");
        assert_eq!(cloned.uptime_secs, 3600);
    }

    #[test]
    fn ranking_construction() {
        let ranking = Ranking {
            session_id: 1,
            total_xp: 500,
            rank: "Silver".to_string(),
        };
        let cloned = ranking.clone();
        assert_eq!(cloned.session_id, 1);
        assert_eq!(cloned.total_xp, 500);
        assert_eq!(cloned.rank, "Silver");
    }
}

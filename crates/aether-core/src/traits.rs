//! Hexagonal port traits for dependency inversion.
//!
//! These traits define the boundaries between crates:
//! - [`SystemProbe`] — implemented by `aether-ingestion` (sysinfo) and `aether-ebpf`
//! - [`Storage`] — implemented by `aether-gamification` (SQLite)

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::{CollectedMetric, Diagnostic, Severity, SystemSnapshot, Target};

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
    ) -> impl std::future::Future<Output = Result<SystemSnapshot, CoreError>> + Send;
}

/// Port for gamification data persistence.
///
/// Implemented by `SqliteStorage` in the `aether-gamification` crate.
pub trait Storage: Send + Sync + 'static {
    /// Persist a game session record.
    fn save_session(
        &self,
        session: &GameSession,
    ) -> impl std::future::Future<Output = Result<(), CoreError>> + Send;

    /// Load the ranking leaderboard.
    fn load_rankings(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Ranking>, CoreError>> + Send;
}

// ---------------------------------------------------------------------------
// Phase 2 adapter traits (object-safe via async_trait)
// ---------------------------------------------------------------------------

/// A source of metrics data (Prometheus scraper, prober, log parser).
#[async_trait]
pub trait DataSource: Send + Sync {
    /// Collect metrics from this source.
    async fn collect(
        &self,
    ) -> Result<Vec<CollectedMetric>, Box<dyn std::error::Error + Send + Sync>>;

    /// Human-readable name of this data source.
    fn name(&self) -> &str;
}

/// A destination for diagnostic alerts (Slack, Discord, file, stdout).
#[async_trait]
pub trait OutputSink: Send + Sync {
    /// Send a diagnostic finding to this output.
    async fn send(
        &self,
        diagnostic: &Diagnostic,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Human-readable name of this output.
    fn name(&self) -> &str;

    /// Minimum severity this sink cares about.
    fn min_severity(&self) -> Severity;
}

/// Discovers services and targets in the infrastructure (port scan, K8s API).
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    /// Scan for available targets.
    async fn discover(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>>;

    /// Human-readable name of this discovery mechanism.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_session_clone_preserves_fields() {
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
    fn test_ranking_clone_preserves_fields() {
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

    /// Verify Phase 2 traits are object-safe (compile-time check).
    #[test]
    fn test_adapter_traits_are_object_safe() {
        let _: Option<Box<dyn DataSource>> = None;
        let _: Option<Box<dyn OutputSink>> = None;
        let _: Option<Box<dyn ServiceDiscovery>> = None;
    }
}

//! SQLite persistence for gamification data.
//!
//! Stores sessions, achievements, and action logs in a local SQLite database.

use std::path::Path;
use std::sync::Mutex;

use aether_core::error::CoreError;
use aether_core::traits::{GameSession, Ranking, Storage};
use rusqlite::Connection;

use crate::error::StorageError;

/// SQLite-backed persistence for gamification state.
pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    /// Open or create the database at `path`, running migrations.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                StorageError::Io(format!("failed to create directory {}: {e}", parent.display()))
            })?;
        }

        let conn = Connection::open(path).map_err(|e| StorageError::Sqlite(e.to_string()))?;
        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.run_migrations()?;
        Ok(storage)
    }

    fn run_migrations(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                total_xp INTEGER NOT NULL DEFAULT 0,
                rank TEXT NOT NULL DEFAULT 'Novice',
                uptime_secs INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS achievements (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                unlocked_at TEXT NOT NULL,
                session_id INTEGER REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS action_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                action TEXT NOT NULL,
                pid INTEGER,
                source TEXT NOT NULL,
                approved INTEGER,
                session_id INTEGER REFERENCES sessions(id)
            );",
        )
        .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        Ok(())
    }

    /// Insert a new session, returning its id.
    pub fn start_session(&self) -> Result<i64, StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT INTO sessions (started_at) VALUES (datetime('now'))",
            [],
        )
        .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        Ok(conn.last_insert_rowid())
    }

    /// Finalize a session with accumulated stats.
    pub fn end_session(
        &self,
        id: i64,
        xp: u32,
        rank: &str,
        uptime: u64,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE sessions SET ended_at = datetime('now'), total_xp = ?1, rank = ?2, uptime_secs = ?3 WHERE id = ?4",
            rusqlite::params![xp, rank, uptime as i64, id],
        )
        .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        Ok(())
    }

    /// Record an unlocked achievement.
    pub fn save_achievement(
        &self,
        id: &str,
        name: &str,
        session_id: i64,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT OR IGNORE INTO achievements (id, name, unlocked_at, session_id) VALUES (?1, ?2, datetime('now'), ?3)",
            rusqlite::params![id, name, session_id],
        )
        .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        Ok(())
    }

    /// Load all unlocked achievements as `(id, name)` pairs.
    pub fn load_achievements(&self) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, name FROM achievements")
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Sqlite(e.to_string()))?);
        }
        Ok(result)
    }

    /// Record an action in the audit log.
    pub fn log_action(
        &self,
        action: &str,
        pid: u32,
        source: &str,
        approved: bool,
        session_id: i64,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT INTO action_log (timestamp, action, pid, source, approved, session_id) VALUES (datetime('now'), ?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![action, pid, source, approved as i32, session_id],
        )
        .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        Ok(())
    }

    /// Load session rankings ordered by XP descending.
    pub fn load_rankings_sync(&self) -> Result<Vec<(i64, u32, String)>, StorageError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, total_xp, rank FROM sessions ORDER BY total_xp DESC")
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, u32>(1)?, row.get(2)?))
            })
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| StorageError::Sqlite(e.to_string()))?);
        }
        Ok(result)
    }
}

impl Storage for SqliteStorage {
    async fn save_session(&self, session: &GameSession) -> Result<(), CoreError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO sessions (id, started_at, total_xp, rank, uptime_secs) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                session.id as i64,
                session.started_at,
                session.total_xp,
                session.rank,
                session.uptime_secs as i64,
            ],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_rankings(&self) -> Result<Vec<Ranking>, CoreError> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, total_xp, rank FROM sessions ORDER BY total_xp DESC")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Ranking {
                    session_id: row.get::<_, i64>(0)? as u64,
                    total_xp: row.get(1)?,
                    rank: row.get(2)?,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_db() -> (PathBuf, SqliteStorage) {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "aether-test-{}-{}",
            std::process::id(),
            n
        ));
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);
        let storage = SqliteStorage::open(&path).expect("failed to open test db");
        (path, storage)
    }

    #[test]
    fn test_open_creates_tables() {
        let (_path, storage) = temp_db();
        let conn = storage.conn.lock().unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"sessions".to_string()), "missing sessions table");
        assert!(tables.contains(&"achievements".to_string()), "missing achievements table");
        assert!(tables.contains(&"action_log".to_string()), "missing action_log table");
    }

    #[test]
    fn test_session_round_trip() {
        let (_path, storage) = temp_db();

        let id = storage.start_session().expect("start_session failed");
        assert!(id > 0, "session id should be positive");

        storage
            .end_session(id, 500, "Engineer", 3600)
            .expect("end_session failed");

        let rankings = storage.load_rankings_sync().expect("load_rankings failed");
        assert_eq!(rankings.len(), 1);
        assert_eq!(rankings[0].0, id);
        assert_eq!(rankings[0].1, 500);
        assert_eq!(rankings[0].2, "Engineer");
    }

    #[test]
    fn test_save_and_load_achievements() {
        let (_path, storage) = temp_db();
        let session_id = storage.start_session().unwrap();

        storage
            .save_achievement("first_blood", "First Blood", session_id)
            .expect("save_achievement failed");
        storage
            .save_achievement("zombie_hunter", "Zombie Hunter", session_id)
            .expect("save_achievement failed");

        let achievements = storage.load_achievements().expect("load_achievements failed");
        assert_eq!(achievements.len(), 2);

        let ids: Vec<&str> = achievements.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"first_blood"));
        assert!(ids.contains(&"zombie_hunter"));
    }

    #[test]
    fn test_rankings_sorted_by_xp_desc() {
        let (_path, storage) = temp_db();

        let s1 = storage.start_session().unwrap();
        storage.end_session(s1, 100, "Operator", 600).unwrap();

        let s2 = storage.start_session().unwrap();
        storage.end_session(s2, 2000, "Architect", 7200).unwrap();

        let s3 = storage.start_session().unwrap();
        storage.end_session(s3, 500, "Engineer", 3600).unwrap();

        let rankings = storage.load_rankings_sync().unwrap();
        assert_eq!(rankings.len(), 3);
        assert_eq!(rankings[0].1, 2000, "highest XP first");
        assert_eq!(rankings[1].1, 500, "middle XP second");
        assert_eq!(rankings[2].1, 100, "lowest XP last");
    }

    #[tokio::test]
    async fn test_storage_trait_save_and_load() {
        let (_path, storage) = temp_db();

        let session = GameSession {
            id: 42,
            started_at: "2026-03-10T12:00:00Z".to_string(),
            total_xp: 1500,
            rank: "Architect".to_string(),
            uptime_secs: 5400,
        };

        Storage::save_session(&storage, &session)
            .await
            .expect("save_session failed");

        let rankings = Storage::load_rankings(&storage)
            .await
            .expect("load_rankings failed");

        assert_eq!(rankings.len(), 1);
        assert_eq!(rankings[0].session_id, 42);
        assert_eq!(rankings[0].total_xp, 1500);
        assert_eq!(rankings[0].rank, "Architect");
    }
}

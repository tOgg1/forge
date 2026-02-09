//! forge-db: SQLite storage + migration engine for Forge.

pub mod alert_repository;
pub mod approval_repository;
pub mod event_repository;
pub mod file_lock_repository;
pub mod loop_queue_repository;
pub mod loop_repository;
pub mod loop_run_repository;
pub mod loop_work_state_repository;
pub mod mail_repository;
pub mod pool_repository;
pub mod port_repository;
pub mod profile_repository;
pub mod transcript_repository;
pub mod usage_repository;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use uuid::Uuid;

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// Crate identity label used for parity verification.
pub fn crate_label() -> &'static str {
    "forge-db"
}

#[derive(Debug, Clone)]
pub struct Config {
    pub path: PathBuf,
    pub busy_timeout_ms: u64,
}

impl Config {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            busy_timeout_ms: 5000,
        }
    }
}

#[derive(Debug)]
pub struct Db {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatus {
    pub version: i32,
    pub description: String,
    pub applied: bool,
    pub applied_at: String,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("open database: {0}")]
    Open(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("migration {version} missing {direction} sql")]
    MissingSQL {
        version: i32,
        direction: &'static str,
    },
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Transaction(String),
    #[error("loop not found")]
    LoopNotFound,
    #[error("loop already exists")]
    LoopAlreadyExists,
    #[error("loop run not found")]
    LoopRunNotFound,
    #[error("usage record not found")]
    UsageRecordNotFound,
    #[error("invalid usage record")]
    InvalidUsageRecord,
    #[error("event not found")]
    EventNotFound,
    #[error("invalid event")]
    InvalidEvent,
    #[error("loop kv not found: {0}")]
    LoopKVNotFound(String),
    #[error("loop work state not found")]
    LoopWorkStateNotFound,
    #[error("pool not found")]
    PoolNotFound,
    #[error("pool already exists")]
    PoolAlreadyExists,
    #[error("profile not found")]
    ProfileNotFound,
    #[error("profile already exists")]
    ProfileAlreadyExists,
    #[error("alert not found")]
    AlertNotFound,
    #[error("approval not found")]
    ApprovalNotFound,
    #[error("mail thread not found")]
    MailThreadNotFound,
    #[error("mail message not found")]
    MailMessageNotFound,
    #[error("transcript not found")]
    TranscriptNotFound,
    #[error("queue item not found")]
    QueueItemNotFound,
    #[error("queue is empty")]
    QueueEmpty,
    #[error("no available ports in range")]
    NoAvailablePorts,
    #[error("port already allocated")]
    PortAlreadyAllocated,
    #[error("port not allocated")]
    PortNotAllocated,
}

impl Db {
    const DEFAULT_RETRY_ATTEMPTS: usize = 3;
    const DEFAULT_RETRY_BACKOFF_MS: u64 = 50;

    pub fn open(cfg: Config) -> Result<Self, DbError> {
        ensure_parent_dir(&cfg.path)?;
        let conn = Connection::open(&cfg.path)?;
        conn.busy_timeout(Duration::from_millis(cfg.busy_timeout_ms))?;
        // Match Go connection defaults as closely as possible.
        // Best-effort: ignore pragma errors on older SQLite builds.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "foreign_keys", "ON");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Ok(Self { conn })
    }

    pub fn migrate_up(&mut self) -> Result<usize, DbError> {
        self.ensure_schema_version_table()?;
        let current = self.current_version()?;

        let mut applied = 0usize;
        for m in MIGRATIONS {
            if m.version <= current {
                continue;
            }
            if m.up_sql.is_empty() {
                return Err(DbError::MissingSQL {
                    version: m.version,
                    direction: "up",
                });
            }

            let tx = self.conn.transaction()?;
            tx.execute_batch(m.up_sql)?;
            tx.execute(
                "INSERT INTO schema_version (version, description) VALUES (?1, ?2)",
                params![m.version, m.description],
            )?;
            tx.commit()?;
            applied += 1;
        }
        Ok(applied)
    }

    pub fn migrate_down(&mut self, steps: i32) -> Result<usize, DbError> {
        self.ensure_schema_version_table()?;
        let current = self.current_version()?;
        if current == 0 || steps <= 0 {
            return Ok(0);
        }

        let mut to_rollback = Vec::new();
        for m in MIGRATIONS.iter().rev() {
            if m.version <= current {
                to_rollback.push(*m);
                if to_rollback.len() >= steps as usize {
                    break;
                }
            }
        }

        let mut rolled_back = 0usize;
        for m in to_rollback {
            if m.down_sql.is_empty() {
                return Err(DbError::MissingSQL {
                    version: m.version,
                    direction: "down",
                });
            }

            let tx = self.conn.transaction()?;
            tx.execute_batch(m.down_sql)?;
            tx.execute(
                "DELETE FROM schema_version WHERE version = ?1",
                params![m.version],
            )?;
            tx.commit()?;
            rolled_back += 1;
        }

        Ok(rolled_back)
    }

    pub fn migrate_to(&mut self, target_version: i32) -> Result<(), DbError> {
        self.ensure_schema_version_table()?;
        let current = self.current_version()?;
        if target_version == current {
            return Ok(());
        }

        if target_version > current {
            for m in MIGRATIONS {
                if m.version <= current || m.version > target_version {
                    continue;
                }
                if m.up_sql.is_empty() {
                    return Err(DbError::MissingSQL {
                        version: m.version,
                        direction: "up",
                    });
                }

                let tx = self.conn.transaction()?;
                tx.execute_batch(m.up_sql)?;
                tx.execute(
                    "INSERT INTO schema_version (version, description) VALUES (?1, ?2)",
                    params![m.version, m.description],
                )?;
                tx.commit()?;
            }
        } else {
            for m in MIGRATIONS.iter().rev() {
                if m.version <= target_version || m.version > current {
                    continue;
                }
                if m.down_sql.is_empty() {
                    return Err(DbError::MissingSQL {
                        version: m.version,
                        direction: "down",
                    });
                }

                let tx = self.conn.transaction()?;
                tx.execute_batch(m.down_sql)?;
                tx.execute(
                    "DELETE FROM schema_version WHERE version = ?1",
                    params![m.version],
                )?;
                tx.commit()?;
            }
        }
        Ok(())
    }

    pub fn migration_status(&mut self) -> Result<Vec<MigrationStatus>, DbError> {
        self.ensure_schema_version_table()?;

        let mut applied_at: BTreeMap<i32, String> = BTreeMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT version, applied_at FROM schema_version ORDER BY version")?;
        let rows = stmt.query_map([], |row| {
            let version: i32 = row.get(0)?;
            let stamp: String = row.get(1)?;
            Ok((version, stamp))
        })?;
        for row in rows {
            let (version, stamp) = row?;
            applied_at.insert(version, stamp);
        }

        let mut status = Vec::with_capacity(MIGRATIONS.len());
        for m in MIGRATIONS {
            let stamp = applied_at.get(&m.version).cloned().unwrap_or_default();
            status.push(MigrationStatus {
                version: m.version,
                description: m.description.to_string(),
                applied: applied_at.contains_key(&m.version),
                applied_at: stamp,
            });
        }
        Ok(status)
    }

    pub fn schema_version(&self) -> Result<i32, DbError> {
        let version: Option<i32> = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(version.unwrap_or(0))
    }

    fn ensure_schema_version_table(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (\n\
                version INTEGER PRIMARY KEY,\n\
                applied_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
                description TEXT\n\
             );",
        )?;
        Ok(())
    }

    /// Returns a reference to the underlying SQLite connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Transaction executes `f` inside a SQLite transaction.
    ///
    /// Mirrors Go's `db.Transaction`: explicit rollback on error, explicit commit on success.
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let tx = self.conn.transaction()?;

        match f(&tx) {
            Ok(v) => {
                tx.commit()?;
                Ok(v)
            }
            Err(e) => {
                if let Err(rb) = tx.rollback() {
                    return Err(DbError::Transaction(format!(
                        "rollback failed: {rb} (original error: {e})"
                    )));
                }
                Err(e)
            }
        }
    }

    /// TransactionWithRetry retries a transaction when SQLite reports busy/locked.
    ///
    /// Mirrors Go's `db.TransactionWithRetry` string-matching behavior.
    pub fn transaction_with_retry<T>(
        &mut self,
        mut max_attempts: usize,
        mut base_backoff: Duration,
        mut f: impl FnMut(&rusqlite::Transaction<'_>) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        if max_attempts == 0 {
            max_attempts = Self::DEFAULT_RETRY_ATTEMPTS;
        }
        if base_backoff.is_zero() {
            base_backoff = Duration::from_millis(Self::DEFAULT_RETRY_BACKOFF_MS);
        }

        let mut backoff = base_backoff;
        for attempt in 1..=max_attempts {
            let result = self.transaction(|tx| f(tx));
            match result {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt >= max_attempts || !is_busy_error(&e) {
                        return Err(e);
                    }
                    std::thread::sleep(backoff);
                    backoff = backoff.saturating_mul(2);
                }
            }
        }

        unreachable!("loop returns on success or final error")
    }

    fn current_version(&self) -> Result<i32, DbError> {
        let version: Option<i32> = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(version.unwrap_or(0))
    }
}

fn is_busy_error(err: &DbError) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("database is locked")
        || msg.contains("database is busy")
        || msg.contains("sqlite_busy")
}

// ---------------------------------------------------------------------------
// LoopKV: per-loop key/value memory (prompt-injected)
// ---------------------------------------------------------------------------

/// A key-value entry scoped to a single loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopKV {
    pub id: String,
    pub loop_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Repository for per-loop key/value storage with Go-parity semantics.
pub struct LoopKVRepository<'a> {
    db: &'a Db,
}

impl<'a> LoopKVRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create or update a key-value pair for a loop.
    ///
    /// Uses "prefer UPDATE then INSERT" to avoid relying on newer SQLite
    /// upsert syntax, matching Go behavior exactly.
    pub fn set(&self, loop_id: &str, key: &str, value: &str) -> Result<(), DbError> {
        let loop_id = loop_id.trim();
        let key = key.trim();
        if loop_id.is_empty() {
            return Err(DbError::Validation("loopID is required".into()));
        }
        if key.is_empty() {
            return Err(DbError::Validation("key is required".into()));
        }
        if value.is_empty() {
            return Err(DbError::Validation("value is required".into()));
        }

        let now = now_rfc3339();

        // Attempt UPDATE first.
        let rows_changed = self.db.conn.execute(
            "UPDATE loop_kv SET value = ?1, updated_at = ?2 WHERE loop_id = ?3 AND key = ?4",
            params![value, now, loop_id, key],
        )?;
        if rows_changed > 0 {
            return Ok(());
        }

        // No existing row — INSERT.
        let id = Uuid::new_v4().to_string();
        let insert_result = self.db.conn.execute(
            "INSERT INTO loop_kv (id, loop_id, key, value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, loop_id, key, value, now, now],
        );
        match insert_result {
            Ok(_) => Ok(()),
            Err(ref err) if is_unique_constraint_error(err) => {
                // Race: key inserted after our UPDATE check; retry UPDATE.
                self.db.conn.execute(
                    "UPDATE loop_kv SET value = ?1, updated_at = ?2 \
                     WHERE loop_id = ?3 AND key = ?4",
                    params![value, now, loop_id, key],
                )?;
                Ok(())
            }
            Err(err) => Err(DbError::Open(err)),
        }
    }

    /// Retrieve a single key-value pair.
    pub fn get(&self, loop_id: &str, key: &str) -> Result<LoopKV, DbError> {
        let row = self
            .db
            .conn
            .query_row(
                "SELECT id, loop_id, key, value, created_at, updated_at \
                 FROM loop_kv WHERE loop_id = ?1 AND key = ?2",
                params![loop_id.trim(), key.trim()],
                |row| {
                    Ok(LoopKV {
                        id: row.get(0)?,
                        loop_id: row.get(1)?,
                        key: row.get(2)?,
                        value: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .optional()?;
        row.ok_or_else(|| DbError::LoopKVNotFound("loop kv not found".into()))
    }

    /// Retrieve all key-value pairs for a specific loop, sorted by key.
    pub fn list_by_loop(&self, loop_id: &str) -> Result<Vec<LoopKV>, DbError> {
        let mut stmt = self.db.conn.prepare(
            "SELECT id, loop_id, key, value, created_at, updated_at \
             FROM loop_kv WHERE loop_id = ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![loop_id.trim()], |row| {
            Ok(LoopKV {
                id: row.get(0)?,
                loop_id: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Delete a key-value pair.
    pub fn delete(&self, loop_id: &str, key: &str) -> Result<(), DbError> {
        let rows = self.db.conn.execute(
            "DELETE FROM loop_kv WHERE loop_id = ?1 AND key = ?2",
            params![loop_id.trim(), key.trim()],
        )?;
        if rows == 0 {
            return Err(DbError::LoopKVNotFound("loop kv not found".into()));
        }
        Ok(())
    }
}

fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_civil(days as i64);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day) in the Gregorian calendar.
fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn is_unique_constraint_error(err: &rusqlite::Error) -> bool {
    err.to_string().contains("UNIQUE constraint failed")
}

fn ensure_parent_dir(path: &Path) -> Result<(), std::io::Error> {
    let parent = match path.parent() {
        Some(parent) => parent,
        None => return Ok(()),
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-db");
    }

    #[test]
    fn embedded_migrations_are_sorted_and_nonempty() {
        assert!(!MIGRATIONS.is_empty());
        let mut prev = 0;
        for m in MIGRATIONS {
            assert!(m.version > prev);
            assert!(!m.description.is_empty());
            prev = m.version;
        }
    }

    #[test]
    fn migration_001_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 1) {
            Some(migration) => migration,
            None => panic!("migration 001 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/001_initial_schema.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/001_initial_schema.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_001_up_down_creates_and_removes_initial_schema() {
        let db_path = temp_db_path("migration-001");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(1) {
            panic!("migrate_to(1): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 1);

        // Tables
        assert!(table_exists(&db_path, "nodes"));
        assert!(table_exists(&db_path, "workspaces"));
        assert!(table_exists(&db_path, "accounts"));
        assert!(table_exists(&db_path, "agents"));
        assert!(table_exists(&db_path, "queue_items"));
        assert!(table_exists(&db_path, "events"));
        assert!(table_exists(&db_path, "alerts"));
        assert!(table_exists(&db_path, "transcripts"));
        assert!(table_exists(&db_path, "approvals"));

        // Indexes
        assert!(index_exists(&db_path, "idx_nodes_name"));
        assert!(index_exists(&db_path, "idx_nodes_status"));
        assert!(index_exists(&db_path, "idx_workspaces_node_id"));
        assert!(index_exists(&db_path, "idx_workspaces_status"));
        assert!(index_exists(&db_path, "idx_workspaces_name"));
        assert!(index_exists(&db_path, "idx_accounts_provider"));
        assert!(index_exists(&db_path, "idx_accounts_is_active"));
        assert!(index_exists(&db_path, "idx_accounts_cooldown"));
        assert!(index_exists(&db_path, "idx_agents_workspace_id"));
        assert!(index_exists(&db_path, "idx_agents_state"));
        assert!(index_exists(&db_path, "idx_agents_account_id"));
        assert!(index_exists(&db_path, "idx_agents_type"));
        assert!(index_exists(&db_path, "idx_queue_items_agent_id"));
        assert!(index_exists(&db_path, "idx_queue_items_status"));
        assert!(index_exists(&db_path, "idx_queue_items_position"));
        assert!(index_exists(&db_path, "idx_events_timestamp"));
        assert!(index_exists(&db_path, "idx_events_type"));
        assert!(index_exists(&db_path, "idx_events_entity"));
        assert!(index_exists(&db_path, "idx_events_entity_timestamp"));
        assert!(index_exists(&db_path, "idx_alerts_workspace_id"));
        assert!(index_exists(&db_path, "idx_alerts_agent_id"));
        assert!(index_exists(&db_path, "idx_alerts_is_resolved"));
        assert!(index_exists(&db_path, "idx_alerts_severity"));
        assert!(index_exists(&db_path, "idx_transcripts_agent_id"));
        assert!(index_exists(&db_path, "idx_transcripts_captured_at"));
        assert!(index_exists(&db_path, "idx_transcripts_hash"));
        assert!(index_exists(&db_path, "idx_approvals_agent_id"));
        assert!(index_exists(&db_path, "idx_approvals_status"));

        // Triggers
        assert!(trigger_exists(&db_path, "update_nodes_timestamp"));
        assert!(trigger_exists(&db_path, "update_workspaces_timestamp"));
        assert!(trigger_exists(&db_path, "update_agents_timestamp"));
        assert!(trigger_exists(&db_path, "update_accounts_timestamp"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 0);

        // Tables removed
        assert!(!table_exists(&db_path, "nodes"));
        assert!(!table_exists(&db_path, "workspaces"));
        assert!(!table_exists(&db_path, "accounts"));
        assert!(!table_exists(&db_path, "agents"));
        assert!(!table_exists(&db_path, "queue_items"));
        assert!(!table_exists(&db_path, "events"));
        assert!(!table_exists(&db_path, "alerts"));
        assert!(!table_exists(&db_path, "transcripts"));
        assert!(!table_exists(&db_path, "approvals"));

        // Indexes removed
        assert!(!index_exists(&db_path, "idx_nodes_name"));
        assert!(!index_exists(&db_path, "idx_nodes_status"));
        assert!(!index_exists(&db_path, "idx_workspaces_node_id"));
        assert!(!index_exists(&db_path, "idx_workspaces_status"));
        assert!(!index_exists(&db_path, "idx_workspaces_name"));
        assert!(!index_exists(&db_path, "idx_accounts_provider"));
        assert!(!index_exists(&db_path, "idx_accounts_is_active"));
        assert!(!index_exists(&db_path, "idx_accounts_cooldown"));
        assert!(!index_exists(&db_path, "idx_agents_workspace_id"));
        assert!(!index_exists(&db_path, "idx_agents_state"));
        assert!(!index_exists(&db_path, "idx_agents_account_id"));
        assert!(!index_exists(&db_path, "idx_agents_type"));
        assert!(!index_exists(&db_path, "idx_queue_items_agent_id"));
        assert!(!index_exists(&db_path, "idx_queue_items_status"));
        assert!(!index_exists(&db_path, "idx_queue_items_position"));
        assert!(!index_exists(&db_path, "idx_events_timestamp"));
        assert!(!index_exists(&db_path, "idx_events_type"));
        assert!(!index_exists(&db_path, "idx_events_entity"));
        assert!(!index_exists(&db_path, "idx_events_entity_timestamp"));
        assert!(!index_exists(&db_path, "idx_alerts_workspace_id"));
        assert!(!index_exists(&db_path, "idx_alerts_agent_id"));
        assert!(!index_exists(&db_path, "idx_alerts_is_resolved"));
        assert!(!index_exists(&db_path, "idx_alerts_severity"));
        assert!(!index_exists(&db_path, "idx_transcripts_agent_id"));
        assert!(!index_exists(&db_path, "idx_transcripts_captured_at"));
        assert!(!index_exists(&db_path, "idx_transcripts_hash"));
        assert!(!index_exists(&db_path, "idx_approvals_agent_id"));
        assert!(!index_exists(&db_path, "idx_approvals_status"));

        // Triggers removed
        assert!(!trigger_exists(&db_path, "update_nodes_timestamp"));
        assert!(!trigger_exists(&db_path, "update_workspaces_timestamp"));
        assert!(!trigger_exists(&db_path, "update_agents_timestamp"));
        assert!(!trigger_exists(&db_path, "update_accounts_timestamp"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_006_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 6) {
            Some(migration) => migration,
            None => panic!("migration 006 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/006_mail_and_file_locks.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/006_mail_and_file_locks.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_006_up_down_creates_and_removes_mail_and_file_lock_schema() {
        let db_path = temp_db_path("migration-006");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(6) {
            panic!("migrate_to(6): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 6);

        assert!(table_exists(&db_path, "mail_threads"));
        assert!(table_exists(&db_path, "mail_messages"));
        assert!(table_exists(&db_path, "file_locks"));
        assert!(index_exists(&db_path, "idx_mail_threads_workspace_id"));
        assert!(index_exists(&db_path, "idx_mail_messages_thread_id"));
        assert!(index_exists(&db_path, "idx_mail_messages_recipient"));
        assert!(index_exists(&db_path, "idx_mail_messages_unread"));
        assert!(index_exists(&db_path, "idx_file_locks_active"));
        assert!(index_exists(&db_path, "idx_file_locks_path"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 5);

        assert!(!table_exists(&db_path, "mail_threads"));
        assert!(!table_exists(&db_path, "mail_messages"));
        assert!(!table_exists(&db_path, "file_locks"));
        assert!(!index_exists(&db_path, "idx_mail_threads_workspace_id"));
        assert!(!index_exists(&db_path, "idx_mail_messages_thread_id"));
        assert!(!index_exists(&db_path, "idx_mail_messages_recipient"));
        assert!(!index_exists(&db_path, "idx_mail_messages_unread"));
        assert!(!index_exists(&db_path, "idx_file_locks_active"));
        assert!(!index_exists(&db_path, "idx_file_locks_path"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_002_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 2) {
            Some(migration) => migration,
            None => panic!("migration 002 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/002_node_connection_prefs.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/002_node_connection_prefs.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_002_up_down_creates_and_removes_node_connection_prefs() {
        let db_path = temp_db_path("migration-002");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(2) {
            panic!("migrate_to(2): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 2);

        assert!(table_exists(&db_path, "nodes"));
        assert!(column_exists(&db_path, "nodes", "ssh_agent_forwarding"));
        assert!(column_exists(&db_path, "nodes", "ssh_proxy_jump"));
        assert!(column_exists(&db_path, "nodes", "ssh_control_master"));
        assert!(column_exists(&db_path, "nodes", "ssh_control_path"));
        assert!(column_exists(&db_path, "nodes", "ssh_control_persist"));
        assert!(column_exists(&db_path, "nodes", "ssh_timeout_seconds"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 1);

        assert!(table_exists(&db_path, "nodes"));
        assert!(!column_exists(&db_path, "nodes", "ssh_agent_forwarding"));
        assert!(!column_exists(&db_path, "nodes", "ssh_proxy_jump"));
        assert!(!column_exists(&db_path, "nodes", "ssh_control_master"));
        assert!(!column_exists(&db_path, "nodes", "ssh_control_path"));
        assert!(!column_exists(&db_path, "nodes", "ssh_control_persist"));
        assert!(!column_exists(&db_path, "nodes", "ssh_timeout_seconds"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_005_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 5) {
            Some(migration) => migration,
            None => panic!("migration 005 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/005_port_allocations.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/005_port_allocations.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_005_up_down_creates_and_removes_port_allocation_schema() {
        let db_path = temp_db_path("migration-005");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(5) {
            panic!("migrate_to(5): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 5);

        assert!(table_exists(&db_path, "port_allocations"));
        assert!(index_exists(&db_path, "idx_port_allocations_agent"));
        assert!(index_exists(&db_path, "idx_port_allocations_node"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 4);

        assert!(!table_exists(&db_path, "port_allocations"));
        assert!(!index_exists(&db_path, "idx_port_allocations_agent"));
        assert!(!index_exists(&db_path, "idx_port_allocations_node"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_003_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 3) {
            Some(migration) => migration,
            None => panic!("migration 003 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/003_queue_item_attempts.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/003_queue_item_attempts.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_003_up_down_creates_and_removes_attempts_column() {
        let db_path = temp_db_path("migration-003");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(3) {
            panic!("migrate_to(3): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 3);

        assert!(table_exists(&db_path, "queue_items"));
        assert!(column_exists(&db_path, "queue_items", "attempts"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 2);

        assert!(table_exists(&db_path, "queue_items"));
        assert!(!column_exists(&db_path, "queue_items", "attempts"));
        assert!(index_exists(&db_path, "idx_queue_items_agent_id"));
        assert!(index_exists(&db_path, "idx_queue_items_status"));
        assert!(index_exists(&db_path, "idx_queue_items_position"));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn migration_004_embedded_sql_matches_go_files() {
        let migration = match MIGRATIONS.iter().find(|m| m.version == 4) {
            Some(migration) => migration,
            None => panic!("migration 004 not embedded"),
        };

        let up = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/004_usage_history.up.sql"
        ));
        let down = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../internal/db/migrations/004_usage_history.down.sql"
        ));

        assert_eq!(migration.up_sql, up);
        assert_eq!(migration.down_sql, down);
    }

    #[test]
    fn migration_004_up_down_creates_and_removes_usage_schema() {
        let db_path = temp_db_path("migration-004");
        let mut db = match Db::open(Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };

        if let Err(err) = db.migrate_to(4) {
            panic!("migrate_to(4): {err}");
        }
        let version_after_up = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after up: {err}"),
        };
        assert_eq!(version_after_up, 4);

        assert!(table_exists(&db_path, "usage_records"));
        assert!(table_exists(&db_path, "daily_usage_cache"));
        assert!(index_exists(&db_path, "idx_usage_records_account_id"));
        assert!(index_exists(&db_path, "idx_usage_records_provider_time"));
        assert!(index_exists(&db_path, "idx_daily_usage_cache_provider"));

        let rolled_back = match db.migrate_down(1) {
            Ok(count) => count,
            Err(err) => panic!("migrate_down(1): {err}"),
        };
        assert_eq!(rolled_back, 1);

        let version_after_down = match db.schema_version() {
            Ok(version) => version,
            Err(err) => panic!("schema_version after down: {err}"),
        };
        assert_eq!(version_after_down, 3);

        assert!(!table_exists(&db_path, "usage_records"));
        assert!(!table_exists(&db_path, "daily_usage_cache"));
        assert!(!index_exists(&db_path, "idx_usage_records_account_id"));
        assert!(!index_exists(&db_path, "idx_usage_records_provider_time"));
        assert!(!index_exists(&db_path, "idx_daily_usage_cache_provider"));

        let _ = std::fs::remove_file(db_path);
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let mut path = std::env::temp_dir();
        path.push(format!(
            "forge-db-{tag}-{nanos}-{}.sqlite",
            std::process::id()
        ));
        path
    }

    fn table_exists(db_path: &Path, table: &str) -> bool {
        object_exists(db_path, "table", table)
    }

    fn index_exists(db_path: &Path, index: &str) -> bool {
        object_exists(db_path, "index", index)
    }

    fn trigger_exists(db_path: &Path, trigger: &str) -> bool {
        object_exists(db_path, "trigger", trigger)
    }

    fn column_exists(db_path: &Path, table: &str, column: &str) -> bool {
        let conn = match Connection::open(db_path) {
            Ok(conn) => conn,
            Err(err) => panic!("open sqlite connection {}: {err}", db_path.display()),
        };
        let sql = format!("PRAGMA table_info({})", table);
        let mut stmt = match conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(err) => panic!("prepare table_info for {table}: {err}"),
        };
        let rows = match stmt.query_map([], |row| row.get::<_, String>(1)) {
            Ok(rows) => rows,
            Err(err) => panic!("query table_info for {table}: {err}"),
        };
        for row in rows {
            let col_name = match row {
                Ok(name) => name,
                Err(err) => panic!("read column name: {err}"),
            };
            if col_name == column {
                return true;
            }
        }
        false
    }

    fn object_exists(db_path: &Path, object_type: &str, name: &str) -> bool {
        let conn = match Connection::open(db_path) {
            Ok(conn) => conn,
            Err(err) => panic!("open sqlite connection {}: {err}", db_path.display()),
        };
        let exists: i64 = match conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
            params![object_type, name],
            |row| row.get(0),
        ) {
            Ok(exists) => exists,
            Err(err) => panic!("sqlite_master lookup ({object_type}/{name}): {err}"),
        };
        exists == 1
    }
}

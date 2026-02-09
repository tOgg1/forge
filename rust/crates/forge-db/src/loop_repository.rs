//! Loop repository — CRUD for the `loops` table with full Go parity.

use std::collections::HashMap;

use rand::Rng;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Loop states matching the Go `LoopState` enum.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LoopState {
    Running,
    Sleeping,
    Waiting,
    #[default]
    Stopped,
    Error,
}

impl LoopState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Waiting => "waiting",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "running" => Ok(Self::Running),
            "sleeping" => Ok(Self::Sleeping),
            "waiting" => Ok(Self::Waiting),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            other => Err(DbError::Validation(format!("invalid loop state: {other}"))),
        }
    }
}

/// A background agent loop tied to a repo. Mirrors the Go `models.Loop` struct.
#[derive(Debug, Clone, Default)]
pub struct Loop {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo_path: String,
    pub base_prompt_path: String,
    pub base_prompt_msg: String,
    pub interval_seconds: i64,
    pub max_iterations: i64,
    pub max_runtime_seconds: i64,
    pub pool_id: String,
    pub profile_id: String,
    pub state: LoopState,
    pub last_run_at: Option<String>,
    pub last_exit_code: Option<i64>,
    pub last_error: String,
    pub log_path: String,
    pub ledger_path: String,
    pub tags: Vec<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Validation (mirrors Go models.Loop.Validate)
// ---------------------------------------------------------------------------

fn validate_loop(l: &Loop) -> Result<(), DbError> {
    let mut errors: Vec<String> = Vec::new();

    if l.name.is_empty() {
        errors.push("name: loop name is required".into());
    }
    if l.repo_path.is_empty() {
        errors.push("repo_path: loop repo path is required".into());
    }
    if !is_valid_loop_short_id(&l.short_id) {
        errors.push("short_id: loop short ID must be 6-9 alphanumeric characters".into());
    }
    if l.interval_seconds < 0 {
        errors.push("interval_seconds: interval_seconds must be >= 0".into());
    }
    if l.max_iterations < 0 {
        errors.push("max_iterations: max_iterations must be >= 0".into());
    }
    if l.max_runtime_seconds < 0 {
        errors.push("max_runtime_seconds: max_runtime_seconds must be >= 0".into());
    }

    if !errors.is_empty() {
        return Err(DbError::Validation(errors.join("; ")));
    }
    Ok(())
}

fn is_valid_loop_short_id(value: &str) -> bool {
    let len = value.len();
    if !(6..=9).contains(&len) {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase() || c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Short-ID generation (mirrors Go generateLoopShortID)
// ---------------------------------------------------------------------------

const LOOP_SHORT_ID_LENGTH: usize = 8;
const LOOP_SHORT_ID_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

fn generate_loop_short_id() -> String {
    let mut rng = rand::thread_rng();
    (0..LOOP_SHORT_ID_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..LOOP_SHORT_ID_ALPHABET.len());
            LOOP_SHORT_ID_ALPHABET[idx] as char
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now();
    let duration = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    format_epoch_as_rfc3339(duration.as_secs())
}

fn format_epoch_as_rfc3339(epoch_secs: u64) -> String {
    let secs_per_day: u64 = 86400;
    let days = epoch_secs / secs_per_day;
    let time_of_day = epoch_secs % secs_per_day;

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
    let msg = err.to_string();
    msg.contains("UNIQUE constraint failed")
}

// ---------------------------------------------------------------------------
// LoopRepository
// ---------------------------------------------------------------------------

pub struct LoopRepository<'a> {
    db: &'a Db,
}

impl<'a> LoopRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create adds a new loop to the database.
    pub fn create(&self, l: &mut Loop) -> Result<(), DbError> {
        if l.id.is_empty() {
            l.id = Uuid::new_v4().to_string();
        }
        self.ensure_short_id(l)?;

        validate_loop(l)?;

        let now = now_rfc3339();
        l.created_at = now.clone();
        l.updated_at = now;

        let tags_json: Option<String> = if l.tags.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&l.tags)
                    .map_err(|e| DbError::Validation(format!("failed to marshal tags: {e}")))?,
            )
        };

        let metadata_json: Option<String> = match &l.metadata {
            Some(m) => Some(
                serde_json::to_string(m)
                    .map_err(|e| DbError::Validation(format!("failed to marshal metadata: {e}")))?,
            ),
            None => None,
        };

        let result = self.db.conn().execute(
            "INSERT INTO loops (
                id, short_id, name, repo_path, base_prompt_path, base_prompt_msg,
                interval_seconds, max_iterations, max_runtime_seconds, pool_id, profile_id, state,
                last_run_at, last_exit_code, last_error,
                log_path, ledger_path, tags_json, metadata_json,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            params![
                l.id,
                l.short_id,
                l.name,
                l.repo_path,
                nullable_string(&l.base_prompt_path),
                nullable_string(&l.base_prompt_msg),
                l.interval_seconds,
                l.max_iterations,
                l.max_runtime_seconds,
                nullable_string(&l.pool_id),
                nullable_string(&l.profile_id),
                l.state.as_str(),
                l.last_run_at.as_deref(),
                l.last_exit_code,
                nullable_string(&l.last_error),
                nullable_string(&l.log_path),
                nullable_string(&l.ledger_path),
                tags_json,
                metadata_json,
                l.created_at,
                l.updated_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::LoopAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    /// Get retrieves a loop by primary ID.
    pub fn get(&self, id: &str) -> Result<Loop, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                id, short_id, name, repo_path, base_prompt_path, base_prompt_msg,
                interval_seconds, max_iterations, max_runtime_seconds, pool_id, profile_id, state,
                last_run_at, last_exit_code, last_error,
                log_path, ledger_path, tags_json, metadata_json,
                created_at, updated_at
            FROM loops WHERE id = ?1",
                params![id],
                |row| scan_loop(row),
            )
            .optional()?;

        result.ok_or(DbError::LoopNotFound)
    }

    /// GetByName retrieves a loop by unique name.
    pub fn get_by_name(&self, name: &str) -> Result<Loop, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                id, short_id, name, repo_path, base_prompt_path, base_prompt_msg,
                interval_seconds, max_iterations, max_runtime_seconds, pool_id, profile_id, state,
                last_run_at, last_exit_code, last_error,
                log_path, ledger_path, tags_json, metadata_json,
                created_at, updated_at
            FROM loops WHERE name = ?1",
                params![name],
                |row| scan_loop(row),
            )
            .optional()?;

        result.ok_or(DbError::LoopNotFound)
    }

    /// GetByShortID retrieves a loop by short ID.
    pub fn get_by_short_id(&self, short_id: &str) -> Result<Loop, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                id, short_id, name, repo_path, base_prompt_path, base_prompt_msg,
                interval_seconds, max_iterations, max_runtime_seconds, pool_id, profile_id, state,
                last_run_at, last_exit_code, last_error,
                log_path, ledger_path, tags_json, metadata_json,
                created_at, updated_at
            FROM loops WHERE short_id = ?1",
                params![short_id],
                |row| scan_loop(row),
            )
            .optional()?;

        result.ok_or(DbError::LoopNotFound)
    }

    /// List retrieves all loops ordered by created_at.
    pub fn list(&self) -> Result<Vec<Loop>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, short_id, name, repo_path, base_prompt_path, base_prompt_msg,
                interval_seconds, max_iterations, max_runtime_seconds, pool_id, profile_id, state,
                last_run_at, last_exit_code, last_error,
                log_path, ledger_path, tags_json, metadata_json,
                created_at, updated_at
            FROM loops
            ORDER BY created_at",
        )?;

        let rows = stmt.query_map([], |row| scan_loop(row))?;

        let mut loops = Vec::new();
        for row in rows {
            loops.push(row?);
        }
        Ok(loops)
    }

    /// Update updates a loop (by ID).
    pub fn update(&self, l: &mut Loop) -> Result<(), DbError> {
        self.ensure_short_id(l)?;
        validate_loop(l)?;

        l.updated_at = now_rfc3339();

        let tags_json: Option<String> = if l.tags.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&l.tags)
                    .map_err(|e| DbError::Validation(format!("failed to marshal tags: {e}")))?,
            )
        };

        let metadata_json: Option<String> = match &l.metadata {
            Some(m) => Some(
                serde_json::to_string(m)
                    .map_err(|e| DbError::Validation(format!("failed to marshal metadata: {e}")))?,
            ),
            None => None,
        };

        let rows_affected = self.db.conn().execute(
            "UPDATE loops
            SET short_id = ?1, name = ?2, repo_path = ?3, base_prompt_path = ?4, base_prompt_msg = ?5,
                interval_seconds = ?6, max_iterations = ?7, max_runtime_seconds = ?8, pool_id = ?9, profile_id = ?10, state = ?11,
                last_run_at = ?12, last_exit_code = ?13, last_error = ?14,
                log_path = ?15, ledger_path = ?16, tags_json = ?17, metadata_json = ?18,
                updated_at = ?19
            WHERE id = ?20",
            params![
                l.short_id,
                l.name,
                l.repo_path,
                nullable_string(&l.base_prompt_path),
                nullable_string(&l.base_prompt_msg),
                l.interval_seconds,
                l.max_iterations,
                l.max_runtime_seconds,
                nullable_string(&l.pool_id),
                nullable_string(&l.profile_id),
                l.state.as_str(),
                l.last_run_at.as_deref(),
                l.last_exit_code,
                nullable_string(&l.last_error),
                nullable_string(&l.log_path),
                nullable_string(&l.ledger_path),
                tags_json,
                metadata_json,
                l.updated_at,
                l.id,
            ],
        )?;

        if rows_affected == 0 {
            return Err(DbError::LoopNotFound);
        }
        Ok(())
    }

    /// Delete removes a loop by ID.
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let rows_affected = self
            .db
            .conn()
            .execute("DELETE FROM loops WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(DbError::LoopNotFound);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Short-ID helpers
    // -----------------------------------------------------------------------

    fn ensure_short_id(&self, l: &mut Loop) -> Result<(), DbError> {
        if !l.short_id.is_empty() {
            l.short_id = l.short_id.to_lowercase();
            return Ok(());
        }

        for _ in 0..10 {
            let candidate = generate_loop_short_id();
            if !self.short_id_exists(&candidate)? {
                l.short_id = candidate;
                return Ok(());
            }
        }

        Err(DbError::Validation(
            "failed to allocate unique loop short ID".into(),
        ))
    }

    fn short_id_exists(&self, short_id: &str) -> Result<bool, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(1) FROM loops WHERE short_id = ?1",
            params![short_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

// ---------------------------------------------------------------------------
// Row scanner (mirrors Go scanLoop)
// ---------------------------------------------------------------------------

fn scan_loop(row: &rusqlite::Row) -> rusqlite::Result<Loop> {
    let id: String = row.get(0)?;
    let short_id: Option<String> = row.get(1)?;
    let name: String = row.get(2)?;
    let repo_path: String = row.get(3)?;
    let base_prompt_path: Option<String> = row.get(4)?;
    let base_prompt_msg: Option<String> = row.get(5)?;
    let interval_seconds: i64 = row.get(6)?;
    let max_iterations: i64 = row.get(7)?;
    let max_runtime_seconds: i64 = row.get(8)?;
    let pool_id: Option<String> = row.get(9)?;
    let profile_id: Option<String> = row.get(10)?;
    let state_str: String = row.get(11)?;
    let last_run_at: Option<String> = row.get(12)?;
    let last_exit_code: Option<i64> = row.get(13)?;
    let last_error: Option<String> = row.get(14)?;
    let log_path: Option<String> = row.get(15)?;
    let ledger_path: Option<String> = row.get(16)?;
    let tags_json: Option<String> = row.get(17)?;
    let metadata_json: Option<String> = row.get(18)?;
    let created_at: String = row.get(19)?;
    let updated_at: String = row.get(20)?;

    let state = match LoopState::parse(&state_str) {
        Ok(s) => s,
        Err(_) => LoopState::default(),
    };

    let tags: Vec<String> = match tags_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).unwrap_or_default(),
        _ => Vec::new(),
    };

    let metadata: Option<HashMap<String, serde_json::Value>> = match metadata_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).ok(),
        _ => None,
    };

    Ok(Loop {
        id,
        short_id: short_id.unwrap_or_default(),
        name,
        repo_path,
        base_prompt_path: base_prompt_path.unwrap_or_default(),
        base_prompt_msg: base_prompt_msg.unwrap_or_default(),
        interval_seconds,
        max_iterations,
        max_runtime_seconds,
        pool_id: pool_id.unwrap_or_default(),
        profile_id: profile_id.unwrap_or_default(),
        state,
        last_run_at: last_run_at.filter(|s| !s.is_empty()),
        last_exit_code,
        last_error: last_error.unwrap_or_default(),
        log_path: log_path.unwrap_or_default(),
        ledger_path: ledger_path.unwrap_or_default(),
        tags,
        metadata,
        created_at,
        updated_at,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(tag: &str) -> PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        std::env::temp_dir().join(format!(
            "forge-db-loop-repo-{tag}-{nanos}-{}.sqlite",
            std::process::id()
        ))
    }

    fn open_migrated(tag: &str) -> (Db, PathBuf) {
        let path = temp_db_path(tag);
        let mut db = match Db::open(Config::new(&path)) {
            Ok(db) => db,
            Err(e) => panic!("open db: {e}"),
        };
        match db.migrate_up() {
            Ok(_) => {}
            Err(e) => panic!("migrate: {e}"),
        }
        (db, path)
    }

    fn sample_loop(name: &str) -> Loop {
        Loop {
            name: name.to_string(),
            repo_path: "/tmp/repo".to_string(),
            ..Loop::default()
        }
    }

    // -- Create tests -------------------------------------------------------

    #[test]
    fn create_assigns_id_and_short_id() {
        let (db, path) = open_migrated("create-ids");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("test-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        assert!(!l.id.is_empty(), "id should be generated");
        assert!(
            is_valid_loop_short_id(&l.short_id),
            "short_id should be valid: {}",
            l.short_id
        );
        assert!(!l.created_at.is_empty());
        assert!(!l.updated_at.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_preserves_given_id() {
        let (db, path) = open_migrated("create-given-id");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("given-id-loop");
        l.id = "my-custom-id".to_string();
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        assert_eq!(l.id, "my-custom-id");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_duplicate_name_returns_already_exists() {
        let (db, path) = open_migrated("create-dup");
        let repo = LoopRepository::new(&db);
        let mut l1 = sample_loop("dup-name");
        match repo.create(&mut l1) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let mut l2 = sample_loop("dup-name");
        let err = repo.create(&mut l2);
        assert!(
            matches!(err, Err(DbError::LoopAlreadyExists)),
            "expected LoopAlreadyExists, got {err:?}"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_validation_requires_name() {
        let (db, path) = open_migrated("create-no-name");
        let repo = LoopRepository::new(&db);
        let mut l = Loop {
            repo_path: "/tmp/repo".to_string(),
            ..Loop::default()
        };
        let err = repo.create(&mut l);
        assert!(matches!(err, Err(DbError::Validation(_))));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_validation_requires_repo_path() {
        let (db, path) = open_migrated("create-no-repo");
        let repo = LoopRepository::new(&db);
        let mut l = Loop {
            name: "no-repo".to_string(),
            ..Loop::default()
        };
        let err = repo.create(&mut l);
        assert!(matches!(err, Err(DbError::Validation(_))));

        let _ = std::fs::remove_file(path);
    }

    // -- Get tests ----------------------------------------------------------

    #[test]
    fn get_by_id() {
        let (db, path) = open_migrated("get-id");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("get-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let fetched = match repo.get(&l.id) {
            Ok(f) => f,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(fetched.id, l.id);
        assert_eq!(fetched.name, "get-loop");
        assert_eq!(fetched.state, LoopState::Stopped);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_not_found() {
        let (db, path) = open_migrated("get-404");
        let repo = LoopRepository::new(&db);
        let err = repo.get("nonexistent");
        assert!(
            matches!(err, Err(DbError::LoopNotFound)),
            "expected LoopNotFound, got {err:?}"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_by_name_works() {
        let (db, path) = open_migrated("get-name");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("named-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let fetched = match repo.get_by_name("named-loop") {
            Ok(f) => f,
            Err(e) => panic!("get_by_name: {e}"),
        };
        assert_eq!(fetched.id, l.id);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_by_name_not_found() {
        let (db, path) = open_migrated("get-name-404");
        let repo = LoopRepository::new(&db);
        let err = repo.get_by_name("nope");
        assert!(matches!(err, Err(DbError::LoopNotFound)));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_by_short_id_works() {
        let (db, path) = open_migrated("get-short");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("short-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let fetched = match repo.get_by_short_id(&l.short_id) {
            Ok(f) => f,
            Err(e) => panic!("get_by_short_id: {e}"),
        };
        assert_eq!(fetched.id, l.id);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_by_short_id_not_found() {
        let (db, path) = open_migrated("get-short-404");
        let repo = LoopRepository::new(&db);
        let err = repo.get_by_short_id("zzzzzzzz");
        assert!(matches!(err, Err(DbError::LoopNotFound)));

        let _ = std::fs::remove_file(path);
    }

    // -- List tests ---------------------------------------------------------

    #[test]
    fn list_empty() {
        let (db, path) = open_migrated("list-empty");
        let repo = LoopRepository::new(&db);
        let loops = match repo.list() {
            Ok(l) => l,
            Err(e) => panic!("list: {e}"),
        };
        assert!(loops.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn list_returns_ordered_by_created_at() {
        let (db, path) = open_migrated("list-order");
        let repo = LoopRepository::new(&db);

        let mut l1 = sample_loop("aaa");
        match repo.create(&mut l1) {
            Ok(()) => {}
            Err(e) => panic!("create l1: {e}"),
        }
        let mut l2 = sample_loop("bbb");
        match repo.create(&mut l2) {
            Ok(()) => {}
            Err(e) => panic!("create l2: {e}"),
        }

        let loops = match repo.list() {
            Ok(l) => l,
            Err(e) => panic!("list: {e}"),
        };
        assert_eq!(loops.len(), 2);
        assert_eq!(loops[0].name, "aaa");
        assert_eq!(loops[1].name, "bbb");

        let _ = std::fs::remove_file(path);
    }

    // -- Update tests -------------------------------------------------------

    #[test]
    fn update_changes_fields() {
        let (db, path) = open_migrated("update");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("upd-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        l.state = LoopState::Running;
        l.interval_seconds = 60;
        l.last_exit_code = Some(0);
        l.last_error = "oops".to_string();
        match repo.update(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("update: {e}"),
        }

        let fetched = match repo.get(&l.id) {
            Ok(f) => f,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(fetched.state, LoopState::Running);
        assert_eq!(fetched.interval_seconds, 60);
        assert_eq!(fetched.last_exit_code, Some(0));
        assert_eq!(fetched.last_error, "oops");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn update_not_found() {
        let (db, path) = open_migrated("update-404");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("phantom");
        l.id = "no-such-id".to_string();
        l.short_id = "abcd1234".to_string();
        let err = repo.update(&mut l);
        assert!(matches!(err, Err(DbError::LoopNotFound)));

        let _ = std::fs::remove_file(path);
    }

    // -- Delete tests -------------------------------------------------------

    #[test]
    fn delete_removes_loop() {
        let (db, path) = open_migrated("delete");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("del-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        match repo.delete(&l.id) {
            Ok(()) => {}
            Err(e) => panic!("delete: {e}"),
        }

        let err = repo.get(&l.id);
        assert!(matches!(err, Err(DbError::LoopNotFound)));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn delete_not_found() {
        let (db, path) = open_migrated("delete-404");
        let repo = LoopRepository::new(&db);
        let err = repo.delete("no-such-id");
        assert!(matches!(err, Err(DbError::LoopNotFound)));

        let _ = std::fs::remove_file(path);
    }

    // -- Serialization roundtrip tests --------------------------------------

    #[test]
    fn tags_and_metadata_roundtrip() {
        let (db, path) = open_migrated("tags-meta");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("tagged-loop");
        l.tags = vec!["alpha".into(), "beta".into()];
        let mut m = HashMap::new();
        m.insert("key".into(), serde_json::Value::String("val".into()));
        l.metadata = Some(m);
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let fetched = match repo.get(&l.id) {
            Ok(f) => f,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(fetched.tags, vec!["alpha", "beta"]);
        let meta = match fetched.metadata.as_ref() {
            Some(m) => m,
            None => panic!("metadata should be Some"),
        };
        assert_eq!(
            meta.get("key"),
            Some(&serde_json::Value::String("val".into()))
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn optional_fields_stored_as_null() {
        let (db, path) = open_migrated("nullable");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("null-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let fetched = match repo.get(&l.id) {
            Ok(f) => f,
            Err(e) => panic!("get: {e}"),
        };
        assert!(fetched.base_prompt_path.is_empty());
        assert!(fetched.pool_id.is_empty());
        assert!(fetched.profile_id.is_empty());
        assert!(fetched.last_run_at.is_none());
        assert!(fetched.last_exit_code.is_none());
        assert!(fetched.tags.is_empty());
        assert!(fetched.metadata.is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn state_roundtrip_all_variants() {
        let (db, path) = open_migrated("state-variants");
        let repo = LoopRepository::new(&db);

        for (i, state) in [
            LoopState::Running,
            LoopState::Sleeping,
            LoopState::Waiting,
            LoopState::Stopped,
            LoopState::Error,
        ]
        .iter()
        .enumerate()
        {
            let mut l = sample_loop(&format!("state-{i}"));
            l.state = state.clone();
            match repo.create(&mut l) {
                Ok(()) => {}
                Err(e) => panic!("create: {e}"),
            }

            let fetched = match repo.get(&l.id) {
                Ok(f) => f,
                Err(e) => panic!("get: {e}"),
            };
            assert_eq!(&fetched.state, state);
        }

        let _ = std::fs::remove_file(path);
    }

    // -- Short-ID tests -----------------------------------------------------

    #[test]
    fn short_id_generation_is_lowercase_alphanumeric() {
        for _ in 0..20 {
            let sid = generate_loop_short_id();
            assert_eq!(sid.len(), LOOP_SHORT_ID_LENGTH);
            assert!(sid
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn short_id_preserved_when_given() {
        let (db, path) = open_migrated("short-id-given");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("given-short");
        l.short_id = "ABCD1234".to_string();
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }
        // Should be lowercased
        assert_eq!(l.short_id, "abcd1234");

        let _ = std::fs::remove_file(path);
    }

    // -- Timestamp format tests ---------------------------------------------

    #[test]
    fn now_rfc3339_format() {
        let ts = now_rfc3339();
        assert!(ts.ends_with('Z'), "timestamp should end with Z: {ts}");
        assert_eq!(ts.len(), 20, "RFC3339 UTC timestamp length: {ts}");
    }

    #[test]
    fn format_epoch_as_rfc3339_known_value() {
        // 2024-01-01T00:00:00Z = 1704067200 seconds since epoch
        assert_eq!(format_epoch_as_rfc3339(1704067200), "2024-01-01T00:00:00Z");
        // Unix epoch
        assert_eq!(format_epoch_as_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    // -- Edge case: loop parse invalid state --------------------------------

    #[test]
    fn loop_state_parse_invalid() {
        let err = LoopState::parse("bogus");
        assert!(matches!(err, Err(DbError::Validation(_))));
    }

    #[test]
    fn loop_state_parse_all_valid() {
        assert_eq!(LoopState::parse("running").ok(), Some(LoopState::Running));
        assert_eq!(LoopState::parse("sleeping").ok(), Some(LoopState::Sleeping));
        assert_eq!(LoopState::parse("waiting").ok(), Some(LoopState::Waiting));
        assert_eq!(LoopState::parse("stopped").ok(), Some(LoopState::Stopped));
        assert_eq!(LoopState::parse("error").ok(), Some(LoopState::Error));
    }

    // -- FK cascade: deleting loop cascades to queue/kv/runs/work-state -----

    #[test]
    fn delete_cascades_to_child_tables() {
        let (db, path) = open_migrated("cascade");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("cascade-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        // Insert child rows directly to verify cascade
        let conn = db.conn();
        match conn.execute(
            "INSERT INTO loop_kv (id, loop_id, key, value) VALUES (?1, ?2, ?3, ?4)",
            params!["kv-1", l.id, "k", "v"],
        ) {
            Ok(_) => {}
            Err(e) => panic!("insert kv: {e}"),
        }

        match conn.execute(
            "INSERT INTO loop_runs (id, loop_id, status, started_at) VALUES (?1, ?2, ?3, ?4)",
            params!["run-1", l.id, "running", "2026-01-01T00:00:00Z"],
        ) {
            Ok(_) => {}
            Err(e) => panic!("insert run: {e}"),
        }

        match conn.execute(
            "INSERT INTO loop_queue_items (id, loop_id, type, position, status, payload_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "qi-1",
                l.id,
                "message_append",
                1,
                "pending",
                r#"{"text":"hi"}"#,
                "2026-01-01T00:00:00Z"
            ],
        ) {
            Ok(_) => {}
            Err(e) => panic!("insert queue item: {e}"),
        }

        match conn.execute(
            "INSERT INTO loop_work_state (id, loop_id, agent_id, task_id, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "ws-1",
                l.id,
                "agent-1",
                "task-1",
                "in_progress",
                "2026-01-01T00:00:00Z",
                "2026-01-01T00:00:00Z"
            ],
        ) {
            Ok(_) => {}
            Err(e) => panic!("insert work state: {e}"),
        }

        // Delete the loop
        match repo.delete(&l.id) {
            Ok(()) => {}
            Err(e) => panic!("delete: {e}"),
        }

        // Verify children are gone
        let kv_count: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM loop_kv WHERE loop_id = ?1",
            params![l.id],
            |row| row.get(0),
        ) {
            Ok(c) => c,
            Err(e) => panic!("count kv: {e}"),
        };
        assert_eq!(kv_count, 0, "kv rows should be cascaded");

        let run_count: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM loop_runs WHERE loop_id = ?1",
            params![l.id],
            |row| row.get(0),
        ) {
            Ok(c) => c,
            Err(e) => panic!("count runs: {e}"),
        };
        assert_eq!(run_count, 0, "run rows should be cascaded");

        let qi_count: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM loop_queue_items WHERE loop_id = ?1",
            params![l.id],
            |row| row.get(0),
        ) {
            Ok(c) => c,
            Err(e) => panic!("count queue items: {e}"),
        };
        assert_eq!(qi_count, 0, "queue item rows should be cascaded");

        let ws_count: i64 = match conn.query_row(
            "SELECT COUNT(1) FROM loop_work_state WHERE loop_id = ?1",
            params![l.id],
            |row| row.get(0),
        ) {
            Ok(c) => c,
            Err(e) => panic!("count work state: {e}"),
        };
        assert_eq!(ws_count, 0, "work state rows should be cascaded");

        let _ = std::fs::remove_file(path);
    }

    // -- Update timestamp refreshed on modify -------------------------------

    #[test]
    fn update_refreshes_updated_at() {
        let (db, path) = open_migrated("update-ts");
        let repo = LoopRepository::new(&db);
        let mut l = sample_loop("ts-loop");
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }
        let original_updated = l.updated_at.clone();

        // Sleep to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1100));

        l.state = LoopState::Running;
        match repo.update(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("update: {e}"),
        }

        assert_ne!(
            l.updated_at, original_updated,
            "updated_at should change after update"
        );

        let _ = std::fs::remove_file(path);
    }

    // -- All fields roundtrip -----------------------------------------------

    #[test]
    fn all_fields_roundtrip() {
        let (db, path) = open_migrated("full-roundtrip");
        let repo = LoopRepository::new(&db);
        let mut l = Loop {
            name: "full-loop".to_string(),
            repo_path: "/tmp/full".to_string(),
            base_prompt_path: "/tmp/prompt.md".to_string(),
            base_prompt_msg: "do the thing".to_string(),
            interval_seconds: 45,
            max_iterations: 10,
            max_runtime_seconds: 3600,
            pool_id: "pool-abc".to_string(),
            profile_id: "prof-xyz".to_string(),
            state: LoopState::Running,
            last_run_at: Some("2026-01-15T10:00:00Z".to_string()),
            last_exit_code: Some(1),
            last_error: "timeout".to_string(),
            log_path: "/var/log/loop.log".to_string(),
            ledger_path: "/var/log/loop.ledger".to_string(),
            tags: vec!["prod".into(), "gpu".into()],
            metadata: Some({
                let mut m = HashMap::new();
                m.insert("version".into(), serde_json::json!(2));
                m
            }),
            ..Loop::default()
        };
        match repo.create(&mut l) {
            Ok(()) => {}
            Err(e) => panic!("create: {e}"),
        }

        let f = match repo.get(&l.id) {
            Ok(f) => f,
            Err(e) => panic!("get: {e}"),
        };
        assert_eq!(f.name, "full-loop");
        assert_eq!(f.repo_path, "/tmp/full");
        assert_eq!(f.base_prompt_path, "/tmp/prompt.md");
        assert_eq!(f.base_prompt_msg, "do the thing");
        assert_eq!(f.interval_seconds, 45);
        assert_eq!(f.max_iterations, 10);
        assert_eq!(f.max_runtime_seconds, 3600);
        assert_eq!(f.pool_id, "pool-abc");
        assert_eq!(f.profile_id, "prof-xyz");
        assert_eq!(f.state, LoopState::Running);
        assert_eq!(f.last_run_at.as_deref(), Some("2026-01-15T10:00:00Z"));
        assert_eq!(f.last_exit_code, Some(1));
        assert_eq!(f.last_error, "timeout");
        assert_eq!(f.log_path, "/var/log/loop.log");
        assert_eq!(f.ledger_path, "/var/log/loop.ledger");
        assert_eq!(f.tags, vec!["prod", "gpu"]);
        let meta = match f.metadata.as_ref() {
            Some(m) => m,
            None => panic!("metadata should be Some"),
        };
        assert_eq!(meta.get("version"), Some(&serde_json::json!(2)));

        let _ = std::fs::remove_file(path);
    }
}

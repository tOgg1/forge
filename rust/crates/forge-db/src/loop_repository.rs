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

    pub fn parse_state(s: &str) -> Result<Self, DbError> {
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
    // Use the same format Go uses: RFC 3339 with second precision, UTC.
    // We avoid pulling in chrono by using a simple system-time approach.
    let duration = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => std::time::Duration::from_secs(0),
    };
    let secs = duration.as_secs();
    // Convert to broken-down time manually.
    format_epoch_as_rfc3339(secs)
}

fn format_epoch_as_rfc3339(epoch_secs: u64) -> String {
    // Days from epoch to year, month, day.
    let secs_per_day: u64 = 86400;
    let mut days = epoch_secs / secs_per_day;
    let time_of_day = epoch_secs % secs_per_day;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 to year/month/day.
    let mut year: u64 = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month: u64 = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn is_unique_constraint_error(err: &rusqlite::Error) -> bool {
    let msg = err.to_string();
    msg.contains("UNIQUE constraint failed")
}

// ---------------------------------------------------------------------------
// Repository errors
// ---------------------------------------------------------------------------

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
                scan_loop,
            )
            .optional()?;

        result.ok_or_else(|| DbError::LoopNotFound)
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
                scan_loop,
            )
            .optional()?;

        result.ok_or_else(|| DbError::LoopNotFound)
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
                scan_loop,
            )
            .optional()?;

        result.ok_or_else(|| DbError::LoopNotFound)
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

        let rows = stmt.query_map([], scan_loop)?;

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

    let state = LoopState::parse_state(&state_str).unwrap_or_default();

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

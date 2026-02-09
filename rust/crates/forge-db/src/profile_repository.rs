//! Profile repository — CRUD for the `profiles` table with full Go parity.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Harness identifies a loop harness implementation.
/// Mirrors Go `models.Harness`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Harness {
    Pi,
    OpenCode,
    Codex,
    Claude,
    Droid,
}

impl Harness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Droid => "droid",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "pi" => Ok(Self::Pi),
            "opencode" => Ok(Self::OpenCode),
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "droid" => Ok(Self::Droid),
            other => Err(DbError::Validation(format!("invalid harness: {other}"))),
        }
    }
}

/// PromptMode controls how prompts are delivered to a harness.
/// Mirrors Go `models.PromptMode`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PromptMode {
    #[default]
    Env,
    Stdin,
    Path,
}

impl PromptMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Stdin => "stdin",
            Self::Path => "path",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "env" => Ok(Self::Env),
            "stdin" => Ok(Self::Stdin),
            "path" => Ok(Self::Path),
            "" => Ok(Self::default()),
            other => Err(DbError::Validation(format!("invalid prompt_mode: {other}"))),
        }
    }
}

/// A harness+auth profile. Mirrors Go `models.Profile`.
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub harness: String,
    pub auth_kind: String,
    pub auth_home: String,
    pub prompt_mode: String,
    pub command_template: String,
    pub model: String,
    pub extra_args: Vec<String>,
    pub env: HashMap<String, String>,
    pub max_concurrency: i64,
    pub cooldown_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            harness: String::new(),
            auth_kind: String::new(),
            auth_home: String::new(),
            prompt_mode: "env".to_string(),
            command_template: String::new(),
            model: String::new(),
            extra_args: Vec::new(),
            env: HashMap::new(),
            max_concurrency: 1,
            cooldown_until: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation (mirrors Go models.Profile.Validate)
// ---------------------------------------------------------------------------

fn validate_profile(p: &Profile) -> Result<(), DbError> {
    let mut errors: Vec<String> = Vec::new();

    if p.name.is_empty() {
        errors.push("name: profile name is required".into());
    }
    if p.command_template.is_empty() {
        errors.push("command_template: command template is required".into());
    }
    if p.max_concurrency < 0 {
        errors.push("max_concurrency: max_concurrency must be >= 0".into());
    }

    if !errors.is_empty() {
        return Err(DbError::Validation(errors.join("; ")));
    }

    // Validate harness if non-empty (Go allows empty harness).
    if !p.harness.is_empty() {
        match p.harness.as_str() {
            "pi" | "opencode" | "codex" | "claude" | "droid" => {}
            other => {
                return Err(DbError::Validation(format!(
                    "profile harness is required: invalid harness {other}"
                )));
            }
        }
    }

    // Validate prompt_mode if non-empty.
    if !p.prompt_mode.is_empty() {
        match p.prompt_mode.as_str() {
            "env" | "stdin" | "path" => {}
            other => {
                return Err(DbError::Validation(format!("invalid prompt_mode: {other}")));
            }
        }
    }

    Ok(())
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
    let duration = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
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

// ---------------------------------------------------------------------------
// ProfileRepository
// ---------------------------------------------------------------------------

pub struct ProfileRepository<'a> {
    db: &'a Db,
}

impl<'a> ProfileRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create adds a new profile to the database.
    pub fn create(&self, p: &mut Profile) -> Result<(), DbError> {
        if p.id.is_empty() {
            p.id = Uuid::new_v4().to_string();
        }
        if p.prompt_mode.is_empty() {
            p.prompt_mode = "env".to_string();
        }

        validate_profile(p)?;

        let now = now_rfc3339();
        p.created_at = now.clone();
        p.updated_at = now;

        let extra_args_json: Option<String> =
            if p.extra_args.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&p.extra_args).map_err(|e| {
                    DbError::Validation(format!("failed to marshal extra args: {e}"))
                })?)
            };

        let env_json: Option<String> = if p.env.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&p.env)
                    .map_err(|e| DbError::Validation(format!("failed to marshal env: {e}")))?,
            )
        };

        let result = self.db.conn().execute(
            "INSERT INTO profiles (
                id, name, harness, auth_kind, auth_home,
                prompt_mode, command_template, model,
                extra_args_json, env_json, max_concurrency,
                cooldown_until, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                p.id,
                p.name,
                p.harness,
                nullable_string(&p.auth_kind),
                nullable_string(&p.auth_home),
                p.prompt_mode,
                p.command_template,
                nullable_string(&p.model),
                extra_args_json,
                env_json,
                p.max_concurrency,
                p.cooldown_until,
                p.created_at,
                p.updated_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::ProfileAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    /// Get retrieves a profile by ID.
    pub fn get(&self, id: &str) -> Result<Profile, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                    id, name, harness, auth_kind, auth_home,
                    prompt_mode, command_template, model,
                    extra_args_json, env_json, max_concurrency,
                    cooldown_until, created_at, updated_at
                FROM profiles WHERE id = ?1",
                params![id],
                scan_profile,
            )
            .optional()?;

        result.ok_or(DbError::ProfileNotFound)
    }

    /// GetByName retrieves a profile by name.
    pub fn get_by_name(&self, name: &str) -> Result<Profile, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                    id, name, harness, auth_kind, auth_home,
                    prompt_mode, command_template, model,
                    extra_args_json, env_json, max_concurrency,
                    cooldown_until, created_at, updated_at
                FROM profiles WHERE name = ?1",
                params![name],
                scan_profile,
            )
            .optional()?;

        result.ok_or(DbError::ProfileNotFound)
    }

    /// List retrieves all profiles ordered by name.
    pub fn list(&self) -> Result<Vec<Profile>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, name, harness, auth_kind, auth_home,
                prompt_mode, command_template, model,
                extra_args_json, env_json, max_concurrency,
                cooldown_until, created_at, updated_at
            FROM profiles
            ORDER BY name",
        )?;

        let rows = stmt.query_map([], scan_profile)?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row?);
        }
        Ok(profiles)
    }

    /// Update updates a profile.
    pub fn update(&self, p: &mut Profile) -> Result<(), DbError> {
        validate_profile(p)?;

        p.updated_at = now_rfc3339();

        let extra_args_json: Option<String> =
            if p.extra_args.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&p.extra_args).map_err(|e| {
                    DbError::Validation(format!("failed to marshal extra args: {e}"))
                })?)
            };

        let env_json: Option<String> = if p.env.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&p.env)
                    .map_err(|e| DbError::Validation(format!("failed to marshal env: {e}")))?,
            )
        };

        let rows_affected = self.db.conn().execute(
            "UPDATE profiles
            SET name = ?1, harness = ?2, auth_kind = ?3, auth_home = ?4,
                prompt_mode = ?5, command_template = ?6, model = ?7,
                extra_args_json = ?8, env_json = ?9, max_concurrency = ?10,
                cooldown_until = ?11, updated_at = ?12
            WHERE id = ?13",
            params![
                p.name,
                p.harness,
                nullable_string(&p.auth_kind),
                nullable_string(&p.auth_home),
                p.prompt_mode,
                p.command_template,
                nullable_string(&p.model),
                extra_args_json,
                env_json,
                p.max_concurrency,
                p.cooldown_until,
                p.updated_at,
                p.id,
            ],
        )?;

        if rows_affected == 0 {
            return Err(DbError::ProfileNotFound);
        }
        Ok(())
    }

    /// Delete removes a profile by ID.
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let rows_affected = self
            .db
            .conn()
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(DbError::ProfileNotFound);
        }
        Ok(())
    }

    /// SetCooldown sets cooldown_until for a profile.
    pub fn set_cooldown(&self, id: &str, until: Option<&str>) -> Result<(), DbError> {
        let updated_at = now_rfc3339();

        let rows_affected = self.db.conn().execute(
            "UPDATE profiles
            SET cooldown_until = ?1, updated_at = ?2
            WHERE id = ?3",
            params![until, updated_at, id],
        )?;

        if rows_affected == 0 {
            return Err(DbError::ProfileNotFound);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Row scanner (mirrors Go scanProfile)
// ---------------------------------------------------------------------------

fn scan_profile(row: &rusqlite::Row) -> rusqlite::Result<Profile> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let harness: String = row.get(2)?;
    let auth_kind: Option<String> = row.get(3)?;
    let auth_home: Option<String> = row.get(4)?;
    let prompt_mode: String = row.get(5)?;
    let command_template: String = row.get(6)?;
    let model: Option<String> = row.get(7)?;
    let extra_args_json: Option<String> = row.get(8)?;
    let env_json: Option<String> = row.get(9)?;
    let max_concurrency: i64 = row.get(10)?;
    let cooldown_until: Option<String> = row.get(11)?;
    let created_at: String = row.get(12)?;
    let updated_at: String = row.get(13)?;

    let extra_args: Vec<String> = match extra_args_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).unwrap_or_default(),
        _ => Vec::new(),
    };

    let env: HashMap<String, String> = match env_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).unwrap_or_default(),
        _ => HashMap::new(),
    };

    Ok(Profile {
        id,
        name,
        harness,
        auth_kind: auth_kind.unwrap_or_default(),
        auth_home: auth_home.unwrap_or_default(),
        prompt_mode,
        command_template,
        model: model.unwrap_or_default(),
        extra_args,
        env,
        max_concurrency,
        cooldown_until: cooldown_until.filter(|s| !s.is_empty()),
        created_at,
        updated_at,
    })
}

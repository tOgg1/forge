//! Pool repository — CRUD for the `pools` and `pool_members` tables with full Go parity.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Pool selection strategy. Mirrors Go `models.PoolStrategy`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PoolStrategy {
    #[default]
    RoundRobin,
    LRU,
}

impl PoolStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::LRU => "lru",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "round_robin" => Ok(Self::RoundRobin),
            "lru" => Ok(Self::LRU),
            other => Err(DbError::Validation(format!(
                "invalid pool strategy: {other}"
            ))),
        }
    }
}

/// A profile pool with selection strategy. Mirrors Go `models.Pool`.
#[derive(Debug, Clone, Default)]
pub struct Pool {
    pub id: String,
    pub name: String,
    pub strategy: String,
    pub is_default: bool,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: String,
    pub updated_at: String,
}

/// A pool membership linking a profile to a pool. Mirrors Go `models.PoolMember`.
#[derive(Debug, Clone, Default)]
pub struct PoolMember {
    pub id: String,
    pub pool_id: String,
    pub profile_id: String,
    pub weight: i64,
    pub position: i64,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Validation (mirrors Go models.Pool.Validate)
// ---------------------------------------------------------------------------

fn validate_pool(p: &Pool) -> Result<(), DbError> {
    if p.name.is_empty() {
        return Err(DbError::Validation(
            "name: pool name is required".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
// PoolRepository
// ---------------------------------------------------------------------------

pub struct PoolRepository<'a> {
    db: &'a Db,
}

impl<'a> PoolRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create adds a new pool to the database.
    pub fn create(&self, p: &mut Pool) -> Result<(), DbError> {
        validate_pool(p)?;

        if p.id.is_empty() {
            p.id = Uuid::new_v4().to_string();
        }

        let now = now_rfc3339();
        p.created_at = now.clone();
        p.updated_at = now;

        let metadata_json: Option<String> = match &p.metadata {
            Some(m) => Some(
                serde_json::to_string(m)
                    .map_err(|e| DbError::Validation(format!("failed to marshal metadata: {e}")))?,
            ),
            None => None,
        };

        let is_default: i64 = if p.is_default { 1 } else { 0 };

        let strategy = if p.strategy.is_empty() {
            "round_robin"
        } else {
            &p.strategy
        };

        let result = self.db.conn().execute(
            "INSERT INTO pools (id, name, strategy, is_default, metadata_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                p.id,
                p.name,
                strategy,
                is_default,
                metadata_json,
                p.created_at,
                p.updated_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::PoolAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    /// Get retrieves a pool by ID.
    pub fn get(&self, id: &str) -> Result<Pool, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, name, strategy, is_default, metadata_json, created_at, updated_at
                FROM pools WHERE id = ?1",
                params![id],
                scan_pool,
            )
            .optional()?;

        result.ok_or(DbError::PoolNotFound)
    }

    /// GetByName retrieves a pool by name.
    pub fn get_by_name(&self, name: &str) -> Result<Pool, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, name, strategy, is_default, metadata_json, created_at, updated_at
                FROM pools WHERE name = ?1",
                params![name],
                scan_pool,
            )
            .optional()?;

        result.ok_or(DbError::PoolNotFound)
    }

    /// GetDefault retrieves the default pool.
    pub fn get_default(&self) -> Result<Pool, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, name, strategy, is_default, metadata_json, created_at, updated_at
                FROM pools WHERE is_default = 1
                ORDER BY created_at
                LIMIT 1",
                [],
                scan_pool,
            )
            .optional()?;

        result.ok_or(DbError::PoolNotFound)
    }

    /// List retrieves all pools ordered by name.
    pub fn list(&self) -> Result<Vec<Pool>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, name, strategy, is_default, metadata_json, created_at, updated_at
            FROM pools
            ORDER BY name",
        )?;

        let rows = stmt.query_map([], scan_pool)?;

        let mut pools = Vec::new();
        for row in rows {
            pools.push(row?);
        }
        Ok(pools)
    }

    /// Update updates a pool.
    pub fn update(&self, p: &mut Pool) -> Result<(), DbError> {
        validate_pool(p)?;

        p.updated_at = now_rfc3339();

        let metadata_json: Option<String> = match &p.metadata {
            Some(m) => Some(
                serde_json::to_string(m)
                    .map_err(|e| DbError::Validation(format!("failed to marshal metadata: {e}")))?,
            ),
            None => None,
        };

        let is_default: i64 = if p.is_default { 1 } else { 0 };

        let rows_affected = self.db.conn().execute(
            "UPDATE pools
            SET name = ?1, strategy = ?2, is_default = ?3, metadata_json = ?4, updated_at = ?5
            WHERE id = ?6",
            params![
                p.name,
                p.strategy,
                is_default,
                metadata_json,
                p.updated_at,
                p.id,
            ],
        )?;

        if rows_affected == 0 {
            return Err(DbError::PoolNotFound);
        }
        Ok(())
    }

    /// Delete removes a pool by ID.
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let rows_affected = self
            .db
            .conn()
            .execute("DELETE FROM pools WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(DbError::PoolNotFound);
        }
        Ok(())
    }

    /// SetDefault marks a pool as default and clears other defaults.
    pub fn set_default(&self, id: &str) -> Result<(), DbError> {
        self.db
            .conn()
            .execute("UPDATE pools SET is_default = 0", [])?;

        let rows_affected = self
            .db
            .conn()
            .execute("UPDATE pools SET is_default = 1 WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(DbError::PoolNotFound);
        }
        Ok(())
    }

    /// AddMember adds a profile to a pool.
    pub fn add_member(&self, m: &mut PoolMember) -> Result<(), DbError> {
        if m.id.is_empty() {
            m.id = Uuid::new_v4().to_string();
        }
        if m.weight == 0 {
            m.weight = 1;
        }
        if m.created_at.is_empty() {
            m.created_at = now_rfc3339();
        }

        let result = self.db.conn().execute(
            "INSERT INTO pool_members (id, pool_id, profile_id, weight, position, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                m.id,
                m.pool_id,
                m.profile_id,
                m.weight,
                m.position,
                m.created_at,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_constraint_error(&err) {
                    Err(DbError::PoolAlreadyExists)
                } else {
                    Err(DbError::Open(err))
                }
            }
        }
    }

    /// RemoveMember removes a profile from a pool.
    pub fn remove_member(&self, pool_id: &str, profile_id: &str) -> Result<(), DbError> {
        let rows_affected = self.db.conn().execute(
            "DELETE FROM pool_members WHERE pool_id = ?1 AND profile_id = ?2",
            params![pool_id, profile_id],
        )?;

        if rows_affected == 0 {
            return Err(DbError::PoolNotFound);
        }
        Ok(())
    }

    /// ListMembers returns members for a pool ordered by position then created_at.
    pub fn list_members(&self, pool_id: &str) -> Result<Vec<PoolMember>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, pool_id, profile_id, weight, position, created_at
            FROM pool_members
            WHERE pool_id = ?1
            ORDER BY position, created_at",
        )?;

        let rows = stmt.query_map(params![pool_id], scan_pool_member)?;

        let mut members = Vec::new();
        for row in rows {
            members.push(row?);
        }
        Ok(members)
    }
}

// ---------------------------------------------------------------------------
// Row scanners (mirrors Go scanPool / scanPoolMember)
// ---------------------------------------------------------------------------

fn scan_pool(row: &rusqlite::Row) -> rusqlite::Result<Pool> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let strategy: String = row.get(2)?;
    let is_default: i64 = row.get(3)?;
    let metadata_json: Option<String> = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    let metadata: Option<HashMap<String, serde_json::Value>> = match metadata_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).ok(),
        _ => None,
    };

    Ok(Pool {
        id,
        name,
        strategy,
        is_default: is_default == 1,
        metadata,
        created_at,
        updated_at,
    })
}

fn scan_pool_member(row: &rusqlite::Row) -> rusqlite::Result<PoolMember> {
    let id: String = row.get(0)?;
    let pool_id: String = row.get(1)?;
    let profile_id: String = row.get(2)?;
    let weight: i64 = row.get(3)?;
    let position: i64 = row.get(4)?;
    let created_at: String = row.get(5)?;

    Ok(PoolMember {
        id,
        pool_id,
        profile_id,
        weight,
        position,
        created_at,
    })
}

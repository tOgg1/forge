//! Loop work state repository — CRUD-ish for the `loop_work_state` table with Go parity.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Task-tech-agnostic "what am I working on" pointer per loop.
/// Mirrors Go `models.LoopWorkState` (timestamps are RFC3339 strings).
#[derive(Debug, Clone, Default)]
pub struct LoopWorkState {
    pub id: String,
    pub loop_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub status: String,
    pub detail: String,
    pub loop_iteration: i64,
    pub is_current: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn validate_state(s: &LoopWorkState) -> Result<(), DbError> {
    let mut errors: Vec<&'static str> = Vec::new();
    if s.loop_id.trim().is_empty() {
        errors.push("loop_id is required");
    }
    if s.agent_id.trim().is_empty() {
        errors.push("agent_id is required");
    }
    if s.task_id.trim().is_empty() {
        errors.push("task_id is required");
    }
    if s.status.trim().is_empty() {
        errors.push("status is required");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DbError::Validation(format!(
            "invalid loop work state: {}",
            errors.join("; ")
        )))
    }
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

fn scan_state(row: &rusqlite::Row<'_>) -> Result<LoopWorkState, rusqlite::Error> {
    let is_current_int: i64 = row.get(7)?;
    Ok(LoopWorkState {
        id: row.get(0)?,
        loop_id: row.get(1)?,
        agent_id: row.get(2)?,
        task_id: row.get(3)?,
        status: row.get(4)?,
        detail: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        loop_iteration: row.get(6)?,
        is_current: is_current_int == 1,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

pub struct LoopWorkStateRepository<'a> {
    db: &'a mut Db,
}

impl<'a> LoopWorkStateRepository<'a> {
    pub fn new(db: &'a mut Db) -> Self {
        Self { db }
    }

    /// SetCurrent upserts `(loop_id, task_id)` and marks it as current.
    /// Clears current marker from other tasks for the loop.
    ///
    /// Mirrors Go `LoopWorkStateRepository.SetCurrent`.
    pub fn set_current(&mut self, state: &mut LoopWorkState) -> Result<(), DbError> {
        state.loop_id = state.loop_id.trim().to_string();
        state.agent_id = state.agent_id.trim().to_string();
        state.task_id = state.task_id.trim().to_string();
        state.status = state.status.trim().to_string();
        if state.status.is_empty() {
            state.status = "in_progress".to_string();
        }
        validate_state(state)?;

        let now = now_rfc3339();

        self.db.transaction(|tx| {
            tx.execute(
                "UPDATE loop_work_state
                 SET is_current = 0
                 WHERE loop_id = ?1 AND is_current = 1",
                params![state.loop_id],
            )
            .map_err(|e| DbError::Transaction(format!("failed to clear current loop work state: {e}")))?;

            let updated = tx
                .execute(
                    "UPDATE loop_work_state
                     SET agent_id = ?1, status = ?2, detail = ?3, loop_iteration = ?4, is_current = 1
                     WHERE loop_id = ?5 AND task_id = ?6",
                    params![
                        state.agent_id,
                        state.status,
                        nullable_string(&state.detail),
                        state.loop_iteration,
                        state.loop_id,
                        state.task_id
                    ],
                )
                .map_err(|e| DbError::Transaction(format!("failed to update loop work state: {e}")))?;

            if updated > 0 {
                state.is_current = true;
                state.updated_at = now.clone();
                return Ok(());
            }

            if state.id.is_empty() {
                state.id = Uuid::new_v4().to_string();
            }
            state.created_at = now.clone();
            state.updated_at = now.clone();
            state.is_current = true;

            let insert_res = tx.execute(
                "INSERT INTO loop_work_state (
                    id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current,
                    created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    state.id,
                    state.loop_id,
                    state.agent_id,
                    state.task_id,
                    state.status,
                    nullable_string(&state.detail),
                    state.loop_iteration,
                    1i64,
                    state.created_at,
                    state.updated_at
                ],
            );

            match insert_res {
                Ok(_) => Ok(()),
                Err(e) => {
                    if is_unique_constraint_error(&e) {
                        let _ = tx.execute(
                            "UPDATE loop_work_state
                             SET agent_id = ?1, status = ?2, detail = ?3, loop_iteration = ?4, is_current = 1
                             WHERE loop_id = ?5 AND task_id = ?6",
                            params![
                                state.agent_id,
                                state.status,
                                nullable_string(&state.detail),
                                state.loop_iteration,
                                state.loop_id,
                                state.task_id
                            ],
                        );
                        return Ok(());
                    }
                    Err(DbError::Transaction(format!(
                        "failed to insert loop work state: {e}"
                    )))
                }
            }
        })
    }

    pub fn clear_current(&mut self, loop_id: &str) -> Result<(), DbError> {
        let loop_id = loop_id.trim();
        if loop_id.is_empty() {
            return Err(DbError::Validation("loop_id is required".into()));
        }
        self.db.conn().execute(
            "UPDATE loop_work_state
             SET is_current = 0
             WHERE loop_id = ?1 AND is_current = 1",
            params![loop_id],
        )?;
        Ok(())
    }

    pub fn get_current(&mut self, loop_id: &str) -> Result<LoopWorkState, DbError> {
        let loop_id = loop_id.trim();
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current, created_at, updated_at
                 FROM loop_work_state
                 WHERE loop_id = ?1 AND is_current = 1
                 ORDER BY updated_at DESC, id DESC
                 LIMIT 1",
                params![loop_id],
                scan_state,
            )
            .optional()?;

        result.ok_or(DbError::LoopWorkStateNotFound)
    }

    pub fn list_by_loop(
        &mut self,
        loop_id: &str,
        mut limit: i64,
    ) -> Result<Vec<LoopWorkState>, DbError> {
        if limit <= 0 {
            limit = 200;
        }
        let mut stmt = self.db.conn().prepare(
            "SELECT id, loop_id, agent_id, task_id, status, detail, loop_iteration, is_current, created_at, updated_at
             FROM loop_work_state
             WHERE loop_id = ?1
             ORDER BY is_current DESC, updated_at DESC, id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![loop_id.trim(), limit], scan_state)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

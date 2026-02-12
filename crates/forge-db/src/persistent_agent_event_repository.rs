//! Persistent agent event repository — append-only audit log for agent operations.

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentAgentEvent {
    pub id: i64,
    pub agent_id: Option<String>,
    pub kind: String,
    pub outcome: String,
    pub detail: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default)]
pub struct PersistentAgentEventQuery {
    pub agent_id: Option<String>,
    pub kind: Option<String>,
    pub since: Option<String>,
    pub limit: i64,
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

pub struct PersistentAgentEventRepository<'a> {
    db: &'a Db,
}

fn now_rfc3339() -> String {
    crate::now_rfc3339()
}

fn scan_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistentAgentEvent> {
    Ok(PersistentAgentEvent {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        kind: row.get(2)?,
        outcome: row.get(3)?,
        detail: row.get(4)?,
        timestamp: row.get(5)?,
    })
}

const SELECT_COLS: &str = "id, agent_id, kind, outcome, detail, timestamp";

impl<'a> PersistentAgentEventRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Append an event. The `id` field is auto-assigned by AUTOINCREMENT.
    pub fn append(&self, event: &mut PersistentAgentEvent) -> Result<(), DbError> {
        if event.kind.trim().is_empty() {
            return Err(DbError::Validation("event kind is required".into()));
        }
        if event.outcome.trim().is_empty() {
            return Err(DbError::Validation("event outcome is required".into()));
        }

        if event.timestamp.is_empty() {
            event.timestamp = now_rfc3339();
        }

        let rowid: i64 = self.db.conn().query_row(
            "INSERT INTO persistent_agent_events (agent_id, kind, outcome, detail, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
            params![
                event.agent_id,
                event.kind,
                event.outcome,
                event.detail,
                event.timestamp,
            ],
            |row| row.get(0),
        )?;
        event.id = rowid;
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<PersistentAgentEvent, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM persistent_agent_events WHERE id = ?1"),
                params![id],
                scan_row,
            )
            .optional()?;
        row.ok_or_else(|| DbError::Validation("persistent agent event not found".into()))
    }

    pub fn list_by_agent(
        &self,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<PersistentAgentEvent>, DbError> {
        let limit = if limit <= 0 { 100 } else { limit };
        let mut stmt = self.db.conn().prepare(&format!(
            "SELECT {SELECT_COLS} FROM persistent_agent_events
             WHERE agent_id = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![agent_id, limit], scan_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn query(
        &self,
        q: PersistentAgentEventQuery,
    ) -> Result<Vec<PersistentAgentEvent>, DbError> {
        let limit = if q.limit <= 0 { 100 } else { q.limit };
        let mut query = format!("SELECT {SELECT_COLS} FROM persistent_agent_events WHERE 1=1");
        let mut args: Vec<Value> = Vec::new();

        if let Some(agent_id) = q.agent_id {
            query.push_str(" AND agent_id = ?");
            args.push(Value::from(agent_id));
        }
        if let Some(kind) = q.kind {
            query.push_str(" AND kind = ?");
            args.push(Value::from(kind));
        }
        if let Some(since) = q.since {
            query.push_str(" AND timestamp >= ?");
            args.push(Value::from(since));
        }

        query.push_str(" ORDER BY timestamp DESC, id DESC LIMIT ?");
        args.push(Value::from(limit));

        let mut stmt = self.db.conn().prepare(&query)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), scan_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn count(&self) -> Result<i64, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM persistent_agent_events",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_by_agent(&self, agent_id: &str) -> Result<i64, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM persistent_agent_events WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

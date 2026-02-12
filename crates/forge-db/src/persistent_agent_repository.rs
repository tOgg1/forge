//! Persistent agent repository — CRUD + list/filter for the `persistent_agents` table.

use std::collections::HashMap;

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentAgent {
    pub id: String,
    pub parent_agent_id: Option<String>,
    pub workspace_id: String,
    pub repo: Option<String>,
    pub node: Option<String>,
    pub harness: String,
    pub mode: String,
    pub state: String,
    pub ttl_seconds: Option<i64>,
    pub labels: HashMap<String, String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub last_activity_at: String,
    pub updated_at: String,
}

impl Default for PersistentAgent {
    fn default() -> Self {
        Self {
            id: String::new(),
            parent_agent_id: None,
            workspace_id: String::new(),
            repo: None,
            node: None,
            harness: String::new(),
            mode: "continuous".to_string(),
            state: "starting".to_string(),
            ttl_seconds: None,
            labels: HashMap::new(),
            tags: Vec::new(),
            created_at: String::new(),
            last_activity_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PersistentAgentFilter {
    pub workspace_id: Option<String>,
    pub parent_agent_id: Option<String>,
    pub states: Vec<String>,
    pub harness: Option<String>,
    pub limit: i64,
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

pub struct PersistentAgentRepository<'a> {
    db: &'a Db,
}

fn now_rfc3339() -> String {
    crate::now_rfc3339()
}

fn scan_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistentAgent> {
    let labels_json: Option<String> = row.get(9)?;
    let tags_json: Option<String> = row.get(10)?;

    let labels: HashMap<String, String> = match labels_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).unwrap_or_default(),
        _ => HashMap::new(),
    };
    let tags: Vec<String> = match tags_json {
        Some(ref s) if !s.is_empty() => serde_json::from_str(s).unwrap_or_default(),
        _ => Vec::new(),
    };

    Ok(PersistentAgent {
        id: row.get(0)?,
        parent_agent_id: row.get(1)?,
        workspace_id: row.get(2)?,
        repo: row.get(3)?,
        node: row.get(4)?,
        harness: row.get(5)?,
        mode: row.get(6)?,
        state: row.get(7)?,
        ttl_seconds: row.get(8)?,
        labels,
        tags,
        created_at: row.get(11)?,
        last_activity_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const SELECT_COLS: &str = "id, parent_agent_id, workspace_id, repo, node, harness, mode, state, \
                           ttl_seconds, labels_json, tags_json, created_at, last_activity_at, updated_at";

impl<'a> PersistentAgentRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn create(&self, agent: &mut PersistentAgent) -> Result<(), DbError> {
        if agent.workspace_id.trim().is_empty() {
            return Err(DbError::Validation("workspace_id is required".into()));
        }
        if agent.harness.trim().is_empty() {
            return Err(DbError::Validation("harness is required".into()));
        }

        if agent.id.is_empty() {
            agent.id = Uuid::new_v4().to_string();
        }
        let now = now_rfc3339();
        if agent.created_at.is_empty() {
            agent.created_at = now.clone();
        }
        if agent.last_activity_at.is_empty() {
            agent.last_activity_at = now.clone();
        }
        if agent.updated_at.is_empty() {
            agent.updated_at = now;
        }

        let labels_json = if agent.labels.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&agent.labels)
                    .map_err(|e| DbError::Validation(format!("failed to serialize labels: {e}")))?,
            )
        };
        let tags_json = if agent.tags.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&agent.tags)
                    .map_err(|e| DbError::Validation(format!("failed to serialize tags: {e}")))?,
            )
        };

        self.db.conn().execute(
            "INSERT INTO persistent_agents (
                id, parent_agent_id, workspace_id, repo, node, harness, mode, state,
                ttl_seconds, labels_json, tags_json, created_at, last_activity_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                agent.id,
                agent.parent_agent_id,
                agent.workspace_id,
                agent.repo,
                agent.node,
                agent.harness,
                agent.mode,
                agent.state,
                agent.ttl_seconds,
                labels_json,
                tags_json,
                agent.created_at,
                agent.last_activity_at,
                agent.updated_at,
            ],
        )?;

        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<PersistentAgent, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                &format!("SELECT {SELECT_COLS} FROM persistent_agents WHERE id = ?1"),
                params![id],
                scan_row,
            )
            .optional()?;
        row.ok_or_else(|| DbError::Validation("persistent agent not found".into()))
    }

    pub fn update_state(&self, id: &str, state: &str) -> Result<(), DbError> {
        let now = now_rfc3339();
        let rows = self.db.conn().execute(
            "UPDATE persistent_agents SET state = ?1, last_activity_at = ?2 WHERE id = ?3",
            params![state, now, id],
        )?;
        if rows == 0 {
            return Err(DbError::Validation("persistent agent not found".into()));
        }
        Ok(())
    }

    pub fn touch_activity(&self, id: &str) -> Result<(), DbError> {
        let now = now_rfc3339();
        let rows = self.db.conn().execute(
            "UPDATE persistent_agents SET last_activity_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if rows == 0 {
            return Err(DbError::Validation("persistent agent not found".into()));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let rows = self
            .db
            .conn()
            .execute("DELETE FROM persistent_agents WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(DbError::Validation("persistent agent not found".into()));
        }
        Ok(())
    }

    pub fn list(&self, filter: PersistentAgentFilter) -> Result<Vec<PersistentAgent>, DbError> {
        let limit = if filter.limit <= 0 { 100 } else { filter.limit };
        let mut query = format!("SELECT {SELECT_COLS} FROM persistent_agents WHERE 1=1");
        let mut args: Vec<Value> = Vec::new();

        if let Some(ws) = filter.workspace_id {
            query.push_str(" AND workspace_id = ?");
            args.push(Value::from(ws));
        }
        if let Some(parent) = filter.parent_agent_id {
            query.push_str(" AND parent_agent_id = ?");
            args.push(Value::from(parent));
        }
        if let Some(harness) = filter.harness {
            query.push_str(" AND harness = ?");
            args.push(Value::from(harness));
        }
        if !filter.states.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(filter.states.len())
                .collect::<Vec<_>>()
                .join(",");
            query.push_str(&format!(" AND state IN ({placeholders})"));
            for s in &filter.states {
                args.push(Value::from(s.clone()));
            }
        }

        query.push_str(" ORDER BY updated_at DESC LIMIT ?");
        args.push(Value::from(limit));

        let mut stmt = self.db.conn().prepare(&query)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), scan_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn count_by_state(&self, state: &str) -> Result<i64, DbError> {
        let count: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM persistent_agents WHERE state = ?1",
            params![state],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

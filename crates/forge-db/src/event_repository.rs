//! Event repository — persistence for append-only `events` audit log.

use std::collections::HashMap;

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: String,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventQuery {
    pub event_type: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: String,
    pub limit: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPage {
    pub events: Vec<Event>,
    pub next_cursor: String,
}

fn now_rfc3339() -> String {
    crate::now_rfc3339()
}

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn scan_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let payload_json: Option<String> = row.get(5)?;
    let metadata_json: Option<String> = row.get(6)?;
    let metadata = match metadata_json {
        Some(value) => serde_json::from_str::<HashMap<String, String>>(&value).ok(),
        None => None,
    };

    Ok(Event {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        event_type: row.get(2)?,
        entity_type: row.get(3)?,
        entity_id: row.get(4)?,
        payload: payload_json.unwrap_or_default(),
        metadata,
    })
}

pub struct EventRepository<'a> {
    db: &'a Db,
}

impl<'a> EventRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Append validates required fields then inserts the event.
    pub fn append(&self, event: &mut Event) -> Result<(), DbError> {
        if event.event_type.trim().is_empty()
            || event.entity_type.trim().is_empty()
            || event.entity_id.trim().is_empty()
        {
            return Err(DbError::InvalidEvent);
        }
        self.create(event)
    }

    /// Create inserts an event and applies Go-compatible defaults.
    pub fn create(&self, event: &mut Event) -> Result<(), DbError> {
        if event.event_type.trim().is_empty() {
            return Err(DbError::Validation("event type is required".into()));
        }
        if event.entity_type.trim().is_empty() {
            return Err(DbError::Validation("event entity type is required".into()));
        }
        if event.entity_id.trim().is_empty() {
            return Err(DbError::Validation("event entity id is required".into()));
        }

        if event.id.is_empty() {
            event.id = Uuid::new_v4().to_string();
        }
        if event.timestamp.is_empty() {
            event.timestamp = now_rfc3339();
        }

        let metadata_json: Option<String> = match &event.metadata {
            Some(value) => Some(serde_json::to_string(value).map_err(|err| {
                DbError::Validation(format!("failed to marshal metadata: {err}"))
            })?),
            None => None,
        };

        self.db.conn().execute(
            "INSERT INTO events (
                id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id,
                event.timestamp,
                event.event_type,
                event.entity_type,
                event.entity_id,
                nullable_string(&event.payload),
                metadata_json,
            ],
        )?;

        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Event, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                "SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
                 FROM events
                 WHERE id = ?1",
                params![id],
                scan_event_row,
            )
            .optional()?;
        row.ok_or(DbError::EventNotFound)
    }

    /// Query with cursor-based pagination. Sort order is timestamp asc, id asc.
    pub fn query(&self, q: EventQuery) -> Result<EventPage, DbError> {
        let limit = if q.limit <= 0 { 100 } else { q.limit };
        let mut query = String::from(
            "SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
             FROM events
             WHERE 1=1",
        );
        let mut args: Vec<Value> = Vec::new();

        if let Some(event_type) = q.event_type {
            query.push_str(" AND type = ?");
            args.push(Value::from(event_type));
        }
        if let Some(entity_type) = q.entity_type {
            query.push_str(" AND entity_type = ?");
            args.push(Value::from(entity_type));
        }
        if let Some(entity_id) = q.entity_id {
            query.push_str(" AND entity_id = ?");
            args.push(Value::from(entity_id));
        }
        if let Some(since) = q.since {
            query.push_str(" AND timestamp >= ?");
            args.push(Value::from(since));
        }
        if let Some(until) = q.until {
            query.push_str(" AND timestamp < ?");
            args.push(Value::from(until));
        }
        if !q.cursor.is_empty() {
            query
                .push_str(" AND (timestamp, id) > (SELECT timestamp, id FROM events WHERE id = ?)");
            args.push(Value::from(q.cursor));
        }

        query.push_str(" ORDER BY timestamp, id LIMIT ?");
        args.push(Value::from(limit + 1));

        let mut stmt = self.db.conn().prepare(&query)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), scan_event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }

        if (events.len() as i64) > limit {
            let next_cursor = events[(limit - 1) as usize].id.clone();
            events.truncate(limit as usize);
            return Ok(EventPage {
                events,
                next_cursor,
            });
        }

        Ok(EventPage {
            events,
            next_cursor: String::new(),
        })
    }

    pub fn list_by_entity(
        &self,
        entity_type: &str,
        entity_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, DbError> {
        let limit = if limit <= 0 { 100 } else { limit };
        let mut stmt = self.db.conn().prepare(
            "SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
             FROM events
             WHERE entity_type = ?1 AND entity_id = ?2
             ORDER BY timestamp
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![entity_type, entity_id, limit], scan_event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn count(&self) -> Result<i64, DbError> {
        let count: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn oldest_timestamp(&self) -> Result<Option<String>, DbError> {
        let value: Option<String> = self
            .db
            .conn()
            .query_row("SELECT MIN(timestamp) FROM events", [], |row| row.get(0))
            .optional()?
            .flatten();
        Ok(value)
    }

    pub fn delete_older_than(&self, before: &str, limit: i64) -> Result<i64, DbError> {
        let limit = if limit <= 0 { 1000 } else { limit };
        let rows = self.db.conn().execute(
            "DELETE FROM events WHERE id IN (
                SELECT id FROM events WHERE timestamp < ?1 ORDER BY timestamp LIMIT ?2
            )",
            params![before, limit],
        )?;
        Ok(rows as i64)
    }

    pub fn delete_excess(&self, max_count: i64, limit: i64) -> Result<i64, DbError> {
        if max_count <= 0 {
            return Ok(0);
        }
        let limit = if limit <= 0 { 1000 } else { limit };
        let total = self.count()?;
        let excess = total - max_count;
        if excess <= 0 {
            return Ok(0);
        }
        let delete_count = excess.min(limit);
        let rows = self.db.conn().execute(
            "DELETE FROM events WHERE id IN (
                SELECT id FROM events ORDER BY timestamp LIMIT ?1
            )",
            params![delete_count],
        )?;
        Ok(rows as i64)
    }

    pub fn list_older_than(&self, before: &str, limit: i64) -> Result<Vec<Event>, DbError> {
        let limit = if limit <= 0 { 1000 } else { limit };
        let mut stmt = self.db.conn().prepare(
            "SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
             FROM events
             WHERE timestamp < ?1
             ORDER BY timestamp
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![before, limit], scan_event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn list_oldest(&self, limit: i64) -> Result<Vec<Event>, DbError> {
        let limit = if limit <= 0 { 1000 } else { limit };
        let mut stmt = self.db.conn().prepare(
            "SELECT id, timestamp, type, entity_type, entity_id, payload_json, metadata_json
             FROM events
             ORDER BY timestamp
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], scan_event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn delete_by_ids(&self, ids: &[String]) -> Result<i64, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }

        let placeholders = std::iter::repeat("?")
            .take(ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("DELETE FROM events WHERE id IN ({placeholders})");

        let values = ids
            .iter()
            .map(|id| Value::from(id.clone()))
            .collect::<Vec<_>>();

        let rows = self
            .db
            .conn()
            .execute(&query, params_from_iter(values.iter()))?;
        Ok(rows as i64)
    }
}

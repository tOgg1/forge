//! Loop queue repository — CRUD for the `loop_queue_items` table with full Go parity.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Queue item type. Mirrors Go `models.LoopQueueItemType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopQueueItemType {
    MessageAppend,
    NextPromptOverride,
    Pause,
    StopGraceful,
    KillNow,
    SteerMessage,
}

impl LoopQueueItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MessageAppend => "message_append",
            Self::NextPromptOverride => "next_prompt_override",
            Self::Pause => "pause",
            Self::StopGraceful => "stop_graceful",
            Self::KillNow => "kill_now",
            Self::SteerMessage => "steer_message",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "message_append" => Ok(Self::MessageAppend),
            "next_prompt_override" => Ok(Self::NextPromptOverride),
            "pause" => Ok(Self::Pause),
            "stop_graceful" => Ok(Self::StopGraceful),
            "kill_now" => Ok(Self::KillNow),
            "steer_message" => Ok(Self::SteerMessage),
            other => Err(DbError::Validation(format!(
                "unknown loop queue item type \"{other}\""
            ))),
        }
    }
}

/// Queue item status. Mirrors Go `models.LoopQueueItemStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopQueueItemStatus {
    Pending,
    Dispatched,
    Completed,
    Failed,
    Skipped,
}

impl LoopQueueItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DbError> {
        match s {
            "pending" => Ok(Self::Pending),
            "dispatched" => Ok(Self::Dispatched),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            other => Err(DbError::Validation(format!(
                "invalid queue item status: {other}"
            ))),
        }
    }
}

/// A loop queue item. Mirrors Go `models.LoopQueueItem`.
#[derive(Debug, Clone)]
pub struct LoopQueueItem {
    pub id: String,
    pub loop_id: String,
    pub item_type: String,
    pub position: i64,
    pub status: String,
    pub attempts: i64,
    pub payload: String,
    pub created_at: String,
    pub dispatched_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: String,
}

impl Default for LoopQueueItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            loop_id: String::new(),
            item_type: String::new(),
            position: 0,
            status: "pending".to_string(),
            attempts: 0,
            payload: String::new(),
            created_at: String::new(),
            dispatched_at: None,
            completed_at: None,
            error: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation (mirrors Go models.LoopQueueItem.Validate)
// ---------------------------------------------------------------------------

fn validate_queue_item(item: &LoopQueueItem) -> Result<(), DbError> {
    let mut errors: Vec<String> = Vec::new();

    if item.item_type.is_empty() {
        errors.push("type: queue item type is required".into());
    }
    if item.attempts < 0 {
        errors.push("attempts: attempts must be >= 0".into());
    }
    if item.payload.is_empty() {
        errors.push("payload: queue item payload is required".into());
    }

    if !errors.is_empty() {
        return Err(DbError::Validation(errors.join("; ")));
    }

    // Validate item type is known and payload matches expected shape.
    match item.item_type.as_str() {
        "message_append" => {
            let parsed: serde_json::Value = serde_json::from_str(&item.payload)
                .map_err(|e| DbError::Validation(format!("invalid message_append payload: {e}")))?;
            let text = parsed.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text.trim().is_empty() {
                return Err(DbError::Validation(
                    "message_append payload text is required".into(),
                ));
            }
        }
        "next_prompt_override" => {
            let parsed: serde_json::Value = serde_json::from_str(&item.payload).map_err(|e| {
                DbError::Validation(format!("invalid next_prompt_override payload: {e}"))
            })?;
            let prompt = parsed.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
            if prompt.trim().is_empty() {
                return Err(DbError::Validation(
                    "next_prompt_override payload prompt is required".into(),
                ));
            }
        }
        "pause" => {
            let parsed: serde_json::Value = serde_json::from_str(&item.payload)
                .map_err(|e| DbError::Validation(format!("invalid pause payload: {e}")))?;
            let duration = parsed
                .get("duration_seconds")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if duration <= 0 {
                return Err(DbError::Validation(
                    "pause payload duration_seconds must be > 0".into(),
                ));
            }
        }
        "stop_graceful" => {
            let _: serde_json::Value = serde_json::from_str(&item.payload)
                .map_err(|e| DbError::Validation(format!("invalid stop_graceful payload: {e}")))?;
        }
        "kill_now" => {
            let _: serde_json::Value = serde_json::from_str(&item.payload)
                .map_err(|e| DbError::Validation(format!("invalid kill_now payload: {e}")))?;
        }
        "steer_message" => {
            let parsed: serde_json::Value = serde_json::from_str(&item.payload)
                .map_err(|e| DbError::Validation(format!("invalid steer_message payload: {e}")))?;
            let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if message.trim().is_empty() {
                return Err(DbError::Validation(
                    "steer_message payload message is required".into(),
                ));
            }
        }
        other => {
            return Err(DbError::Validation(format!(
                "unknown loop queue item type \"{other}\""
            )));
        }
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

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

// ---------------------------------------------------------------------------
// Row scanner
// ---------------------------------------------------------------------------

fn scan_loop_queue_item(row: &rusqlite::Row) -> rusqlite::Result<LoopQueueItem> {
    let id: String = row.get(0)?;
    let loop_id: String = row.get(1)?;
    let item_type: String = row.get(2)?;
    let position: i64 = row.get(3)?;
    let status: String = row.get(4)?;
    let attempts: i64 = row.get(5)?;
    let payload: String = row.get(6)?;
    let error_msg: Option<String> = row.get(7)?;
    let created_at: String = row.get(8)?;
    let dispatched_at: Option<String> = row.get(9)?;
    let completed_at: Option<String> = row.get(10)?;

    Ok(LoopQueueItem {
        id,
        loop_id,
        item_type,
        position,
        status,
        attempts,
        payload,
        error: error_msg.unwrap_or_default(),
        created_at,
        dispatched_at: dispatched_at.filter(|s| !s.is_empty()),
        completed_at: completed_at.filter(|s| !s.is_empty()),
    })
}

// ---------------------------------------------------------------------------
// LoopQueueRepository
// ---------------------------------------------------------------------------

pub struct LoopQueueRepository<'a> {
    db: &'a Db,
}

impl<'a> LoopQueueRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Enqueue adds items to a loop's queue. Mirrors Go `Enqueue`.
    pub fn enqueue(&self, loop_id: &str, items: &mut [LoopQueueItem]) -> Result<(), DbError> {
        if items.is_empty() {
            return Ok(());
        }

        let max_pos = self.get_max_position(loop_id)?;
        let now = now_rfc3339();

        for (i, item) in items.iter_mut().enumerate() {
            validate_queue_item(item)?;

            if item.id.is_empty() {
                item.id = Uuid::new_v4().to_string();
            }
            item.loop_id = loop_id.to_string();
            item.created_at = now.clone();
            item.position = max_pos + (i as i64) + 1;
            if item.status.is_empty() {
                item.status = "pending".to_string();
            }

            self.db.conn().execute(
                "INSERT INTO loop_queue_items (
                    id, loop_id, type, position, status, attempts, payload_json,
                    error_message, created_at, dispatched_at, completed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    item.id,
                    item.loop_id,
                    item.item_type,
                    item.position,
                    item.status,
                    item.attempts,
                    item.payload,
                    nullable_string(&item.error),
                    item.created_at,
                    item.dispatched_at,
                    item.completed_at,
                ],
            )?;
        }

        Ok(())
    }

    /// Peek returns the next pending item without changing its status.
    /// Returns `DbError::QueueEmpty` if no pending items exist.
    pub fn peek(&self, loop_id: &str) -> Result<LoopQueueItem, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, loop_id, type, position, status, attempts, payload_json,
                    error_message, created_at, dispatched_at, completed_at
                FROM loop_queue_items
                WHERE loop_id = ?1 AND status = ?2
                ORDER BY position ASC
                LIMIT 1",
                params![loop_id, "pending"],
                scan_loop_queue_item,
            )
            .optional()?;

        result.ok_or(DbError::QueueEmpty)
    }

    /// Dequeue returns the next pending item and marks it as dispatched.
    /// Returns `DbError::QueueEmpty` if no pending items exist.
    pub fn dequeue(&self, loop_id: &str) -> Result<LoopQueueItem, DbError> {
        let mut item = self.peek(loop_id)?;

        let now = now_rfc3339();
        self.db.conn().execute(
            "UPDATE loop_queue_items
            SET status = ?1, dispatched_at = ?2
            WHERE id = ?3",
            params!["dispatched", now, item.id],
        )?;

        item.status = "dispatched".to_string();
        item.dispatched_at = Some(now);
        Ok(item)
    }

    /// List returns all queue items for a loop ordered by position.
    pub fn list(&self, loop_id: &str) -> Result<Vec<LoopQueueItem>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, loop_id, type, position, status, attempts, payload_json,
                error_message, created_at, dispatched_at, completed_at
            FROM loop_queue_items
            WHERE loop_id = ?1
            ORDER BY position ASC",
        )?;

        let rows = stmt.query_map(params![loop_id], scan_loop_queue_item)?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// Clear removes all pending items from a loop queue.
    /// Returns the number of items removed.
    pub fn clear(&self, loop_id: &str) -> Result<usize, DbError> {
        let count = self.db.conn().execute(
            "DELETE FROM loop_queue_items
            WHERE loop_id = ?1 AND status = ?2",
            params![loop_id, "pending"],
        )?;
        Ok(count)
    }

    /// Remove deletes a queue item by ID.
    pub fn remove(&self, item_id: &str) -> Result<(), DbError> {
        let rows_affected = self.db.conn().execute(
            "DELETE FROM loop_queue_items WHERE id = ?1",
            params![item_id],
        )?;

        if rows_affected == 0 {
            return Err(DbError::QueueItemNotFound);
        }
        Ok(())
    }

    /// UpdateStatus updates the status of a queue item.
    pub fn update_status(
        &self,
        item_id: &str,
        status: &str,
        error_msg: &str,
    ) -> Result<(), DbError> {
        let completed_at = now_rfc3339();
        let rows_affected = self.db.conn().execute(
            "UPDATE loop_queue_items
            SET status = ?1, error_message = ?2, completed_at = ?3
            WHERE id = ?4",
            params![status, nullable_string(error_msg), completed_at, item_id],
        )?;

        if rows_affected == 0 {
            return Err(DbError::QueueItemNotFound);
        }
        Ok(())
    }

    /// Reorder updates queue item positions based on the provided ordered IDs.
    /// Uses a transaction to ensure atomicity, matching Go behavior.
    pub fn reorder(&self, loop_id: &str, ordered_ids: &[String]) -> Result<(), DbError> {
        if ordered_ids.is_empty() {
            return Ok(());
        }

        let tx = self.db.conn().unchecked_transaction()?;
        for (i, id) in ordered_ids.iter().enumerate() {
            let position = (i as i64) + 1;
            tx.execute(
                "UPDATE loop_queue_items
                SET position = ?1
                WHERE id = ?2 AND loop_id = ?3",
                params![position, id, loop_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_max_position(&self, loop_id: &str) -> Result<i64, DbError> {
        let max_pos: i64 = self.db.conn().query_row(
            "SELECT COALESCE(MAX(position), 0) FROM loop_queue_items WHERE loop_id = ?1",
            params![loop_id],
            |row| row.get(0),
        )?;
        Ok(max_pos)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::loop_repository::LoopRepository;
    use crate::{Config, Db};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(tag: &str) -> PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let mut path = std::env::temp_dir();
        path.push(format!(
            "forge-db-queue-{tag}-{nanos}-{}.sqlite",
            std::process::id()
        ));
        path
    }

    fn setup_db(tag: &str) -> (Db, PathBuf) {
        let path = temp_db_path(tag);
        let mut db = Db::open(Config::new(&path)).unwrap_or_else(|e| panic!("open db: {e}"));
        db.migrate_up().unwrap_or_else(|e| panic!("migrate: {e}"));
        (db, path)
    }

    fn create_test_loop(db: &Db) -> crate::loop_repository::Loop {
        let repo = LoopRepository::new(db);
        let mut lp = crate::loop_repository::Loop {
            name: format!("test-loop-{}", Uuid::new_v4()),
            repo_path: "/repo".to_string(),
            interval_seconds: 10,
            ..Default::default()
        };
        repo.create(&mut lp)
            .unwrap_or_else(|e| panic!("create loop: {e}"));
        lp
    }

    fn new_message_item(text: &str) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "message_append".to_string(),
            payload: format!("{{\"text\":\"{text}\"}}"),
            ..Default::default()
        }
    }

    fn new_stop_item(reason: &str) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "stop_graceful".to_string(),
            payload: format!("{{\"reason\":\"{reason}\"}}"),
            ..Default::default()
        }
    }

    fn new_kill_item(reason: &str) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "kill_now".to_string(),
            payload: format!("{{\"reason\":\"{reason}\"}}"),
            ..Default::default()
        }
    }

    fn new_pause_item(duration_seconds: i64) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "pause".to_string(),
            payload: format!("{{\"duration_seconds\":{duration_seconds}}}"),
            ..Default::default()
        }
    }

    fn new_steer_item(message: &str) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "steer_message".to_string(),
            payload: format!("{{\"message\":\"{message}\"}}"),
            ..Default::default()
        }
    }

    fn new_prompt_override_item(prompt: &str, is_path: bool) -> LoopQueueItem {
        LoopQueueItem {
            id: Uuid::new_v4().to_string(),
            item_type: "next_prompt_override".to_string(),
            payload: format!("{{\"prompt\":\"{prompt}\",\"is_path\":{is_path}}}"),
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // Enqueue / Peek / Dequeue
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_peek_dequeue() {
        let (db, path) = setup_db("enqueue-peek-dequeue");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("first"), new_message_item("second")];
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        // Peek returns first item.
        let peeked = repo.peek(&lp.id).unwrap_or_else(|e| panic!("peek: {e}"));
        assert_eq!(peeked.position, 1);
        assert_eq!(peeked.status, "pending");

        // Dequeue returns first item and marks dispatched.
        let dequeued = repo
            .dequeue(&lp.id)
            .unwrap_or_else(|e| panic!("dequeue: {e}"));
        assert_eq!(dequeued.status, "dispatched");
        assert!(dequeued.dispatched_at.is_some());
        assert_eq!(dequeued.id, peeked.id);

        // List returns both items.
        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn peek_empty_queue_returns_queue_empty() {
        let (db, path) = setup_db("peek-empty");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        match repo.peek(&lp.id) {
            Err(DbError::QueueEmpty) => {}
            other => panic!("expected QueueEmpty, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dequeue_empty_queue_returns_queue_empty() {
        let (db, path) = setup_db("dequeue-empty");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        match repo.dequeue(&lp.id) {
            Err(DbError::QueueEmpty) => {}
            other => panic!("expected QueueEmpty, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Position auto-increment
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_auto_increments_position() {
        let (db, path) = setup_db("position-auto");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        // First batch.
        let mut batch1 = vec![new_message_item("a"), new_message_item("b")];
        repo.enqueue(&lp.id, &mut batch1)
            .unwrap_or_else(|e| panic!("enqueue batch1: {e}"));

        // Second batch starts after first batch.
        let mut batch2 = vec![new_message_item("c")];
        repo.enqueue(&lp.id, &mut batch2)
            .unwrap_or_else(|e| panic!("enqueue batch2: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].position, 1);
        assert_eq!(all[1].position, 2);
        assert_eq!(all[2].position, 3);

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Enqueue empty is no-op
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_empty_is_noop() {
        let (db, path) = setup_db("enqueue-empty");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        repo.enqueue(&lp.id, &mut [])
            .unwrap_or_else(|e| panic!("enqueue empty: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert!(all.is_empty());

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Clear
    // -----------------------------------------------------------------------

    #[test]
    fn clear_removes_only_pending_items() {
        let (db, path) = setup_db("clear");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![
            new_message_item("first"),
            new_message_item("second"),
            new_message_item("third"),
        ];
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        // Dequeue first item (marks as dispatched).
        repo.dequeue(&lp.id)
            .unwrap_or_else(|e| panic!("dequeue: {e}"));

        // Clear should remove only the 2 remaining pending items.
        let cleared = repo.clear(&lp.id).unwrap_or_else(|e| panic!("clear: {e}"));
        assert_eq!(cleared, 2);

        // Only the dispatched item remains.
        let remaining = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].status, "dispatched");

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Remove
    // -----------------------------------------------------------------------

    #[test]
    fn remove_deletes_item_by_id() {
        let (db, path) = setup_db("remove");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("only")];
        let item_id = items[0].id.clone();
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        repo.remove(&item_id)
            .unwrap_or_else(|e| panic!("remove: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert!(all.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn remove_nonexistent_returns_not_found() {
        let (db, path) = setup_db("remove-notfound");
        let repo = LoopQueueRepository::new(&db);

        match repo.remove("nonexistent-id") {
            Err(DbError::QueueItemNotFound) => {}
            other => panic!("expected QueueItemNotFound, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // UpdateStatus
    // -----------------------------------------------------------------------

    #[test]
    fn update_status_sets_status_and_completed_at() {
        let (db, path) = setup_db("update-status");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("task")];
        let item_id = items[0].id.clone();
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        repo.update_status(&item_id, "completed", "")
            .unwrap_or_else(|e| panic!("update_status: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, "completed");
        assert!(all[0].completed_at.is_some());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn update_status_with_error_message() {
        let (db, path) = setup_db("update-status-error");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("fail")];
        let item_id = items[0].id.clone();
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        repo.update_status(&item_id, "failed", "something broke")
            .unwrap_or_else(|e| panic!("update_status: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all[0].status, "failed");
        assert_eq!(all[0].error, "something broke");
        assert!(all[0].completed_at.is_some());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn update_status_nonexistent_returns_not_found() {
        let (db, path) = setup_db("update-status-notfound");
        let repo = LoopQueueRepository::new(&db);

        match repo.update_status("nope", "completed", "") {
            Err(DbError::QueueItemNotFound) => {}
            other => panic!("expected QueueItemNotFound, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Reorder
    // -----------------------------------------------------------------------

    #[test]
    fn reorder_updates_positions() {
        let (db, path) = setup_db("reorder");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![
            new_message_item("a"),
            new_message_item("b"),
            new_message_item("c"),
        ];
        let id_a = items[0].id.clone();
        let id_b = items[1].id.clone();
        let id_c = items[2].id.clone();
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        // Reverse the order: c, b, a
        let new_order = vec![id_c.clone(), id_b.clone(), id_a.clone()];
        repo.reorder(&lp.id, &new_order)
            .unwrap_or_else(|e| panic!("reorder: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all.len(), 3);
        // List returns by position ASC, so new order is c(1), b(2), a(3).
        assert_eq!(all[0].id, id_c);
        assert_eq!(all[0].position, 1);
        assert_eq!(all[1].id, id_b);
        assert_eq!(all[1].position, 2);
        assert_eq!(all[2].id, id_a);
        assert_eq!(all[2].position, 3);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reorder_empty_is_noop() {
        let (db, path) = setup_db("reorder-empty");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        repo.reorder(&lp.id, &[])
            .unwrap_or_else(|e| panic!("reorder empty: {e}"));

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_rejects_empty_type() {
        let (db, path) = setup_db("validate-type");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            payload: "{\"text\":\"hi\"}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_empty_payload() {
        let (db, path) = setup_db("validate-payload");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "message_append".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_unknown_type() {
        let (db, path) = setup_db("validate-unknown-type");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "banana".to_string(),
            payload: "{\"text\":\"hi\"}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_message_append_empty_text() {
        let (db, path) = setup_db("validate-msg-text");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: "{\"text\":\"\"}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_steer_empty_message() {
        let (db, path) = setup_db("validate-steer-msg");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "steer_message".to_string(),
            payload: "{\"message\":\"\"}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_pause_zero_duration() {
        let (db, path) = setup_db("validate-pause-dur");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "pause".to_string(),
            payload: "{\"duration_seconds\":0}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enqueue_rejects_prompt_override_empty_prompt() {
        let (db, path) = setup_db("validate-prompt");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "next_prompt_override".to_string(),
            payload: "{\"prompt\":\"\",\"is_path\":false}".to_string(),
            ..Default::default()
        }];
        match repo.enqueue(&lp.id, &mut items) {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // All item types roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn all_item_types_enqueue_and_list_roundtrip() {
        let (db, path) = setup_db("all-types");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![
            new_message_item("hello"),
            new_prompt_override_item("do this", false),
            new_pause_item(30),
            new_stop_item("done"),
            new_kill_item("now"),
            new_steer_item("go left"),
        ];

        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all.len(), 6);
        assert_eq!(all[0].item_type, "message_append");
        assert_eq!(all[1].item_type, "next_prompt_override");
        assert_eq!(all[2].item_type, "pause");
        assert_eq!(all[3].item_type, "stop_graceful");
        assert_eq!(all[4].item_type, "kill_now");
        assert_eq!(all[5].item_type, "steer_message");

        // Positions are sequential.
        for (i, item) in all.iter().enumerate() {
            assert_eq!(item.position, (i as i64) + 1);
            assert_eq!(item.status, "pending");
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Dequeue skips dispatched items
    // -----------------------------------------------------------------------

    #[test]
    fn dequeue_skips_already_dispatched() {
        let (db, path) = setup_db("dequeue-skip");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("first"), new_message_item("second")];
        let id_second = items[1].id.clone();
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        // Dequeue first.
        repo.dequeue(&lp.id)
            .unwrap_or_else(|e| panic!("dequeue 1: {e}"));

        // Dequeue again gets second.
        let second = repo
            .dequeue(&lp.id)
            .unwrap_or_else(|e| panic!("dequeue 2: {e}"));
        assert_eq!(second.id, id_second);

        // Queue now empty.
        match repo.dequeue(&lp.id) {
            Err(DbError::QueueEmpty) => {}
            other => panic!("expected QueueEmpty, got: {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Cascade delete (loop deletion removes queue items)
    // -----------------------------------------------------------------------

    #[test]
    fn cascade_delete_removes_queue_items() {
        let (db, path) = setup_db("cascade");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![new_message_item("orphan")];
        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        // Delete the loop.
        let loop_repo = LoopRepository::new(&db);
        loop_repo
            .delete(&lp.id)
            .unwrap_or_else(|e| panic!("delete loop: {e}"));

        // Queue items should be gone via CASCADE.
        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert!(all.is_empty());

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // ID auto-generation
    // -----------------------------------------------------------------------

    #[test]
    fn enqueue_generates_id_if_empty() {
        let (db, path) = setup_db("id-gen");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items = vec![LoopQueueItem {
            item_type: "stop_graceful".to_string(),
            payload: "{\"reason\":\"test\"}".to_string(),
            ..Default::default()
        }];
        assert!(items[0].id.is_empty());

        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        assert!(!items[0].id.is_empty());

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // List for unknown loop returns empty
    // -----------------------------------------------------------------------

    #[test]
    fn list_unknown_loop_returns_empty() {
        let (db, path) = setup_db("list-unknown");
        let repo = LoopQueueRepository::new(&db);

        let all = repo
            .list("nonexistent")
            .unwrap_or_else(|e| panic!("list: {e}"));
        assert!(all.is_empty());

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Enum type/status parse roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn item_type_parse_roundtrip() {
        let types = vec![
            ("message_append", LoopQueueItemType::MessageAppend),
            (
                "next_prompt_override",
                LoopQueueItemType::NextPromptOverride,
            ),
            ("pause", LoopQueueItemType::Pause),
            ("stop_graceful", LoopQueueItemType::StopGraceful),
            ("kill_now", LoopQueueItemType::KillNow),
            ("steer_message", LoopQueueItemType::SteerMessage),
        ];
        for (s, expected) in types {
            let parsed = LoopQueueItemType::parse(s).unwrap_or_else(|e| panic!("parse {s}: {e}"));
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), s);
        }
    }

    #[test]
    fn item_type_parse_unknown_fails() {
        match LoopQueueItemType::parse("unknown") {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[test]
    fn item_status_parse_roundtrip() {
        let statuses = vec![
            ("pending", LoopQueueItemStatus::Pending),
            ("dispatched", LoopQueueItemStatus::Dispatched),
            ("completed", LoopQueueItemStatus::Completed),
            ("failed", LoopQueueItemStatus::Failed),
            ("skipped", LoopQueueItemStatus::Skipped),
        ];
        for (s, expected) in statuses {
            let parsed = LoopQueueItemStatus::parse(s).unwrap_or_else(|e| panic!("parse {s}: {e}"));
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), s);
        }
    }

    #[test]
    fn item_status_parse_unknown_fails() {
        match LoopQueueItemStatus::parse("invalid") {
            Err(DbError::Validation(_)) => {}
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Payload JSON roundtrip through DB
    // -----------------------------------------------------------------------

    #[test]
    fn payload_json_roundtrip() {
        let (db, path) = setup_db("payload-roundtrip");
        let lp = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let payload = "{\"text\":\"hello world\"}";
        let mut items = vec![LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: payload.to_string(),
            ..Default::default()
        }];

        repo.enqueue(&lp.id, &mut items)
            .unwrap_or_else(|e| panic!("enqueue: {e}"));

        let all = repo.list(&lp.id).unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(all[0].payload, payload);

        let _ = std::fs::remove_file(path);
    }

    // -----------------------------------------------------------------------
    // Separate loops have independent queues
    // -----------------------------------------------------------------------

    #[test]
    fn separate_loops_independent_queues() {
        let (db, path) = setup_db("separate-queues");
        let lp1 = create_test_loop(&db);
        let lp2 = create_test_loop(&db);
        let repo = LoopQueueRepository::new(&db);

        let mut items1 = vec![new_message_item("for-loop-1")];
        let mut items2 = vec![
            new_message_item("for-loop-2"),
            new_message_item("also-loop-2"),
        ];
        repo.enqueue(&lp1.id, &mut items1)
            .unwrap_or_else(|e| panic!("enqueue lp1: {e}"));
        repo.enqueue(&lp2.id, &mut items2)
            .unwrap_or_else(|e| panic!("enqueue lp2: {e}"));

        let all1 = repo
            .list(&lp1.id)
            .unwrap_or_else(|e| panic!("list lp1: {e}"));
        let all2 = repo
            .list(&lp2.id)
            .unwrap_or_else(|e| panic!("list lp2: {e}"));

        assert_eq!(all1.len(), 1);
        assert_eq!(all2.len(), 2);

        let _ = std::fs::remove_file(path);
    }
}

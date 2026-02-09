//! File lock repository — cleanup semantics for `file_locks` table.

use rusqlite::params;

use crate::{Db, DbError};

pub struct FileLockRepository<'a> {
    db: &'a Db,
}

impl<'a> FileLockRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Marks expired locks as released and returns rows affected.
    /// Mirrors Go `CleanupExpired` update semantics.
    pub fn cleanup_expired(&self, now: Option<&str>) -> Result<i64, DbError> {
        let timestamp = match now {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => crate::now_rfc3339(),
        };

        let rows = self.db.conn().execute(
            "UPDATE file_locks
             SET released_at = ?1
             WHERE released_at IS NULL
               AND expires_at <= ?2",
            params![timestamp, timestamp],
        )?;

        i64::try_from(rows).map_err(|_| DbError::Validation("rows affected overflow".into()))
    }
}

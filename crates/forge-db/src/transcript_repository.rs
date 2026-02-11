//! Transcript repository — append/query for the `transcripts` table.

use rusqlite::{params, OptionalExtension};

use crate::{Db, DbError};

#[derive(Debug, Clone, Default)]
pub struct Transcript {
    pub id: i64,
    pub agent_id: String,
    pub content: String,
    pub content_hash: String,
    pub captured_at: String,
}

pub struct TranscriptRepository<'a> {
    db: &'a Db,
}

impl<'a> TranscriptRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn create(&self, transcript: &mut Transcript) -> Result<(), DbError> {
        if transcript.agent_id.trim().is_empty() {
            return Err(DbError::Validation(
                "transcript agent id is required".into(),
            ));
        }

        if transcript.captured_at.is_empty() {
            transcript.captured_at = crate::now_rfc3339();
        }

        self.db.conn().execute(
            "INSERT INTO transcripts (agent_id, content, content_hash, captured_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                transcript.agent_id,
                transcript.content,
                transcript.content_hash,
                transcript.captured_at,
            ],
        )?;

        transcript.id = self.db.conn().last_insert_rowid();
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Transcript, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, agent_id, content, content_hash, captured_at
                 FROM transcripts
                 WHERE id = ?1",
                params![id],
                scan_transcript,
            )
            .optional()?;

        result.ok_or(DbError::TranscriptNotFound)
    }

    pub fn latest_by_agent(&self, agent_id: &str) -> Result<Transcript, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT id, agent_id, content, content_hash, captured_at
                 FROM transcripts
                 WHERE agent_id = ?1
                 ORDER BY captured_at DESC, id DESC
                 LIMIT 1",
                params![agent_id],
                scan_transcript,
            )
            .optional()?;

        result.ok_or(DbError::TranscriptNotFound)
    }

    pub fn list_by_agent(&self, agent_id: &str, limit: usize) -> Result<Vec<Transcript>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, agent_id, content, content_hash, captured_at
             FROM transcripts
             WHERE agent_id = ?1
             ORDER BY captured_at DESC, id DESC
             LIMIT ?2",
        )?;

        let capped_limit = if limit == 0 {
            i64::MAX
        } else {
            i64::try_from(limit).map_err(|_| DbError::Validation("limit out of range".into()))?
        };

        let rows = stmt.query_map(params![agent_id, capped_limit], scan_transcript)?;

        let mut transcripts = Vec::new();
        for row in rows {
            transcripts.push(row?);
        }

        Ok(transcripts)
    }
}

fn scan_transcript(row: &rusqlite::Row<'_>) -> rusqlite::Result<Transcript> {
    Ok(Transcript {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        content: row.get(2)?,
        content_hash: row.get(3)?,
        captured_at: row.get(4)?,
    })
}

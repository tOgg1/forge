//! Mail repository — persistence for `mail_threads` and `mail_messages`.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RecipientType {
    #[default]
    Agent,
    Workspace,
    Broadcast,
}

impl RecipientType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Workspace => "workspace",
            Self::Broadcast => "broadcast",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "agent" => Ok(Self::Agent),
            "workspace" => Ok(Self::Workspace),
            "broadcast" => Ok(Self::Broadcast),
            other => Err(DbError::Validation(format!(
                "invalid recipient_type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MailThread {
    pub id: String,
    pub workspace_id: String,
    pub subject: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct MailMessage {
    pub id: String,
    pub thread_id: String,
    pub sender_agent_id: Option<String>,
    pub recipient_type: RecipientType,
    pub recipient_id: Option<String>,
    pub subject: Option<String>,
    pub body: String,
    pub importance: String,
    pub ack_required: bool,
    pub read_at: Option<String>,
    pub acked_at: Option<String>,
    pub created_at: String,
}

pub struct MailRepository<'a> {
    db: &'a Db,
}

impl<'a> MailRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    // ── Threads ──────────────────────────────────────────────────────

    pub fn create_thread(&self, thread: &mut MailThread) -> Result<(), DbError> {
        if thread.workspace_id.trim().is_empty() {
            return Err(DbError::Validation("workspace_id is required".into()));
        }
        if thread.subject.trim().is_empty() {
            return Err(DbError::Validation("subject is required".into()));
        }
        if thread.id.is_empty() {
            thread.id = Uuid::new_v4().to_string();
        }
        let now = crate::now_rfc3339();
        thread.created_at = now.clone();
        thread.updated_at = now;

        let result = self.db.conn().execute(
            "INSERT INTO mail_threads (id, workspace_id, subject, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                thread.id,
                thread.workspace_id,
                thread.subject,
                thread.created_at,
                thread.updated_at
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(ref err) if is_unique_constraint_error(err) => {
                Err(DbError::MailThreadAlreadyExists)
            }
            Err(err) => Err(DbError::Open(err)),
        }
    }

    pub fn get_thread(&self, id: &str) -> Result<MailThread, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                "SELECT id, workspace_id, subject, created_at, updated_at
                 FROM mail_threads
                 WHERE id = ?1",
                params![id],
                scan_thread,
            )
            .optional()?;
        row.ok_or(DbError::MailThreadNotFound)
    }

    pub fn list_threads_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<MailThread>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, workspace_id, subject, created_at, updated_at
             FROM mail_threads
             WHERE workspace_id = ?1
             ORDER BY updated_at DESC, created_at DESC",
        )?;
        let rows = stmt.query_map(params![workspace_id], scan_thread)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_thread(&self, thread: &mut MailThread) -> Result<(), DbError> {
        if thread.id.trim().is_empty() {
            return Err(DbError::Validation("thread id is required".into()));
        }
        if thread.subject.trim().is_empty() {
            return Err(DbError::Validation("subject is required".into()));
        }
        thread.updated_at = crate::now_rfc3339();

        let rows = self.db.conn().execute(
            "UPDATE mail_threads SET subject = ?1, updated_at = ?2 WHERE id = ?3",
            params![thread.subject, thread.updated_at, thread.id],
        )?;
        if rows == 0 {
            return Err(DbError::MailThreadNotFound);
        }
        Ok(())
    }

    pub fn delete_thread(&self, id: &str) -> Result<(), DbError> {
        if id.trim().is_empty() {
            return Err(DbError::Validation("thread id is required".into()));
        }
        let rows = self
            .db
            .conn()
            .execute("DELETE FROM mail_threads WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(DbError::MailThreadNotFound);
        }
        Ok(())
    }

    // ── Messages ─────────────────────────────────────────────────────

    pub fn create_message(&self, msg: &mut MailMessage) -> Result<(), DbError> {
        if msg.thread_id.trim().is_empty() {
            return Err(DbError::Validation("thread_id is required".into()));
        }
        if msg.body.trim().is_empty() {
            return Err(DbError::Validation("body is required".into()));
        }
        if msg.recipient_type != RecipientType::Broadcast
            && msg.recipient_id.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(DbError::Validation("recipient_id is required".into()));
        }

        if msg.id.is_empty() {
            msg.id = Uuid::new_v4().to_string();
        }
        if msg.importance.trim().is_empty() {
            msg.importance = "normal".to_string();
        }
        let now = crate::now_rfc3339();
        msg.created_at = now.clone();

        let result = self.db.conn().execute(
            "INSERT INTO mail_messages (
                id, thread_id, sender_agent_id,
                recipient_type, recipient_id,
                subject, body, importance,
                ack_required, read_at, acked_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                msg.id,
                msg.thread_id,
                msg.sender_agent_id,
                msg.recipient_type.as_str(),
                if msg.recipient_type == RecipientType::Broadcast {
                    None::<String>
                } else {
                    msg.recipient_id.clone()
                },
                msg.subject,
                msg.body,
                msg.importance,
                bool_to_int(msg.ack_required),
                msg.read_at,
                msg.acked_at,
                msg.created_at,
            ],
        );

        match result {
            Ok(_) => {}
            Err(ref err) if is_unique_constraint_error(err) => {
                return Err(DbError::MailMessageAlreadyExists);
            }
            Err(err) => return Err(DbError::Open(err)),
        }

        // Best-effort bump updated_at for thread ordering.
        let _ = self.db.conn().execute(
            "UPDATE mail_threads SET updated_at = ?1 WHERE id = ?2",
            params![now, msg.thread_id],
        );

        Ok(())
    }

    pub fn get_message(&self, id: &str) -> Result<MailMessage, DbError> {
        let row = self
            .db
            .conn()
            .query_row(
                "SELECT
                    id, thread_id, sender_agent_id,
                    recipient_type, recipient_id,
                    subject, body, importance,
                    ack_required, read_at, acked_at, created_at
                 FROM mail_messages
                 WHERE id = ?1",
                params![id],
                scan_message,
            )
            .optional()?;
        row.ok_or(DbError::MailMessageNotFound)
    }

    pub fn list_messages_by_thread(&self, thread_id: &str) -> Result<Vec<MailMessage>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, thread_id, sender_agent_id,
                recipient_type, recipient_id,
                subject, body, importance,
                ack_required, read_at, acked_at, created_at
             FROM mail_messages
             WHERE thread_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![thread_id], scan_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_by_recipient(
        &self,
        recipient_type: &RecipientType,
        recipient_id: &str,
    ) -> Result<Vec<MailMessage>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, thread_id, sender_agent_id,
                recipient_type, recipient_id,
                subject, body, importance,
                ack_required, read_at, acked_at, created_at
             FROM mail_messages
             WHERE recipient_type = ?1 AND recipient_id = ?2
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![recipient_type.as_str(), recipient_id], scan_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_unread_by_recipient(
        &self,
        recipient_id: &str,
    ) -> Result<Vec<MailMessage>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, thread_id, sender_agent_id,
                recipient_type, recipient_id,
                subject, body, importance,
                ack_required, read_at, acked_at, created_at
             FROM mail_messages
             WHERE recipient_id = ?1 AND read_at IS NULL
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![recipient_id], scan_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_inbox(
        &self,
        recipient_type: &str,
        recipient_id: Option<&str>,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<MailMessage>, DbError> {
        let kind = RecipientType::parse(recipient_type)?;

        let mut sql = String::from(
            "SELECT
                id, thread_id, sender_agent_id,
                recipient_type, recipient_id,
                subject, body, importance,
                ack_required, read_at, acked_at, created_at
             FROM mail_messages
             WHERE recipient_type = ?1",
        );

        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        args.push(kind.as_str().to_string().into());

        match kind {
            RecipientType::Broadcast => {}
            _ => {
                let rid = recipient_id.unwrap_or("").trim().to_string();
                if rid.is_empty() {
                    return Err(DbError::Validation("recipient_id is required".into()));
                }
                sql.push_str(" AND recipient_id = ?2");
                args.push(rid.into());
            }
        }

        if unread_only {
            sql.push_str(" AND read_at IS NULL");
        }
        sql.push_str(" ORDER BY created_at DESC");
        if limit > 0 {
            sql.push_str(" LIMIT ");
            sql.push_str(&limit.to_string());
        }

        let mut stmt = self.db.conn().prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), scan_message)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Mark a message as read (idempotent — already-read messages are not an error).
    pub fn mark_read(&self, message_id: &str) -> Result<(), DbError> {
        if message_id.trim().is_empty() {
            return Err(DbError::Validation("message_id is required".into()));
        }
        let rows = self.db.conn().execute(
            "UPDATE mail_messages SET read_at = ?1 WHERE id = ?2 AND read_at IS NULL",
            params![crate::now_rfc3339(), message_id],
        )?;
        if rows == 0 {
            // Check if the message exists at all (may already be read).
            let exists: bool = self
                .db
                .conn()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mail_messages WHERE id = ?1)",
                    params![message_id],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !exists {
                return Err(DbError::MailMessageNotFound);
            }
        }
        Ok(())
    }

    /// Mark a message as acknowledged (idempotent — already-acked messages are not an error).
    pub fn mark_acked(&self, message_id: &str) -> Result<(), DbError> {
        if message_id.trim().is_empty() {
            return Err(DbError::Validation("message_id is required".into()));
        }
        let rows = self.db.conn().execute(
            "UPDATE mail_messages SET acked_at = ?1 WHERE id = ?2 AND acked_at IS NULL",
            params![crate::now_rfc3339(), message_id],
        )?;
        if rows == 0 {
            // Check if the message exists at all (may already be acked).
            let exists: bool = self
                .db
                .conn()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM mail_messages WHERE id = ?1)",
                    params![message_id],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !exists {
                return Err(DbError::MailMessageNotFound);
            }
        }
        Ok(())
    }

    pub fn delete_message(&self, id: &str) -> Result<(), DbError> {
        if id.trim().is_empty() {
            return Err(DbError::Validation("message id is required".into()));
        }
        let rows = self
            .db
            .conn()
            .execute("DELETE FROM mail_messages WHERE id = ?1", params![id])?;
        if rows == 0 {
            return Err(DbError::MailMessageNotFound);
        }
        Ok(())
    }
}

fn scan_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailThread> {
    Ok(MailThread {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        subject: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn scan_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailMessage> {
    let kind: String = row.get(3)?;
    let ack_required: i64 = row.get(8)?;
    let recipient_type = RecipientType::parse(&kind).map_err(to_sql_conversion_error)?;
    Ok(MailMessage {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        sender_agent_id: row.get(2)?,
        recipient_type,
        recipient_id: row.get(4)?,
        subject: row.get(5)?,
        body: row.get(6)?,
        importance: row.get(7)?,
        ack_required: ack_required != 0,
        read_at: row.get(9)?,
        acked_at: row.get(10)?,
        created_at: row.get(11)?,
    })
}

fn to_sql_conversion_error(err: DbError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err.to_string(),
        )),
    )
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn is_unique_constraint_error(err: &rusqlite::Error) -> bool {
    err.to_string().contains("UNIQUE constraint failed")
}

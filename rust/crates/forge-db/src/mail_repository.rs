//! Mail repository — persistence for `mail_threads` and `mail_messages`.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipientType {
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
    pub recipient_type: String,
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

        self.db.conn().execute(
            "INSERT INTO mail_threads (id, workspace_id, subject, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                thread.id,
                thread.workspace_id,
                thread.subject,
                thread.created_at,
                thread.updated_at
            ],
        )?;
        Ok(())
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
                |row| {
                    Ok(MailThread {
                        id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        subject: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
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
        let rows = stmt.query_map(params![workspace_id], |row| {
            Ok(MailThread {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                subject: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn create_message(&self, msg: &mut MailMessage) -> Result<(), DbError> {
        if msg.thread_id.trim().is_empty() {
            return Err(DbError::Validation("thread_id is required".into()));
        }
        if msg.body.trim().is_empty() {
            return Err(DbError::Validation("body is required".into()));
        }
        if msg.recipient_type.trim().is_empty() {
            return Err(DbError::Validation("recipient_type is required".into()));
        }
        let recipient_type = RecipientType::parse(msg.recipient_type.trim())?;
        if recipient_type != RecipientType::Broadcast
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

        self.db.conn().execute(
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
                recipient_type.as_str(),
                if recipient_type == RecipientType::Broadcast {
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
        )?;

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

    pub fn mark_read(&self, message_id: &str) -> Result<(), DbError> {
        if message_id.trim().is_empty() {
            return Err(DbError::Validation("message_id is required".into()));
        }
        let rows = self.db.conn().execute(
            "UPDATE mail_messages SET read_at = ?1 WHERE id = ?2",
            params![crate::now_rfc3339(), message_id],
        )?;
        if rows == 0 {
            return Err(DbError::MailMessageNotFound);
        }
        Ok(())
    }

    pub fn mark_acked(&self, message_id: &str) -> Result<(), DbError> {
        if message_id.trim().is_empty() {
            return Err(DbError::Validation("message_id is required".into()));
        }
        let rows = self.db.conn().execute(
            "UPDATE mail_messages SET acked_at = ?1 WHERE id = ?2",
            params![crate::now_rfc3339(), message_id],
        )?;
        if rows == 0 {
            return Err(DbError::MailMessageNotFound);
        }
        Ok(())
    }
}

fn scan_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MailMessage> {
    let ack_required: i64 = row.get(8)?;
    Ok(MailMessage {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        sender_agent_id: row.get(2)?,
        recipient_type: row.get(3)?,
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

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

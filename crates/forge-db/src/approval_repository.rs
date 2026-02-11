//! Approval repository — CRUD for the `approvals` table with Go parity.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ApprovalStatus {
    #[default]
    Pending,
    Approved,
    Denied,
    Expired,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            other => Err(DbError::Validation(format!(
                "invalid approval status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Approval {
    pub id: String,
    pub agent_id: String,
    pub request_type: String,
    pub request_details_json: String,
    pub status: ApprovalStatus,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: String,
}

pub struct ApprovalRepository<'a> {
    db: &'a Db,
}

impl<'a> ApprovalRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Create inserts a new approval request.
    /// Mirrors Go behavior: auto-id, created_at overwrite, default pending status.
    pub fn create(&self, approval: &mut Approval) -> Result<(), DbError> {
        if approval.agent_id.trim().is_empty() {
            return Err(DbError::Validation("approval agent id is required".into()));
        }
        if approval.request_type.trim().is_empty() {
            return Err(DbError::Validation(
                "approval request type is required".into(),
            ));
        }

        if approval.id.is_empty() {
            approval.id = Uuid::new_v4().to_string();
        }

        approval.created_at = crate::now_rfc3339();

        self.db.conn().execute(
            "INSERT INTO approvals (
                id, agent_id, request_type, request_details_json,
                status, created_at, resolved_at, resolved_by
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                approval.id,
                approval.agent_id,
                approval.request_type,
                approval.request_details_json,
                approval.status.as_str(),
                approval.created_at,
                approval.resolved_at,
                nullable_string(&approval.resolved_by),
            ],
        )?;

        Ok(())
    }

    /// List pending approvals for one agent, oldest first.
    pub fn list_pending_by_agent(&self, agent_id: &str) -> Result<Vec<Approval>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, agent_id, request_type, request_details_json,
                status, created_at, resolved_at, resolved_by
             FROM approvals
             WHERE agent_id = ?1 AND status = 'pending'
             ORDER BY created_at",
        )?;

        let rows = stmt.query_map(params![agent_id], scan_approval)?;
        let mut approvals = Vec::new();
        for row in rows {
            approvals.push(row?);
        }
        Ok(approvals)
    }

    /// Update approval status and resolution metadata.
    pub fn update_status(
        &self,
        id: &str,
        status: ApprovalStatus,
        resolved_by: &str,
    ) -> Result<(), DbError> {
        if id.trim().is_empty() {
            return Err(DbError::Validation("approval id is required".into()));
        }

        let now = crate::now_rfc3339();

        let rows = self.db.conn().execute(
            "UPDATE approvals
             SET status = ?1, resolved_at = ?2, resolved_by = ?3
             WHERE id = ?4",
            params![status.as_str(), now, nullable_string(resolved_by), id],
        )?;

        if rows == 0 {
            return Err(DbError::ApprovalNotFound);
        }

        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Approval, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                    id, agent_id, request_type, request_details_json,
                    status, created_at, resolved_at, resolved_by
                 FROM approvals
                 WHERE id = ?1",
                params![id],
                scan_approval,
            )
            .optional()?;

        result.ok_or(DbError::ApprovalNotFound)
    }
}

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn scan_approval(row: &rusqlite::Row<'_>) -> rusqlite::Result<Approval> {
    let status_str: String = row.get(4)?;
    let status = ApprovalStatus::parse(&status_str).map_err(to_sql_conversion_error)?;

    let resolved_by: Option<String> = row.get(7)?;

    Ok(Approval {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        request_type: row.get(2)?,
        request_details_json: row.get(3)?,
        status,
        created_at: row.get(5)?,
        resolved_at: row.get(6)?,
        resolved_by: resolved_by.unwrap_or_default(),
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

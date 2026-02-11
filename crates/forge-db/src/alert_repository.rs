//! Alert repository — persistence for the `alerts` table.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertType {
    ApprovalNeeded,
    Cooldown,
    Error,
    RateLimit,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApprovalNeeded => "approval_needed",
            Self::Cooldown => "cooldown",
            Self::Error => "error",
            Self::RateLimit => "rate_limit",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "approval_needed" => Ok(Self::ApprovalNeeded),
            "cooldown" => Ok(Self::Cooldown),
            "error" => Ok(Self::Error),
            "rate_limit" => Ok(Self::RateLimit),
            other => Err(DbError::Validation(format!("invalid alert type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AlertSeverity {
    Info,
    #[default]
    Warning,
    Error,
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            "critical" => Ok(Self::Critical),
            other => Err(DbError::Validation(format!(
                "invalid alert severity: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub is_resolved: bool,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

impl Default for Alert {
    fn default() -> Self {
        Self {
            id: String::new(),
            workspace_id: String::new(),
            agent_id: String::new(),
            alert_type: AlertType::Error,
            severity: AlertSeverity::Warning,
            message: String::new(),
            is_resolved: false,
            created_at: String::new(),
            resolved_at: None,
        }
    }
}

pub struct AlertRepository<'a> {
    db: &'a Db,
}

impl<'a> AlertRepository<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn create(&self, alert: &mut Alert) -> Result<(), DbError> {
        if alert.message.trim().is_empty() {
            return Err(DbError::Validation("alert message is required".into()));
        }

        if alert.id.is_empty() {
            alert.id = Uuid::new_v4().to_string();
        }

        alert.created_at = crate::now_rfc3339();

        self.db.conn().execute(
            "INSERT INTO alerts (
                id, workspace_id, agent_id, type,
                severity, message, is_resolved, created_at, resolved_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                alert.id,
                nullable_string(&alert.workspace_id),
                nullable_string(&alert.agent_id),
                alert.alert_type.as_str(),
                alert.severity.as_str(),
                alert.message,
                bool_to_int(alert.is_resolved),
                alert.created_at,
                alert.resolved_at,
            ],
        )?;

        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Alert, DbError> {
        let result = self
            .db
            .conn()
            .query_row(
                "SELECT
                    id, workspace_id, agent_id, type,
                    severity, message, is_resolved, created_at, resolved_at
                 FROM alerts
                 WHERE id = ?1",
                params![id],
                scan_alert,
            )
            .optional()?;

        result.ok_or(DbError::AlertNotFound)
    }

    pub fn list_by_workspace(
        &self,
        workspace_id: &str,
        include_resolved: bool,
    ) -> Result<Vec<Alert>, DbError> {
        let sql = if include_resolved {
            "SELECT
                id, workspace_id, agent_id, type,
                severity, message, is_resolved, created_at, resolved_at
             FROM alerts
             WHERE workspace_id = ?1
             ORDER BY created_at"
        } else {
            "SELECT
                id, workspace_id, agent_id, type,
                severity, message, is_resolved, created_at, resolved_at
             FROM alerts
             WHERE workspace_id = ?1 AND is_resolved = 0
             ORDER BY created_at"
        };

        let mut stmt = self.db.conn().prepare(sql)?;
        let rows = stmt.query_map(params![workspace_id], scan_alert)?;
        let mut alerts = Vec::new();
        for row in rows {
            alerts.push(row?);
        }
        Ok(alerts)
    }

    pub fn list_unresolved_by_agent(&self, agent_id: &str) -> Result<Vec<Alert>, DbError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT
                id, workspace_id, agent_id, type,
                severity, message, is_resolved, created_at, resolved_at
             FROM alerts
             WHERE agent_id = ?1 AND is_resolved = 0
             ORDER BY created_at",
        )?;

        let rows = stmt.query_map(params![agent_id], scan_alert)?;
        let mut alerts = Vec::new();
        for row in rows {
            alerts.push(row?);
        }
        Ok(alerts)
    }

    pub fn resolve(&self, id: &str) -> Result<(), DbError> {
        if id.trim().is_empty() {
            return Err(DbError::Validation("alert id is required".into()));
        }

        let rows = self.db.conn().execute(
            "UPDATE alerts
             SET is_resolved = 1, resolved_at = ?1
             WHERE id = ?2",
            params![crate::now_rfc3339(), id],
        )?;

        if rows == 0 {
            return Err(DbError::AlertNotFound);
        }

        Ok(())
    }
}

fn scan_alert(row: &rusqlite::Row<'_>) -> rusqlite::Result<Alert> {
    let kind: String = row.get(3)?;
    let severity: String = row.get(4)?;
    let is_resolved: i64 = row.get(6)?;

    let alert_type = AlertType::parse(&kind).map_err(to_sql_conversion_error)?;
    let alert_severity = AlertSeverity::parse(&severity).map_err(to_sql_conversion_error)?;

    let workspace_id: Option<String> = row.get(1)?;
    let agent_id: Option<String> = row.get(2)?;

    Ok(Alert {
        id: row.get(0)?,
        workspace_id: workspace_id.unwrap_or_default(),
        agent_id: agent_id.unwrap_or_default(),
        alert_type,
        severity: alert_severity,
        message: row.get(5)?,
        is_resolved: is_resolved != 0,
        created_at: row.get(7)?,
        resolved_at: row.get(8)?,
    })
}

fn nullable_string(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
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

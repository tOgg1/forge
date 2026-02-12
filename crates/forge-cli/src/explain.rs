use std::io::Write;
use std::path::PathBuf;

use chrono::TimeZone;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;

use crate::context::{ContextBackend, FilesystemContextBackend};

const EXPLAIN_EVENT_LIMIT: i64 = 48;
const PERSISTENT_STALE_IDLE_SECONDS: i64 = 3600;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Agent type matching Go's `models.AgentType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentType {
    OpenCode,
    ClaudeCode,
    Codex,
    Gemini,
    Generic,
}

impl AgentType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Generic => "generic",
        }
    }
}

/// Agent state matching Go's `models.AgentState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Working,
    Idle,
    AwaitingApproval,
    RateLimited,
    Error,
    Paused,
    Starting,
    Stopped,
}

impl AgentState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Idle => "idle",
            Self::AwaitingApproval => "awaiting_approval",
            Self::RateLimited => "rate_limited",
            Self::Error => "error",
            Self::Paused => "paused",
            Self::Starting => "starting",
            Self::Stopped => "stopped",
        }
    }

    fn is_blocked(&self) -> bool {
        matches!(
            self,
            Self::AwaitingApproval | Self::RateLimited | Self::Error
        )
    }
}

/// State information associated with an agent.
#[derive(Debug, Clone, Default)]
pub struct StateInfo {
    pub reason: String,
    pub confidence: String,
}

/// Queue item type matching Go's `models.QueueItemType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueItemType {
    Message,
    Pause,
    Conditional,
}

impl QueueItemType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Pause => "pause",
            Self::Conditional => "conditional",
        }
    }
}

/// Queue item status matching Go's `models.QueueItemStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueItemStatus {
    Pending,
    Dispatched,
    Completed,
    Failed,
    Skipped,
}

impl QueueItemStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Condition type for conditional queue items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionType {
    WhenIdle,
    AfterCooldown,
    AfterPrevious,
    Custom,
}

impl ConditionType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::WhenIdle => "when_idle",
            Self::AfterCooldown => "after_cooldown",
            Self::AfterPrevious => "after_previous",
            Self::Custom => "custom",
        }
    }
}

/// Conditional payload attached to a conditional queue item.
#[derive(Debug, Clone)]
pub struct ConditionalPayload {
    pub condition_type: ConditionType,
    pub expression: String,
    pub message: String,
}

/// A full agent record for explain.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub agent_type: AgentType,
    pub state: AgentState,
    pub state_info: StateInfo,
    pub last_activity: Option<String>,
    pub paused_until: Option<String>,
    pub account_id: String,
    pub created_at: Option<String>,
    pub ttl_seconds: Option<i64>,
    pub persistent: bool,
}

/// Account record for cooldown information.
#[derive(Debug, Clone)]
pub struct AccountRecord {
    pub profile_name: String,
    pub cooldown_until: Option<String>,
    pub is_in_cooldown: bool,
}

/// A queue item record for explain.
#[derive(Debug, Clone)]
pub struct QueueItemRecord {
    pub id: String,
    pub agent_id: String,
    pub item_type: QueueItemType,
    pub status: QueueItemStatus,
    pub position: i32,
    pub created_at: String,
    pub content: Option<String>,
    pub condition: Option<ConditionalPayload>,
}

#[derive(Debug, Clone)]
pub struct AgentEventRecord {
    pub kind: String,
    pub outcome: String,
    pub detail: Option<String>,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Backend trait for fetching explain data.
pub trait ExplainBackend {
    /// Resolve an agent by ID or prefix.
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String>;

    /// Load the current agent context from `forge use`.
    fn load_agent_context(&self) -> Result<Option<String>, String>;

    /// Load the current workspace context and return the first agent if any.
    fn load_workspace_first_agent(&self) -> Result<Option<String>, String>;

    /// List queue items for an agent.
    fn list_queue(&self, agent_id: &str) -> Result<Vec<QueueItemRecord>, String>;

    /// Get a queue item by ID.
    fn get_queue_item(&self, item_id: &str) -> Result<QueueItemRecord, String>;

    /// Get account info for an agent (if it has an account).
    fn get_account(&self, account_id: &str) -> Result<Option<AccountRecord>, String>;

    /// List recent persistent-agent events for observability context.
    fn list_agent_events(
        &self,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentEventRecord>, String>;
}

// ---------------------------------------------------------------------------
// In-memory test backend
// ---------------------------------------------------------------------------

/// In-memory backend for testing.
#[derive(Debug, Default)]
pub struct InMemoryExplainBackend {
    pub agents: Vec<AgentRecord>,
    pub queue_items: Vec<QueueItemRecord>,
    pub accounts: Vec<(String, AccountRecord)>,
    pub agent_events: Vec<(String, AgentEventRecord)>,
    pub context_agent_id: Option<String>,
    pub workspace_first_agent_id: Option<String>,
}

impl ExplainBackend for InMemoryExplainBackend {
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String> {
        // Try exact match first
        if let Some(agent) = self.agents.iter().find(|a| a.id == target) {
            return Ok(agent.clone());
        }
        // Try prefix match
        let matches: Vec<&AgentRecord> = self
            .agents
            .iter()
            .filter(|a| a.id.starts_with(target))
            .collect();
        match matches.len() {
            1 => Ok(matches[0].clone()),
            0 => Err(format!("agent '{target}' not found")),
            _ => Err(format!("agent '{target}' is ambiguous")),
        }
    }

    fn load_agent_context(&self) -> Result<Option<String>, String> {
        Ok(self.context_agent_id.clone())
    }

    fn load_workspace_first_agent(&self) -> Result<Option<String>, String> {
        Ok(self.workspace_first_agent_id.clone())
    }

    fn list_queue(&self, agent_id: &str) -> Result<Vec<QueueItemRecord>, String> {
        Ok(self
            .queue_items
            .iter()
            .filter(|qi| qi.agent_id == agent_id)
            .cloned()
            .collect())
    }

    fn get_queue_item(&self, item_id: &str) -> Result<QueueItemRecord, String> {
        self.queue_items
            .iter()
            .find(|qi| qi.id == item_id)
            .cloned()
            .ok_or_else(|| format!("queue item not found: {item_id}"))
    }

    fn get_account(&self, account_id: &str) -> Result<Option<AccountRecord>, String> {
        Ok(self
            .accounts
            .iter()
            .find(|(id, _)| id == account_id)
            .map(|(_, acct)| acct.clone()))
    }

    fn list_agent_events(
        &self,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentEventRecord>, String> {
        let max = if limit <= 0 { 100 } else { limit as usize };
        Ok(self
            .agent_events
            .iter()
            .filter(|(id, _)| id == agent_id)
            .map(|(_, event)| event.clone())
            .take(max)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// SQLite backend (production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SqliteExplainBackend {
    db_path: PathBuf,
    context_backend: FilesystemContextBackend,
}

impl SqliteExplainBackend {
    pub fn open_from_env() -> Self {
        let db_path = resolve_database_path();
        let context_backend = FilesystemContextBackend::default();
        Self {
            db_path,
            context_backend,
        }
    }

    pub fn new(db_path: PathBuf, context_backend: FilesystemContextBackend) -> Self {
        Self {
            db_path,
            context_backend,
        }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }
}

impl ExplainBackend for SqliteExplainBackend {
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err("agent ID required".to_string());
        }
        if !self.db_path.exists() {
            return Err(format!("agent '{trimmed}' not found"));
        }

        let db = self.open_db()?;
        let conn = db.conn();

        if let Some(agent) = resolve_agent_from_agents_table(conn, trimmed)? {
            return Ok(agent);
        }
        if let Some(agent) = resolve_agent_from_persistent_table(conn, trimmed)? {
            return Ok(agent);
        }

        Err(format!("agent '{trimmed}' not found"))
    }

    fn load_agent_context(&self) -> Result<Option<String>, String> {
        let context = self.context_backend.load_context()?;
        if context.agent_id.is_empty() {
            return Ok(None);
        }
        Ok(Some(context.agent_id))
    }

    fn load_workspace_first_agent(&self) -> Result<Option<String>, String> {
        let context = self.context_backend.load_context()?;
        if context.workspace_id.is_empty() {
            return Ok(None);
        }
        if !self.db_path.exists() {
            return Ok(None);
        }

        let db = self.open_db()?;
        let conn = db.conn();

        let row = conn
            .query_row(
                "SELECT id
                 FROM agents
                 WHERE workspace_id = ?1
                 ORDER BY id
                 LIMIT 1",
                rusqlite::params![context.workspace_id],
                |row| row.get::<_, String>(0),
            )
            .optional();
        match row {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => {
                let fallback = conn
                    .query_row(
                        "SELECT id
                         FROM persistent_agents
                         WHERE workspace_id = ?1
                         ORDER BY id
                         LIMIT 1",
                        rusqlite::params![context.workspace_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional();
                match fallback {
                    Ok(value) => Ok(value),
                    Err(err) if err.to_string().contains("no such table: persistent_agents") => {
                        Ok(None)
                    }
                    Err(err) => Err(err.to_string()),
                }
            }
            Err(err) if err.to_string().contains("no such table: agents") => {
                let fallback = conn
                    .query_row(
                        "SELECT id
                         FROM persistent_agents
                         WHERE workspace_id = ?1
                         ORDER BY id
                         LIMIT 1",
                        rusqlite::params![context.workspace_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional();
                match fallback {
                    Ok(value) => Ok(value),
                    Err(err) if err.to_string().contains("no such table: persistent_agents") => {
                        Ok(None)
                    }
                    Err(err) => Err(err.to_string()),
                }
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn list_queue(&self, agent_id: &str) -> Result<Vec<QueueItemRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let mut stmt = match conn.prepare(
            "SELECT
                id,
                agent_id,
                type,
                status,
                position,
                created_at,
                payload_json
             FROM queue_items
             WHERE agent_id = ?1
             ORDER BY position, id",
        ) {
            Ok(stmt) => stmt,
            Err(err) if err.to_string().contains("no such table: queue_items") => {
                return Ok(Vec::new());
            }
            Err(err) => return Err(err.to_string()),
        };

        let rows = stmt
            .query_map(rusqlite::params![agent_id], scan_queue_item_row)
            .map_err(|err| err.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|err| err.to_string())?);
        }
        Ok(out)
    }

    fn get_queue_item(&self, item_id: &str) -> Result<QueueItemRecord, String> {
        if !self.db_path.exists() {
            return Err(format!("queue item not found: {item_id}"));
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let row = conn
            .query_row(
                "SELECT
                    id,
                    agent_id,
                    type,
                    status,
                    position,
                    created_at,
                    payload_json
                 FROM queue_items
                 WHERE id = ?1",
                rusqlite::params![item_id],
                scan_queue_item_row,
            )
            .optional();
        match row {
            Ok(Some(item)) => Ok(item),
            Ok(None) => Err(format!("queue item not found: {item_id}")),
            Err(err) if err.to_string().contains("no such table: queue_items") => {
                Err(format!("queue item not found: {item_id}"))
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn get_account(&self, account_id: &str) -> Result<Option<AccountRecord>, String> {
        if account_id.trim().is_empty() {
            return Ok(None);
        }
        if !self.db_path.exists() {
            return Ok(None);
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let row = conn
            .query_row(
                "SELECT
                    COALESCE(profile_name, ''),
                    NULLIF(cooldown_until, '')
                 FROM accounts
                 WHERE id = ?1",
                rusqlite::params![account_id],
                |row| {
                    let profile_name: String = row.get(0)?;
                    let cooldown_until: Option<String> = row.get(1)?;
                    Ok((profile_name, cooldown_until))
                },
            )
            .optional();

        match row {
            Ok(Some((profile_name, cooldown_until))) => {
                let is_in_cooldown = cooldown_until
                    .as_deref()
                    .and_then(parse_timestamp_utc)
                    .is_some_and(|value| value > chrono::Utc::now());
                Ok(Some(AccountRecord {
                    profile_name,
                    cooldown_until,
                    is_in_cooldown,
                }))
            }
            Ok(None) => Ok(None),
            Err(err) if err.to_string().contains("no such table: accounts") => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn list_agent_events(
        &self,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentEventRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        let max = if limit <= 0 { 100 } else { limit };

        let db = self.open_db()?;
        let conn = db.conn();
        let mut stmt = match conn.prepare(
            "SELECT kind, outcome, detail, timestamp
             FROM persistent_agent_events
             WHERE agent_id = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT ?2",
        ) {
            Ok(stmt) => stmt,
            Err(err)
                if err
                    .to_string()
                    .contains("no such table: persistent_agent_events") =>
            {
                return Ok(Vec::new());
            }
            Err(err) => return Err(err.to_string()),
        };

        let rows = stmt
            .query_map(rusqlite::params![agent_id, max], |row| {
                Ok(AgentEventRecord {
                    kind: row.get(0)?,
                    outcome: row.get(1)?,
                    detail: row.get(2)?,
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|err| err.to_string())?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|err| err.to_string())?);
        }
        Ok(out)
    }
}

fn resolve_agent_from_agents_table(
    conn: &rusqlite::Connection,
    target: &str,
) -> Result<Option<AgentRecord>, String> {
    let exact = conn
        .query_row(
            "SELECT
                id,
                type,
                state,
                COALESCE(state_reason, ''),
                COALESCE(state_confidence, ''),
                NULLIF(last_activity_at, ''),
                NULLIF(paused_until, ''),
                COALESCE(account_id, '')
             FROM agents
             WHERE id = ?1",
            rusqlite::params![target],
            scan_agent_row,
        )
        .optional();
    let exact = match exact {
        Ok(value) => value,
        Err(err) if err.to_string().contains("no such table: agents") => return Ok(None),
        Err(err) => return Err(err.to_string()),
    };
    if exact.is_some() {
        return Ok(exact);
    }

    let mut stmt = match conn.prepare(
        "SELECT
            id,
            type,
            state,
            COALESCE(state_reason, ''),
            COALESCE(state_confidence, ''),
            NULLIF(last_activity_at, ''),
            NULLIF(paused_until, ''),
            COALESCE(account_id, '')
         FROM agents
         WHERE id LIKE ?1
         ORDER BY id
         LIMIT 2",
    ) {
        Ok(stmt) => stmt,
        Err(err) if err.to_string().contains("no such table: agents") => return Ok(None),
        Err(err) => return Err(err.to_string()),
    };

    let like = format!("{target}%");
    let rows = stmt
        .query_map(rusqlite::params![like], scan_agent_row)
        .map_err(|err| err.to_string())?;
    let mut matches = Vec::new();
    for row in rows {
        matches.push(row.map_err(|err| err.to_string())?);
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0))),
        _ => Err(format!("agent '{target}' is ambiguous")),
    }
}

fn resolve_agent_from_persistent_table(
    conn: &rusqlite::Connection,
    target: &str,
) -> Result<Option<AgentRecord>, String> {
    let exact = conn
        .query_row(
            "SELECT
                id,
                harness,
                state,
                NULLIF(last_activity_at, ''),
                NULLIF(created_at, ''),
                ttl_seconds,
                workspace_id
             FROM persistent_agents
             WHERE id = ?1",
            rusqlite::params![target],
            scan_persistent_agent_row,
        )
        .optional();
    let exact = match exact {
        Ok(value) => value,
        Err(err) if err.to_string().contains("no such table: persistent_agents") => {
            return Ok(None)
        }
        Err(err) => return Err(err.to_string()),
    };
    if exact.is_some() {
        return Ok(exact);
    }

    let mut stmt = match conn.prepare(
        "SELECT
            id,
            harness,
            state,
            NULLIF(last_activity_at, ''),
            NULLIF(created_at, ''),
            ttl_seconds,
            workspace_id
         FROM persistent_agents
         WHERE id LIKE ?1
         ORDER BY id
         LIMIT 2",
    ) {
        Ok(stmt) => stmt,
        Err(err) if err.to_string().contains("no such table: persistent_agents") => {
            return Ok(None)
        }
        Err(err) => return Err(err.to_string()),
    };

    let like = format!("{target}%");
    let rows = stmt
        .query_map(rusqlite::params![like], scan_persistent_agent_row)
        .map_err(|err| err.to_string())?;
    let mut matches = Vec::new();
    for row in rows {
        matches.push(row.map_err(|err| err.to_string())?);
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0))),
        _ => Err(format!("agent '{target}' is ambiguous")),
    }
}

fn scan_agent_row(row: &rusqlite::Row) -> rusqlite::Result<AgentRecord> {
    let id: String = row.get(0)?;
    let agent_type: String = row.get(1)?;
    let state: String = row.get(2)?;
    let reason: String = row.get(3)?;
    let confidence: String = row.get(4)?;
    let last_activity: Option<String> = row.get(5)?;
    let paused_until: Option<String> = row.get(6)?;
    let account_id: String = row.get(7)?;
    Ok(AgentRecord {
        id,
        agent_type: map_agent_type(&agent_type),
        state: map_agent_state(&state),
        state_info: StateInfo { reason, confidence },
        last_activity,
        paused_until,
        account_id,
        created_at: None,
        ttl_seconds: None,
        persistent: false,
    })
}

fn scan_persistent_agent_row(row: &rusqlite::Row) -> rusqlite::Result<AgentRecord> {
    let id: String = row.get(0)?;
    let harness: String = row.get(1)?;
    let state: String = row.get(2)?;
    let last_activity: Option<String> = row.get(3)?;
    let created_at: Option<String> = row.get(4)?;
    let ttl_seconds: Option<i64> = row.get(5)?;
    Ok(AgentRecord {
        id,
        agent_type: map_agent_type(&harness),
        state: map_persistent_agent_state(&state),
        state_info: StateInfo::default(),
        last_activity,
        paused_until: None,
        account_id: String::new(),
        created_at,
        ttl_seconds,
        persistent: true,
    })
}

fn scan_queue_item_row(row: &rusqlite::Row) -> rusqlite::Result<QueueItemRecord> {
    let id: String = row.get(0)?;
    let agent_id: String = row.get(1)?;
    let item_type: String = row.get(2)?;
    let status: String = row.get(3)?;
    let position: i64 = row.get(4)?;
    let created_at: String = row.get(5)?;
    let payload_json: String = row.get(6)?;

    let typed_item = map_queue_item_type(&item_type);
    let (content, condition) = parse_queue_payload(&typed_item, &payload_json);

    Ok(QueueItemRecord {
        id,
        agent_id,
        item_type: typed_item,
        status: map_queue_item_status(&status),
        position: i32::try_from(position).unwrap_or(i32::MAX),
        created_at,
        content,
        condition,
    })
}

fn map_agent_type(value: &str) -> AgentType {
    match value {
        "opencode" => AgentType::OpenCode,
        "claude" | "claude_code" | "claude-code" => AgentType::ClaudeCode,
        "codex" => AgentType::Codex,
        "gemini" => AgentType::Gemini,
        _ => AgentType::Generic,
    }
}

fn map_agent_state(value: &str) -> AgentState {
    match value {
        "working" | "running" => AgentState::Working,
        "idle" => AgentState::Idle,
        "awaiting_approval" | "waiting_approval" => AgentState::AwaitingApproval,
        "rate_limited" => AgentState::RateLimited,
        "error" | "failed" => AgentState::Error,
        "paused" => AgentState::Paused,
        "starting" => AgentState::Starting,
        _ => AgentState::Stopped,
    }
}

fn map_persistent_agent_state(value: &str) -> AgentState {
    match value {
        "unspecified" => AgentState::Starting,
        "starting" => AgentState::Starting,
        "running" => AgentState::Working,
        "idle" => AgentState::Idle,
        "waiting_approval" => AgentState::AwaitingApproval,
        "paused" => AgentState::Paused,
        "stopping" | "stopped" => AgentState::Stopped,
        "failed" => AgentState::Error,
        _ => AgentState::Stopped,
    }
}

fn map_queue_item_type(value: &str) -> QueueItemType {
    match value {
        "message" => QueueItemType::Message,
        "pause" => QueueItemType::Pause,
        _ => QueueItemType::Conditional,
    }
}

fn map_queue_item_status(value: &str) -> QueueItemStatus {
    match value {
        "pending" => QueueItemStatus::Pending,
        "dispatched" => QueueItemStatus::Dispatched,
        "completed" => QueueItemStatus::Completed,
        "failed" => QueueItemStatus::Failed,
        "skipped" => QueueItemStatus::Skipped,
        _ => QueueItemStatus::Pending,
    }
}

fn map_condition_type(value: &str) -> ConditionType {
    match value {
        "when_idle" => ConditionType::WhenIdle,
        "after_cooldown" => ConditionType::AfterCooldown,
        "after_previous" => ConditionType::AfterPrevious,
        _ => ConditionType::Custom,
    }
}

#[derive(Debug, Default, Deserialize)]
struct MessagePayload {
    text: String,
}

#[derive(Debug, Default, Deserialize)]
struct PausePayload {
    duration_seconds: i64,
    reason: String,
}

#[derive(Debug, Default, Deserialize)]
struct ConditionalPayloadJson {
    condition_type: String,
    expression: String,
    message: String,
}

fn parse_queue_payload(
    item_type: &QueueItemType,
    payload_json: &str,
) -> (Option<String>, Option<ConditionalPayload>) {
    match item_type {
        QueueItemType::Message => {
            let payload = serde_json::from_str::<MessagePayload>(payload_json).ok();
            let content = payload
                .map(|value| truncate_string(&value.text, 100))
                .filter(|value| !value.is_empty());
            (content, None)
        }
        QueueItemType::Pause => {
            let payload = serde_json::from_str::<PausePayload>(payload_json).ok();
            let content = payload
                .and_then(|value| {
                    if value.duration_seconds <= 0 {
                        return None;
                    }
                    if value.reason.trim().is_empty() {
                        return Some(format!("{}s pause", value.duration_seconds));
                    }
                    Some(format!(
                        "{}s pause ({})",
                        value.duration_seconds, value.reason
                    ))
                })
                .filter(|value| !value.is_empty());
            (content, None)
        }
        QueueItemType::Conditional => {
            let payload = serde_json::from_str::<ConditionalPayloadJson>(payload_json).ok();
            match payload {
                Some(value) => {
                    let content = if value.message.is_empty() {
                        None
                    } else {
                        Some(truncate_string(&value.message, 100))
                    };
                    let condition = Some(ConditionalPayload {
                        condition_type: map_condition_type(&value.condition_type),
                        expression: value.expression,
                        message: value.message,
                    });
                    (content, condition)
                }
                None => (None, None),
            }
        }
    }
}

fn parse_timestamp_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&chrono::Utc));
    }
    if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::Utc.from_utc_datetime(&parsed));
    }
    if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Some(chrono::Utc.from_utc_datetime(&parsed));
    }
    None
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}

// ---------------------------------------------------------------------------
// CLI entry points
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn ExplainBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &dyn ExplainBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Parsed arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    target: Option<String>,
    json: bool,
    jsonl: bool,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|t| t == "explain") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut target: Option<String> = None;

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err(HELP_TEXT.to_string());
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for explain: '{flag}'"));
            }
            positional => {
                if target.is_some() {
                    return Err("error: explain takes at most one positional argument".to_string());
                }
                target = Some(positional.to_string());
                index += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs {
        target,
        json,
        jsonl,
    })
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn ExplainBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    let target = match parsed.target.as_ref() {
        Some(t) => t.clone(),
        None => resolve_context_target(backend)?,
    };

    // If target starts with "qi_", explain as queue item
    if target.starts_with("qi_") {
        return explain_queue_item(&target, backend, &parsed, stdout);
    }

    explain_agent(&target, backend, &parsed, stdout)
}

fn resolve_context_target(backend: &dyn ExplainBackend) -> Result<String, String> {
    // Try agent context first
    if let Some(agent_id) = backend.load_agent_context()? {
        if !agent_id.is_empty() {
            return Ok(agent_id);
        }
    }
    // Try workspace context and get first agent
    if let Some(agent_id) = backend.load_workspace_first_agent()? {
        if !agent_id.is_empty() {
            return Ok(agent_id);
        }
    }
    Err(
        "no agent specified and no context set (use 'forge use <agent>' or provide agent ID)"
            .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Agent explanation
// ---------------------------------------------------------------------------

fn explain_agent(
    target: &str,
    backend: &dyn ExplainBackend,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let agent = backend.resolve_agent(target)?;
    let queue_items = backend.list_queue(&agent.id)?;
    let events = backend.list_agent_events(&agent.id, EXPLAIN_EVENT_LIMIT)?;

    let mut explanation = build_agent_explanation(&agent, &queue_items);

    // Get account info if available
    if !agent.account_id.is_empty() {
        if let Some(account) = backend.get_account(&agent.account_id)? {
            let acct_status = AccountExplanation {
                profile_name: account.profile_name,
                cooldown_until: account.cooldown_until,
                is_in_cooldown: account.is_in_cooldown,
            };
            if acct_status.is_in_cooldown {
                explanation
                    .block_reasons
                    .push("account cooldown active".to_string());
            }
            explanation.account_status = Some(acct_status);
        }
    }

    apply_observability(&mut explanation, &agent, &events);

    if parsed.json || parsed.jsonl {
        return write_agent_json(&explanation, parsed, stdout);
    }

    write_agent_explanation_human(&explanation, stdout)
}

#[derive(Debug, Clone, Serialize)]
struct AgentExplanationJson<'a> {
    agent_id: &'a str,
    #[serde(rename = "type")]
    agent_type: &'a str,
    state: &'a str,
    state_info: StateInfoJson<'a>,
    is_blocked: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    block_reasons: &'a Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suggestions: &'a Vec<String>,
    queue_status: QueueExplanationJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_status: Option<AccountExplanationJson<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observability: Option<ObservabilityJson<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_activity: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    paused_until: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
struct StateInfoJson<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    reason: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    confidence: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct QueueExplanationJson {
    total_items: i32,
    pending_items: i32,
    blocked_items: i32,
}

#[derive(Debug, Clone, Serialize)]
struct AccountExplanationJson<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    profile_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cooldown_until: Option<&'a str>,
    is_in_cooldown: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ObservabilityJson<'a> {
    persistent: bool,
    stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale_reason: Option<&'a str>,
    recent_failure_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_failure: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_send_to_idle_ms: Option<i64>,
}

struct AgentExplanation {
    agent_id: String,
    agent_type: AgentType,
    state: AgentState,
    state_info: StateInfo,
    is_blocked: bool,
    block_reasons: Vec<String>,
    suggestions: Vec<String>,
    queue_status: QueueExplanation,
    account_status: Option<AccountExplanation>,
    observability: Option<ObservabilitySummary>,
    last_activity: Option<String>,
    paused_until: Option<String>,
}

struct QueueExplanation {
    total_items: i32,
    pending_items: i32,
    blocked_items: i32,
}

struct AccountExplanation {
    profile_name: String,
    cooldown_until: Option<String>,
    is_in_cooldown: bool,
}

struct ObservabilitySummary {
    persistent: bool,
    stale: bool,
    stale_reason: Option<String>,
    recent_failure_count: usize,
    last_failure: Option<String>,
    last_send_to_idle_ms: Option<i64>,
}

fn build_agent_explanation(
    agent: &AgentRecord,
    queue_items: &[QueueItemRecord],
) -> AgentExplanation {
    let is_blocked = agent.state.is_blocked() || agent.state == AgentState::Paused;

    let mut explanation = AgentExplanation {
        agent_id: agent.id.clone(),
        agent_type: agent.agent_type.clone(),
        state: agent.state.clone(),
        state_info: agent.state_info.clone(),
        is_blocked,
        block_reasons: Vec::new(),
        suggestions: Vec::new(),
        queue_status: QueueExplanation {
            total_items: 0,
            pending_items: 0,
            blocked_items: 0,
        },
        account_status: None,
        observability: None,
        last_activity: agent.last_activity.clone(),
        paused_until: agent.paused_until.clone(),
    };

    // Count queue items
    for item in queue_items {
        explanation.queue_status.total_items += 1;
        if item.status == QueueItemStatus::Pending {
            explanation.queue_status.pending_items += 1;
        }
    }

    let short = short_id(&agent.id);

    // Determine block reasons and suggestions
    match agent.state {
        AgentState::AwaitingApproval => {
            explanation
                .block_reasons
                .push("waiting for user approval".to_string());
            explanation.suggestions.push(format!(
                "Approve pending request: forge agent approve {short}"
            ));
        }
        AgentState::RateLimited => {
            explanation
                .block_reasons
                .push("rate limited by provider".to_string());
            explanation
                .suggestions
                .push("Wait for rate limit to expire".to_string());
            explanation.suggestions.push(format!(
                "Switch to different account: forge agent rotate {short}"
            ));
        }
        AgentState::Error => {
            explanation
                .block_reasons
                .push("agent encountered an error".to_string());
            if !agent.state_info.reason.is_empty() {
                explanation
                    .block_reasons
                    .push(agent.state_info.reason.clone());
            }
            explanation
                .suggestions
                .push(format!("Check agent status: forge agent status {short}"));
            explanation
                .suggestions
                .push(format!("Restart agent: forge agent restart {short}"));
        }
        AgentState::Paused => {
            explanation
                .block_reasons
                .push("agent is paused".to_string());
            if agent.paused_until.is_some() {
                explanation
                    .block_reasons
                    .push("will resume when pause expires".to_string());
            }
            explanation
                .suggestions
                .push(format!("Resume agent: forge agent resume {short}"));
        }
        AgentState::Working => {
            explanation.suggestions.push(format!(
                "Wait for completion: forge wait --agent {short} --until idle"
            ));
            explanation.suggestions.push(format!(
                "Queue a message: forge send {short} \"your message\""
            ));
        }
        AgentState::Idle => {
            if explanation.queue_status.pending_items > 0 {
                explanation.suggestions.push(
                    "Queue items are pending - scheduler will dispatch next item".to_string(),
                );
            } else {
                explanation.suggestions.push(format!(
                    "Send a message: forge send {short} \"your prompt\""
                ));
            }
        }
        AgentState::Starting | AgentState::Stopped => {}
    }

    explanation
}

fn apply_observability(
    explanation: &mut AgentExplanation,
    agent: &AgentRecord,
    events: &[AgentEventRecord],
) {
    let mut summary = ObservabilitySummary {
        persistent: agent.persistent,
        stale: false,
        stale_reason: None,
        recent_failure_count: 0,
        last_failure: None,
        last_send_to_idle_ms: None,
    };

    if agent.persistent {
        if let Some(reason) = stale_reason(agent) {
            summary.stale = true;
            summary.stale_reason = Some(reason.clone());
            explanation.is_blocked = true;
            explanation.block_reasons.push(format!("stale: {reason}"));
            explanation
                .suggestions
                .push("Inspect and clean stale agents: forge agent gc --dry-run".to_string());
        }
    }

    for event in events {
        let outcome = event.outcome.to_ascii_lowercase();
        if outcome.contains("error") || outcome.contains("fail") {
            summary.recent_failure_count += 1;
            if summary.last_failure.is_none() {
                let message = event
                    .detail
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&event.outcome)
                    .to_string();
                summary.last_failure = Some(message);
            }
        }

        if event.kind == "metric_send_to_idle_duration" {
            if let Some(value) = event.detail.as_deref().and_then(extract_metric_value_ms) {
                summary.last_send_to_idle_ms = Some(value);
            }
        }
    }

    if summary.recent_failure_count > 0 {
        explanation.is_blocked = true;
        explanation
            .block_reasons
            .push(format!("recent failures: {}", summary.recent_failure_count));
        explanation.suggestions.push(format!(
            "Inspect recent audit trail: forge agent summary {}",
            short_id(&agent.id)
        ));
    }

    if summary.persistent
        || summary.recent_failure_count > 0
        || summary.last_send_to_idle_ms.is_some()
    {
        explanation.observability = Some(summary);
    }
}

fn stale_reason(agent: &AgentRecord) -> Option<String> {
    let now = chrono::Utc::now();

    if let (Some(created_at), Some(ttl)) = (agent.created_at.as_deref(), agent.ttl_seconds) {
        if ttl > 0 {
            if let Some(created) = parse_timestamp_utc(created_at) {
                let age = now.signed_duration_since(created).num_seconds().max(0);
                if age >= ttl {
                    return Some(format!("ttl expired ({age}s >= {ttl}s)"));
                }
            }
        }
    }

    if matches!(
        agent.state,
        AgentState::Idle | AgentState::Stopped | AgentState::Error
    ) {
        if let Some(last) = agent.last_activity.as_deref().and_then(parse_timestamp_utc) {
            let idle_for = now.signed_duration_since(last).num_seconds().max(0);
            if idle_for >= PERSISTENT_STALE_IDLE_SECONDS {
                return Some(format!(
                    "idle for {}s (>= {}s)",
                    idle_for, PERSISTENT_STALE_IDLE_SECONDS
                ));
            }
        }
    }

    None
}

fn extract_metric_value_ms(detail: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(detail)
        .ok()
        .and_then(|value| value.get("value_ms").and_then(|item| item.as_i64()))
}

fn write_agent_explanation_human(
    e: &AgentExplanation,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let state_display = format_agent_state(&e.state);

    if e.is_blocked {
        writeln!(
            stdout,
            "Agent {} is {} (BLOCKED)",
            short_id(&e.agent_id),
            state_display
        )
        .map_err(|err| err.to_string())?;
    } else {
        writeln!(
            stdout,
            "Agent {} is {}",
            short_id(&e.agent_id),
            state_display
        )
        .map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;

    // State details
    writeln!(stdout, "Type: {}", e.agent_type.as_str()).map_err(|err| err.to_string())?;
    if !e.state_info.reason.is_empty() {
        writeln!(stdout, "Reason: {}", e.state_info.reason).map_err(|err| err.to_string())?;
    }
    if !e.state_info.confidence.is_empty() {
        writeln!(stdout, "Confidence: {}", e.state_info.confidence)
            .map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;

    // Block reasons
    if !e.block_reasons.is_empty() {
        writeln!(stdout, "Block Reasons:").map_err(|err| err.to_string())?;
        for reason in &e.block_reasons {
            writeln!(stdout, "  - {reason}").map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    // Queue status
    writeln!(stdout, "Queue Status:").map_err(|err| err.to_string())?;
    writeln!(stdout, "  Total items: {}", e.queue_status.total_items)
        .map_err(|err| err.to_string())?;
    writeln!(stdout, "  Pending: {}", e.queue_status.pending_items)
        .map_err(|err| err.to_string())?;
    writeln!(stdout).map_err(|err| err.to_string())?;

    // Account status
    if let Some(acct) = &e.account_status {
        writeln!(stdout, "Account Status:").map_err(|err| err.to_string())?;
        if !acct.profile_name.is_empty() {
            writeln!(stdout, "  Profile: {}", acct.profile_name).map_err(|err| err.to_string())?;
        }
        if acct.is_in_cooldown {
            if let Some(until) = &acct.cooldown_until {
                writeln!(stdout, "  Cooldown: active (ends at {until})")
                    .map_err(|err| err.to_string())?;
            } else {
                writeln!(stdout, "  Cooldown: active").map_err(|err| err.to_string())?;
            }
        } else {
            writeln!(stdout, "  Cooldown: none").map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    if let Some(obs) = &e.observability {
        writeln!(stdout, "Observability:").map_err(|err| err.to_string())?;
        writeln!(stdout, "  Persistent: {}", obs.persistent).map_err(|err| err.to_string())?;
        writeln!(stdout, "  Stale: {}", obs.stale).map_err(|err| err.to_string())?;
        if let Some(reason) = &obs.stale_reason {
            writeln!(stdout, "  Stale reason: {reason}").map_err(|err| err.to_string())?;
        }
        writeln!(stdout, "  Recent failures: {}", obs.recent_failure_count)
            .map_err(|err| err.to_string())?;
        if let Some(last_failure) = &obs.last_failure {
            writeln!(stdout, "  Last failure: {last_failure}").map_err(|err| err.to_string())?;
        }
        if let Some(ms) = obs.last_send_to_idle_ms {
            writeln!(stdout, "  Last send->idle: {ms}ms").map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    // Suggestions
    if !e.suggestions.is_empty() {
        writeln!(stdout, "Suggestions:").map_err(|err| err.to_string())?;
        for (i, suggestion) in e.suggestions.iter().enumerate() {
            writeln!(stdout, "  {}. {suggestion}", i + 1).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn write_json<T: Serialize>(value: &T, jsonl: bool, stdout: &mut dyn Write) -> Result<(), String> {
    if jsonl {
        serde_json::to_writer(&mut *stdout, value).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, value).map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

impl AgentExplanation {
    fn to_json(&self) -> AgentExplanationJson<'_> {
        AgentExplanationJson {
            agent_id: &self.agent_id,
            agent_type: self.agent_type.as_str(),
            state: self.state.as_str(),
            state_info: StateInfoJson {
                reason: &self.state_info.reason,
                confidence: &self.state_info.confidence,
            },
            is_blocked: self.is_blocked,
            block_reasons: &self.block_reasons,
            suggestions: &self.suggestions,
            queue_status: QueueExplanationJson {
                total_items: self.queue_status.total_items,
                pending_items: self.queue_status.pending_items,
                blocked_items: self.queue_status.blocked_items,
            },
            account_status: self
                .account_status
                .as_ref()
                .map(|acct| AccountExplanationJson {
                    profile_name: &acct.profile_name,
                    cooldown_until: acct.cooldown_until.as_deref(),
                    is_in_cooldown: acct.is_in_cooldown,
                }),
            observability: self.observability.as_ref().map(|obs| ObservabilityJson {
                persistent: obs.persistent,
                stale: obs.stale,
                stale_reason: obs.stale_reason.as_deref(),
                recent_failure_count: obs.recent_failure_count,
                last_failure: obs.last_failure.as_deref(),
                last_send_to_idle_ms: obs.last_send_to_idle_ms,
            }),
            last_activity: self.last_activity.as_deref(),
            paused_until: self.paused_until.as_deref(),
        }
    }
}

// Override write_json for AgentExplanation to use the conversion
fn write_agent_json(
    explanation: &AgentExplanation,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let json_val = explanation.to_json();
    write_json(&json_val, parsed.jsonl, stdout)
}

// ---------------------------------------------------------------------------
// Queue item explanation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct QueueItemExplanationJson<'a> {
    item_id: &'a str,
    agent_id: &'a str,
    #[serde(rename = "type")]
    item_type: &'a str,
    status: &'a str,
    position: i32,
    is_blocked: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    block_reasons: &'a Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suggestions: &'a Vec<String>,
    agent_state: &'a str,
    created_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<ConditionJson<'a>>,
}

#[derive(Debug, Clone, Serialize)]
struct ConditionJson<'a> {
    condition_type: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    expression: &'a str,
}

struct QueueItemExplanation {
    item_id: String,
    agent_id: String,
    item_type: QueueItemType,
    status: QueueItemStatus,
    position: i32,
    is_blocked: bool,
    block_reasons: Vec<String>,
    suggestions: Vec<String>,
    agent_state: AgentState,
    created_at: String,
    content: Option<String>,
    condition: Option<ConditionalPayload>,
}

impl QueueItemExplanation {
    fn to_json(&self) -> QueueItemExplanationJson<'_> {
        QueueItemExplanationJson {
            item_id: &self.item_id,
            agent_id: &self.agent_id,
            item_type: self.item_type.as_str(),
            status: self.status.as_str(),
            position: self.position,
            is_blocked: self.is_blocked,
            block_reasons: &self.block_reasons,
            suggestions: &self.suggestions,
            agent_state: self.agent_state.as_str(),
            created_at: &self.created_at,
            content: self.content.as_deref(),
            condition: self.condition.as_ref().map(|c| ConditionJson {
                condition_type: c.condition_type.as_str(),
                expression: &c.expression,
            }),
        }
    }
}

fn explain_queue_item(
    item_id: &str,
    backend: &dyn ExplainBackend,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let item = backend.get_queue_item(item_id)?;
    let agent = backend.resolve_agent(&item.agent_id)?;

    let explanation = build_queue_item_explanation(&item, &agent);

    if parsed.json || parsed.jsonl {
        let json_val = explanation.to_json();
        return write_json(&json_val, parsed.jsonl, stdout);
    }

    write_queue_item_explanation_human(&explanation, stdout)
}

fn build_queue_item_explanation(
    item: &QueueItemRecord,
    agent: &AgentRecord,
) -> QueueItemExplanation {
    let mut explanation = QueueItemExplanation {
        item_id: item.id.clone(),
        agent_id: item.agent_id.clone(),
        item_type: item.item_type.clone(),
        status: item.status.clone(),
        position: item.position,
        is_blocked: false,
        block_reasons: Vec::new(),
        suggestions: Vec::new(),
        agent_state: agent.state.clone(),
        created_at: item.created_at.clone(),
        content: item.content.clone(),
        condition: item.condition.clone(),
    };

    if item.status == QueueItemStatus::Pending {
        let short = short_id(&agent.id);

        match agent.state {
            AgentState::Working => {
                explanation.is_blocked = true;
                explanation
                    .block_reasons
                    .push("agent is currently working".to_string());
                explanation
                    .suggestions
                    .push("Wait for agent to become idle".to_string());
                explanation
                    .suggestions
                    .push(format!("forge wait --agent {short} --until idle"));
            }
            AgentState::AwaitingApproval => {
                explanation.is_blocked = true;
                explanation
                    .block_reasons
                    .push("agent is waiting for approval".to_string());
                explanation.suggestions.push(format!(
                    "Approve pending request: forge agent approve {short}"
                ));
            }
            AgentState::Paused => {
                explanation.is_blocked = true;
                explanation
                    .block_reasons
                    .push("agent is paused".to_string());
                if agent.paused_until.is_some() {
                    explanation
                        .block_reasons
                        .push("will resume when pause expires".to_string());
                }
                explanation
                    .suggestions
                    .push(format!("Resume agent: forge agent resume {short}"));
            }
            AgentState::Error | AgentState::Stopped => {
                explanation.is_blocked = true;
                explanation
                    .block_reasons
                    .push(format!("agent is in {} state", agent.state.as_str()));
                explanation
                    .suggestions
                    .push(format!("Check agent status: forge agent status {short}"));
                explanation
                    .suggestions
                    .push(format!("Restart agent: forge agent restart {short}"));
            }
            AgentState::RateLimited => {
                explanation.is_blocked = true;
                explanation
                    .block_reasons
                    .push("agent is rate limited".to_string());
                explanation
                    .suggestions
                    .push("Wait for rate limit to expire".to_string());
                explanation
                    .suggestions
                    .push(format!("Rotate account: forge agent rotate {short}"));
            }
            AgentState::Idle | AgentState::Starting => {}
        }

        // Check conditional gates
        if item.item_type == QueueItemType::Conditional {
            if let Some(condition) = &item.condition {
                match condition.condition_type {
                    ConditionType::WhenIdle => {
                        if agent.state != AgentState::Idle {
                            explanation.is_blocked = true;
                            explanation
                                .block_reasons
                                .push("conditional: waiting for agent to be idle".to_string());
                        }
                    }
                    ConditionType::AfterCooldown => {
                        explanation.is_blocked = true;
                        explanation
                            .block_reasons
                            .push("conditional: waiting for cooldown to expire".to_string());
                    }
                    ConditionType::AfterPrevious | ConditionType::Custom => {}
                }
            }
        }

        // Position-based blocking
        if item.position > 1 && !explanation.is_blocked {
            explanation.block_reasons.push(format!(
                "waiting for {} items ahead in queue",
                item.position - 1
            ));
        }
    }

    explanation
}

fn write_queue_item_explanation_human(
    e: &QueueItemExplanation,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let status_display = if e.is_blocked {
        format!("{} (BLOCKED)", e.status.as_str())
    } else {
        e.status.as_str().to_string()
    };
    writeln!(stdout, "Queue Item {} is {}", e.item_id, status_display)
        .map_err(|err| err.to_string())?;
    writeln!(stdout).map_err(|err| err.to_string())?;

    // Details
    writeln!(stdout, "Type: {}", e.item_type.as_str()).map_err(|err| err.to_string())?;
    writeln!(stdout, "Position: {}", e.position).map_err(|err| err.to_string())?;
    writeln!(
        stdout,
        "Agent: {} ({})",
        short_id(&e.agent_id),
        format_agent_state(&e.agent_state)
    )
    .map_err(|err| err.to_string())?;
    writeln!(stdout, "Created: {}", e.created_at).map_err(|err| err.to_string())?;
    if let Some(content) = &e.content {
        writeln!(stdout, "Content: {content}").map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;

    // Condition details
    if let Some(condition) = &e.condition {
        writeln!(stdout, "Condition:").map_err(|err| err.to_string())?;
        writeln!(stdout, "  Type: {}", condition.condition_type.as_str())
            .map_err(|err| err.to_string())?;
        if !condition.expression.is_empty() {
            writeln!(stdout, "  Expression: {}", condition.expression)
                .map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    // Block reasons
    if !e.block_reasons.is_empty() {
        writeln!(stdout, "Block Reasons:").map_err(|err| err.to_string())?;
        for reason in &e.block_reasons {
            writeln!(stdout, "  - {reason}").map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    // Suggestions
    if !e.suggestions.is_empty() {
        writeln!(stdout, "Suggestions:").map_err(|err| err.to_string())?;
        for (i, suggestion) in e.suggestions.iter().enumerate() {
            writeln!(stdout, "  {}. {suggestion}", i + 1).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn short_id(id: &str) -> &str {
    const LIMIT: usize = 8;
    if id.len() <= LIMIT {
        id
    } else {
        &id[..LIMIT]
    }
}

fn format_agent_state(state: &AgentState) -> &'static str {
    state.as_str()
}

#[allow(dead_code)]
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len < 3 {
        s[..max_len].to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

const HELP_TEXT: &str = "\
Explain agent or queue item status

Show a human-readable explanation of why an agent or queue item is in its current state.

If no argument is given, explains the agent from the current context (set with 'forge use').

Usage:
  forge explain [agent-id|queue-item-id] [flags]

Examples:
  forge explain abc123        # Explain agent status
  forge explain qi_789        # Explain queue item status
  forge explain               # Explain context agent

Flags:
  -h, --help    help for explain";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_agent(id: &str, state: AgentState) -> AgentRecord {
        AgentRecord {
            id: id.to_string(),
            agent_type: AgentType::OpenCode,
            state,
            state_info: StateInfo::default(),
            last_activity: None,
            paused_until: None,
            account_id: String::new(),
            created_at: None,
            ttl_seconds: None,
            persistent: false,
        }
    }

    fn make_queue_item(id: &str, agent_id: &str, status: QueueItemStatus) -> QueueItemRecord {
        QueueItemRecord {
            id: id.to_string(),
            agent_id: agent_id.to_string(),
            item_type: QueueItemType::Message,
            status,
            position: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            content: None,
            condition: None,
        }
    }

    // --- parse_args tests ---

    fn s(val: &str) -> String {
        val.to_string()
    }

    #[test]
    fn parse_no_args() {
        let args = vec![s("explain")];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.target.is_none());
        assert!(!parsed.json);
        assert!(!parsed.jsonl);
    }

    #[test]
    fn parse_with_target() {
        let args = vec![s("explain"), s("agent_123")];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.target.as_deref(), Some("agent_123"));
    }

    #[test]
    fn parse_json_flag() {
        let args = vec![s("explain"), s("--json"), s("agent_123")];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.json);
        assert_eq!(parsed.target.as_deref(), Some("agent_123"));
    }

    #[test]
    fn parse_jsonl_flag() {
        let args = vec![s("explain"), s("--jsonl")];
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.jsonl);
    }

    #[test]
    fn parse_rejects_json_and_jsonl() {
        let args = vec![s("explain"), s("--json"), s("--jsonl")];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let args = vec![s("explain"), s("--bogus")];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown argument for explain"));
    }

    #[test]
    fn parse_rejects_multiple_positional() {
        let args = vec![s("explain"), s("arg1"), s("arg2")];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("at most one positional argument"));
    }

    #[test]
    fn parse_help_flag() {
        let args = vec![s("explain"), s("--help")];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Explain agent or queue item status"));
    }

    #[test]
    fn parse_short_help_flag() {
        let args = vec![s("explain"), s("-h")];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Explain agent or queue item status"));
    }

    // --- build_agent_explanation tests ---

    #[test]
    fn agent_explanation_idle() {
        let agent = make_agent("agent_123", AgentState::Idle);
        let explanation = build_agent_explanation(&agent, &[]);

        assert_eq!(explanation.agent_id, "agent_123");
        assert_eq!(explanation.state, AgentState::Idle);
        assert!(!explanation.is_blocked);
        assert!(!explanation.suggestions.is_empty());
    }

    #[test]
    fn agent_explanation_idle_with_pending_queue() {
        let agent = make_agent("agent_123", AgentState::Idle);
        let items = vec![
            make_queue_item("qi_1", "agent_123", QueueItemStatus::Pending),
            make_queue_item("qi_2", "agent_123", QueueItemStatus::Pending),
        ];
        let explanation = build_agent_explanation(&agent, &items);

        assert_eq!(explanation.queue_status.total_items, 2);
        assert_eq!(explanation.queue_status.pending_items, 2);
        assert!(
            explanation.suggestions[0].contains("scheduler will dispatch"),
            "got: {}",
            explanation.suggestions[0]
        );
    }

    #[test]
    fn agent_explanation_awaiting_approval() {
        let agent = make_agent("agent_123", AgentState::AwaitingApproval);
        let explanation = build_agent_explanation(&agent, &[]);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("approval")));
        assert!(explanation
            .suggestions
            .iter()
            .any(|s| s.contains("approve")));
    }

    #[test]
    fn agent_explanation_rate_limited() {
        let agent = make_agent("agent_123", AgentState::RateLimited);
        let explanation = build_agent_explanation(&agent, &[]);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("rate limited")));
        assert!(explanation
            .suggestions
            .iter()
            .any(|s| s.contains("rate limit")));
        assert!(explanation.suggestions.iter().any(|s| s.contains("rotate")));
    }

    #[test]
    fn agent_explanation_error() {
        let mut agent = make_agent("agent_123", AgentState::Error);
        agent.state_info.reason = "connection timeout".to_string();
        let explanation = build_agent_explanation(&agent, &[]);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("error")));
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("connection timeout")));
        assert!(explanation.suggestions.iter().any(|s| s.contains("status")));
        assert!(explanation
            .suggestions
            .iter()
            .any(|s| s.contains("restart")));
    }

    #[test]
    fn agent_explanation_paused() {
        let mut agent = make_agent("agent_123", AgentState::Paused);
        agent.paused_until = Some("2026-01-01T01:00:00Z".to_string());
        let explanation = build_agent_explanation(&agent, &[]);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("paused")));
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("resume")));
        assert!(explanation.suggestions.iter().any(|s| s.contains("resume")));
    }

    #[test]
    fn agent_explanation_working() {
        let agent = make_agent("agent_123", AgentState::Working);
        let explanation = build_agent_explanation(&agent, &[]);

        assert!(!explanation.is_blocked);
        assert!(explanation.suggestions.iter().any(|s| s.contains("wait")));
        assert!(explanation.suggestions.iter().any(|s| s.contains("send")));
    }

    #[test]
    fn agent_explanation_with_mixed_queue() {
        let agent = make_agent("agent_123", AgentState::Idle);
        let items = vec![
            make_queue_item("qi_1", "agent_123", QueueItemStatus::Pending),
            make_queue_item("qi_2", "agent_123", QueueItemStatus::Completed),
            make_queue_item("qi_3", "agent_123", QueueItemStatus::Pending),
        ];
        let explanation = build_agent_explanation(&agent, &items);

        assert_eq!(explanation.queue_status.total_items, 3);
        assert_eq!(explanation.queue_status.pending_items, 2);
    }

    // --- build_queue_item_explanation tests ---

    #[test]
    fn queue_item_pending_agent_working() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let agent = make_agent("agent_123", AgentState::Working);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("currently working")));
    }

    #[test]
    fn queue_item_pending_agent_idle_first_position() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let agent = make_agent("agent_123", AgentState::Idle);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(!explanation.is_blocked);
    }

    #[test]
    fn queue_item_pending_agent_idle_later_position() {
        let mut item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        item.position = 3;
        let agent = make_agent("agent_123", AgentState::Idle);
        let explanation = build_queue_item_explanation(&item, &agent);

        // Position-based blocking (not marked as is_blocked, but has block_reasons)
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("waiting for 2 items ahead")));
    }

    #[test]
    fn queue_item_pending_agent_error() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let agent = make_agent("agent_123", AgentState::Error);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("error state")));
    }

    #[test]
    fn queue_item_pending_agent_paused() {
        let mut agent = make_agent("agent_123", AgentState::Paused);
        agent.paused_until = Some("2026-01-01T01:00:00Z".to_string());
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("paused")));
    }

    #[test]
    fn queue_item_conditional_when_idle_agent_working() {
        let mut item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        item.item_type = QueueItemType::Conditional;
        item.condition = Some(ConditionalPayload {
            condition_type: ConditionType::WhenIdle,
            expression: String::new(),
            message: "do something".to_string(),
        });
        let agent = make_agent("agent_123", AgentState::Working);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("conditional: waiting for agent to be idle")));
    }

    #[test]
    fn queue_item_conditional_after_cooldown() {
        let mut item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        item.item_type = QueueItemType::Conditional;
        item.condition = Some(ConditionalPayload {
            condition_type: ConditionType::AfterCooldown,
            expression: String::new(),
            message: "do something".to_string(),
        });
        let agent = make_agent("agent_123", AgentState::Idle);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("cooldown to expire")));
    }

    #[test]
    fn queue_item_completed_not_blocked() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Completed);
        let agent = make_agent("agent_123", AgentState::Working);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(!explanation.is_blocked);
        assert!(explanation.block_reasons.is_empty());
    }

    #[test]
    fn queue_item_agent_rate_limited() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let agent = make_agent("agent_123", AgentState::RateLimited);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("rate limited")));
        assert!(explanation.suggestions.iter().any(|s| s.contains("Rotate")));
    }

    #[test]
    fn queue_item_agent_awaiting_approval() {
        let item = make_queue_item("qi_123", "agent_123", QueueItemStatus::Pending);
        let agent = make_agent("agent_123", AgentState::AwaitingApproval);
        let explanation = build_queue_item_explanation(&item, &agent);

        assert!(explanation.is_blocked);
        assert!(explanation
            .block_reasons
            .iter()
            .any(|r| r.contains("approval")));
    }

    // --- short_id tests ---

    #[test]
    fn short_id_short_input() {
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn short_id_exact_limit() {
        assert_eq!(short_id("12345678"), "12345678");
    }

    #[test]
    fn short_id_long_input() {
        assert_eq!(short_id("123456789abcdef"), "12345678");
    }

    // --- truncate_string tests ---

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate_string("short", 10), "short");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate_string("this is a longer string", 10), "this is...");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate_string("exact", 5), "exact");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate_string("", 10), "");
    }

    // --- integration tests via run_for_test ---

    #[test]
    fn explain_agent_human_output() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::Idle)],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("Agent agent_12 is idle"));
        assert!(out.stdout.contains("Type: opencode"));
        assert!(out.stdout.contains("Queue Status:"));
        assert!(out.stdout.contains("Suggestions:"));
    }

    #[test]
    fn explain_agent_blocked_output() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::AwaitingApproval)],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("BLOCKED"));
        assert!(out.stdout.contains("Block Reasons:"));
        assert!(out.stdout.contains("approval"));
    }

    #[test]
    fn explain_agent_json_output() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::Idle)],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--json", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_id"], "agent_12345678");
        assert_eq!(parsed["state"], "idle");
        assert_eq!(parsed["type"], "opencode");
        assert!(!parsed["is_blocked"].as_bool().unwrap());
        assert!(parsed["queue_status"]["total_items"].as_i64().is_some());
    }

    #[test]
    fn explain_agent_jsonl_output() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::Idle)],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--jsonl", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["agent_id"], "agent_12345678");
    }

    #[test]
    fn explain_agent_with_account() {
        let backend = InMemoryExplainBackend {
            agents: vec![{
                let mut a = make_agent("agent_12345678", AgentState::Idle);
                a.account_id = "acct_1".to_string();
                a
            }],
            accounts: vec![(
                "acct_1".to_string(),
                AccountRecord {
                    profile_name: "main-profile".to_string(),
                    cooldown_until: None,
                    is_in_cooldown: false,
                },
            )],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Account Status:"));
        assert!(out.stdout.contains("Profile: main-profile"));
        assert!(out.stdout.contains("Cooldown: none"));
    }

    #[test]
    fn explain_agent_with_cooldown() {
        let backend = InMemoryExplainBackend {
            agents: vec![{
                let mut a = make_agent("agent_12345678", AgentState::Idle);
                a.account_id = "acct_1".to_string();
                a
            }],
            accounts: vec![(
                "acct_1".to_string(),
                AccountRecord {
                    profile_name: "main-profile".to_string(),
                    cooldown_until: Some("2026-01-01T01:00:00Z".to_string()),
                    is_in_cooldown: true,
                },
            )],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Cooldown: active"));
        assert!(out.stdout.contains("account cooldown active"));
    }

    #[test]
    fn explain_queue_item_human_output() {
        let agent = make_agent("agent_12345678", AgentState::Working);
        let mut item = make_queue_item("qi_789", "agent_12345678", QueueItemStatus::Pending);
        item.content = Some("hello world".to_string());
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            queue_items: vec![item],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "qi_789"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out
            .stdout
            .contains("Queue Item qi_789 is pending (BLOCKED)"));
        assert!(out.stdout.contains("Type: message"));
        assert!(out.stdout.contains("Content: hello world"));
        assert!(out.stdout.contains("currently working"));
    }

    #[test]
    fn explain_queue_item_json_output() {
        let agent = make_agent("agent_12345678", AgentState::Idle);
        let item = make_queue_item("qi_789", "agent_12345678", QueueItemStatus::Pending);
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            queue_items: vec![item],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--json", "qi_789"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["item_id"], "qi_789");
        assert_eq!(parsed["type"], "message");
        assert_eq!(parsed["status"], "pending");
        assert_eq!(parsed["agent_state"], "idle");
    }

    #[test]
    fn explain_queue_item_with_condition() {
        let agent = make_agent("agent_12345678", AgentState::Working);
        let mut item = make_queue_item("qi_789", "agent_12345678", QueueItemStatus::Pending);
        item.item_type = QueueItemType::Conditional;
        item.condition = Some(ConditionalPayload {
            condition_type: ConditionType::WhenIdle,
            expression: String::new(),
            message: "run when idle".to_string(),
        });
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            queue_items: vec![item],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "qi_789"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Condition:"));
        assert!(out.stdout.contains("Type: when_idle"));
        assert!(out
            .stdout
            .contains("conditional: waiting for agent to be idle"));
    }

    #[test]
    fn explain_context_agent() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::Idle)],
            context_agent_id: Some("agent_12345678".to_string()),
            ..Default::default()
        };
        let out = run_for_test(&["explain"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Agent agent_12 is idle"));
    }

    #[test]
    fn explain_workspace_first_agent() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678", AgentState::Working)],
            workspace_first_agent_id: Some("agent_12345678".to_string()),
            ..Default::default()
        };
        let out = run_for_test(&["explain"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Agent agent_12 is working"));
    }

    #[test]
    fn explain_no_context_errors() {
        let backend = InMemoryExplainBackend::default();
        let out = run_for_test(&["explain"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("no agent specified"));
    }

    #[test]
    fn explain_agent_not_found_errors() {
        let backend = InMemoryExplainBackend::default();
        let out = run_for_test(&["explain", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn explain_queue_item_not_found_errors() {
        let backend = InMemoryExplainBackend::default();
        let out = run_for_test(&["explain", "qi_nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn explain_help_output() {
        let backend = InMemoryExplainBackend::default();
        let out = run_for_test(&["explain", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Explain agent or queue item status"));
        assert!(out.stderr.contains("forge explain"));
    }

    #[test]
    fn explain_agent_error_with_reason_json() {
        let mut agent = make_agent("agent_12345678", AgentState::Error);
        agent.state_info.reason = "connection timeout".to_string();
        agent.state_info.confidence = "high".to_string();
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--json", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed["is_blocked"].as_bool().unwrap());
        assert_eq!(parsed["state_info"]["reason"], "connection timeout");
        assert_eq!(parsed["state_info"]["confidence"], "high");
    }

    #[test]
    fn explain_agent_paused_with_time() {
        let mut agent = make_agent("agent_12345678", AgentState::Paused);
        agent.paused_until = Some("2026-01-01T01:00:00Z".to_string());
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("BLOCKED"));
        assert!(out.stdout.contains("paused"));
        assert!(out.stdout.contains("resume"));
    }

    #[test]
    fn explain_agent_prefix_match() {
        let backend = InMemoryExplainBackend {
            agents: vec![make_agent("agent_12345678abcdef", AgentState::Idle)],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "agent_12"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("is idle"));
    }

    #[test]
    fn explain_persistent_agent_includes_observability_context() {
        let now = chrono::Utc::now();
        let mut agent = make_agent("persistent_123", AgentState::Idle);
        agent.persistent = true;
        agent.created_at = Some((now - chrono::Duration::seconds(7200)).to_rfc3339());
        agent.last_activity = Some((now - chrono::Duration::seconds(7200)).to_rfc3339());
        agent.ttl_seconds = Some(60);

        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            agent_events: vec![
                (
                    "persistent_123".to_string(),
                    AgentEventRecord {
                        kind: "metric_send_to_idle_duration".to_string(),
                        outcome: "success".to_string(),
                        detail: Some("{\"value_ms\":512}".to_string()),
                        timestamp: "2026-02-12T00:00:00Z".to_string(),
                    },
                ),
                (
                    "persistent_123".to_string(),
                    AgentEventRecord {
                        kind: "send".to_string(),
                        outcome: "error: backend failure".to_string(),
                        detail: Some("backend failure".to_string()),
                        timestamp: "2026-02-12T00:00:01Z".to_string(),
                    },
                ),
            ],
            ..Default::default()
        };

        let out = run_for_test(&["explain", "--json", "persistent_123"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["observability"]["persistent"], true);
        assert_eq!(parsed["observability"]["stale"], true);
        assert_eq!(parsed["observability"]["recent_failure_count"], 1);
        assert_eq!(parsed["observability"]["last_send_to_idle_ms"], 512);
        assert!(parsed["is_blocked"].as_bool().unwrap());
    }

    #[test]
    fn explain_agent_json_omits_empty_fields() {
        let agent = make_agent("agent_12345678", AgentState::Idle);
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--json", "agent_12345678"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        // Empty block_reasons and suggestions should be omitted
        assert!(parsed.get("block_reasons").is_none());
        assert!(parsed.get("account_status").is_none());
        assert!(parsed.get("last_activity").is_none());
        assert!(parsed.get("paused_until").is_none());
    }

    #[test]
    fn explain_queue_item_json_omits_empty_fields() {
        let agent = make_agent("agent_12345678", AgentState::Idle);
        let item = make_queue_item("qi_789", "agent_12345678", QueueItemStatus::Pending);
        let backend = InMemoryExplainBackend {
            agents: vec![agent],
            queue_items: vec![item],
            ..Default::default()
        };
        let out = run_for_test(&["explain", "--json", "qi_789"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        // No content or condition
        assert!(parsed.get("content").is_none());
        assert!(parsed.get("condition").is_none());
    }

    #[test]
    fn sqlite_backend_explain_uses_workspace_context_and_real_db_facts() {
        let fixture = SqliteExplainFixture::new("sqlite_backend_explain_uses_workspace_context");
        let backend = fixture.backend();
        let out = run_for_test(&["explain", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());

        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_id"], "agent_12345678");
        assert_eq!(parsed["type"], "codex");
        assert_eq!(parsed["queue_status"]["total_items"], 3);
        assert_eq!(parsed["queue_status"]["pending_items"], 3);
        assert_eq!(parsed["account_status"]["profile_name"], "main-profile");
        assert_eq!(parsed["account_status"]["is_in_cooldown"], true);
        assert!(parsed["block_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("account cooldown active")));
    }

    #[test]
    fn sqlite_backend_queue_payload_parity() {
        let fixture = SqliteExplainFixture::new("sqlite_backend_queue_payload_parity");
        let backend = fixture.backend();

        let conditional = run_for_test(&["explain", "qi_2"], &backend);
        assert_eq!(conditional.exit_code, 0);
        assert!(conditional.stdout.contains("Content: ship patch"));
        assert!(conditional.stdout.contains("Condition:"));
        assert!(conditional.stdout.contains("Type: when_idle"));
        assert!(conditional.stdout.contains("Expression: state == idle"));

        let pause = run_for_test(&["explain", "qi_3"], &backend);
        assert_eq!(pause.exit_code, 0);
        assert!(pause.stdout.contains("Content: 30s pause (cooldown)"));
    }

    struct SqliteExplainFixture {
        _temp: TempDir,
        db_path: std::path::PathBuf,
        context_path: std::path::PathBuf,
    }

    impl SqliteExplainFixture {
        fn new(name: &str) -> Self {
            let temp = TempDir::new(name);
            let db_path = temp.path.join("forge.db");
            let context_path = temp.path.join(".config/forge/context.yaml");
            let parent = context_path.parent().unwrap();
            std::fs::create_dir_all(parent).unwrap();

            std::fs::write(
                &context_path,
                "workspace: ws_1\nworkspace_name: alpha\nupdated_at: 2026-01-01T00:00:00Z\n",
            )
            .unwrap();

            {
                let mut db = forge_db::Db::open(forge_db::Config::new(&db_path)).unwrap();
                db.migrate_up().unwrap();
                let conn = db.conn();

                conn.execute(
                    "INSERT INTO nodes (id, name, is_local, status) VALUES (?1, ?2, 1, 'online')",
                    rusqlite::params!["node_1", "local"],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
                    rusqlite::params![
                        "ws_1",
                        "alpha",
                        "node_1",
                        "/tmp/repo-alpha",
                        "alpha-session"
                    ],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO accounts (id, provider, profile_name, credential_ref, cooldown_until, is_active)
                     VALUES (?1, 'anthropic', ?2, 'env:ANTHROPIC_API_KEY', ?3, 1)",
                    rusqlite::params!["acct_1", "main-profile", "2999-01-01T00:00:00Z"],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO agents (
                        id, workspace_id, type, tmux_pane, account_id,
                        state, state_confidence, state_reason, last_activity_at
                    ) VALUES (
                        ?1, ?2, 'codex', 'alpha-session:1.1', 'acct_1',
                        'idle', 'high', 'ready', '2026-01-01T00:00:00Z'
                    )",
                    rusqlite::params!["agent_12345678", "ws_1"],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO agents (
                        id, workspace_id, type, tmux_pane, state, state_confidence
                    ) VALUES (
                        ?1, ?2, 'opencode', 'alpha-session:1.2', 'working', 'medium'
                    )",
                    rusqlite::params!["agent_99999999", "ws_1"],
                )
                .unwrap();

                conn.execute(
                    "INSERT INTO queue_items (
                        id, agent_id, type, position, status, payload_json, attempts
                    ) VALUES (
                        'qi_1', 'agent_12345678', 'message', 1, 'pending',
                        '{\"text\":\"hello from queue\"}', 0
                    )",
                    [],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO queue_items (
                        id, agent_id, type, position, status, payload_json, attempts
                    ) VALUES (
                        'qi_2', 'agent_12345678', 'conditional', 2, 'pending',
                        '{\"condition_type\":\"when_idle\",\"expression\":\"state == idle\",\"message\":\"ship patch\"}', 0
                    )",
                    [],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO queue_items (
                        id, agent_id, type, position, status, payload_json, attempts
                    ) VALUES (
                        'qi_3', 'agent_12345678', 'pause', 3, 'pending',
                        '{\"duration_seconds\":30,\"reason\":\"cooldown\"}', 0
                    )",
                    [],
                )
                .unwrap();
            }

            Self {
                _temp: temp,
                db_path,
                context_path,
            }
        }

        fn backend(&self) -> SqliteExplainBackend {
            let context_backend =
                FilesystemContextBackend::new(self.context_path.clone(), self.db_path.clone());
            SqliteExplainBackend::new(self.db_path.clone(), context_backend)
        }
    }

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            let uniq = format!(
                "{}-{}-{}",
                prefix,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            path.push(uniq);
            std::fs::create_dir_all(&path)
                .unwrap_or_else(|err| panic!("mkdir {}: {err}", path.display()));
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// A mail message as returned by the backend.
#[derive(Debug, Clone, Serialize)]
pub struct MailMessage {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub from: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub ack_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !v
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait MailBackend {
    /// Send a message to one or more recipients. Returns message IDs for local backend.
    fn send_message(&self, req: &MailSendRequest) -> Result<Vec<i64>, String>;

    /// Fetch inbox messages for an agent.
    fn fetch_inbox(&self, req: &MailInboxRequest) -> Result<Vec<MailMessage>, String>;

    /// Get a specific message by ID.
    fn get_message(
        &self,
        project: &str,
        agent: &str,
        message_id: i64,
    ) -> Result<MailMessage, String>;

    /// Mark a message as read. Returns the read timestamp.
    fn mark_read(&self, project: &str, agent: &str, message_id: i64) -> Result<String, String>;

    /// Acknowledge a message. Returns the ack timestamp.
    fn acknowledge(&self, project: &str, agent: &str, message_id: i64) -> Result<String, String>;

    /// Resolve the project key (e.g. from git root).
    fn resolve_project(&self) -> Result<String, String>;

    /// Read the agent name from environment or config.
    fn resolve_agent(&self) -> Result<Option<String>, String>;

    /// Determine the backend kind ("mcp" or "local").
    fn backend_kind(&self) -> &str;

    /// Read the body from a file path.
    fn read_body_file(&self, path: &str) -> Result<String, String>;

    /// Read the body from stdin.
    fn read_body_stdin(&self) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

pub struct MailSendRequest {
    pub project: String,
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
    pub priority: String,
    pub ack_required: bool,
}

pub struct MailInboxRequest {
    pub project: String,
    pub agent: String,
    pub limit: i32,
    pub since: Option<String>,
    pub unread_only: bool,
}

// ---------------------------------------------------------------------------
// JSON result types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SendResult {
    backend: String,
    project: String,
    from: String,
    to: Vec<String>,
    subject: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    message_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
struct AckResult {
    id: i64,
    agent: String,
    project: String,
    acked_at: String,
    backend: String,
    acknowledged: bool,
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct InMemoryMailBackend {
    pub messages: RefCell<Vec<MailMessage>>,
    pub project: String,
    pub agent: Option<String>,
    pub backend_kind: String,
    next_id: RefCell<i64>,
    pub read_body_file_result: Option<Result<String, String>>,
    pub read_body_stdin_result: Option<Result<String, String>>,
}

impl Default for InMemoryMailBackend {
    fn default() -> Self {
        Self {
            messages: RefCell::new(Vec::new()),
            project: "test-project".to_string(),
            agent: Some("test-agent".to_string()),
            backend_kind: "local".to_string(),
            next_id: RefCell::new(0),
            read_body_file_result: None,
            read_body_stdin_result: None,
        }
    }
}

impl InMemoryMailBackend {
    pub fn with_project(mut self, project: &str) -> Self {
        self.project = project.to_string();
        self
    }

    pub fn with_agent(mut self, agent: &str) -> Self {
        self.agent = Some(agent.to_string());
        self
    }

    pub fn with_no_agent(mut self) -> Self {
        self.agent = None;
        self
    }

    pub fn with_backend_kind(mut self, kind: &str) -> Self {
        self.backend_kind = kind.to_string();
        self
    }

    pub fn with_messages(self, messages: Vec<MailMessage>) -> Self {
        *self.messages.borrow_mut() = messages;
        self
    }
}

impl MailBackend for InMemoryMailBackend {
    fn send_message(&self, req: &MailSendRequest) -> Result<Vec<i64>, String> {
        let mut messages = self.messages.borrow_mut();
        let mut ids = Vec::new();
        for recipient in &req.to {
            let mut next_id = self.next_id.borrow_mut();
            *next_id += 1;
            let id = *next_id;
            messages.push(MailMessage {
                id,
                thread_id: None,
                from: req.from.clone(),
                subject: req.subject.clone(),
                body: Some(req.body.clone()),
                created_at: "2026-02-09T12:00:00Z".to_string(),
                importance: if req.priority.is_empty() || req.priority == "normal" {
                    None
                } else {
                    Some(req.priority.clone())
                },
                ack_required: req.ack_required,
                read_at: None,
                acked_at: None,
                backend: Some(self.backend_kind.clone()),
            });
            let _ = recipient;
            ids.push(id);
        }
        Ok(ids)
    }

    fn fetch_inbox(&self, req: &MailInboxRequest) -> Result<Vec<MailMessage>, String> {
        let messages = self.messages.borrow();
        let mut result: Vec<MailMessage> = messages.clone();

        if req.unread_only {
            result.retain(|m| m.read_at.is_none());
        }

        if req.limit > 0 && result.len() > req.limit as usize {
            result.truncate(req.limit as usize);
        }

        Ok(result)
    }

    fn get_message(
        &self,
        _project: &str,
        _agent: &str,
        message_id: i64,
    ) -> Result<MailMessage, String> {
        let messages = self.messages.borrow();
        messages
            .iter()
            .find(|m| m.id == message_id)
            .cloned()
            .ok_or_else(|| format!("message m-{message_id} not found"))
    }

    fn mark_read(&self, _project: &str, _agent: &str, message_id: i64) -> Result<String, String> {
        let mut messages = self.messages.borrow_mut();
        let ts = "2026-02-09T12:05:00Z".to_string();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.read_at = Some(ts.clone());
        }
        Ok(ts)
    }

    fn acknowledge(&self, _project: &str, _agent: &str, message_id: i64) -> Result<String, String> {
        let mut messages = self.messages.borrow_mut();
        let ts = "2026-02-09T12:10:00Z".to_string();
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.acked_at = Some(ts.clone());
        }
        Ok(ts)
    }

    fn resolve_project(&self) -> Result<String, String> {
        Ok(self.project.clone())
    }

    fn resolve_agent(&self) -> Result<Option<String>, String> {
        Ok(self.agent.clone())
    }

    fn backend_kind(&self) -> &str {
        &self.backend_kind
    }

    fn read_body_file(&self, path: &str) -> Result<String, String> {
        if let Some(ref result) = self.read_body_file_result {
            return result.clone();
        }
        Err(format!("failed to read message file \"{path}\": not found"))
    }

    fn read_body_stdin(&self) -> Result<String, String> {
        if let Some(ref result) = self.read_body_stdin_result {
            return result.clone();
        }
        Err("stdin was empty (pipe a message or use --file/--body)".to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilesystemMailBackend {
    root_override: Option<PathBuf>,
}

impl FilesystemMailBackend {
    pub fn open_from_env() -> Self {
        Self {
            root_override: None,
        }
    }

    #[cfg(test)]
    pub fn for_root(root: PathBuf) -> Self {
        Self {
            root_override: Some(root),
        }
    }

    fn project_root(&self) -> Result<PathBuf, String> {
        if let Some(root) = &self.root_override {
            return Ok(root.clone());
        }
        fmail_core::root::discover_project_root(None)
    }

    fn store(&self) -> Result<fmail_core::store::Store, String> {
        let root = self.project_root()?;
        fmail_core::store::Store::new(&root)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MailIndex {
    next_id: i64,
    ids: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MailStatusEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    read_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acked_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MailStatusState {
    entries: BTreeMap<String, MailStatusEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MailEnvelope {
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    ack_required: bool,
    #[serde(default)]
    thread_id: String,
}

fn index_path(store: &fmail_core::store::Store) -> PathBuf {
    store.root().join("forge-mail-index.json")
}

fn status_path(store: &fmail_core::store::Store, agent: &str) -> Result<PathBuf, String> {
    let normalized = fmail_core::validate::normalize_agent_name(agent)?;
    Ok(store
        .root()
        .join("forge-mail-status")
        .join(format!("{normalized}.json")))
}

fn load_index(store: &fmail_core::store::Store) -> Result<MailIndex, String> {
    let path = index_path(store);
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(MailIndex::default());
    };
    serde_json::from_str::<MailIndex>(&raw).map_err(|err| format!("parse mail index: {err}"))
}

fn save_index(store: &fmail_core::store::Store, index: &MailIndex) -> Result<(), String> {
    let path = index_path(store);
    let data =
        serde_json::to_string_pretty(index).map_err(|err| format!("encode mail index: {err}"))?;
    fs::write(&path, data).map_err(|err| format!("write mail index {}: {err}", path.display()))
}

fn load_statuses(store: &fmail_core::store::Store, agent: &str) -> Result<MailStatusState, String> {
    let path = status_path(store, agent)?;
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(MailStatusState::default());
    };
    serde_json::from_str::<MailStatusState>(&raw).map_err(|err| format!("parse mail status: {err}"))
}

fn save_statuses(
    store: &fmail_core::store::Store,
    agent: &str,
    statuses: &MailStatusState,
) -> Result<(), String> {
    let path = status_path(store, agent)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create status dir {}: {err}", parent.display()))?;
    }
    let data = serde_json::to_string_pretty(statuses)
        .map_err(|err| format!("encode mail statuses: {err}"))?;
    fs::write(&path, data).map_err(|err| format!("write mail status {}: {err}", path.display()))
}

fn local_id_for_message(index: &mut MailIndex, message_id: &str) -> i64 {
    if let Some(existing) = index.ids.get(message_id) {
        return *existing;
    }
    index.next_id = index.next_id.saturating_add(1).max(1);
    let assigned = index.next_id;
    index.ids.insert(message_id.to_string(), assigned);
    assigned
}

fn parse_since_cutoff(raw: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.eq_ignore_ascii_case("now") {
        return Ok(Some(Utc::now()));
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Some(parsed.with_timezone(&Utc)));
    }
    if let Some(amount) = trimmed.strip_suffix('s') {
        let seconds: i64 = amount
            .parse()
            .map_err(|_| format!("invalid --since value: {trimmed}"))?;
        return Ok(Some(Utc::now() - Duration::seconds(seconds.max(0))));
    }
    if let Some(amount) = trimmed.strip_suffix('m') {
        let minutes: i64 = amount
            .parse()
            .map_err(|_| format!("invalid --since value: {trimmed}"))?;
        return Ok(Some(Utc::now() - Duration::minutes(minutes.max(0))));
    }
    if let Some(amount) = trimmed.strip_suffix('h') {
        let hours: i64 = amount
            .parse()
            .map_err(|_| format!("invalid --since value: {trimmed}"))?;
        return Ok(Some(Utc::now() - Duration::hours(hours.max(0))));
    }
    if let Some(amount) = trimmed.strip_suffix('d') {
        let days: i64 = amount
            .parse()
            .map_err(|_| format!("invalid --since value: {trimmed}"))?;
        return Ok(Some(Utc::now() - Duration::days(days.max(0))));
    }
    Err(format!("invalid --since value: {trimmed}"))
}

fn decode_envelope(message: &fmail_core::message::Message) -> MailEnvelope {
    match &message.body {
        serde_json::Value::Object(_) => {
            serde_json::from_value::<MailEnvelope>(message.body.clone()).unwrap_or_else(|_| {
                MailEnvelope {
                    subject: "(no subject)".to_string(),
                    body: message.body.to_string(),
                    ..MailEnvelope::default()
                }
            })
        }
        serde_json::Value::String(text) => MailEnvelope {
            subject: "(no subject)".to_string(),
            body: text.clone(),
            ..MailEnvelope::default()
        },
        other => MailEnvelope {
            subject: "(no subject)".to_string(),
            body: other.to_string(),
            ..MailEnvelope::default()
        },
    }
}

fn to_mail_message(
    message: &fmail_core::message::Message,
    local_id: i64,
    status: Option<&MailStatusEntry>,
) -> MailMessage {
    let envelope = decode_envelope(message);
    MailMessage {
        id: local_id,
        thread_id: if !envelope.thread_id.trim().is_empty() {
            Some(envelope.thread_id)
        } else if !message.reply_to.trim().is_empty() {
            Some(message.reply_to.clone())
        } else {
            None
        },
        from: message.from.clone(),
        subject: if envelope.subject.trim().is_empty() {
            "(no subject)".to_string()
        } else {
            envelope.subject
        },
        body: if envelope.body.is_empty() {
            None
        } else {
            Some(envelope.body)
        },
        created_at: message.time.to_rfc3339(),
        importance: if envelope.priority.trim().is_empty() {
            None
        } else {
            Some(envelope.priority)
        },
        ack_required: envelope.ack_required,
        read_at: status.and_then(|entry| entry.read_at.clone()),
        acked_at: status.and_then(|entry| entry.acked_at.clone()),
        backend: Some("local".to_string()),
    }
}

impl MailBackend for FilesystemMailBackend {
    fn send_message(&self, req: &MailSendRequest) -> Result<Vec<i64>, String> {
        let store = self.store()?;
        store.ensure_root()?;
        let now = Utc::now();
        let mut index = load_index(&store)?;
        let mut ids = Vec::new();

        for recipient in &req.to {
            let body = MailEnvelope {
                subject: req.subject.clone(),
                body: req.body.clone(),
                priority: req.priority.clone(),
                ack_required: req.ack_required,
                thread_id: String::new(),
            };
            let mut message = fmail_core::message::Message {
                id: String::new(),
                from: req.from.clone(),
                to: format!("@{}", recipient.trim()),
                time: now,
                body: serde_json::to_value(body).map_err(|err| format!("encode message: {err}"))?,
                reply_to: String::new(),
                priority: String::new(),
                host: String::new(),
                tags: Vec::new(),
            };
            let saved_id = store
                .save_message(&mut message, now)
                .map_err(|err| format!("save message: {err}"))?;
            let local_id = local_id_for_message(&mut index, &saved_id);
            ids.push(local_id);
        }

        save_index(&store, &index)?;
        Ok(ids)
    }

    fn fetch_inbox(&self, req: &MailInboxRequest) -> Result<Vec<MailMessage>, String> {
        let store = self.store()?;
        let cutoff = parse_since_cutoff(req.since.as_deref())?;
        let mut index = load_index(&store)?;
        let statuses = load_statuses(&store, &req.agent)?;
        let mut dirty_index = false;

        let normalized_agent = fmail_core::validate::normalize_agent_name(&req.agent)?;
        let mut messages = Vec::new();
        let paths = store
            .list_dm_message_files(&normalized_agent)
            .map_err(|err| format!("list inbox: {err}"))?;

        for path in paths {
            let message = store
                .read_message(&path)
                .map_err(|err| format!("read message {}: {err}", path.display()))?;
            let local_id = if let Some(existing) = index.ids.get(&message.id) {
                *existing
            } else {
                dirty_index = true;
                local_id_for_message(&mut index, &message.id)
            };

            if let Some(since) = cutoff {
                if message.time < since {
                    continue;
                }
            }

            let status = statuses.entries.get(&local_id.to_string());
            let mail_message = to_mail_message(&message, local_id, status);
            if req.unread_only && mail_message.read_at.is_some() {
                continue;
            }
            messages.push(mail_message);
        }

        messages.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        if req.limit > 0 && messages.len() > req.limit as usize {
            messages.truncate(req.limit as usize);
        }

        if dirty_index {
            save_index(&store, &index)?;
        }
        Ok(messages)
    }

    fn get_message(
        &self,
        _project: &str,
        agent: &str,
        message_id: i64,
    ) -> Result<MailMessage, String> {
        let inbox = self.fetch_inbox(&MailInboxRequest {
            project: String::new(),
            agent: agent.to_string(),
            limit: i32::MAX,
            since: None,
            unread_only: false,
        })?;
        inbox
            .into_iter()
            .find(|message| message.id == message_id)
            .ok_or_else(|| format!("message m-{message_id} not found"))
    }

    fn mark_read(&self, _project: &str, agent: &str, message_id: i64) -> Result<String, String> {
        let store = self.store()?;
        let mut statuses = load_statuses(&store, agent)?;
        let now = Utc::now().to_rfc3339();
        let entry = statuses
            .entries
            .entry(message_id.to_string())
            .or_insert_with(MailStatusEntry::default);
        entry.read_at = Some(now.clone());
        save_statuses(&store, agent, &statuses)?;
        Ok(now)
    }

    fn acknowledge(&self, _project: &str, agent: &str, message_id: i64) -> Result<String, String> {
        let store = self.store()?;
        let mut statuses = load_statuses(&store, agent)?;
        let now = Utc::now().to_rfc3339();
        let entry = statuses
            .entries
            .entry(message_id.to_string())
            .or_insert_with(MailStatusEntry::default);
        entry.acked_at = Some(now.clone());
        save_statuses(&store, agent, &statuses)?;
        Ok(now)
    }

    fn resolve_project(&self) -> Result<String, String> {
        if let Ok(project) = std::env::var("FORGE_AGENT_MAIL_PROJECT") {
            let trimmed = project.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
        if let Ok(project) = std::env::var(fmail_core::constants::ENV_PROJECT) {
            let trimmed = project.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
        let root = self.project_root()?;
        Ok(root.to_string_lossy().to_string())
    }

    fn resolve_agent(&self) -> Result<Option<String>, String> {
        if let Ok(agent) = std::env::var("FORGE_AGENT_MAIL_AGENT") {
            let trimmed = agent.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
        }
        if let Ok(agent) = std::env::var(fmail_core::constants::ENV_AGENT) {
            let trimmed = agent.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
        }
        Ok(None)
    }

    fn backend_kind(&self) -> &str {
        "local"
    }

    fn read_body_file(&self, path: &str) -> Result<String, String> {
        fs::read_to_string(path)
            .map_err(|err| format!("failed to read message file \"{path}\": {err}"))
    }

    fn read_body_stdin(&self) -> Result<String, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        if input.trim().is_empty() {
            return Err("stdin was empty (pipe a message or use --file/--body)".to_string());
        }
        Ok(input)
    }
}

// ---------------------------------------------------------------------------
// SQLite backend (forge-db bridge)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SqliteMailBackend {
    db_path: PathBuf,
}

impl SqliteMailBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    #[cfg(test)]
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    /// Ensure a workspace row exists for the given project key, creating one if needed.
    fn ensure_workspace(&self, db: &forge_db::Db, project: &str) -> Result<String, String> {
        let conn = db.conn();
        // Try to find an existing workspace matching the project key.
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM workspaces WHERE name = ?1 LIMIT 1",
                rusqlite::params![project],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if let Some(id) = existing {
            return Ok(id);
        }

        // Create a minimal workspace + node so FKs are satisfied.
        let node_id = "mail-node";
        let ws_id = format!("mail-ws-{}", &project[..project.len().min(16)]);
        let repo_path = format!("mail:{project}");
        let tmux_session = format!("mail-session:{project}");
        let _ = conn.execute(
            "INSERT OR IGNORE INTO nodes (id, name, is_local, status) VALUES (?1, ?2, 1, 'online')",
            rusqlite::params![node_id, "mail-local"],
        );
        conn.execute(
            "INSERT OR IGNORE INTO workspaces (id, name, node_id, repo_path, tmux_session, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
            rusqlite::params![ws_id, project, node_id, repo_path, tmux_session],
        )
        .map_err(|err| format!("create workspace for mail: {err}"))?;
        Ok(ws_id)
    }

    /// Ensure an agent row exists for the sender so FK constraints pass.
    fn ensure_sender_agent(
        &self,
        db: &forge_db::Db,
        workspace_id: &str,
        sender: &str,
    ) -> Result<(), String> {
        if sender.is_empty() {
            return Ok(());
        }
        let conn = db.conn();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?1)",
                rusqlite::params![sender],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if exists {
            return Ok(());
        }
        conn.execute(
            "INSERT OR IGNORE INTO agents (
                id, workspace_id, type, tmux_pane,
                state, state_confidence
             ) VALUES (?1, ?2, 'generic', '.', 'idle', 'high')",
            rusqlite::params![sender, workspace_id],
        )
        .map_err(|err| format!("create sender agent: {err}"))?;
        Ok(())
    }

    /// Get a message's UUID by its rowid.
    fn uuid_for_rowid(&self, db: &forge_db::Db, rowid: i64) -> Result<String, String> {
        db.conn()
            .query_row(
                "SELECT id FROM mail_messages WHERE rowid = ?1",
                rusqlite::params![rowid],
                |row| row.get(0),
            )
            .map_err(|err| format!("message m-{rowid} not found: {err}"))
    }

    /// Convert a DB mail message into CLI MailMessage using rowid as the local ID.
    fn to_cli_message(
        &self,
        db: &forge_db::Db,
        msg: &forge_db::mail_repository::MailMessage,
    ) -> Result<MailMessage, String> {
        let rowid: i64 = db
            .conn()
            .query_row(
                "SELECT rowid FROM mail_messages WHERE id = ?1",
                rusqlite::params![msg.id],
                |row| row.get(0),
            )
            .map_err(|err| format!("resolve rowid for {}: {err}", msg.id))?;

        Ok(MailMessage {
            id: rowid,
            thread_id: if msg.thread_id.is_empty() {
                None
            } else {
                Some(msg.thread_id.clone())
            },
            from: msg.sender_agent_id.clone().unwrap_or_default(),
            subject: msg
                .subject
                .clone()
                .unwrap_or_else(|| "(no subject)".to_string()),
            body: if msg.body.is_empty() {
                None
            } else {
                Some(msg.body.clone())
            },
            created_at: msg.created_at.clone(),
            importance: if msg.importance.is_empty() || msg.importance == "normal" {
                None
            } else {
                Some(msg.importance.clone())
            },
            ack_required: msg.ack_required,
            read_at: msg.read_at.clone(),
            acked_at: msg.acked_at.clone(),
            backend: Some("sqlite".to_string()),
        })
    }
}

impl MailBackend for SqliteMailBackend {
    fn send_message(&self, req: &MailSendRequest) -> Result<Vec<i64>, String> {
        if !self.db_path.exists() {
            return Err(format!("database not found: {}", self.db_path.display()));
        }
        let db = self.open_db()?;
        let workspace_id = self.ensure_workspace(&db, &req.project)?;
        self.ensure_sender_agent(&db, &workspace_id, &req.from)?;
        let mail_repo = forge_db::mail_repository::MailRepository::new(&db);

        // Create one thread per send operation.
        let mut thread = forge_db::mail_repository::MailThread {
            workspace_id: workspace_id.clone(),
            subject: req.subject.clone(),
            ..Default::default()
        };
        mail_repo
            .create_thread(&mut thread)
            .map_err(|err| format!("create mail thread: {err}"))?;

        let mut ids = Vec::new();
        for recipient in &req.to {
            let mut msg = forge_db::mail_repository::MailMessage {
                thread_id: thread.id.clone(),
                sender_agent_id: if req.from.is_empty() {
                    None
                } else {
                    Some(req.from.clone())
                },
                recipient_type: forge_db::mail_repository::RecipientType::Agent,
                recipient_id: Some(recipient.clone()),
                subject: Some(req.subject.clone()),
                body: req.body.clone(),
                importance: if req.priority.is_empty() {
                    "normal".to_string()
                } else {
                    req.priority.clone()
                },
                ack_required: req.ack_required,
                ..Default::default()
            };
            mail_repo
                .create_message(&mut msg)
                .map_err(|err| format!("create mail message: {err}"))?;

            // Retrieve the rowid for the new message.
            let rowid: i64 = db
                .conn()
                .query_row(
                    "SELECT rowid FROM mail_messages WHERE id = ?1",
                    rusqlite::params![msg.id],
                    |row| row.get(0),
                )
                .map_err(|err| format!("resolve rowid: {err}"))?;
            ids.push(rowid);
        }

        Ok(ids)
    }

    fn fetch_inbox(&self, req: &MailInboxRequest) -> Result<Vec<MailMessage>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        let db = self.open_db()?;
        let mail_repo = forge_db::mail_repository::MailRepository::new(&db);

        let limit = if req.limit > 0 {
            req.limit as usize
        } else {
            50
        };

        let db_messages = mail_repo
            .list_inbox("agent", Some(&req.agent), req.unread_only, limit)
            .map_err(|err| {
                if err.to_string().contains("no such table") {
                    return String::new(); // treat as empty
                }
                format!("list inbox: {err}")
            })?;

        let cutoff = parse_since_cutoff(req.since.as_deref())?;
        let mut messages = Vec::new();
        for msg in &db_messages {
            if let Some(since) = cutoff {
                if let Ok(parsed) = DateTime::parse_from_rfc3339(&msg.created_at) {
                    if parsed.with_timezone(&Utc) < since {
                        continue;
                    }
                }
            }
            messages.push(self.to_cli_message(&db, msg)?);
        }

        Ok(messages)
    }

    fn get_message(
        &self,
        _project: &str,
        _agent: &str,
        message_id: i64,
    ) -> Result<MailMessage, String> {
        if !self.db_path.exists() {
            return Err(format!("message m-{message_id} not found"));
        }
        let db = self.open_db()?;
        let uuid = self.uuid_for_rowid(&db, message_id)?;
        let mail_repo = forge_db::mail_repository::MailRepository::new(&db);
        let msg = mail_repo
            .get_message(&uuid)
            .map_err(|err| format!("message m-{message_id} not found: {err}"))?;
        self.to_cli_message(&db, &msg)
    }

    fn mark_read(&self, _project: &str, _agent: &str, message_id: i64) -> Result<String, String> {
        let db = self.open_db()?;
        let uuid = self.uuid_for_rowid(&db, message_id)?;
        let mail_repo = forge_db::mail_repository::MailRepository::new(&db);
        mail_repo
            .mark_read(&uuid)
            .map_err(|err| format!("mark read: {err}"))?;

        // Fetch the updated read_at timestamp.
        let msg = mail_repo
            .get_message(&uuid)
            .map_err(|err| format!("get message after mark_read: {err}"))?;
        Ok(msg.read_at.unwrap_or_else(|| Utc::now().to_rfc3339()))
    }

    fn acknowledge(&self, _project: &str, _agent: &str, message_id: i64) -> Result<String, String> {
        let db = self.open_db()?;
        let uuid = self.uuid_for_rowid(&db, message_id)?;
        let mail_repo = forge_db::mail_repository::MailRepository::new(&db);
        mail_repo
            .mark_acked(&uuid)
            .map_err(|err| format!("acknowledge: {err}"))?;

        let msg = mail_repo
            .get_message(&uuid)
            .map_err(|err| format!("get message after ack: {err}"))?;
        Ok(msg.acked_at.unwrap_or_else(|| Utc::now().to_rfc3339()))
    }

    fn resolve_project(&self) -> Result<String, String> {
        if let Ok(project) = std::env::var("FORGE_AGENT_MAIL_PROJECT") {
            let trimmed = project.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
        if let Ok(project) = std::env::var("FORGE_PROJECT") {
            let trimmed = project.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(trimmed);
            }
        }
        Ok("default".to_string())
    }

    fn resolve_agent(&self) -> Result<Option<String>, String> {
        if let Ok(agent) = std::env::var("FORGE_AGENT_MAIL_AGENT") {
            let trimmed = agent.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
        }
        if let Ok(agent) = std::env::var("FMAIL_AGENT") {
            let trimmed = agent.trim().to_string();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
        }
        Ok(None)
    }

    fn backend_kind(&self) -> &str {
        "sqlite"
    }

    fn read_body_file(&self, path: &str) -> Result<String, String> {
        fs::read_to_string(path)
            .map_err(|err| format!("failed to read message file \"{path}\": {err}"))
    }

    fn read_body_stdin(&self) -> Result<String, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        if input.trim().is_empty() {
            return Err("stdin was empty (pipe a message or use --file/--body)".to_string());
        }
        Ok(input)
    }
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
// Entry points
// ---------------------------------------------------------------------------

pub fn run_for_test(args: &[&str], backend: &dyn MailBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
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
    backend: &dyn MailBackend,
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
enum Subcommand {
    Send,
    Inbox,
    Read,
    Ack,
    Help,
}

#[derive(Debug, Clone)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    subcommand: Subcommand,
    // Global persistent flags
    #[allow(dead_code)]
    url: Option<String>,
    project: Option<String>,
    agent: Option<String>,
    limit: Option<i32>,
    #[allow(dead_code)]
    timeout: Option<String>,
    // Send flags
    to: Vec<String>,
    subject: String,
    body: String,
    file: String,
    stdin: bool,
    priority: String,
    ack_required: bool,
    from: String,
    // Inbox flags
    unread: bool,
    since: Option<String>,
    // Read/Ack positional
    message_id: String,
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn MailBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.subcommand {
        Subcommand::Help => {
            write_help(stdout).map_err(|e| e.to_string())?;
            Ok(())
        }
        Subcommand::Send => execute_send(&parsed, backend, stdout),
        Subcommand::Inbox => execute_inbox(&parsed, backend, stdout),
        Subcommand::Read => execute_read(&parsed, backend, stdout),
        Subcommand::Ack => execute_ack(&parsed, backend, stdout),
    }
}

fn resolve_agent_name(parsed: &ParsedArgs, backend: &dyn MailBackend) -> Result<String, String> {
    if let Some(ref agent) = parsed.agent {
        let trimmed = agent.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(Some(agent)) = backend.resolve_agent() {
        let trimmed = agent.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    Err("--agent is required (or set FORGE_AGENT_MAIL_AGENT)".to_string())
}

fn resolve_project(parsed: &ParsedArgs, backend: &dyn MailBackend) -> Result<String, String> {
    if let Some(ref project) = parsed.project {
        let trimmed = project.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    backend.resolve_project()
}

fn resolve_limit(parsed: &ParsedArgs) -> i32 {
    parsed.limit.unwrap_or(50)
}

fn execute_send(
    parsed: &ParsedArgs,
    backend: &dyn MailBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    // Resolve sender
    let from = if !parsed.from.trim().is_empty() {
        parsed.from.trim().to_string()
    } else if let Some(ref agent) = parsed.agent {
        let trimmed = agent.trim();
        if !trimmed.is_empty() {
            trimmed.to_string()
        } else {
            return Err("sender required (set --from or FORGE_AGENT_MAIL_AGENT)".to_string());
        }
    } else if let Ok(Some(agent)) = backend.resolve_agent() {
        let trimmed = agent.trim().to_string();
        if !trimmed.is_empty() {
            trimmed
        } else {
            return Err("sender required (set --from or FORGE_AGENT_MAIL_AGENT)".to_string());
        }
    } else {
        return Err("sender required (set --from or FORGE_AGENT_MAIL_AGENT)".to_string());
    };

    // Resolve recipients
    let recipients = normalize_recipients(&parsed.to);
    if recipients.is_empty() {
        return Err("--to is required".to_string());
    }

    let subject = parsed.subject.trim().to_string();
    if subject.is_empty() {
        return Err("--subject is required".to_string());
    }

    // Resolve body
    let body = resolve_body(parsed, backend)?;

    // Resolve priority
    let priority = normalize_priority(&parsed.priority)?;

    let project = resolve_project(parsed, backend)?;

    let req = MailSendRequest {
        project: project.clone(),
        from: from.clone(),
        to: recipients.clone(),
        subject: subject.clone(),
        body,
        priority,
        ack_required: parsed.ack_required,
    };

    let ids = backend.send_message(&req)?;

    let result = SendResult {
        backend: backend.backend_kind().to_string(),
        project,
        from,
        to: recipients.clone(),
        subject,
        message_ids: ids.clone(),
    };

    if parsed.json || parsed.jsonl {
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &result).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &result).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if ids.is_empty() {
        writeln!(stdout, "Sent message to {} recipient(s)", recipients.len())
            .map_err(|e| e.to_string())?;
    } else {
        writeln!(
            stdout,
            "Saved message to local mailbox for {} recipient(s)",
            ids.len()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn execute_inbox(
    parsed: &ParsedArgs,
    backend: &dyn MailBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let agent = resolve_agent_name(parsed, backend)?;
    let project = resolve_project(parsed, backend)?;
    let limit = resolve_limit(parsed);

    let req = MailInboxRequest {
        project,
        agent,
        limit,
        since: parsed.since.clone(),
        unread_only: parsed.unread,
    };

    let messages = backend.fetch_inbox(&req)?;

    if parsed.json || parsed.jsonl {
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &messages).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &messages).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if messages.is_empty() {
        writeln!(stdout, "No messages found").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let mut rows = Vec::with_capacity(messages.len());
    for msg in &messages {
        rows.push(vec![
            format_mail_id(msg.id),
            msg.from.clone(),
            msg.subject.clone(),
            format_relative_time(&msg.created_at),
            format_mail_status(msg),
        ]);
    }

    write_table(stdout, &["ID", "FROM", "SUBJECT", "TIME", "STATUS"], &rows)
}

fn execute_read(
    parsed: &ParsedArgs,
    backend: &dyn MailBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let agent = resolve_agent_name(parsed, backend)?;
    let project = resolve_project(parsed, backend)?;
    let message_id = parse_mail_id(&parsed.message_id)?;

    let mut message = backend.get_message(&project, &agent, message_id)?;

    // Mark as read
    let read_ts = backend.mark_read(&project, &agent, message_id)?;
    message.read_at = Some(read_ts);

    if parsed.json || parsed.jsonl {
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &message).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &message).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "ID:      {}", format_mail_id(message.id)).map_err(|e| e.to_string())?;
    writeln!(stdout, "From:    {}", message.from).map_err(|e| e.to_string())?;
    writeln!(stdout, "Subject: {}", message.subject).map_err(|e| e.to_string())?;
    writeln!(stdout, "Date:    {}", message.created_at).map_err(|e| e.to_string())?;
    if let Some(ref thread_id) = message.thread_id {
        if !thread_id.is_empty() {
            writeln!(stdout, "Thread:  {thread_id}").map_err(|e| e.to_string())?;
        }
    }
    if let Some(ref importance) = message.importance {
        if !importance.is_empty() {
            writeln!(stdout, "Priority: {importance}").map_err(|e| e.to_string())?;
        }
    }
    if message.ack_required {
        writeln!(stdout, "Ack:     required").map_err(|e| e.to_string())?;
    }
    writeln!(stdout).map_err(|e| e.to_string())?;
    if let Some(ref body) = message.body {
        writeln!(stdout, "{body}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn execute_ack(
    parsed: &ParsedArgs,
    backend: &dyn MailBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let agent = resolve_agent_name(parsed, backend)?;
    let project = resolve_project(parsed, backend)?;
    let message_id = parse_mail_id(&parsed.message_id)?;

    let acked_at = backend.acknowledge(&project, &agent, message_id)?;

    if parsed.json || parsed.jsonl {
        let result = AckResult {
            id: message_id,
            agent: agent.clone(),
            project: project.clone(),
            acked_at,
            backend: backend.backend_kind().to_string(),
            acknowledged: true,
        };
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &result).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &result).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Acknowledged message {}",
        format_mail_id(message_id)
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize_recipients(values: &[String]) -> Vec<String> {
    let mut recipients = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in values {
        for item in raw.split(',') {
            let value = item.trim();
            if value.is_empty() {
                continue;
            }
            let key = value.to_ascii_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            recipients.push(value.to_string());
        }
    }
    recipients
}

fn normalize_priority(value: &str) -> Result<String, String> {
    let priority = value.trim().to_ascii_lowercase();
    if priority.is_empty() {
        return Ok("normal".to_string());
    }
    match priority.as_str() {
        "low" | "normal" | "high" | "urgent" => Ok(priority),
        _ => Err("invalid priority (use low, normal, high, or urgent)".to_string()),
    }
}

fn resolve_body(parsed: &ParsedArgs, backend: &dyn MailBackend) -> Result<String, String> {
    let mut source_count = 0;
    if !parsed.body.trim().is_empty() {
        source_count += 1;
    }
    if !parsed.file.is_empty() {
        source_count += 1;
    }
    if parsed.stdin {
        source_count += 1;
    }

    if source_count == 0 {
        return Err("message body required (--body, --file, or --stdin)".to_string());
    }
    if source_count > 1 {
        return Err("choose only one body source: --body, --file, or --stdin".to_string());
    }

    if !parsed.file.is_empty() {
        return backend.read_body_file(&parsed.file);
    }
    if parsed.stdin {
        return backend.read_body_stdin();
    }

    let body = parsed.body.trim();
    if body.is_empty() {
        return Err("message body is empty".to_string());
    }
    Ok(body.to_string())
}

fn parse_mail_id(value: &str) -> Result<i64, String> {
    let trimmed = value.trim();
    let number_part = if trimmed.to_ascii_lowercase().starts_with("m-") {
        &trimmed[2..]
    } else {
        trimmed
    };
    if number_part.is_empty() {
        return Err("message id required".to_string());
    }
    match number_part.parse::<i64>() {
        Ok(id) if id > 0 => Ok(id),
        _ => Err(format!("invalid message id: {value}")),
    }
}

fn format_mail_id(id: i64) -> String {
    if id <= 0 {
        "-".to_string()
    } else {
        format!("m-{id}")
    }
}

fn format_mail_status(msg: &MailMessage) -> String {
    if msg.ack_required && msg.acked_at.is_some() {
        "acked".to_string()
    } else if msg.read_at.is_some() {
        "read".to_string()
    } else {
        "unread".to_string()
    }
}

fn format_relative_time(ts: &str) -> String {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        return "-".to_string();
    }

    // Try to parse as RFC3339 and compute relative time
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(parsed);

        if duration.num_seconds() < 60 {
            return "just now".to_string();
        }
        if duration.num_minutes() < 60 {
            return format!("{}m ago", duration.num_minutes());
        }
        if duration.num_hours() < 24 {
            return format!("{}h ago", duration.num_hours());
        }
        return format!("{}d ago", duration.num_hours() / 24);
    }

    // Fallback: return the timestamp as-is
    trimmed.to_string()
}

fn write_table(out: &mut dyn Write, headers: &[&str], rows: &[Vec<String>]) -> Result<(), String> {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() && cell.len() > widths[index] {
                widths[index] = cell.len();
            }
        }
    }

    let mut header_line = String::new();
    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            header_line.push_str("  ");
        }
        header_line.push_str(&format!("{header:<width$}", width = widths[index]));
    }
    writeln!(out, "{header_line}").map_err(|e| e.to_string())?;

    for row in rows {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            line.push_str(&format!("{cell:<width$}", width = widths[index]));
        }
        writeln!(out, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    // Skip "mail" command word
    if args.get(index).is_some_and(|token| token == "mail") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;

    // Persistent flags (can appear before or after subcommand)
    let mut url: Option<String> = None;
    let mut project: Option<String> = None;
    let mut agent: Option<String> = None;
    let mut limit: Option<i32> = None;
    let mut timeout: Option<String> = None;

    // Send flags
    let mut to: Vec<String> = Vec::new();
    let mut subject = String::new();
    let mut body = String::new();
    let mut file = String::new();
    let mut stdin_flag = false;
    let mut priority = "normal".to_string();
    let mut ack_required = false;
    let mut from = String::new();

    // Inbox flags
    let mut unread = false;
    let mut since: Option<String> = None;

    // Positional for read/ack
    let mut message_id = String::new();

    // Detect subcommand
    let mut subcommand: Option<Subcommand> = None;

    // First pass: find subcommand among the non-flag tokens
    // We'll parse everything in a single pass
    let mut positionals: Vec<String> = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" if subcommand.is_none() => {
                return Ok(ParsedArgs {
                    json: false,
                    jsonl: false,
                    subcommand: Subcommand::Help,
                    url: None,
                    project: None,
                    agent: None,
                    limit: None,
                    timeout: None,
                    to: Vec::new(),
                    subject: String::new(),
                    body: String::new(),
                    file: String::new(),
                    stdin: false,
                    priority: "normal".to_string(),
                    ack_required: false,
                    from: String::new(),
                    unread: false,
                    since: None,
                    message_id: String::new(),
                });
            }
            "-h" | "--help" | "help" => {
                return Ok(ParsedArgs {
                    json: false,
                    jsonl: false,
                    subcommand: Subcommand::Help,
                    url: None,
                    project: None,
                    agent: None,
                    limit: None,
                    timeout: None,
                    to: Vec::new(),
                    subject: String::new(),
                    body: String::new(),
                    file: String::new(),
                    stdin: false,
                    priority: "normal".to_string(),
                    ack_required: false,
                    from: String::new(),
                    unread: false,
                    since: None,
                    message_id: String::new(),
                });
            }
            // JSON flags
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            // Persistent flags
            "--url" => {
                url = Some(take_value(args, index, "--url")?);
                index += 2;
            }
            "--project" => {
                project = Some(take_value(args, index, "--project")?);
                index += 2;
            }
            "--agent" => {
                agent = Some(take_value(args, index, "--agent")?);
                index += 2;
            }
            "--limit" => {
                let val = take_value(args, index, "--limit")?;
                limit = Some(val.parse::<i32>().map_err(|_| "invalid --limit value")?);
                index += 2;
            }
            "--timeout" => {
                timeout = Some(take_value(args, index, "--timeout")?);
                index += 2;
            }
            // Send flags
            "--to" => {
                to.push(take_value(args, index, "--to")?);
                index += 2;
            }
            "--subject" | "-s" => {
                subject = take_value(args, index, "--subject")?;
                index += 2;
            }
            "--body" | "-b" => {
                body = take_value(args, index, "--body")?;
                index += 2;
            }
            "--file" | "-f" => {
                file = take_value(args, index, "--file")?;
                index += 2;
            }
            "--stdin" => {
                stdin_flag = true;
                index += 1;
            }
            "--priority" => {
                priority = take_value(args, index, "--priority")?;
                index += 2;
            }
            "--ack-required" => {
                ack_required = true;
                index += 1;
            }
            "--from" => {
                from = take_value(args, index, "--from")?;
                index += 2;
            }
            // Inbox flags
            "--unread" => {
                unread = true;
                index += 1;
            }
            "--since" => {
                since = Some(take_value(args, index, "--since")?);
                index += 2;
            }
            // Unknown flags
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag: '{flag}'"));
            }
            // Subcommands and positionals
            "send" if subcommand.is_none() => {
                subcommand = Some(Subcommand::Send);
                index += 1;
            }
            "inbox" if subcommand.is_none() => {
                subcommand = Some(Subcommand::Inbox);
                index += 1;
            }
            "read" if subcommand.is_none() => {
                subcommand = Some(Subcommand::Read);
                index += 1;
            }
            "ack" if subcommand.is_none() => {
                subcommand = Some(Subcommand::Ack);
                index += 1;
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let subcommand = subcommand.unwrap_or(Subcommand::Help);

    // For read/ack, first positional is message-id
    match subcommand {
        Subcommand::Read | Subcommand::Ack => {
            if let Some(first) = positionals.first() {
                message_id = first.clone();
            } else {
                return Err(format!(
                    "{} requires a <message-id> argument",
                    if subcommand == Subcommand::Read {
                        "read"
                    } else {
                        "ack"
                    }
                ));
            }
        }
        _ => {}
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        subcommand,
        url,
        project,
        agent,
        limit,
        timeout,
        to,
        subject,
        body,
        file,
        stdin: stdin_flag,
        priority,
        ack_required,
        from,
        unread,
        since,
        message_id,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => Ok(value.clone()),
        Some(_) | None => Err(format!("error: {flag} requires a value")),
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Forge Mail messaging")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Forge Mail provides lightweight agent-to-agent messaging."
    )?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "If Agent Mail MCP is configured, messages are sent through the MCP server."
    )?;
    writeln!(
        stdout,
        "Otherwise, Forge falls back to a local mail store in ~/.config/forge/mail.db."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge mail <command> [flags]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  send      Send a message to an agent mailbox")?;
    writeln!(stdout, "  inbox     List mailbox messages")?;
    writeln!(stdout, "  read      Read a mailbox message")?;
    writeln!(stdout, "  ack       Acknowledge a mailbox message")?;
    writeln!(stdout)?;
    writeln!(stdout, "Persistent Flags:")?;
    writeln!(
        stdout,
        "      --url string      Agent Mail MCP URL (default FORGE_AGENT_MAIL_URL)"
    )?;
    writeln!(
        stdout,
        "      --project string  Agent Mail project key (default FORGE_AGENT_MAIL_PROJECT or repo root)"
    )?;
    writeln!(
        stdout,
        "      --agent string    Agent Mail agent name (default FORGE_AGENT_MAIL_AGENT)"
    )?;
    writeln!(
        stdout,
        "      --limit int       max inbox messages to fetch (default FORGE_AGENT_MAIL_LIMIT or 50)"
    )?;
    writeln!(
        stdout,
        "      --timeout string  Agent Mail request timeout (default FORGE_AGENT_MAIL_TIMEOUT or 5s)"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Send Flags:")?;
    writeln!(
        stdout,
        "      --to string       recipient agent name (required, repeatable)"
    )?;
    writeln!(stdout, "  -s, --subject string  message subject (required)")?;
    writeln!(stdout, "  -b, --body string     message body")?;
    writeln!(
        stdout,
        "  -f, --file string     read message body from file"
    )?;
    writeln!(
        stdout,
        "      --stdin           read message body from stdin"
    )?;
    writeln!(
        stdout,
        "      --priority string message priority (low, normal, high, urgent) (default \"normal\")"
    )?;
    writeln!(stdout, "      --ack-required    request acknowledgement")?;
    writeln!(
        stdout,
        "      --from string     sender agent name (default --agent or FORGE_AGENT_MAIL_AGENT)"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Inbox Flags:")?;
    writeln!(stdout, "      --unread          show only unread messages")?;
    writeln!(stdout, "      --since string    filter by time window")?;
    writeln!(stdout)?;
    writeln!(stdout, "Examples:")?;
    writeln!(
        stdout,
        "  forge mail send --to agent-a1 --subject \"Task handoff\" --body \"Please review PR #123\""
    )?;
    writeln!(
        stdout,
        "  forge mail send --to agent-a1 --subject \"Task handoff\" --file message.md"
    )?;
    writeln!(stdout, "  forge mail inbox --agent agent-a1")?;
    writeln!(stdout, "  forge mail inbox --agent agent-a1 --unread")?;
    writeln!(stdout, "  forge mail read m-001 --agent agent-a1")?;
    writeln!(stdout, "  forge mail ack m-001 --agent agent-a1")?;
    writeln!(stdout)?;
    writeln!(stdout, "Output Flags:")?;
    writeln!(stdout, "      --json            output in JSON format")?;
    writeln!(
        stdout,
        "      --jsonl           output in JSON Lines format"
    )?;
    writeln!(stdout, "  -h, --help            help for mail")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn default_backend() -> InMemoryMailBackend {
        InMemoryMailBackend::default()
    }

    fn backend_with_messages() -> InMemoryMailBackend {
        InMemoryMailBackend::default().with_messages(vec![
            MailMessage {
                id: 1,
                thread_id: Some("t-1".to_string()),
                from: "sender-a".to_string(),
                subject: "Task handoff".to_string(),
                body: Some("Please review PR #123".to_string()),
                created_at: "2026-02-09T10:00:00Z".to_string(),
                importance: Some("high".to_string()),
                ack_required: true,
                read_at: None,
                acked_at: None,
                backend: Some("local".to_string()),
            },
            MailMessage {
                id: 2,
                thread_id: None,
                from: "sender-b".to_string(),
                subject: "Status update".to_string(),
                body: Some("All tests passing".to_string()),
                created_at: "2026-02-09T11:00:00Z".to_string(),
                importance: None,
                ack_required: false,
                read_at: Some("2026-02-09T11:05:00Z".to_string()),
                acked_at: None,
                backend: Some("local".to_string()),
            },
        ])
    }

    fn run(args: &[&str], backend: &dyn MailBackend) -> CommandOutput {
        run_for_test(args, backend)
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    fn temp_project_root(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-mail-{tag}-{nanos}-{}-{suffix}",
            std::process::id(),
        ))
    }

    // --- Help ---

    #[test]
    fn help_flag_renders() {
        let backend = default_backend();
        let out = run(&["mail", "--help"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Forge Mail messaging"));
        assert!(out.stdout.contains("send"));
        assert!(out.stdout.contains("inbox"));
        assert!(out.stdout.contains("read"));
        assert!(out.stdout.contains("ack"));
    }

    #[test]
    fn no_subcommand_renders_help() {
        let backend = default_backend();
        let out = run(&["mail"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Forge Mail messaging"));
    }

    // --- Send ---

    #[test]
    fn send_basic_message_json() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["backend"], "local");
        assert_eq!(parsed["from"], "test-agent");
        assert_eq!(parsed["to"][0], "agent-a1");
        assert_eq!(parsed["subject"], "hello");
        assert!(!parsed["message_ids"].as_array().unwrap().is_empty());
    }

    #[test]
    fn send_basic_message_human() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
            ],
            &backend,
        );
        assert_success(&out);
        assert!(out
            .stdout
            .contains("Saved message to local mailbox for 1 recipient(s)"));
    }

    #[test]
    fn send_multiple_recipients() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1,agent-a2",
                "--subject",
                "hello",
                "--body",
                "world",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["to"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["message_ids"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn send_deduplicates_recipients() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1,AGENT-A1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["to"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn send_missing_to_errors() {
        let backend = default_backend();
        let out = run(
            &["mail", "send", "--subject", "hello", "--body", "world"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--to is required"));
    }

    #[test]
    fn send_missing_subject_errors() {
        let backend = default_backend();
        let out = run(
            &["mail", "send", "--to", "agent-a1", "--body", "world"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--subject is required"));
    }

    #[test]
    fn send_missing_body_errors() {
        let backend = default_backend();
        let out = run(
            &["mail", "send", "--to", "agent-a1", "--subject", "hello"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("message body required"));
    }

    #[test]
    fn send_multiple_body_sources_errors() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--stdin",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("choose only one body source"));
    }

    #[test]
    fn send_invalid_priority_errors() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--priority",
                "ultra",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("invalid priority"));
    }

    #[test]
    fn send_no_sender_errors() {
        let backend = InMemoryMailBackend::default().with_no_agent();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("sender required"));
    }

    #[test]
    fn send_with_from_flag() {
        let backend = InMemoryMailBackend::default().with_no_agent();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--from",
                "custom-sender",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["from"], "custom-sender");
    }

    #[test]
    fn send_with_ack_required() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--ack-required",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        // Message stored with ack_required
        let messages = backend.messages.borrow();
        assert!(messages[0].ack_required);
    }

    #[test]
    fn send_jsonl_format() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "send",
                "--to",
                "agent-a1",
                "--subject",
                "hello",
                "--body",
                "world",
                "--jsonl",
            ],
            &backend,
        );
        assert_success(&out);
        // JSONL should be single-line
        let lines: Vec<&str> = out.stdout.trim().lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["backend"], "local");
    }

    // --- Inbox ---

    #[test]
    fn inbox_lists_messages_json() {
        let backend = backend_with_messages();
        let out = run(
            &["mail", "inbox", "--agent", "test-agent", "--json"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn inbox_lists_messages_human() {
        let backend = backend_with_messages();
        let out = run(&["mail", "inbox", "--agent", "test-agent"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("ID"));
        assert!(out.stdout.contains("FROM"));
        assert!(out.stdout.contains("SUBJECT"));
        assert!(out.stdout.contains("m-1"));
        assert!(out.stdout.contains("sender-a"));
        assert!(out.stdout.contains("Task handoff"));
    }

    #[test]
    fn inbox_empty_shows_no_messages() {
        let backend = default_backend();
        let out = run(&["mail", "inbox", "--agent", "test-agent"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("No messages found"));
    }

    #[test]
    fn inbox_unread_filter() {
        let backend = backend_with_messages();
        let out = run(
            &[
                "mail",
                "inbox",
                "--agent",
                "test-agent",
                "--unread",
                "--json",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        assert_eq!(parsed[0]["id"], 1);
    }

    #[test]
    fn inbox_missing_agent_errors() {
        let backend = InMemoryMailBackend::default().with_no_agent();
        let out = run(&["mail", "inbox"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--agent is required"));
    }

    // --- Read ---

    #[test]
    fn read_message_json() {
        let backend = backend_with_messages();
        let out = run(
            &["mail", "read", "m-1", "--agent", "test-agent", "--json"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["subject"], "Task handoff");
        assert!(parsed["read_at"].is_string());
    }

    #[test]
    fn read_message_human() {
        let backend = backend_with_messages();
        let out = run(&["mail", "read", "m-1", "--agent", "test-agent"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("ID:      m-1"));
        assert!(out.stdout.contains("From:    sender-a"));
        assert!(out.stdout.contains("Subject: Task handoff"));
        assert!(out.stdout.contains("Priority: high"));
        assert!(out.stdout.contains("Ack:     required"));
        assert!(out.stdout.contains("Please review PR #123"));
    }

    #[test]
    fn read_numeric_id() {
        let backend = backend_with_messages();
        let out = run(
            &["mail", "read", "1", "--agent", "test-agent", "--json"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["id"], 1);
    }

    #[test]
    fn read_missing_id_errors() {
        let backend = backend_with_messages();
        let out = run(&["mail", "read", "--agent", "test-agent"], &backend);
        assert_eq!(out.exit_code, 1);
        // The --agent flag value is consumed, so read gets no positional
    }

    #[test]
    fn read_not_found_errors() {
        let backend = backend_with_messages();
        let out = run(
            &["mail", "read", "m-999", "--agent", "test-agent"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    // --- Ack ---

    #[test]
    fn ack_message_json() {
        let backend = backend_with_messages();
        let out = run(
            &["mail", "ack", "m-1", "--agent", "test-agent", "--json"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["id"], 1);
        assert_eq!(parsed["acknowledged"], true);
        assert!(parsed["acked_at"].is_string());
    }

    #[test]
    fn ack_message_human() {
        let backend = backend_with_messages();
        let out = run(&["mail", "ack", "m-1", "--agent", "test-agent"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Acknowledged message m-1"));
    }

    #[test]
    fn filesystem_backend_round_trip_send_inbox_read_ack() {
        let root = temp_project_root("filesystem");
        std::fs::create_dir_all(&root).unwrap();

        let backend = FilesystemMailBackend::for_root(root.clone());
        let send = run(
            &[
                "mail",
                "send",
                "--project",
                "proj-mail",
                "--from",
                "sender-a",
                "--to",
                "agent-x",
                "--subject",
                "handoff",
                "--body",
                "please review",
                "--priority",
                "high",
                "--ack-required",
                "--json",
            ],
            &backend,
        );
        assert_success(&send);

        let inbox = run(
            &[
                "mail",
                "inbox",
                "--project",
                "proj-mail",
                "--agent",
                "agent-x",
                "--json",
            ],
            &backend,
        );
        assert_success(&inbox);
        let inbox_json: serde_json::Value = serde_json::from_str(&inbox.stdout).unwrap();
        let id = inbox_json[0]["id"].as_i64().unwrap();
        assert_eq!(inbox_json[0]["subject"], "handoff");
        assert_eq!(inbox_json[0]["ack_required"], true);
        assert_eq!(inbox_json[0]["importance"], "high");

        let read = run(
            &[
                "mail",
                "read",
                &format!("m-{id}"),
                "--project",
                "proj-mail",
                "--agent",
                "agent-x",
                "--json",
            ],
            &backend,
        );
        assert_success(&read);
        let read_json: serde_json::Value = serde_json::from_str(&read.stdout).unwrap();
        assert!(read_json["read_at"].is_string());

        let ack = run(
            &[
                "mail",
                "ack",
                &format!("m-{id}"),
                "--project",
                "proj-mail",
                "--agent",
                "agent-x",
                "--json",
            ],
            &backend,
        );
        assert_success(&ack);
        let ack_json: serde_json::Value = serde_json::from_str(&ack.stdout).unwrap();
        assert_eq!(ack_json["acknowledged"], true);

        let inbox_after_ack = run(
            &[
                "mail",
                "inbox",
                "--project",
                "proj-mail",
                "--agent",
                "agent-x",
                "--json",
            ],
            &backend,
        );
        assert_success(&inbox_after_ack);
        let after_json: serde_json::Value = serde_json::from_str(&inbox_after_ack.stdout).unwrap();
        assert!(after_json[0]["read_at"].is_string());
        assert!(after_json[0]["acked_at"].is_string());

        let _ = std::fs::remove_dir_all(root);
    }

    // --- Parsing ---

    #[test]
    fn parse_mail_id_with_prefix() {
        assert_eq!(parse_mail_id("m-42").unwrap(), 42);
    }

    #[test]
    fn parse_mail_id_numeric() {
        assert_eq!(parse_mail_id("42").unwrap(), 42);
    }

    #[test]
    fn parse_mail_id_invalid() {
        assert!(parse_mail_id("abc").is_err());
    }

    #[test]
    fn parse_mail_id_zero() {
        assert!(parse_mail_id("0").is_err());
    }

    #[test]
    fn format_mail_id_positive() {
        assert_eq!(format_mail_id(42), "m-42");
    }

    #[test]
    fn format_mail_id_zero() {
        assert_eq!(format_mail_id(0), "-");
    }

    #[test]
    fn normalize_recipients_dedup() {
        let input = vec!["a,B,a".to_string(), "c,b".to_string()];
        let result = normalize_recipients(&input);
        assert_eq!(result, vec!["a", "B", "c"]);
    }

    #[test]
    fn normalize_priority_valid() {
        assert_eq!(normalize_priority("LOW").unwrap(), "low");
        assert_eq!(normalize_priority("normal").unwrap(), "normal");
        assert_eq!(normalize_priority("HIGH").unwrap(), "high");
        assert_eq!(normalize_priority("urgent").unwrap(), "urgent");
    }

    #[test]
    fn normalize_priority_invalid() {
        assert!(normalize_priority("ultra").is_err());
    }

    #[test]
    fn format_mail_status_unread() {
        let msg = MailMessage {
            id: 1,
            thread_id: None,
            from: "a".to_string(),
            subject: "s".to_string(),
            body: None,
            created_at: String::new(),
            importance: None,
            ack_required: false,
            read_at: None,
            acked_at: None,
            backend: None,
        };
        assert_eq!(format_mail_status(&msg), "unread");
    }

    #[test]
    fn format_mail_status_read() {
        let msg = MailMessage {
            id: 1,
            thread_id: None,
            from: "a".to_string(),
            subject: "s".to_string(),
            body: None,
            created_at: String::new(),
            importance: None,
            ack_required: false,
            read_at: Some("2026-02-09T12:00:00Z".to_string()),
            acked_at: None,
            backend: None,
        };
        assert_eq!(format_mail_status(&msg), "read");
    }

    #[test]
    fn format_mail_status_acked() {
        let msg = MailMessage {
            id: 1,
            thread_id: None,
            from: "a".to_string(),
            subject: "s".to_string(),
            body: None,
            created_at: String::new(),
            importance: None,
            ack_required: true,
            read_at: Some("2026-02-09T12:00:00Z".to_string()),
            acked_at: Some("2026-02-09T12:05:00Z".to_string()),
            backend: None,
        };
        assert_eq!(format_mail_status(&msg), "acked");
    }

    #[test]
    fn json_jsonl_conflict() {
        let backend = default_backend();
        let out = run(
            &[
                "mail",
                "inbox",
                "--agent",
                "test-agent",
                "--json",
                "--jsonl",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn unknown_flag_errors() {
        let backend = default_backend();
        let out = run(&["mail", "inbox", "--unknown"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown flag: '--unknown'"));
    }

    // --- SQLite backend tests ---

    fn sqlite_temp_db(tag: &str) -> PathBuf {
        let root = temp_project_root(&format!("sqlite-{tag}"));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("forge.db");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path)).unwrap();
        db.migrate_up().unwrap();
        db_path
    }

    fn sqlite_backend(db_path: PathBuf) -> SqliteMailBackend {
        SqliteMailBackend::new(db_path)
    }

    #[test]
    fn sqlite_send_inbox_round_trip() {
        let db_path = sqlite_temp_db("round-trip");
        let backend = sqlite_backend(db_path);

        let send = run(
            &[
                "mail",
                "send",
                "--project",
                "proj-sqlite",
                "--from",
                "sender-a",
                "--to",
                "agent-x",
                "--subject",
                "handoff",
                "--body",
                "please review",
                "--priority",
                "high",
                "--ack-required",
                "--json",
            ],
            &backend,
        );
        assert_success(&send);
        let send_json: serde_json::Value = serde_json::from_str(&send.stdout).unwrap();
        assert_eq!(send_json["backend"], "sqlite");
        assert!(!send_json["message_ids"].as_array().unwrap().is_empty());

        let inbox = run(
            &[
                "mail",
                "inbox",
                "--project",
                "proj-sqlite",
                "--agent",
                "agent-x",
                "--json",
            ],
            &backend,
        );
        assert_success(&inbox);
        let inbox_json: serde_json::Value = serde_json::from_str(&inbox.stdout).unwrap();
        let arr = inbox_json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["subject"], "handoff");
        assert_eq!(arr[0]["from"], "sender-a");
        assert_eq!(arr[0]["ack_required"], true);
        assert_eq!(arr[0]["importance"], "high");
        assert_eq!(arr[0]["backend"], "sqlite");
    }

    #[test]
    fn sqlite_read_marks_as_read() {
        let db_path = sqlite_temp_db("read");
        let backend = sqlite_backend(db_path);

        run(
            &[
                "mail",
                "send",
                "--project",
                "p1",
                "--from",
                "s1",
                "--to",
                "agent-r",
                "--subject",
                "test",
                "--body",
                "body",
            ],
            &backend,
        );

        let inbox = run(&["mail", "inbox", "--agent", "agent-r", "--json"], &backend);
        assert_success(&inbox);
        let inbox_json: serde_json::Value = serde_json::from_str(&inbox.stdout).unwrap();
        let id = inbox_json[0]["id"].as_i64().unwrap();

        let read = run(
            &[
                "mail",
                "read",
                &format!("m-{id}"),
                "--agent",
                "agent-r",
                "--json",
            ],
            &backend,
        );
        assert_success(&read);
        let read_json: serde_json::Value = serde_json::from_str(&read.stdout).unwrap();
        assert!(read_json["read_at"].is_string());
        assert_eq!(read_json["subject"], "test");
    }

    #[test]
    fn sqlite_ack_message() {
        let db_path = sqlite_temp_db("ack");
        let backend = sqlite_backend(db_path);

        run(
            &[
                "mail",
                "send",
                "--project",
                "p1",
                "--from",
                "s1",
                "--to",
                "agent-a",
                "--subject",
                "ack-test",
                "--body",
                "body",
                "--ack-required",
            ],
            &backend,
        );

        let inbox = run(&["mail", "inbox", "--agent", "agent-a", "--json"], &backend);
        let inbox_json: serde_json::Value = serde_json::from_str(&inbox.stdout).unwrap();
        let id = inbox_json[0]["id"].as_i64().unwrap();

        let ack = run(
            &[
                "mail",
                "ack",
                &format!("m-{id}"),
                "--agent",
                "agent-a",
                "--json",
            ],
            &backend,
        );
        assert_success(&ack);
        let ack_json: serde_json::Value = serde_json::from_str(&ack.stdout).unwrap();
        assert_eq!(ack_json["acknowledged"], true);
        assert!(ack_json["acked_at"].is_string());
    }

    #[test]
    fn sqlite_inbox_empty_when_no_db() {
        let root = temp_project_root("sqlite-nodb");
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("nonexistent.db");
        let backend = sqlite_backend(db_path);

        let out = run(&["mail", "inbox", "--agent", "agent-x", "--json"], &backend);
        assert_success(&out);
        let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[test]
    fn sqlite_multiple_recipients() {
        let db_path = sqlite_temp_db("multi-recip");
        let backend = sqlite_backend(db_path);

        let send = run(
            &[
                "mail",
                "send",
                "--project",
                "p1",
                "--from",
                "s1",
                "--to",
                "agent-a,agent-b",
                "--subject",
                "multi",
                "--body",
                "hello",
                "--json",
            ],
            &backend,
        );
        assert_success(&send);
        let json: serde_json::Value = serde_json::from_str(&send.stdout).unwrap();
        assert_eq!(json["message_ids"].as_array().unwrap().len(), 2);

        // Each agent sees their own message
        let inbox_a = run(&["mail", "inbox", "--agent", "agent-a", "--json"], &backend);
        assert_success(&inbox_a);
        let a_json: serde_json::Value = serde_json::from_str(&inbox_a.stdout).unwrap();
        assert_eq!(a_json.as_array().unwrap().len(), 1);

        let inbox_b = run(&["mail", "inbox", "--agent", "agent-b", "--json"], &backend);
        assert_success(&inbox_b);
        let b_json: serde_json::Value = serde_json::from_str(&inbox_b.stdout).unwrap();
        assert_eq!(b_json.as_array().unwrap().len(), 1);
    }

    #[test]
    fn sqlite_unread_filter() {
        let db_path = sqlite_temp_db("unread");
        let backend = sqlite_backend(db_path);

        // Send two messages
        run(
            &[
                "mail",
                "send",
                "--project",
                "p1",
                "--from",
                "s1",
                "--to",
                "agent-u",
                "--subject",
                "msg1",
                "--body",
                "b1",
            ],
            &backend,
        );
        run(
            &[
                "mail",
                "send",
                "--project",
                "p1",
                "--from",
                "s1",
                "--to",
                "agent-u",
                "--subject",
                "msg2",
                "--body",
                "b2",
            ],
            &backend,
        );

        // Read the first one
        let inbox = run(&["mail", "inbox", "--agent", "agent-u", "--json"], &backend);
        let inbox_json: serde_json::Value = serde_json::from_str(&inbox.stdout).unwrap();
        let first_id = inbox_json[0]["id"].as_i64().unwrap();
        run(
            &[
                "mail",
                "read",
                &format!("m-{first_id}"),
                "--agent",
                "agent-u",
            ],
            &backend,
        );

        // Unread filter should show only one
        let unread = run(
            &["mail", "inbox", "--agent", "agent-u", "--unread", "--json"],
            &backend,
        );
        assert_success(&unread);
        let unread_json: serde_json::Value = serde_json::from_str(&unread.stdout).unwrap();
        assert_eq!(unread_json.as_array().unwrap().len(), 1);
    }
}

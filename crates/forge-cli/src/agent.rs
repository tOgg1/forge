use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_agent::error::AgentServiceError;
use forge_agent::event::NullEventSink;
use forge_agent::forged::{ForgedTransport, ForgedTransportConfig};
use forge_agent::mock::MockAgentService;
use forge_agent::service::AgentService;
use forge_agent::types::{
    AgentRequestMode, AgentSnapshot, AgentState, KillAgentParams, ListAgentsFilter,
    SendMessageParams, SpawnAgentParams, WaitStateParams,
};
use forge_db::persistent_agent_event_repository::{
    PersistentAgentEvent, PersistentAgentEventRepository,
};
use forge_db::persistent_agent_repository::{
    PersistentAgent, PersistentAgentFilter, PersistentAgentRepository,
};
use forge_db::transcript_repository::{Transcript, TranscriptRepository};
use serde::Serialize;

// ── JSON output types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AgentJson {
    id: String,
    workspace_id: String,
    state: String,
    pane_id: String,
    pid: i32,
    command: String,
    adapter: String,
    spawned_at: String,
    last_activity_at: String,
}

impl From<&AgentSnapshot> for AgentJson {
    fn from(s: &AgentSnapshot) -> Self {
        Self {
            id: s.id.clone(),
            workspace_id: s.workspace_id.clone(),
            state: s.state.to_string(),
            pane_id: s.pane_id.clone(),
            pid: s.pid,
            command: s.command.clone(),
            adapter: s.adapter.clone(),
            spawned_at: s.spawned_at.to_rfc3339(),
            last_activity_at: s.last_activity_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AgentListJson {
    agents: Vec<AgentJson>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct BoolResultJson {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct RunResultJson {
    agent_id: String,
    reused: bool,
    revived: bool,
    observed_state: String,
    wait_target: Option<String>,
    task_id: Option<String>,
    tags: Vec<String>,
    labels: HashMap<String, String>,
    quick_tail: String,
}

#[derive(Debug, Serialize)]
struct AgentSummarySnapshot {
    agent_id: String,
    concise_status: String,
    latest_task_outcome: String,
    unresolved_blockers: Vec<String>,
    transcript_excerpt: Vec<String>,
    transcript_captured_at: Option<String>,
    recent_events_considered: usize,
    generated_at: String,
}

#[derive(Debug, Serialize)]
struct AgentSummaryJson {
    #[serde(flatten)]
    snapshot: AgentSummarySnapshot,
    snapshot_event_id: i64,
}

#[derive(Debug, Serialize)]
struct AgentGcEvictionJson {
    agent_id: String,
    reason: String,
    age_seconds: i64,
    idle_seconds: i64,
    ttl_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AgentGcResultJson {
    scanned: usize,
    evicted: usize,
    kept: usize,
    dry_run: bool,
    evictions: Vec<AgentGcEvictionJson>,
}

#[derive(Debug, Serialize)]
struct AgentValidationIssueJson {
    agent_id: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct AgentValidationResultJson {
    valid: bool,
    checked: usize,
    errors: Vec<AgentValidationIssueJson>,
}

// ── Backend trait ─────────────────────────────────────────────────────────────

/// Abstraction over the agent service for CLI commands.
/// Each method is synchronous to fit the CLI execution model; the
/// real implementation creates a Tokio runtime internally.
#[allow(unused)]
pub trait AgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String>;
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String>;
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String>;
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String>;
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String>;
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String>;
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String>;
}

// ── InMemory backend (testing) ───────────────────────────────────────────────

#[derive(Default)]
pub struct InMemoryAgentBackend {
    mock: MockAgentService,
}

impl InMemoryAgentBackend {
    pub fn new() -> Self {
        Self {
            mock: MockAgentService::new(),
        }
    }

    pub fn with_agent(mut self, snapshot: AgentSnapshot) -> Self {
        self.mock = self.mock.with_agent(snapshot);
        self
    }

    pub fn with_spawn_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_spawn_error(err);
        self
    }

    pub fn with_send_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_send_error(err);
        self
    }

    pub fn with_kill_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_kill_error(err);
        self
    }

    pub fn with_get_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_get_error(err);
        self
    }
}

fn block_on<F: std::future::Future<Output = T>, T>(future: F) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|err| panic!("failed to create tokio runtime: {err}"));
    runtime.block_on(future)
}

impl AgentBackend for InMemoryAgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String> {
        block_on(self.mock.spawn_agent(params)).map_err(|e| e.to_string())
    }
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String> {
        block_on(self.mock.send_message(params)).map_err(|e| e.to_string())
    }
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String> {
        block_on(self.mock.wait_state(params)).map_err(|e| e.to_string())
    }
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String> {
        block_on(self.mock.interrupt_agent(agent_id)).map_err(|e| e.to_string())
    }
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String> {
        block_on(self.mock.kill_agent(params)).map_err(|e| e.to_string())
    }
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String> {
        block_on(self.mock.list_agents(filter)).map_err(|e| e.to_string())
    }
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String> {
        block_on(self.mock.get_agent(agent_id)).map_err(|e| e.to_string())
    }
}

// ── ForgedAgentBackend (production) ──────────────────────────────────────────

pub struct ForgedAgentBackend {
    service: ForgedTransport,
}

impl ForgedAgentBackend {
    pub fn open_from_env() -> Self {
        let target = resolved_daemon_target();
        let config = ForgedTransportConfig {
            target,
            ..ForgedTransportConfig::default()
        };
        Self {
            service: ForgedTransport::new(config, Arc::new(NullEventSink)),
        }
    }
}

impl AgentBackend for ForgedAgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String> {
        block_on(self.service.spawn_agent(params)).map_err(|e| e.to_string())
    }
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String> {
        block_on(self.service.send_message(params)).map_err(|e| e.to_string())
    }
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String> {
        block_on(self.service.wait_state(params)).map_err(|e| e.to_string())
    }
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String> {
        block_on(self.service.interrupt_agent(agent_id)).map_err(|e| e.to_string())
    }
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String> {
        block_on(self.service.kill_agent(params)).map_err(|e| e.to_string())
    }
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String> {
        block_on(self.service.list_agents(filter)).map_err(|e| e.to_string())
    }
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String> {
        block_on(self.service.get_agent(agent_id)).map_err(|e| e.to_string())
    }
}

fn resolved_daemon_target() -> String {
    let env_target = std::env::var("FORGE_DAEMON_TARGET")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("FORGED_ADDR")
                .ok()
                .filter(|v| !v.trim().is_empty())
        });
    if let Some(target) = env_target {
        let trimmed = target.trim();
        if trimmed.contains("://") {
            return trimmed.to_string();
        }
        return format!("http://{trimmed}");
    }
    "http://127.0.0.1:50051".to_string()
}

// ── Arg parsing ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    subcommand: Subcommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subcommand {
    Run(RunArgs),
    Spawn(SpawnArgs),
    Send(SendArgs),
    Wait(WaitArgs),
    Ps(PsArgs),
    Show(ShowArgs),
    Validate(ValidateArgs),
    Summary(SummaryArgs),
    Gc(GcArgs),
    Interrupt(InterruptArgs),
    Kill(KillArgs),
    Revive(ReviveArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpawnArgs {
    agent_id: String,
    workspace_id: String,
    command: String,
    args: Vec<String>,
    working_dir: String,
    session_name: String,
    adapter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SendArgs {
    agent_id: String,
    text: String,
    send_enter: bool,
    keys: Vec<String>,
    approval_policy: String,
    allow_risky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WaitArgs {
    agent_id: String,
    until: Vec<String>,
    timeout: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PsArgs {
    workspace_id: String,
    state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShowArgs {
    agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidateArgs {
    agent_id: String,
    workspace_id: String,
    state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SummaryArgs {
    agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GcArgs {
    workspace_id: String,
    idle_timeout_seconds: Option<i64>,
    max_age_seconds: Option<i64>,
    dry_run: bool,
    limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InterruptArgs {
    agent_id: String,
    approval_policy: String,
    allow_risky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KillArgs {
    agent_id: String,
    force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviveArgs {
    agent_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevivePolicy {
    Never,
    Ask,
    Auto,
}

impl RevivePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Ask => "ask",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunArgs {
    text: String,
    agent_id: String,
    workspace_id: String,
    agent_type: String,
    command: String,
    approval_policy: String,
    account_id: String,
    profile: String,
    wait_for: String,
    wait_timeout: u64,
    revive_policy: RevivePolicy,
    task_id: String,
    tags: Vec<String>,
    labels: Vec<String>,
}

// ── Public entry points ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn AgentBackend) -> CommandOutput {
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
    backend: &dyn AgentBackend,
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

// ── Core dispatcher ──────────────────────────────────────────────────────────

fn execute(
    args: &[String],
    backend: &dyn AgentBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match &parsed.subcommand {
        Subcommand::Run(run_args) => exec_run(backend, run_args, &parsed, stdout),
        Subcommand::Spawn(spawn_args) => exec_spawn(backend, spawn_args, &parsed, stdout),
        Subcommand::Send(send_args) => exec_send(backend, send_args, &parsed, stdout),
        Subcommand::Wait(wait_args) => exec_wait(backend, wait_args, &parsed, stdout),
        Subcommand::Ps(ps_args) => exec_ps(backend, ps_args, &parsed, stdout),
        Subcommand::Show(show_args) => exec_show(backend, show_args, &parsed, stdout),
        Subcommand::Validate(validate_args) => {
            exec_validate(backend, validate_args, &parsed, stdout)
        }
        Subcommand::Summary(summary_args) => exec_summary(summary_args, &parsed, stdout),
        Subcommand::Gc(gc_args) => exec_gc(gc_args, &parsed, stdout),
        Subcommand::Interrupt(int_args) => exec_interrupt(backend, int_args, &parsed, stdout),
        Subcommand::Kill(kill_args) => exec_kill(backend, kill_args, &parsed, stdout),
        Subcommand::Revive(revive_args) => exec_revive(backend, revive_args, &parsed, stdout),
        Subcommand::Help => {
            write_help(stdout)?;
            Ok(())
        }
    }
}

// ── Subcommand implementations ───────────────────────────────────────────────

const SUMMARY_EVENT_SCAN_LIMIT: i64 = 48;
const SUMMARY_TRANSCRIPT_SCAN_LIMIT: usize = 160;
const SUMMARY_EXCERPT_LIMIT: usize = 8;
const SUMMARY_BLOCKER_LIMIT: usize = 6;
const SUMMARY_TEXT_LIMIT: usize = 220;
const OBSERVABILITY_AGENT_SCAN_LIMIT: i64 = 20_000;
const DEFAULT_APPROVAL_POLICY: &str = "strict";
const APPROVAL_POLICY_ENV_KEY: &str = "FORGE_APPROVAL_POLICY";
const ACCOUNT_ID_ENV_KEY: &str = "FORGE_ACCOUNT_ID";
const PROFILE_ENV_KEY: &str = "FORGE_PROFILE";
const APPROVAL_POLICY_LABEL_KEY: &str = "approval_policy";
const ACCOUNT_ID_LABEL_KEY: &str = "account_id";
const PROFILE_LABEL_KEY: &str = "profile";
const REDACTED_VALUE: &str = "[REDACTED]";
const SENSITIVE_KEY_TOKENS: &[&str] = &[
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "authorization",
    "cookie",
    "session",
    "private_key",
];
const SENSITIVE_VALUE_MARKERS: &[&str] = &[
    "bearer ",
    "token=",
    "token:",
    "secret=",
    "secret:",
    "password=",
    "password:",
    "api_key=",
    "api_key:",
    "apikey=",
    "apikey:",
    "authorization:",
    "authorization=",
    "xoxb-",
    "xoxp-",
    "ghp_",
    "gho_",
    "ghu_",
    "sk-",
    "-----begin",
];
#[allow(dead_code)]
const RISKY_SEND_TOKENS: &[&str] = &[
    "rm -rf",
    "rm -fr",
    "git push --force",
    "git reset --hard",
    "git clean -fd",
    "drop table",
    "truncate table",
    "mkfs",
    "dd if=",
    "curl ",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApprovalContext {
    approval_policy: String,
    account_id: Option<String>,
    profile: Option<String>,
}

impl ApprovalContext {
    fn to_spawn_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert(
            APPROVAL_POLICY_ENV_KEY.to_string(),
            self.approval_policy.clone(),
        );
        if let Some(account_id) = self
            .account_id
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            env.insert(ACCOUNT_ID_ENV_KEY.to_string(), account_id.to_string());
        }
        if let Some(profile) = self
            .profile
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            env.insert(PROFILE_ENV_KEY.to_string(), profile.to_string());
        }
        env
    }

    fn to_labels(&self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert(
            APPROVAL_POLICY_LABEL_KEY.to_string(),
            self.approval_policy.clone(),
        );
        if let Some(account_id) = self
            .account_id
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            labels.insert(ACCOUNT_ID_LABEL_KEY.to_string(), account_id.to_string());
        }
        if let Some(profile) = self
            .profile
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            labels.insert(PROFILE_LABEL_KEY.to_string(), profile.to_string());
        }
        labels
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn record_label_non_empty(record: Option<&PersistentAgent>, key: &str) -> Option<String> {
    record
        .and_then(|entry| entry.labels.get(key))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn pick_context_value(
    explicit: &str,
    record: Option<&PersistentAgent>,
    label_key: &str,
    env_key: &str,
) -> Option<String> {
    if !explicit.trim().is_empty() {
        return Some(explicit.trim().to_string());
    }
    if let Some(value) = record_label_non_empty(record, label_key) {
        return Some(value);
    }
    env_non_empty(env_key)
}

fn resolve_run_approval_context(
    args: &RunArgs,
    record: Option<&PersistentAgent>,
) -> ApprovalContext {
    let approval_policy = pick_context_value(
        &args.approval_policy,
        record,
        APPROVAL_POLICY_LABEL_KEY,
        APPROVAL_POLICY_ENV_KEY,
    )
    .unwrap_or_else(|| DEFAULT_APPROVAL_POLICY.to_string());
    let account_id = pick_context_value(
        &args.account_id,
        record,
        ACCOUNT_ID_LABEL_KEY,
        ACCOUNT_ID_ENV_KEY,
    );
    let profile = pick_context_value(&args.profile, record, PROFILE_LABEL_KEY, PROFILE_ENV_KEY);
    ApprovalContext {
        approval_policy,
        account_id,
        profile,
    }
}

fn resolve_control_policy(explicit: &str, record: Option<&PersistentAgent>) -> String {
    pick_context_value(
        explicit,
        record,
        APPROVAL_POLICY_LABEL_KEY,
        APPROVAL_POLICY_ENV_KEY,
    )
    .unwrap_or_else(|| DEFAULT_APPROVAL_POLICY.to_string())
}

#[allow(dead_code)]
fn is_protective_policy(policy: &str) -> bool {
    matches!(
        policy.trim().to_ascii_lowercase().as_str(),
        "" | "strict" | "default" | "plan"
    )
}

#[allow(dead_code)]
fn detect_risky_send_reason(text: &str, keys: &[String]) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if let Some(token) = RISKY_SEND_TOKENS.iter().find(|token| {
        if **token == "curl " {
            lower.contains("curl ") && (lower.contains("| sh") || lower.contains("| bash"))
        } else {
            lower.contains(**token)
        }
    }) {
        return Some(format!("payload contains risky token '{token}'"));
    }

    if let Some(key) = keys
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .find(|key| matches!(key.as_str(), "c-c" | "c-z" | "c-d"))
    {
        return Some(format!("key sequence '{key}' may interrupt live work"));
    }

    None
}

#[allow(dead_code)]
fn detect_risky_interrupt_reason(state: AgentState) -> Option<String> {
    match state {
        AgentState::WaitingApproval => {
            Some("agent is waiting_approval; interrupt may discard pending approval".to_string())
        }
        AgentState::Paused => {
            Some("agent is paused; interrupt may lose paused context".to_string())
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn enforce_risky_action_policy(
    policy: &str,
    allow_risky: bool,
    action: &str,
    agent_id: &str,
    reason: Option<String>,
) -> Result<(), String> {
    let Some(reason) = reason else {
        return Ok(());
    };
    if allow_risky || !is_protective_policy(policy) {
        return Ok(());
    }
    Err(format!(
        "policy '{policy}' blocked risky {action} for agent '{agent_id}': {reason}. Retry with --allow-risky or set --approval-policy to a less restrictive mode"
    ))
}

fn key_is_sensitive(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_TOKENS
        .iter()
        .any(|token| lower.contains(token))
}

fn text_contains_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    SENSITIVE_VALUE_MARKERS
        .iter()
        .any(|token| lower.contains(token))
}

fn redact_sensitive_text(value: &str) -> String {
    if text_contains_sensitive_marker(value) {
        REDACTED_VALUE.to_string()
    } else {
        value.to_string()
    }
}

fn redact_sensitive_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut redacted = serde_json::Map::with_capacity(map.len());
            for (key, value) in map {
                if key_is_sensitive(key) {
                    redacted.insert(
                        key.clone(),
                        serde_json::Value::String(REDACTED_VALUE.to_string()),
                    );
                } else {
                    redacted.insert(key.clone(), redact_sensitive_json(value));
                }
            }
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_sensitive_json).collect())
        }
        serde_json::Value::String(text) => serde_json::Value::String(redact_sensitive_text(text)),
        _ => value.clone(),
    }
}

fn redact_detail_payload(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => serde_json::to_string(&redact_sensitive_json(&value))
            .unwrap_or_else(|_| REDACTED_VALUE.to_string()),
        Err(_) => redact_sensitive_text(raw),
    }
}

fn append_sanitized_event(
    repo: &PersistentAgentEventRepository<'_>,
    event: &mut PersistentAgentEvent,
) -> Result<(), forge_db::DbError> {
    event.outcome = redact_sensitive_text(&event.outcome);
    event.detail = event
        .detail
        .as_ref()
        .map(|value| redact_detail_payload(value));
    repo.append(event)
}

fn exec_summary(
    args: &SummaryArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let db_path = resolve_database_path();
    exec_summary_with_db_path(args, parsed, stdout, &db_path)
}

fn exec_summary_with_db_path(
    args: &SummaryArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
    db_path: &Path,
) -> Result<(), String> {
    if args.agent_id.trim().is_empty() {
        return Err("error: agent ID is required for agent summary".to_string());
    }

    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open db {}: {err}", db_path.display()))?;
    db.migrate_up()
        .map_err(|err| format!("migrate db {}: {err}", db_path.display()))?;

    let transcript_repo = TranscriptRepository::new(&db);
    let event_repo = PersistentAgentEventRepository::new(&db);

    let transcript = match transcript_repo.latest_by_agent(&args.agent_id) {
        Ok(value) => Some(value),
        Err(forge_db::DbError::TranscriptNotFound) => None,
        Err(err) => return Err(format!("load transcript for {}: {err}", args.agent_id)),
    };

    let events = event_repo
        .list_by_agent(&args.agent_id, SUMMARY_EVENT_SCAN_LIMIT)
        .map_err(|err| format!("load events for {}: {err}", args.agent_id))?;

    let snapshot = summarize_agent_snapshot(&args.agent_id, transcript.as_ref(), &events);
    let snapshot_event_id = persist_summary_snapshot(&event_repo, &snapshot)?;
    let output = AgentSummaryJson {
        snapshot,
        snapshot_event_id,
    };

    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &output).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &output).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if parsed.quiet {
        return Ok(());
    }

    writeln!(stdout, "agent: {}", output.snapshot.agent_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "status: {}", output.snapshot.concise_status).map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "latest_task_outcome: {}",
        output.snapshot.latest_task_outcome
    )
    .map_err(|e| e.to_string())?;
    if output.snapshot.unresolved_blockers.is_empty() {
        writeln!(stdout, "unresolved_blockers: none").map_err(|e| e.to_string())?;
    } else {
        writeln!(stdout, "unresolved_blockers:").map_err(|e| e.to_string())?;
        for blocker in &output.snapshot.unresolved_blockers {
            writeln!(stdout, "- {blocker}").map_err(|e| e.to_string())?;
        }
    }
    if !output.snapshot.transcript_excerpt.is_empty() {
        writeln!(stdout, "transcript_excerpt:").map_err(|e| e.to_string())?;
        for line in &output.snapshot.transcript_excerpt {
            writeln!(stdout, "- {line}").map_err(|e| e.to_string())?;
        }
    }
    writeln!(stdout, "snapshot_event_id: {}", output.snapshot_event_id)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn summarize_agent_snapshot(
    agent_id: &str,
    transcript: Option<&Transcript>,
    events: &[PersistentAgentEvent],
) -> AgentSummarySnapshot {
    let transcript_lines = transcript
        .map(|t| collect_recent_lines(&t.content, SUMMARY_TRANSCRIPT_SCAN_LIMIT))
        .unwrap_or_default();
    let blockers = extract_unresolved_blockers(&transcript_lines, events);
    let latest_task_outcome = latest_task_outcome(&transcript_lines, events);
    let concise_status = concise_status(events, &blockers, &latest_task_outcome);

    AgentSummarySnapshot {
        agent_id: agent_id.to_string(),
        concise_status,
        latest_task_outcome,
        unresolved_blockers: blockers,
        transcript_excerpt: transcript_lines
            .iter()
            .rev()
            .take(SUMMARY_EXCERPT_LIMIT)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
        transcript_captured_at: transcript.map(|t| t.captured_at.clone()),
        recent_events_considered: events.len(),
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn persist_summary_snapshot(
    repo: &PersistentAgentEventRepository<'_>,
    snapshot: &AgentSummarySnapshot,
) -> Result<i64, String> {
    let detail_value = serde_json::to_value(snapshot).map_err(|err| err.to_string())?;
    let detail = serde_json::to_string(&redact_sensitive_json(&detail_value))
        .map_err(|err| err.to_string())?;
    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: Some(snapshot.agent_id.clone()),
        kind: "summary_snapshot".to_string(),
        outcome: snapshot.concise_status.clone(),
        detail: Some(detail),
        timestamp: String::new(),
    };
    append_sanitized_event(repo, &mut event).map_err(|err| err.to_string())?;
    Ok(event.id)
}

fn observe_counter_metric(
    name: &str,
    outcome: &str,
    agent_id: Option<&str>,
    detail: Option<serde_json::Value>,
) {
    with_observability_repos(|event_repo, _agent_repo| {
        let mut payload = serde_json::json!({
            "type": "counter",
            "name": name,
            "value": 1
        });
        if let Some(extra) = &detail {
            payload["detail"] = extra.clone();
        }
        append_metric_event(event_repo, name, outcome, agent_id, payload);
    });
}

fn observe_latency_metric(name: &str, outcome: &str, agent_id: Option<&str>, value_ms: u128) {
    with_observability_repos(|event_repo, _agent_repo| {
        let payload = serde_json::json!({
            "type": "latency_ms",
            "name": name,
            "value_ms": value_ms
        });
        append_metric_event(event_repo, name, outcome, agent_id, payload);
    });
}

fn observe_gauge_metrics(workspace_id: Option<&str>) {
    with_observability_repos(|event_repo, agent_repo| {
        let filter = PersistentAgentFilter {
            workspace_id: workspace_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            limit: OBSERVABILITY_AGENT_SCAN_LIMIT,
            ..Default::default()
        };
        let agents = match agent_repo.list(filter) {
            Ok(value) => value,
            Err(_) => return,
        };
        let active = agents
            .iter()
            .filter(|agent| !matches!(agent.state.as_str(), "failed" | "stopped"))
            .count() as i64;
        let idle = agents.iter().filter(|agent| agent.state == "idle").count() as i64;

        let scope = workspace_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("all");
        append_metric_event(
            event_repo,
            "active_agents",
            "snapshot",
            None,
            serde_json::json!({
                "type": "gauge",
                "name": "active_agents",
                "value": active,
                "scope": scope
            }),
        );
        append_metric_event(
            event_repo,
            "idle_agents",
            "snapshot",
            None,
            serde_json::json!({
                "type": "gauge",
                "name": "idle_agents",
                "value": idle,
                "scope": scope
            }),
        );
    });
}

fn append_metric_event(
    repo: &PersistentAgentEventRepository<'_>,
    name: &str,
    outcome: &str,
    agent_id: Option<&str>,
    detail: serde_json::Value,
) {
    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: agent_id.map(ToOwned::to_owned),
        kind: format!("metric_{name}"),
        outcome: outcome.to_string(),
        detail: serde_json::to_string(&redact_sensitive_json(&detail)).ok(),
        timestamp: String::new(),
    };
    let _ = append_sanitized_event(repo, &mut event);
}

fn with_observability_repos(
    mut callback: impl FnMut(&PersistentAgentEventRepository<'_>, &PersistentAgentRepository<'_>),
) {
    let db_path = resolve_database_path();
    let mut db = match forge_db::Db::open(forge_db::Config::new(&db_path)) {
        Ok(value) => value,
        Err(_) => return,
    };
    if db.migrate_up().is_err() {
        return;
    }
    let event_repo = PersistentAgentEventRepository::new(&db);
    let agent_repo = PersistentAgentRepository::new(&db);
    callback(&event_repo, &agent_repo);
}

fn is_not_found_error(message: &str) -> bool {
    message.to_ascii_lowercase().contains("not found")
}

fn load_persistent_agent_record(
    agent_id: &str,
) -> Option<forge_db::persistent_agent_repository::PersistentAgent> {
    let db_path = resolve_database_path();
    let mut db = forge_db::Db::open(forge_db::Config::new(&db_path)).ok()?;
    db.migrate_up().ok()?;
    let repo = PersistentAgentRepository::new(&db);
    repo.get(agent_id).ok()
}

fn revive_policy_error_message(agent_id: &str, reason: &str, policy: RevivePolicy) -> String {
    match policy {
        RevivePolicy::Never => format!(
            "agent '{agent_id}' requires revive ({reason}); revive policy is never. Retry with --revive-policy auto or --revive"
        ),
        RevivePolicy::Ask => format!(
            "agent '{agent_id}' requires revive ({reason}); rerun with --revive-policy auto (or --revive) to continue"
        ),
        RevivePolicy::Auto => format!(
            "agent '{agent_id}' requires revive ({reason}); retry with --revive-policy auto"
        ),
    }
}

fn revive_pending_objective(summary: &AgentSummarySnapshot) -> String {
    if let Some(blocker) = summary.unresolved_blockers.first() {
        return format!("resolve blocker: {blocker}");
    }
    if let Some(line) = summary
        .transcript_excerpt
        .iter()
        .rev()
        .find(|line| !line.is_empty())
    {
        return line.clone();
    }
    "continue previous delegated objective and report concise status".to_string()
}

fn build_revive_preamble(agent_id: &str, parent_agent_id: Option<&str>) -> String {
    let db_path = resolve_database_path();
    let mut transcript = None;
    let mut events = Vec::new();
    if let Ok(mut db) = forge_db::Db::open(forge_db::Config::new(&db_path)) {
        if db.migrate_up().is_ok() {
            let transcript_repo = TranscriptRepository::new(&db);
            if let Ok(value) = transcript_repo.latest_by_agent(agent_id) {
                transcript = Some(value);
            }
            let event_repo = PersistentAgentEventRepository::new(&db);
            if let Ok(value) = event_repo.list_by_agent(agent_id, SUMMARY_EVENT_SCAN_LIMIT) {
                events = value;
            }
        }
    }

    let summary = summarize_agent_snapshot(agent_id, transcript.as_ref(), &events);
    let pending_objective = revive_pending_objective(&summary);

    let mut lines = vec![
        "Context rehydration after revive.".to_string(),
        format!("Agent-ID: {agent_id}"),
    ];
    if let Some(parent) = parent_agent_id.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("Parent-Agent-ID: {parent}"));
    }
    lines.push(format!("Last-task: {}", summary.latest_task_outcome));
    lines.push(format!("Summary-status: {}", summary.concise_status));
    lines.push(format!("Pending-objective: {pending_objective}"));
    lines.push("Resume work from this context and return concise status + next step.".to_string());

    lines.join("\n")
}

fn is_wait_timeout_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("wait timeout") || lower.contains("timed out")
}

fn concise_status(
    events: &[PersistentAgentEvent],
    unresolved_blockers: &[String],
    latest_task_outcome: &str,
) -> String {
    if !unresolved_blockers.is_empty() || contains_blocker_keyword(latest_task_outcome) {
        return "blocked".to_string();
    }

    if let Some(event) = events.iter().find(|event| event.kind != "summary_snapshot") {
        let outcome = event.outcome.to_ascii_lowercase();
        if outcome.contains("error") || outcome.contains("fail") {
            return "needs_attention".to_string();
        }
        if event.kind == "wait_state" && outcome.contains("success") {
            return "idle".to_string();
        }
        if event.kind == "spawn" && outcome.contains("success") {
            return "running".to_string();
        }
        return "active".to_string();
    }

    if latest_task_outcome == "unknown" {
        "unknown".to_string()
    } else {
        "active".to_string()
    }
}

fn latest_task_outcome(transcript_lines: &[String], events: &[PersistentAgentEvent]) -> String {
    if let Some(event) = events.iter().find(|event| event.kind != "summary_snapshot") {
        let mut value = format!("{}: {}", event.kind, event.outcome);
        if let Some(detail) = event.detail.as_ref().filter(|d| !d.trim().is_empty()) {
            value.push_str(" :: ");
            value.push_str(detail.trim());
        }
        return truncate_summary_text(&value);
    }

    for line in transcript_lines.iter().rev() {
        if line.is_empty() {
            continue;
        }
        if contains_outcome_keyword(line) {
            return truncate_summary_text(line);
        }
    }

    transcript_lines
        .iter()
        .rev()
        .find(|line| !line.is_empty())
        .map(|line| truncate_summary_text(line))
        .unwrap_or_else(|| "unknown".to_string())
}

fn extract_unresolved_blockers(
    transcript_lines: &[String],
    events: &[PersistentAgentEvent],
) -> Vec<String> {
    let mut blockers = Vec::new();
    let mut seen = HashSet::new();

    for line in transcript_lines.iter().rev() {
        if !contains_blocker_keyword(line) {
            continue;
        }
        let normalized = line.trim().to_ascii_lowercase();
        if seen.insert(normalized) {
            blockers.push(truncate_summary_text(line));
        }
        if blockers.len() >= SUMMARY_BLOCKER_LIMIT {
            return blockers;
        }
    }

    for event in events
        .iter()
        .filter(|event| event.kind != "summary_snapshot")
    {
        let mut candidate = None;
        if contains_blocker_keyword(&event.outcome) {
            candidate = Some(event.outcome.trim().to_string());
        } else if let Some(detail) = event.detail.as_ref() {
            if contains_blocker_keyword(detail) {
                candidate = Some(detail.trim().to_string());
            }
        }

        if let Some(text) = candidate {
            let normalized = text.to_ascii_lowercase();
            if seen.insert(normalized) {
                blockers.push(truncate_summary_text(&text));
            }
        }
        if blockers.len() >= SUMMARY_BLOCKER_LIMIT {
            break;
        }
    }

    blockers
}

fn contains_blocker_keyword(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "blocked",
        "blocker",
        "waiting on",
        "waiting for",
        "cannot",
        "can't",
        "failed",
        "error",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn contains_outcome_keyword(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "done",
        "complete",
        "completed",
        "shipped",
        "fixed",
        "passed",
        "failed",
        "error",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn collect_recent_lines(content: &str, limit: usize) -> Vec<String> {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(truncate_summary_text)
        .collect::<Vec<_>>();
    if lines.len() > limit {
        let split = lines.len() - limit;
        lines.drain(0..split);
    }
    lines
}

fn truncate_summary_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= SUMMARY_TEXT_LIMIT {
        return trimmed.to_string();
    }
    let mut truncated = trimmed
        .chars()
        .take(SUMMARY_TEXT_LIMIT.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

const PARKED_AGENT_STATES: &[&str] = &["idle", "stopped", "failed"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvictionReason {
    Ttl,
    IdleTimeout,
    MaxAge,
}

impl EvictionReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ttl => "ttl",
            Self::IdleTimeout => "idle_timeout",
            Self::MaxAge => "max_age",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvictionCandidate {
    reason: EvictionReason,
    age_seconds: i64,
    idle_seconds: i64,
    ttl_seconds: Option<i64>,
}

fn exec_gc(args: &GcArgs, parsed: &ParsedArgs, stdout: &mut dyn Write) -> Result<(), String> {
    let db_path = resolve_database_path();
    exec_gc_with_db_path(args, parsed, stdout, &db_path)
}

fn exec_gc_with_db_path(
    args: &GcArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
    db_path: &Path,
) -> Result<(), String> {
    let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open db {}: {err}", db_path.display()))?;
    db.migrate_up()
        .map_err(|err| format!("migrate db {}: {err}", db_path.display()))?;

    let repo = forge_db::persistent_agent_repository::PersistentAgentRepository::new(&db);
    let event_repo = PersistentAgentEventRepository::new(&db);

    let agents = repo
        .list(
            forge_db::persistent_agent_repository::PersistentAgentFilter {
                workspace_id: if args.workspace_id.trim().is_empty() {
                    None
                } else {
                    Some(args.workspace_id.clone())
                },
                states: PARKED_AGENT_STATES
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
                limit: args.limit,
                ..Default::default()
            },
        )
        .map_err(|err| format!("list persistent agents: {err}"))?;

    let now = chrono::Utc::now();
    let mut evictions = Vec::new();

    for agent in &agents {
        let Some(candidate) = evaluate_eviction_candidate(agent, args, &now) else {
            continue;
        };

        let detail = serde_json::json!({
            "reason": candidate.reason.as_str(),
            "age_seconds": candidate.age_seconds,
            "idle_seconds": candidate.idle_seconds,
            "ttl_seconds": candidate.ttl_seconds,
            "workspace_id": agent.workspace_id,
        })
        .to_string();

        if !args.dry_run {
            let mut start_event = PersistentAgentEvent {
                id: 0,
                agent_id: Some(agent.id.clone()),
                kind: "gc_evict_start".to_string(),
                outcome: "candidate".to_string(),
                detail: Some(detail.clone()),
                timestamp: String::new(),
            };
            append_sanitized_event(&event_repo, &mut start_event)
                .map_err(|err| format!("append gc start event for {}: {err}", agent.id))?;

            if let Err(err) = repo.delete(&agent.id) {
                let mut failed_event = PersistentAgentEvent {
                    id: 0,
                    agent_id: Some(agent.id.clone()),
                    kind: "gc_evict_done".to_string(),
                    outcome: format!("error: {err}"),
                    detail: Some(detail),
                    timestamp: String::new(),
                };
                let _ = append_sanitized_event(&event_repo, &mut failed_event);
                return Err(format!("evict persistent agent {}: {err}", agent.id));
            }

            let mut done_event = PersistentAgentEvent {
                id: 0,
                agent_id: Some(agent.id.clone()),
                kind: "gc_evict_done".to_string(),
                outcome: "success".to_string(),
                detail: Some(detail),
                timestamp: String::new(),
            };
            append_sanitized_event(&event_repo, &mut done_event)
                .map_err(|err| format!("append gc done event for {}: {err}", agent.id))?;
        }

        evictions.push(AgentGcEvictionJson {
            agent_id: agent.id.clone(),
            reason: candidate.reason.as_str().to_string(),
            age_seconds: candidate.age_seconds,
            idle_seconds: candidate.idle_seconds,
            ttl_seconds: candidate.ttl_seconds,
        });
    }

    let result = AgentGcResultJson {
        scanned: agents.len(),
        evicted: evictions.len(),
        kept: agents.len().saturating_sub(evictions.len()),
        dry_run: args.dry_run,
        evictions,
    };

    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if parsed.quiet {
        return Ok(());
    }

    if result.dry_run {
        writeln!(
            stdout,
            "dry-run: {} eviction candidates (scanned {}, kept {})",
            result.evicted, result.scanned, result.kept
        )
        .map_err(|e| e.to_string())?;
    } else {
        writeln!(
            stdout,
            "evicted {} stale agents (scanned {}, kept {})",
            result.evicted, result.scanned, result.kept
        )
        .map_err(|e| e.to_string())?;
    }
    for item in &result.evictions {
        writeln!(
            stdout,
            "{}	{}	age={}s	idle={}s",
            item.agent_id, item.reason, item.age_seconds, item.idle_seconds
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn evaluate_eviction_candidate(
    agent: &forge_db::persistent_agent_repository::PersistentAgent,
    args: &GcArgs,
    now: &chrono::DateTime<chrono::Utc>,
) -> Option<EvictionCandidate> {
    if !PARKED_AGENT_STATES.contains(&agent.state.as_str()) {
        return None;
    }

    let created_at = parse_agent_timestamp(&agent.created_at)?;
    let last_activity_at = parse_agent_timestamp(&agent.last_activity_at)?;
    let age_seconds = now.signed_duration_since(created_at).num_seconds().max(0);
    let idle_seconds = now
        .signed_duration_since(last_activity_at)
        .num_seconds()
        .max(0);

    if let Some(ttl) = agent.ttl_seconds.filter(|v| *v > 0) {
        if age_seconds >= ttl {
            return Some(EvictionCandidate {
                reason: EvictionReason::Ttl,
                age_seconds,
                idle_seconds,
                ttl_seconds: Some(ttl),
            });
        }
    }

    if let Some(idle_timeout) = args.idle_timeout_seconds {
        if idle_seconds >= idle_timeout {
            return Some(EvictionCandidate {
                reason: EvictionReason::IdleTimeout,
                age_seconds,
                idle_seconds,
                ttl_seconds: agent.ttl_seconds,
            });
        }
    }

    if let Some(max_age) = args.max_age_seconds {
        if age_seconds >= max_age {
            return Some(EvictionCandidate {
                reason: EvictionReason::MaxAge,
                age_seconds,
                idle_seconds,
                ttl_seconds: agent.ttl_seconds,
            });
        }
    }

    None
}

fn parse_agent_timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

fn append_revive_audit_event(agent_id: &str, kind: &str, outcome: &str, detail: serde_json::Value) {
    with_observability_repos(|event_repo, _agent_repo| {
        let mut event = PersistentAgentEvent {
            id: 0,
            agent_id: Some(agent_id.to_string()),
            kind: kind.to_string(),
            outcome: outcome.to_string(),
            detail: serde_json::to_string(&detail).ok(),
            timestamp: String::new(),
        };
        let _ = append_sanitized_event(event_repo, &mut event);
    });
}

fn ensure_persistent_agent_record(
    agent_id: &str,
    workspace_id: &str,
    harness: &str,
    parent_agent_id: Option<&str>,
    approval_context: Option<&ApprovalContext>,
) {
    with_observability_repos(|_event_repo, agent_repo| {
        if let Ok(current) = agent_repo.get(agent_id) {
            if current.parent_agent_id.is_none() {
                let _ = agent_repo.touch_activity(agent_id);
            }
            let _ = agent_repo.update_state(agent_id, "starting");
            return;
        }

        let mut persistent = PersistentAgent {
            id: agent_id.to_string(),
            parent_agent_id: parent_agent_id.map(ToOwned::to_owned),
            workspace_id: workspace_id.to_string(),
            harness: harness.to_string(),
            mode: "continuous".to_string(),
            state: "starting".to_string(),
            labels: approval_context
                .map(ApprovalContext::to_labels)
                .unwrap_or_default(),
            ..Default::default()
        };
        let _ = agent_repo.create(&mut persistent);
    });
}

fn send_revive_context(
    backend: &dyn AgentBackend,
    agent_id: &str,
    parent_agent_id: Option<&str>,
) -> Result<(), String> {
    let preamble = build_revive_preamble(agent_id, parent_agent_id);
    let params = SendMessageParams {
        agent_id: agent_id.to_string(),
        text: preamble,
        send_enter: true,
        keys: Vec::new(),
    };

    match backend.send_message(params) {
        Ok(true) => {
            observe_counter_metric(
                "sends",
                "success",
                Some(agent_id),
                Some(serde_json::json!({ "phase": "revive_preamble" })),
            );
            Ok(())
        }
        Ok(false) => {
            let message = format!(
                "revive context injection returned false for agent '{agent_id}'; retry with `forge agent send {agent_id} <context>`"
            );
            observe_counter_metric(
                "sends",
                "error",
                Some(agent_id),
                Some(serde_json::json!({ "phase": "revive_preamble", "error": message })),
            );
            Err(message)
        }
        Err(err) => {
            observe_counter_metric(
                "sends",
                "error",
                Some(agent_id),
                Some(serde_json::json!({ "phase": "revive_preamble", "error": err })),
            );
            Err(format!(
                "failed to inject revive context for agent '{agent_id}': {err}. Remediation: run `forge agent send {agent_id} <context>`"
            ))
        }
    }
}

struct ReviveContext<'a> {
    workspace_id: &'a str,
    command: &'a str,
    adapter: &'a str,
    harness: &'a str,
    parent_agent_id: Option<&'a str>,
    reason: &'a str,
    policy: RevivePolicy,
    approval_context: ApprovalContext,
    kill_before_spawn: bool,
}

fn revive_agent_with_context(
    backend: &dyn AgentBackend,
    agent_id: &str,
    ctx: ReviveContext<'_>,
) -> Result<AgentSnapshot, String> {
    let start_detail = serde_json::json!({
        "reason": ctx.reason,
        "policy": ctx.policy.as_str(),
        "approval_policy": ctx.approval_context.approval_policy.as_str(),
        "account_id": ctx.approval_context.account_id.as_deref(),
        "profile": ctx.approval_context.profile.as_deref(),
        "workspace_id": ctx.workspace_id,
        "command": ctx.command,
        "adapter": ctx.adapter,
        "harness": ctx.harness,
        "parent_agent_id": ctx.parent_agent_id,
    });
    append_revive_audit_event(agent_id, "revive_start", "started", start_detail.clone());

    if ctx.kill_before_spawn {
        match backend.kill_agent(KillAgentParams {
            agent_id: agent_id.to_string(),
            force: true,
            grace_period: None,
        }) {
            Ok(_) => observe_counter_metric("kill", "success", Some(agent_id), None),
            Err(err) => {
                observe_counter_metric(
                    "kill",
                    "error",
                    Some(agent_id),
                    Some(serde_json::json!({ "error": err })),
                );
                append_revive_audit_event(
                    agent_id,
                    "revive_done",
                    "error",
                    serde_json::json!({
                        "reason": ctx.reason,
                        "phase": "kill",
                        "error": err,
                    }),
                );
                observe_counter_metric(
                    "agents_revived",
                    "error",
                    Some(agent_id),
                    Some(serde_json::json!({ "error": err })),
                );
                return Err(format!(
                    "failed to stop terminal agent '{agent_id}' before revive: {err}"
                ));
            }
        }
    }

    let spawned = match spawn_for_run_with_metrics(
        backend,
        agent_id,
        ctx.workspace_id,
        ctx.command,
        ctx.adapter,
        ctx.harness,
        Some(ctx.adapter),
        &ctx.approval_context,
    ) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            append_revive_audit_event(
                agent_id,
                "revive_done",
                "error",
                serde_json::json!({
                    "reason": ctx.reason,
                    "phase": "spawn",
                    "error": err,
                }),
            );
            observe_counter_metric(
                "agents_revived",
                "error",
                Some(agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(format!(
                "failed to respawn agent '{agent_id}' during revive: {err}"
            ));
        }
    };

    if let Err(err) = send_revive_context(backend, agent_id, ctx.parent_agent_id) {
        append_revive_audit_event(
            agent_id,
            "revive_done",
            "error",
            serde_json::json!({
                "reason": ctx.reason,
                "phase": "preamble",
                "error": err,
            }),
        );
        observe_counter_metric(
            "agents_revived",
            "error",
            Some(agent_id),
            Some(serde_json::json!({ "error": err })),
        );
        return Err(err);
    }

    ensure_persistent_agent_record(
        agent_id,
        ctx.workspace_id,
        ctx.harness,
        ctx.parent_agent_id,
        Some(&ctx.approval_context),
    );
    append_revive_audit_event(agent_id, "revive_done", "success", start_detail);
    observe_counter_metric("agents_revived", "success", Some(agent_id), None);

    Ok(spawned)
}

fn exec_run(
    backend: &dyn AgentBackend,
    args: &RunArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.text.trim().is_empty() {
        return Err("error: task text is required for agent run".to_string());
    }

    let agent_id = if args.agent_id.is_empty() {
        next_auto_agent_id()
    } else {
        args.agent_id.clone()
    };
    let mut workspace_id = if args.workspace_id.is_empty() {
        "default".to_string()
    } else {
        args.workspace_id.clone()
    };
    let (default_command, default_adapter) = defaults_for_agent_type(&args.agent_type);
    let command = if args.command.is_empty() {
        default_command.to_string()
    } else {
        args.command.clone()
    };
    let wait_target = if args.wait_for.is_empty() {
        None
    } else {
        Some(parse_agent_state(&args.wait_for)?)
    };
    let labels = parse_run_labels(&args.labels)?;

    let mut reused = false;
    let mut revived = false;

    let persistent_record = load_persistent_agent_record(&agent_id);
    let approval_context = resolve_run_approval_context(args, persistent_record.as_ref());

    let existing = match backend.get_agent(&agent_id) {
        Ok(snapshot) => Some(snapshot),
        Err(err) if is_not_found_error(&err) => None,
        Err(err) => return Err(err),
    };

    let start_snapshot = if let Some(snapshot) = existing {
        if snapshot.state.is_terminal() {
            if args.revive_policy != RevivePolicy::Auto {
                return Err(revive_policy_error_message(
                    &snapshot.id,
                    &format!("terminal state '{}'", snapshot.state),
                    args.revive_policy,
                ));
            }
            revived = true;
            if args.workspace_id.is_empty() {
                workspace_id = snapshot.workspace_id.clone();
            }
            let revive_command = if args.command.is_empty() {
                snapshot.command.clone()
            } else {
                args.command.clone()
            };
            let revive_adapter = if args.command.is_empty() {
                snapshot.adapter.clone()
            } else {
                default_adapter.to_string()
            };
            let harness = if !snapshot.adapter.trim().is_empty() {
                snapshot.adapter.clone()
            } else {
                args.agent_type.clone()
            };
            let parent_agent_id = persistent_record
                .as_ref()
                .and_then(|record| record.parent_agent_id.as_deref());

            revive_agent_with_context(
                backend,
                &agent_id,
                ReviveContext {
                    workspace_id: &workspace_id,
                    command: &revive_command,
                    adapter: &revive_adapter,
                    harness: &harness,
                    parent_agent_id,
                    reason: &format!("terminal_state:{}", snapshot.state),
                    policy: args.revive_policy,
                    approval_context: approval_context.clone(),
                    kill_before_spawn: true,
                },
            )?
        } else {
            reused = true;
            if args.workspace_id.is_empty() {
                workspace_id = snapshot.workspace_id.clone();
            }
            snapshot
        }
    } else if let Some(record) = persistent_record.as_ref() {
        if args.revive_policy != RevivePolicy::Auto {
            return Err(revive_policy_error_message(
                &agent_id,
                "process/pane missing",
                args.revive_policy,
            ));
        }

        revived = true;
        if args.workspace_id.is_empty() {
            workspace_id = record.workspace_id.clone();
        }

        let harness = if args.agent_type == "codex" {
            record.harness.clone()
        } else {
            args.agent_type.clone()
        };
        let (revive_default_command, revive_default_adapter) = defaults_for_agent_type(&harness);
        let revive_command = if args.command.is_empty() {
            revive_default_command.to_string()
        } else {
            args.command.clone()
        };
        let revive_adapter = if args.command.is_empty() {
            revive_default_adapter.to_string()
        } else {
            default_adapter.to_string()
        };

        revive_agent_with_context(
            backend,
            &agent_id,
            ReviveContext {
                workspace_id: &workspace_id,
                command: &revive_command,
                adapter: &revive_adapter,
                harness: &harness,
                parent_agent_id: record.parent_agent_id.as_deref(),
                reason: "missing_process",
                policy: args.revive_policy,
                approval_context: approval_context.clone(),
                kill_before_spawn: false,
            },
        )?
    } else {
        spawn_for_run_with_metrics(
            backend,
            &agent_id,
            &workspace_id,
            &command,
            default_adapter,
            &args.agent_type,
            None,
            &approval_context,
        )?
    };

    let persisted_parent = persistent_record
        .as_ref()
        .and_then(|record| record.parent_agent_id.as_deref());
    let persisted_harness = if !start_snapshot.adapter.trim().is_empty() {
        start_snapshot.adapter.clone()
    } else {
        args.agent_type.clone()
    };
    ensure_persistent_agent_record(
        &agent_id,
        &workspace_id,
        &persisted_harness,
        persisted_parent,
        Some(&approval_context),
    );
    let run_text = build_run_message(args);
    let send_started_at = Instant::now();
    let send_params = SendMessageParams {
        agent_id: agent_id.clone(),
        text: run_text.clone(),
        send_enter: true,
        keys: Vec::new(),
    };
    if let Err(err) = backend.send_message(send_params) {
        observe_counter_metric(
            "sends",
            "error",
            Some(&agent_id),
            Some(serde_json::json!({ "error": err })),
        );
        return Err(err);
    }
    observe_counter_metric("sends", "success", Some(&agent_id), None);

    let observed = if let Some(target) = wait_target {
        match backend.wait_state(WaitStateParams {
            agent_id: agent_id.clone(),
            target_states: vec![target],
            timeout: Duration::from_secs(args.wait_timeout),
            poll_interval: Duration::from_millis(500),
        }) {
            Ok(snapshot) => {
                if target == AgentState::Idle {
                    observe_latency_metric(
                        "send_to_idle_duration",
                        "success",
                        Some(&agent_id),
                        send_started_at.elapsed().as_millis(),
                    );
                }
                snapshot
            }
            Err(err) => {
                if is_wait_timeout_error(&err) {
                    observe_counter_metric(
                        "wait_timeout",
                        "error",
                        Some(&agent_id),
                        Some(serde_json::json!({ "error": err })),
                    );
                }
                return Err(err);
            }
        }
    } else {
        backend.get_agent(&agent_id).unwrap_or(start_snapshot)
    };

    observe_gauge_metrics(Some(&workspace_id));

    let quick_tail = quick_tail_from_text(&run_text);
    let result = RunResultJson {
        agent_id: observed.id.clone(),
        reused,
        revived,
        observed_state: observed.state.to_string(),
        wait_target: wait_target.map(|state| state.to_string()),
        task_id: empty_to_none(&args.task_id),
        tags: args.tags.clone(),
        labels,
        quick_tail: quick_tail.clone(),
    };

    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if !parsed.quiet {
        let mode = if revived {
            "revived"
        } else if reused {
            "reused"
        } else {
            "spawned"
        };
        writeln!(
            stdout,
            "{}	{}	{}	{}",
            result.agent_id, result.observed_state, mode, quick_tail
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn exec_spawn(
    backend: &dyn AgentBackend,
    args: &SpawnArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.command.is_empty() {
        return Err("error: --command is required for agent spawn".to_string());
    }
    let params = SpawnAgentParams {
        agent_id: args.agent_id.clone(),
        workspace_id: args.workspace_id.clone(),
        command: args.command.clone(),
        args: args.args.clone(),
        env: HashMap::new(),
        working_dir: args.working_dir.clone(),
        session_name: args.session_name.clone(),
        adapter: args.adapter.clone(),
        requested_mode: AgentRequestMode::Continuous,
        allow_oneshot_fallback: false,
    };
    let snapshot = match backend.spawn_agent(params) {
        Ok(value) => value,
        Err(err) => {
            observe_counter_metric(
                "agents_spawned",
                "error",
                empty_to_none(&args.agent_id).as_deref(),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(err);
        }
    };
    observe_counter_metric("agents_spawned", "success", Some(&snapshot.id), None);
    observe_gauge_metrics(Some(&snapshot.workspace_id));
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_send(
    backend: &dyn AgentBackend,
    args: &SendArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent send".to_string());
    }

    let persistent_record = load_persistent_agent_record(&args.agent_id);
    let approval_policy = resolve_control_policy(&args.approval_policy, persistent_record.as_ref());
    if let Err(err) = enforce_risky_action_policy(
        &approval_policy,
        args.allow_risky,
        "send",
        &args.agent_id,
        detect_risky_send_reason(&args.text, &args.keys),
    ) {
        observe_counter_metric(
            "sends",
            "error",
            Some(&args.agent_id),
            Some(serde_json::json!({
                "error": err,
                "policy": approval_policy,
                "reason": "policy_denied"
            })),
        );
        return Err(err);
    }

    let params = SendMessageParams {
        agent_id: args.agent_id.clone(),
        text: args.text.clone(),
        send_enter: args.send_enter,
        keys: args.keys.clone(),
    };
    let ok = match backend.send_message(params) {
        Ok(value) => value,
        Err(err) => {
            observe_counter_metric(
                "sends",
                "error",
                Some(&args.agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(err);
        }
    };
    let outcome = if ok { "success" } else { "error" };
    observe_counter_metric("sends", outcome, Some(&args.agent_id), None);
    write_bool_output(ok, parsed, stdout)
}

fn exec_wait(
    backend: &dyn AgentBackend,
    args: &WaitArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent wait".to_string());
    }
    let target_states: Vec<AgentState> = if args.until.is_empty() {
        vec![AgentState::Idle]
    } else {
        args.until
            .iter()
            .map(|s| parse_agent_state(s))
            .collect::<Result<Vec<_>, _>>()?
    };
    let params = WaitStateParams {
        agent_id: args.agent_id.clone(),
        target_states,
        timeout: Duration::from_secs(args.timeout),
        poll_interval: Duration::from_millis(500),
    };
    let snapshot = match backend.wait_state(params) {
        Ok(value) => value,
        Err(err) => {
            if is_wait_timeout_error(&err) {
                observe_counter_metric(
                    "wait_timeout",
                    "error",
                    Some(&args.agent_id),
                    Some(serde_json::json!({ "error": err })),
                );
            }
            return Err(err);
        }
    };
    observe_gauge_metrics(Some(&snapshot.workspace_id));
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_ps(
    backend: &dyn AgentBackend,
    args: &PsArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let states = if args.state.is_empty() {
        Vec::new()
    } else {
        vec![parse_agent_state(&args.state)?]
    };
    let filter = ListAgentsFilter {
        workspace_id: if args.workspace_id.is_empty() {
            None
        } else {
            Some(args.workspace_id.clone())
        },
        states,
    };
    let agents = backend.list_agents(filter)?;
    write_agent_list_output(&agents, parsed, stdout)
}

fn exec_show(
    backend: &dyn AgentBackend,
    args: &ShowArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent show".to_string());
    }
    let snapshot = backend.get_agent(&args.agent_id)?;
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_validate(
    backend: &dyn AgentBackend,
    args: &ValidateArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let snapshots = if !args.agent_id.is_empty() {
        vec![backend.get_agent(&args.agent_id)?]
    } else {
        let states = if args.state.is_empty() {
            Vec::new()
        } else {
            vec![parse_agent_state(&args.state)?]
        };
        let filter = ListAgentsFilter {
            workspace_id: if args.workspace_id.is_empty() {
                None
            } else {
                Some(args.workspace_id.clone())
            },
            states,
        };
        backend.list_agents(filter)?
    };

    let mut errors = Vec::new();
    for snapshot in &snapshots {
        for issue in validate_agent_snapshot(snapshot) {
            errors.push(AgentValidationIssueJson {
                agent_id: snapshot.id.clone(),
                message: issue,
            });
        }
    }
    let result = AgentValidationResultJson {
        valid: errors.is_empty(),
        checked: snapshots.len(),
        errors,
    };

    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &result).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if result.checked == 0 {
        writeln!(stdout, "No agents found").map_err(|e| e.to_string())?;
    } else if result.valid {
        writeln!(
            stdout,
            "Agent definitions valid: {} checked",
            result.checked
        )
        .map_err(|e| e.to_string())?;
    } else {
        writeln!(
            stdout,
            "Agent definitions invalid: {} issue(s)",
            result.errors.len()
        )
        .map_err(|e| e.to_string())?;
        for issue in &result.errors {
            writeln!(stdout, "- {}: {}", issue.agent_id, issue.message)
                .map_err(|e| e.to_string())?;
        }
    }

    if !result.valid {
        return Err("agent validation failed".to_string());
    }
    Ok(())
}

fn validate_agent_snapshot(snapshot: &AgentSnapshot) -> Vec<String> {
    let mut errors = Vec::new();
    if snapshot.id.trim().is_empty() {
        errors.push("id is empty".to_string());
    }
    if snapshot.workspace_id.trim().is_empty() {
        errors.push("workspace_id is empty".to_string());
    }
    if snapshot.state == AgentState::Unspecified {
        errors.push("state is unspecified".to_string());
    }
    errors
}

fn exec_interrupt(
    backend: &dyn AgentBackend,
    args: &InterruptArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent interrupt".to_string());
    }

    let persistent_record = load_persistent_agent_record(&args.agent_id);
    let approval_policy = resolve_control_policy(&args.approval_policy, persistent_record.as_ref());
    let current = match backend.get_agent(&args.agent_id) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            observe_counter_metric(
                "interrupts",
                "error",
                Some(&args.agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(err);
        }
    };

    if let Err(err) = enforce_risky_action_policy(
        &approval_policy,
        args.allow_risky,
        "interrupt",
        &args.agent_id,
        detect_risky_interrupt_reason(current.state),
    ) {
        observe_counter_metric(
            "interrupts",
            "error",
            Some(&args.agent_id),
            Some(serde_json::json!({
                "error": err,
                "policy": approval_policy,
                "reason": "policy_denied"
            })),
        );
        return Err(err);
    }

    let ok = match backend.interrupt_agent(&args.agent_id) {
        Ok(value) => value,
        Err(err) => {
            observe_counter_metric(
                "interrupts",
                "error",
                Some(&args.agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(err);
        }
    };
    let outcome = if ok { "success" } else { "error" };
    observe_counter_metric("interrupts", outcome, Some(&args.agent_id), None);
    write_bool_output(ok, parsed, stdout)
}

fn exec_kill(
    backend: &dyn AgentBackend,
    args: &KillArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent kill".to_string());
    }
    let params = KillAgentParams {
        agent_id: args.agent_id.clone(),
        force: args.force,
        grace_period: None,
    };
    let ok = match backend.kill_agent(params) {
        Ok(value) => value,
        Err(err) => {
            observe_counter_metric(
                "kill",
                "error",
                Some(&args.agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            return Err(err);
        }
    };
    let outcome = if ok { "success" } else { "error" };
    observe_counter_metric("kill", outcome, Some(&args.agent_id), None);
    observe_gauge_metrics(None);
    write_bool_output(ok, parsed, stdout)
}

fn exec_revive(
    backend: &dyn AgentBackend,
    args: &ReviveArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent revive".to_string());
    }

    let persistent_record = load_persistent_agent_record(&args.agent_id);
    let approval_context = ApprovalContext {
        approval_policy: resolve_control_policy("", persistent_record.as_ref()),
        account_id: pick_context_value(
            "",
            persistent_record.as_ref(),
            ACCOUNT_ID_LABEL_KEY,
            ACCOUNT_ID_ENV_KEY,
        ),
        profile: pick_context_value(
            "",
            persistent_record.as_ref(),
            PROFILE_LABEL_KEY,
            PROFILE_ENV_KEY,
        ),
    };
    let existing = match backend.get_agent(&args.agent_id) {
        Ok(snapshot) => Some(snapshot),
        Err(err) if is_not_found_error(&err) => None,
        Err(err) => return Err(err),
    };

    let revived = if let Some(snapshot) = existing {
        if !snapshot.state.is_terminal() {
            return Err(format!(
                "agent '{}' is in state '{}'; revive only allowed from stopped/failed. Use `forge agent send` or `forge agent run`",
                snapshot.id, snapshot.state
            ));
        }

        let harness = if !snapshot.adapter.trim().is_empty() {
            snapshot.adapter.clone()
        } else {
            snapshot.command.clone()
        };
        let parent_agent_id = persistent_record
            .as_ref()
            .and_then(|record| record.parent_agent_id.as_deref());

        revive_agent_with_context(
            backend,
            &snapshot.id,
            ReviveContext {
                workspace_id: &snapshot.workspace_id,
                command: &snapshot.command,
                adapter: &snapshot.adapter,
                harness: &harness,
                parent_agent_id,
                reason: &format!("terminal_state:{}", snapshot.state),
                policy: RevivePolicy::Auto,
                approval_context: approval_context.clone(),
                kill_before_spawn: true,
            },
        )?
    } else if let Some(record) = persistent_record.as_ref() {
        let (command, adapter) = defaults_for_agent_type(&record.harness);
        revive_agent_with_context(
            backend,
            &args.agent_id,
            ReviveContext {
                workspace_id: &record.workspace_id,
                command,
                adapter,
                harness: &record.harness,
                parent_agent_id: record.parent_agent_id.as_deref(),
                reason: "missing_process",
                policy: RevivePolicy::Auto,
                approval_context: approval_context.clone(),
                kill_before_spawn: false,
            },
        )?
    } else {
        return Err(format!(
            "agent '{}' not found and no persistent record exists; use `forge agent spawn {}` to create a new agent",
            args.agent_id, args.agent_id
        ));
    };

    observe_gauge_metrics(Some(&revived.workspace_id));
    write_agent_output(&revived, parsed, stdout)
}

fn spawn_for_run(
    backend: &dyn AgentBackend,
    agent_id: &str,
    workspace_id: &str,
    command: &str,
    default_adapter: &str,
    agent_type: &str,
    adapter_override: Option<&str>,
    approval_context: &ApprovalContext,
) -> Result<AgentSnapshot, String> {
    let working_dir = std::env::current_dir()
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| ".".to_string());
    let (_, mapped_adapter) = defaults_for_agent_type(agent_type);
    let adapter = if let Some(value) = adapter_override.filter(|value| !value.trim().is_empty()) {
        value.to_string()
    } else if mapped_adapter.is_empty() {
        default_adapter.to_string()
    } else {
        mapped_adapter.to_string()
    };
    backend.spawn_agent(SpawnAgentParams {
        agent_id: agent_id.to_string(),
        workspace_id: workspace_id.to_string(),
        command: command.to_string(),
        args: Vec::new(),
        env: approval_context.to_spawn_env(),
        working_dir,
        session_name: agent_id.to_string(),
        adapter,
        requested_mode: AgentRequestMode::Continuous,
        allow_oneshot_fallback: false,
    })
}

fn spawn_for_run_with_metrics(
    backend: &dyn AgentBackend,
    agent_id: &str,
    workspace_id: &str,
    command: &str,
    default_adapter: &str,
    agent_type: &str,
    adapter_override: Option<&str>,
    approval_context: &ApprovalContext,
) -> Result<AgentSnapshot, String> {
    let spawned = spawn_for_run(
        backend,
        agent_id,
        workspace_id,
        command,
        default_adapter,
        agent_type,
        adapter_override,
        approval_context,
    );
    match spawned {
        Ok(snapshot) => {
            observe_counter_metric("agents_spawned", "success", Some(&snapshot.id), None);
            Ok(snapshot)
        }
        Err(err) => {
            observe_counter_metric(
                "agents_spawned",
                "error",
                Some(agent_id),
                Some(serde_json::json!({ "error": err })),
            );
            Err(err)
        }
    }
}

fn defaults_for_agent_type(agent_type: &str) -> (&str, &str) {
    match agent_type {
        "claude" | "claude_code" => ("claude", "claude_code"),
        "opencode" => ("opencode", "opencode"),
        "droid" => ("droid", "droid"),
        "codex" | "" => ("codex", "codex"),
        _ => ("codex", "codex"),
    }
}

fn build_run_message(args: &RunArgs) -> String {
    let mut lines = Vec::new();
    if !args.task_id.is_empty() {
        lines.push(format!("Task-ID: {}", args.task_id));
    }
    if !args.tags.is_empty() {
        lines.push(format!("Tags: {}", args.tags.join(",")));
    }
    if !args.labels.is_empty() {
        lines.push(format!("Labels: {}", args.labels.join(",")));
    }
    if lines.is_empty() {
        return args.text.clone();
    }
    lines.push(String::new());
    lines.push(args.text.clone());
    lines.join("\n")
}

fn parse_run_labels(raw: &[String]) -> Result<HashMap<String, String>, String> {
    let mut labels = HashMap::new();
    for entry in raw {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(format!(
                "invalid --label value '{}'; expected KEY=VALUE format",
                entry
            ));
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(format!(
                "invalid --label value '{}'; expected non-empty KEY and VALUE",
                entry
            ));
        }
        labels.insert(key.to_string(), value.to_string());
    }
    Ok(labels)
}

fn empty_to_none(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn quick_tail_from_text(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    if line.len() <= 96 {
        line.to_string()
    } else {
        format!("{}...", &line[..93])
    }
}

fn next_auto_agent_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let next = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("agent-run-{next}")
}

// ── Output helpers ───────────────────────────────────────────────────────────

fn write_agent_output(
    snapshot: &AgentSnapshot,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let json_entry = AgentJson::from(snapshot);
    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &json_entry).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &json_entry).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if !parsed.quiet {
        writeln!(
            stdout,
            "{}\t{}\t{}\t{}",
            snapshot.id, snapshot.state, snapshot.command, snapshot.adapter,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_agent_list_output(
    agents: &[AgentSnapshot],
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if parsed.json {
        let entries: Vec<AgentJson> = agents.iter().map(AgentJson::from).collect();
        let output = AgentListJson {
            total: entries.len(),
            agents: entries,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        for agent in agents {
            let entry = AgentJson::from(agent);
            serde_json::to_writer(&mut *stdout, &entry).map_err(|e| e.to_string())?;
            writeln!(stdout).map_err(|e| e.to_string())?;
        }
    } else if !parsed.quiet {
        if agents.is_empty() {
            writeln!(stdout, "No agents found").map_err(|e| e.to_string())?;
        } else {
            writeln!(stdout, "ID\tSTATE\tCOMMAND\tADAPTER\tWORKSPACE")
                .map_err(|e| e.to_string())?;
            for agent in agents {
                writeln!(
                    stdout,
                    "{}\t{}\t{}\t{}\t{}",
                    agent.id, agent.state, agent.command, agent.adapter, agent.workspace_id,
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

fn write_bool_output(ok: bool, parsed: &ParsedArgs, stdout: &mut dyn Write) -> Result<(), String> {
    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &BoolResultJson { ok })
            .map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &BoolResultJson { ok }).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if !parsed.quiet {
        writeln!(stdout, "{ok}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn parse_agent_state(s: &str) -> Result<AgentState, String> {
    match AgentState::from_str(s) {
        Some(state) if state != AgentState::Unspecified => Ok(state),
        _ => Err(format!(
            "invalid agent state: '{s}'. Valid states: {}",
            AgentState::command_filter_values().join(", ")
        )),
    }
}

// ── Arg parser ───────────────────────────────────────────────────────────────

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;

    // Skip the command name "agent" if present.
    if args.get(index).is_some_and(|t| t == "agent") {
        index += 1;
    }

    // Collect top-level flags before subcommand.
    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--quiet" => {
                quiet = true;
                index += 1;
            }
            _ => break,
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let subcommand_name = args.get(index).map(|s| s.as_str());
    match subcommand_name {
        None | Some("help") | Some("-h") | Some("--help") => Ok(ParsedArgs {
            json,
            jsonl,
            quiet,
            subcommand: Subcommand::Help,
        }),
        Some("run") => {
            index += 1;
            let run_args = parse_run_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Run(run_args),
            })
        }
        Some("spawn") => {
            index += 1;
            let spawn_args = parse_spawn_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Spawn(spawn_args),
            })
        }
        Some("send") => {
            index += 1;
            let send_args = parse_send_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Send(send_args),
            })
        }
        Some("wait") => {
            index += 1;
            let wait_args = parse_wait_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Wait(wait_args),
            })
        }
        Some("ps") | Some("list") | Some("ls") => {
            index += 1;
            let ps_args = parse_ps_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Ps(ps_args),
            })
        }
        Some("show") | Some("get") => {
            index += 1;
            let show_args = parse_show_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Show(show_args),
            })
        }
        Some("validate") => {
            index += 1;
            let validate_args = parse_validate_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Validate(validate_args),
            })
        }
        Some("summary") => {
            index += 1;
            let summary_args = parse_summary_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Summary(summary_args),
            })
        }
        Some("gc") => {
            index += 1;
            let gc_args = parse_gc_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Gc(gc_args),
            })
        }
        Some("interrupt") => {
            index += 1;
            let int_args = parse_interrupt_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Interrupt(int_args),
            })
        }
        Some("kill") => {
            index += 1;
            let kill_args = parse_kill_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Kill(kill_args),
            })
        }
        Some("revive") => {
            index += 1;
            let revive_args = parse_revive_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Revive(revive_args),
            })
        }
        Some(other) => Err(format!(
            "unknown agent subcommand: '{other}'. Run `forge agent help` for usage."
        )),
    }
}

fn parse_revive_policy(raw: &str) -> Result<RevivePolicy, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "never" => Ok(RevivePolicy::Never),
        "ask" => Ok(RevivePolicy::Ask),
        "auto" => Ok(RevivePolicy::Auto),
        _ => Err(format!(
            "invalid revive policy: '{raw}'. Valid values: never, ask, auto"
        )),
    }
}

fn parse_run_args(args: &[String], mut index: usize) -> Result<RunArgs, String> {
    let mut result = RunArgs {
        text: String::new(),
        agent_id: String::new(),
        workspace_id: String::new(),
        agent_type: "codex".to_string(),
        command: String::new(),
        approval_policy: String::new(),
        account_id: String::new(),
        profile: String::new(),
        wait_for: String::new(),
        wait_timeout: 300,
        revive_policy: RevivePolicy::Never,
        task_id: String::new(),
        tags: Vec::new(),
        labels: Vec::new(),
    };

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(RUN_HELP.to_string()),
            "--agent" => {
                result.agent_id = take_value(args, index, "--agent")?;
                index += 2;
            }
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--type" => {
                result.agent_type = take_value(args, index, "--type")?;
                index += 2;
            }
            "--command" | "-c" => {
                result.command = take_value(args, index, "--command")?;
                index += 2;
            }
            "--wait" => {
                result.wait_for = take_value(args, index, "--wait")?;
                index += 2;
            }
            "--timeout" => {
                let timeout = take_value(args, index, "--timeout")?;
                result.wait_timeout = timeout
                    .parse::<u64>()
                    .map_err(|_| format!("invalid timeout value: '{timeout}'"))?;
                index += 2;
            }
            "--revive" => {
                result.revive_policy = RevivePolicy::Auto;
                index += 1;
            }
            "--revive-policy" => {
                let value = take_value(args, index, "--revive-policy")?;
                result.revive_policy = parse_revive_policy(&value)?;
                index += 2;
            }
            "--task-id" => {
                result.task_id = take_value(args, index, "--task-id")?;
                index += 2;
            }
            "--tag" => {
                result.tags.push(take_value(args, index, "--tag")?);
                index += 2;
            }
            "--label" => {
                result.labels.push(take_value(args, index, "--label")?);
                index += 2;
            }
            "--text" | "-t" => {
                result.text = take_value(args, index, "--text")?;
                index += 2;
            }
            "--approval-policy" => {
                result.approval_policy = take_value(args, index, "--approval-policy")?;
                index += 2;
            }
            "--account-id" => {
                result.account_id = take_value(args, index, "--account-id")?;
                index += 2;
            }
            "--profile" => {
                result.profile = take_value(args, index, "--profile")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent run: '{flag}'"));
            }
            positional => {
                if result.text.is_empty() {
                    result.text = positional.to_string();
                    index += 1;
                } else {
                    return Err(format!(
                        "error: unexpected positional argument: '{positional}'"
                    ));
                }
            }
        }
    }

    if result.text.trim().is_empty() {
        return Err("error: task text is required for agent run".to_string());
    }
    Ok(result)
}

fn parse_spawn_args(args: &[String], mut index: usize) -> Result<SpawnArgs, String> {
    let mut result = SpawnArgs {
        agent_id: String::new(),
        workspace_id: String::new(),
        command: String::new(),
        args: Vec::new(),
        working_dir: String::new(),
        session_name: String::new(),
        adapter: String::new(),
    };

    // First positional arg is agent_id (optional, auto-generated if empty).
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(SPAWN_HELP.to_string()),
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--command" | "-c" => {
                result.command = take_value(args, index, "--command")?;
                index += 2;
            }
            "--arg" => {
                result.args.push(take_value(args, index, "--arg")?);
                index += 2;
            }
            "--working-dir" => {
                result.working_dir = take_value(args, index, "--working-dir")?;
                index += 2;
            }
            "--session" => {
                result.session_name = take_value(args, index, "--session")?;
                index += 2;
            }
            "--adapter" => {
                result.adapter = take_value(args, index, "--adapter")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent spawn: '{flag}'"));
            }
            _ => {
                return Err(format!(
                    "error: unexpected positional argument: '{}'",
                    token
                ));
            }
        }
    }

    Ok(result)
}

fn parse_send_args(args: &[String], mut index: usize) -> Result<SendArgs, String> {
    let mut result = SendArgs {
        agent_id: String::new(),
        text: String::new(),
        send_enter: true,
        keys: Vec::new(),
        approval_policy: String::new(),
        allow_risky: false,
    };

    // Positional: agent_id.
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(SEND_HELP.to_string()),
            "--text" | "-t" => {
                result.text = take_value(args, index, "--text")?;
                index += 2;
            }
            "--no-enter" => {
                result.send_enter = false;
                index += 1;
            }
            "--key" => {
                result.keys.push(take_value(args, index, "--key")?);
                index += 2;
            }
            "--approval-policy" => {
                result.approval_policy = take_value(args, index, "--approval-policy")?;
                index += 2;
            }
            "--allow-risky" => {
                result.allow_risky = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent send: '{flag}'"));
            }
            positional => {
                // If text is empty and we have a positional, use it as text.
                if result.text.is_empty() {
                    result.text = positional.to_string();
                    index += 1;
                } else {
                    return Err(format!(
                        "error: unexpected positional argument: '{positional}'"
                    ));
                }
            }
        }
    }

    Ok(result)
}

fn parse_wait_args(args: &[String], mut index: usize) -> Result<WaitArgs, String> {
    let mut result = WaitArgs {
        agent_id: String::new(),
        until: Vec::new(),
        timeout: 300,
    };

    // Positional: agent_id.
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(WAIT_HELP.to_string()),
            "--until" => {
                result.until.push(take_value(args, index, "--until")?);
                index += 2;
            }
            "--timeout" => {
                let val = take_value(args, index, "--timeout")?;
                result.timeout = val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid timeout value: '{val}'"))?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent wait: '{flag}'"));
            }
            other => {
                return Err(format!("error: unexpected positional argument: '{other}'"));
            }
        }
    }

    Ok(result)
}

fn parse_ps_args(args: &[String], mut index: usize) -> Result<PsArgs, String> {
    let mut result = PsArgs {
        workspace_id: String::new(),
        state: String::new(),
    };

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(PS_HELP.to_string()),
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--state" => {
                result.state = take_value(args, index, "--state")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent ps: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: agent ps takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    Ok(result)
}

fn parse_show_args(args: &[String], mut index: usize) -> Result<ShowArgs, String> {
    let mut result = ShowArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(SHOW_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!("error: unknown flag for agent show: '{token}'"));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn parse_validate_args(args: &[String], mut index: usize) -> Result<ValidateArgs, String> {
    let mut result = ValidateArgs {
        agent_id: String::new(),
        workspace_id: String::new(),
        state: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(VALIDATE_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--state" => {
                result.state = take_value(args, index, "--state")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent validate: '{flag}'"));
            }
            other => {
                return Err(format!("error: unexpected positional argument: '{other}'"));
            }
        }
    }

    Ok(result)
}

fn parse_summary_args(args: &[String], mut index: usize) -> Result<SummaryArgs, String> {
    let mut result = SummaryArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(SUMMARY_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!("error: unknown flag for agent summary: '{token}'"));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn parse_gc_args(args: &[String], mut index: usize) -> Result<GcArgs, String> {
    let mut result = GcArgs {
        workspace_id: String::new(),
        idle_timeout_seconds: None,
        max_age_seconds: None,
        dry_run: false,
        limit: 500,
    };

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(GC_HELP.to_string()),
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--idle-timeout" => {
                let raw = take_value(args, index, "--idle-timeout")?;
                result.idle_timeout_seconds = Some(parse_gc_seconds(&raw, "--idle-timeout")?);
                index += 2;
            }
            "--max-age" => {
                let raw = take_value(args, index, "--max-age")?;
                result.max_age_seconds = Some(parse_gc_seconds(&raw, "--max-age")?);
                index += 2;
            }
            "--limit" => {
                let raw = take_value(args, index, "--limit")?;
                result.limit = raw
                    .parse::<i64>()
                    .map_err(|_| format!("invalid limit value: '{raw}'"))?;
                if result.limit <= 0 {
                    return Err("error: --limit must be > 0".to_string());
                }
                index += 2;
            }
            "--dry-run" => {
                result.dry_run = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent gc: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: agent gc takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    Ok(result)
}

fn parse_gc_seconds(raw: &str, flag: &str) -> Result<i64, String> {
    let value = raw
        .parse::<i64>()
        .map_err(|_| format!("invalid {flag} value: '{raw}'"))?;
    if value <= 0 {
        return Err(format!("error: {flag} must be > 0"));
    }
    Ok(value)
}

fn parse_interrupt_args(args: &[String], mut index: usize) -> Result<InterruptArgs, String> {
    let mut result = InterruptArgs {
        agent_id: String::new(),
        approval_policy: String::new(),
        allow_risky: false,
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(INTERRUPT_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(INTERRUPT_HELP.to_string()),
            "--approval-policy" => {
                result.approval_policy = take_value(args, index, "--approval-policy")?;
                index += 2;
            }
            "--allow-risky" => {
                result.allow_risky = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent interrupt: '{flag}'"));
            }
            positional => {
                return Err(format!(
                    "error: unexpected positional argument: '{positional}'"
                ));
            }
        }
    }

    Ok(result)
}

fn parse_kill_args(args: &[String], mut index: usize) -> Result<KillArgs, String> {
    let mut result = KillArgs {
        agent_id: String::new(),
        force: false,
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(KILL_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--force" | "-f" => {
                result.force = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent kill: '{flag}'"));
            }
            other => {
                return Err(format!("error: unexpected positional argument: '{other}'"));
            }
        }
    }

    Ok(result)
}

fn parse_revive_args(args: &[String], mut index: usize) -> Result<ReviveArgs, String> {
    let mut result = ReviveArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(REVIVE_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!("error: unknown flag for agent revive: '{token}'"));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

// ── Help text ────────────────────────────────────────────────────────────────

fn write_help(stdout: &mut dyn Write) -> Result<(), String> {
    writeln!(stdout, "{AGENT_HELP}").map_err(|e| e.to_string())
}

const AGENT_HELP: &str = "\
Manage persistent agents

Usage:
  forge agent <subcommand> [options]

Subcommands:
  run         Reuse/spawn an agent and send a task
  spawn       Spawn a new persistent agent
  send        Send a message to an agent
  wait        Wait for an agent to reach a target state
  ps          List agents
  show        Show agent details
  validate    Validate agent definitions
  summary     Generate concise parent rehydration summary
  gc          Evict stale parked persistent agents
  interrupt   Interrupt an agent (Ctrl+C)
  kill        Kill an agent
  revive      Revive a stopped/failed agent

Flags:
  -h, --help    help for agent
      --json    output in JSON format
      --jsonl   output in JSON Lines format";

const SPAWN_HELP: &str = "\
Spawn a new persistent agent

Usage:
  forge agent spawn [agent-id] [flags]

Flags:
  -c, --command string     command to run (required)
  -w, --workspace string   workspace ID
      --arg string         command argument (repeatable)
      --working-dir string working directory
      --session string     tmux session name
      --adapter string     adapter name";

const RUN_HELP: &str = "\
Reuse/spawn an agent and send a delegated task

Usage:
  forge agent run [task-text] [flags]

Flags:
      --agent string       agent id to reuse/spawn
  -w, --workspace string   workspace ID (default: \"default\")
      --type string        agent harness type (default: codex)
  -c, --command string     command override
      --wait string        wait for state (e.g. idle)
      --timeout int        wait timeout in seconds (default: 300)
      --revive             shorthand for --revive-policy auto
      --revive-policy str  revive behavior: never|ask|auto (default: never)
      --approval-policy str parent approval policy hint (default: strict)
      --account-id string   parent account context for child spawn
      --profile string      parent profile context for child spawn
      --task-id string     correlation id for parent task
      --tag string         correlation tag (repeatable)
      --label string       correlation label KEY=VALUE (repeatable)
  -t, --text string        task text";

const SEND_HELP: &str = "\
Send a message to an agent

Usage:
  forge agent send <agent-id> [text] [flags]

Flags:
  -t, --text string       message text
      --no-enter           do not send Enter after text
      --key string         send a key (repeatable)
      --approval-policy str approval policy for risk checks
      --allow-risky        allow risky send payloads under strict policy";

const WAIT_HELP: &str = "\
Wait for an agent to reach a target state

Usage:
  forge agent wait <agent-id> [flags]

Flags:
      --until string    target state (repeatable, default: idle)
      --timeout int     timeout in seconds (default: 300)";

const PS_HELP: &str = "\
List agents

Usage:
  forge agent ps [flags]

Aliases:
  ps, list, ls

Flags:
  -w, --workspace string   filter by workspace
      --state string        filter by state";

const SHOW_HELP: &str = "\
Show agent details

Usage:
  forge agent show <agent-id>

Aliases:
  show, get";

const VALIDATE_HELP: &str = "\
Validate agent definitions

Usage:
  forge agent validate [agent-id] [flags]

Flags:
  -w, --workspace string   filter by workspace (when validating multiple agents)
      --state string       filter by state (when validating multiple agents)";

const SUMMARY_HELP: &str = "\
Generate concise summary for parent rehydration

Usage:
  forge agent summary <agent-id>";

const GC_HELP: &str = "\
Evict stale parked persistent agents

Usage:
  forge agent gc [flags]

Flags:
  -w, --workspace string   filter by workspace
      --idle-timeout int   evict idle agents at/after this age in seconds
      --max-age int        evict agents at/after this total age in seconds
      --limit int          max parked agents scanned (default: 500)
      --dry-run            report candidates without deleting";

const INTERRUPT_HELP: &str = "\
Interrupt an agent (send Ctrl+C)

Usage:
  forge agent interrupt <agent-id> [flags]

Flags:
      --approval-policy str approval policy for risk checks
      --allow-risky        allow risky interrupts under strict policy";

const KILL_HELP: &str = "\
Kill an agent

Usage:
  forge agent kill <agent-id> [flags]

Flags:
  -f, --force   force kill without grace period";

const REVIVE_HELP: &str = "\
Revive a stopped or failed agent

Usage:
  forge agent revive <agent-id>

Notes:
  - Revive restores an existing terminal/missing agent id.
  - Context preamble is injected before returning control.";

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_agent::mock::{test_snapshot, MockCall};
    use forge_agent::types::AgentState;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_ok(args: &[String]) -> ParsedArgs {
        match parse_args(args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("should parse: {err}"),
        }
    }

    fn parse_err(args: &[String]) -> String {
        match parse_args(args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        }
    }

    fn parse_json(text: &str) -> serde_json::Value {
        match serde_json::from_str(text) {
            Ok(value) => value,
            Err(err) => panic!("expected valid json: {err}\ninput: {text}"),
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let nonce = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "forge-agent-{prefix}-{}-{now}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|err| panic!("create temp dir {}: {err}", dir.display()));
        dir
    }

    fn setup_migrated_db(db_path: &Path) {
        let mut db = forge_db::Db::open(forge_db::Config::new(db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db {}: {err}", db_path.display()));
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_observability_db_path(prefix: &str, callback: impl FnOnce(&Path)) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let temp = temp_dir(prefix);
        let db_path = temp.join("forge.db");
        setup_migrated_db(&db_path);

        let old_db_path = std::env::var_os("FORGE_DATABASE_PATH");
        let old_forge_db_path = std::env::var_os("FORGE_DB_PATH");
        std::env::set_var("FORGE_DATABASE_PATH", &db_path);
        std::env::set_var("FORGE_DB_PATH", &db_path);

        let callback_result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&db_path)));

        match old_db_path {
            Some(value) => std::env::set_var("FORGE_DATABASE_PATH", value),
            None => std::env::remove_var("FORGE_DATABASE_PATH"),
        }
        match old_forge_db_path {
            Some(value) => std::env::set_var("FORGE_DB_PATH", value),
            None => std::env::remove_var("FORGE_DB_PATH"),
        }

        if let Err(err) = std::fs::remove_dir_all(&temp) {
            eprintln!("warning: remove temp dir {}: {err}", temp.display());
        }

        if let Err(payload) = callback_result {
            std::panic::resume_unwind(payload);
        }
    }

    fn with_temp_env(vars: &[(&str, &str)], callback: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());

        let saved: Vec<(String, Option<std::ffi::OsString>)> = vars
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
            .collect();

        for (key, value) in vars {
            std::env::set_var(key, value);
        }

        let callback_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));

        for (key, prior) in saved {
            match prior {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }

        if let Err(payload) = callback_result {
            std::panic::resume_unwind(payload);
        }
    }

    fn seed_persistent_agent(
        db_path: &Path,
        id: &str,
        workspace_id: &str,
        harness: &str,
        parent_agent_id: Option<&str>,
        state: &str,
    ) {
        let db = forge_db::Db::open(forge_db::Config::new(db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        let repo = PersistentAgentRepository::new(&db);
        let mut agent = forge_db::persistent_agent_repository::PersistentAgent {
            id: id.to_string(),
            parent_agent_id: parent_agent_id.map(ToOwned::to_owned),
            workspace_id: workspace_id.to_string(),
            harness: harness.to_string(),
            mode: "continuous".to_string(),
            state: state.to_string(),
            ..Default::default()
        };
        repo.create(&mut agent)
            .unwrap_or_else(|err| panic!("create persistent agent {id}: {err}"));
    }

    fn parse_metric_detail(event: &PersistentAgentEvent) -> serde_json::Value {
        let detail = event.detail.as_ref().expect("metric detail");
        parse_json(detail)
    }

    // ── Parse tests ──────────────────────────────────────────────────────

    #[test]
    fn parse_agent_help() {
        let args = vec!["agent".to_string()];
        let parsed = parse_ok(&args);
        assert_eq!(parsed.subcommand, Subcommand::Help);
    }

    #[test]
    fn parse_agent_help_flag() {
        let args = vec!["agent".to_string(), "--help".to_string()];
        let parsed = parse_ok(&args);
        assert_eq!(parsed.subcommand, Subcommand::Help);
    }

    #[test]
    fn parse_unknown_subcommand() {
        let args = vec!["agent".to_string(), "bogus".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("unknown agent subcommand"));
        assert!(err.contains("bogus"));
    }

    #[test]
    fn parse_json_and_jsonl_conflict() {
        let args = vec![
            "agent".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
            "ps".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_spawn_with_flags() {
        let args = vec![
            "agent".to_string(),
            "spawn".to_string(),
            "my-agent".to_string(),
            "--command".to_string(),
            "claude".to_string(),
            "--workspace".to_string(),
            "ws-1".to_string(),
            "--adapter".to_string(),
            "claude_code".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Spawn(a) => {
                assert_eq!(a.agent_id, "my-agent");
                assert_eq!(a.command, "claude");
                assert_eq!(a.workspace_id, "ws-1");
                assert_eq!(a.adapter, "claude_code");
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_with_flags() {
        let args = vec![
            "agent".to_string(),
            "run".to_string(),
            "fix tests".to_string(),
            "--agent".to_string(),
            "ag-1".to_string(),
            "--type".to_string(),
            "claude".to_string(),
            "--wait".to_string(),
            "idle".to_string(),
            "--task-id".to_string(),
            "forge-45p".to_string(),
            "--tag".to_string(),
            "m10".to_string(),
            "--label".to_string(),
            "epic=persistent".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Run(a) => {
                assert_eq!(a.text, "fix tests");
                assert_eq!(a.agent_id, "ag-1");
                assert_eq!(a.agent_type, "claude");
                assert_eq!(a.wait_for, "idle");
                assert_eq!(a.task_id, "forge-45p");
                assert_eq!(a.tags, vec!["m10"]);
                assert_eq!(a.labels, vec!["epic=persistent"]);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_includes_approval_context_flags() {
        let args = vec![
            "agent".to_string(),
            "run".to_string(),
            "delegate".to_string(),
            "--approval-policy".to_string(),
            "plan".to_string(),
            "--account-id".to_string(),
            "acct-9".to_string(),
            "--profile".to_string(),
            "ops".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Run(a) => {
                assert_eq!(a.approval_policy, "plan");
                assert_eq!(a.account_id, "acct-9");
                assert_eq!(a.profile, "ops");
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_send_allow_risky_and_policy_flags() {
        let args = vec![
            "agent".to_string(),
            "send".to_string(),
            "ag-1".to_string(),
            "--approval-policy".to_string(),
            "strict".to_string(),
            "--allow-risky".to_string(),
            "--text".to_string(),
            "rm -rf /tmp/demo".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Send(a) => {
                assert_eq!(a.approval_policy, "strict");
                assert!(a.allow_risky);
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_interrupt_allow_risky_and_policy_flags() {
        let args = vec![
            "agent".to_string(),
            "interrupt".to_string(),
            "ag-1".to_string(),
            "--approval-policy".to_string(),
            "plan".to_string(),
            "--allow-risky".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Interrupt(a) => {
                assert_eq!(a.approval_policy, "plan");
                assert!(a.allow_risky);
            }
            other => panic!("expected Interrupt, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_requires_text() {
        let args = vec!["agent".to_string(), "run".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("task text is required"));
    }

    #[test]
    fn parse_run_revive_policy_auto() {
        let args = vec![
            "agent".to_string(),
            "run".to_string(),
            "continue".to_string(),
            "--revive-policy".to_string(),
            "auto".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Run(a) => assert_eq!(a.revive_policy, RevivePolicy::Auto),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_revive_policy_invalid() {
        let args = vec![
            "agent".to_string(),
            "run".to_string(),
            "continue".to_string(),
            "--revive-policy".to_string(),
            "later".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("invalid revive policy"));
        assert!(err.contains("never, ask, auto"));
    }

    #[test]
    fn parse_send_positional_text() {
        let args = vec![
            "agent".to_string(),
            "send".to_string(),
            "my-agent".to_string(),
            "hello world".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Send(a) => {
                assert_eq!(a.agent_id, "my-agent");
                assert_eq!(a.text, "hello world");
                assert!(a.send_enter);
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_wait_with_until_and_timeout() {
        let args = vec![
            "agent".to_string(),
            "wait".to_string(),
            "ag-1".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "--until".to_string(),
            "stopped".to_string(),
            "--timeout".to_string(),
            "60".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Wait(a) => {
                assert_eq!(a.agent_id, "ag-1");
                assert_eq!(a.until, vec!["idle", "stopped"]);
                assert_eq!(a.timeout, 60);
            }
            other => panic!("expected Wait, got {other:?}"),
        }
    }

    #[test]
    fn parse_ps_with_state_filter() {
        let args = vec![
            "agent".to_string(),
            "ps".to_string(),
            "--state".to_string(),
            "running".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Ps(a) => {
                assert_eq!(a.state, "running");
            }
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_positional() {
        let args = vec![
            "agent".to_string(),
            "show".to_string(),
            "agent-42".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Show(a) => {
                assert_eq!(a.agent_id, "agent-42");
            }
            other => panic!("expected Show, got {other:?}"),
        }
    }

    #[test]
    fn parse_summary_positional() {
        let args = vec![
            "agent".to_string(),
            "summary".to_string(),
            "agent-42".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Summary(a) => {
                assert_eq!(a.agent_id, "agent-42");
            }
            other => panic!("expected Summary, got {other:?}"),
        }
    }

    #[test]
    fn parse_gc_with_flags() {
        let args = vec![
            "agent".to_string(),
            "gc".to_string(),
            "--workspace".to_string(),
            "ws-1".to_string(),
            "--idle-timeout".to_string(),
            "60".to_string(),
            "--max-age".to_string(),
            "600".to_string(),
            "--limit".to_string(),
            "25".to_string(),
            "--dry-run".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Gc(a) => {
                assert_eq!(a.workspace_id, "ws-1");
                assert_eq!(a.idle_timeout_seconds, Some(60));
                assert_eq!(a.max_age_seconds, Some(600));
                assert_eq!(a.limit, 25);
                assert!(a.dry_run);
            }
            other => panic!("expected Gc, got {other:?}"),
        }
    }

    #[test]
    fn parse_kill_with_force() {
        let args = vec![
            "agent".to_string(),
            "kill".to_string(),
            "agent-42".to_string(),
            "--force".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Kill(a) => {
                assert_eq!(a.agent_id, "agent-42");
                assert!(a.force);
            }
            other => panic!("expected Kill, got {other:?}"),
        }
    }

    #[test]
    fn parse_list_alias() {
        let args = vec!["agent".to_string(), "list".to_string()];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Ps(_) => {}
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn parse_ls_alias() {
        let args = vec!["agent".to_string(), "ls".to_string()];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Ps(_) => {}
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn parse_get_alias() {
        let args = vec!["agent".to_string(), "get".to_string(), "ag-1".to_string()];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Show(a) => {
                assert_eq!(a.agent_id, "ag-1");
            }
            other => panic!("expected Show, got {other:?}"),
        }
    }

    #[test]
    fn parse_validate_with_filters() {
        let args = vec![
            "agent".to_string(),
            "validate".to_string(),
            "--workspace".to_string(),
            "ws-1".to_string(),
            "--state".to_string(),
            "idle".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Validate(a) => {
                assert_eq!(a.agent_id, "");
                assert_eq!(a.workspace_id, "ws-1");
                assert_eq!(a.state, "idle");
            }
            other => panic!("expected Validate, got {other:?}"),
        }
    }

    #[test]
    fn parse_spawn_unknown_flag() {
        let args = vec![
            "agent".to_string(),
            "spawn".to_string(),
            "--bogus".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("unknown flag"));
    }

    #[test]
    fn parse_agent_state_valid() {
        assert_eq!(parse_agent_state("idle").unwrap(), AgentState::Idle);
        assert_eq!(parse_agent_state("running").unwrap(), AgentState::Running);
        assert_eq!(parse_agent_state("stopped").unwrap(), AgentState::Stopped);
        assert_eq!(parse_agent_state("failed").unwrap(), AgentState::Failed);
    }

    #[test]
    fn parse_agent_state_invalid() {
        let err = parse_agent_state("bogus").unwrap_err();
        assert!(err.contains("invalid agent state"));
        assert!(err.contains("bogus"));
    }

    #[test]
    fn metric_counter_hook_persists_event() {
        with_observability_db_path("metric-counter", |db_path| {
            observe_counter_metric(
                "sends",
                "success",
                Some("ag-metric"),
                Some(serde_json::json!({ "source": "test" })),
            );

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let repo = PersistentAgentEventRepository::new(&db);
            let events = repo
                .list_by_agent("ag-metric", 8)
                .unwrap_or_else(|err| panic!("list metric events: {err}"));
            assert!(!events.is_empty());
            assert_eq!(events[0].kind, "metric_sends");
            assert_eq!(events[0].outcome, "success");
            let detail = parse_metric_detail(&events[0]);
            assert_eq!(detail["type"], "counter");
            assert_eq!(detail["name"], "sends");
            assert_eq!(detail["value"], 1);
        });
    }

    #[test]
    fn metric_event_redacts_sensitive_payload_fields() {
        with_observability_db_path("metric-redact", |db_path| {
            observe_counter_metric(
                "sends",
                "error",
                Some("ag-redact"),
                Some(serde_json::json!({
                    "token": "super-secret-token",
                    "note": "Authorization: Bearer sk-demo",
                    "safe": "ok"
                })),
            );

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let repo = PersistentAgentEventRepository::new(&db);
            let events = repo
                .list_by_agent("ag-redact", 4)
                .unwrap_or_else(|err| panic!("list redaction events: {err}"));
            let detail = events[0].detail.as_ref().expect("detail");
            assert!(!detail.contains("super-secret-token"));
            assert!(!detail.contains("sk-demo"));

            let parsed = parse_metric_detail(&events[0]);
            assert_eq!(parsed["detail"]["token"], REDACTED_VALUE);
            assert_eq!(parsed["detail"]["note"], REDACTED_VALUE);
            assert_eq!(parsed["detail"]["safe"], "ok");
        });
    }

    #[test]
    fn revive_audit_event_redacts_sensitive_detail_text() {
        with_observability_db_path("revive-redact", |db_path| {
            append_revive_audit_event(
                "ag-redact",
                "revive_start",
                "started",
                serde_json::json!({
                    "api_key": "sk-live-value",
                    "message": "token=abc123"
                }),
            );

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let repo = PersistentAgentEventRepository::new(&db);
            let events = repo
                .list_by_agent("ag-redact", 4)
                .unwrap_or_else(|err| panic!("list revive events: {err}"));
            let detail = events[0].detail.as_ref().expect("detail");
            assert!(!detail.contains("sk-live-value"));
            assert!(!detail.contains("abc123"));
            assert!(detail.contains(REDACTED_VALUE));
        });
    }

    #[test]
    fn metric_gauge_hook_emits_active_and_idle() {
        with_observability_db_path("metric-gauge", |db_path| {
            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let agent_repo = PersistentAgentRepository::new(&db);

            let mut idle = forge_db::persistent_agent_repository::PersistentAgent {
                id: "ag-idle".to_string(),
                workspace_id: "ws-metric".to_string(),
                harness: "codex".to_string(),
                mode: "continuous".to_string(),
                state: "idle".to_string(),
                ..Default::default()
            };
            agent_repo
                .create(&mut idle)
                .unwrap_or_else(|err| panic!("create idle: {err}"));

            let mut running = forge_db::persistent_agent_repository::PersistentAgent {
                id: "ag-running".to_string(),
                workspace_id: "ws-metric".to_string(),
                harness: "codex".to_string(),
                mode: "continuous".to_string(),
                state: "running".to_string(),
                ..Default::default()
            };
            agent_repo
                .create(&mut running)
                .unwrap_or_else(|err| panic!("create running: {err}"));

            observe_gauge_metrics(Some("ws-metric"));

            let event_repo = PersistentAgentEventRepository::new(&db);
            let active = event_repo
                .query(
                    forge_db::persistent_agent_event_repository::PersistentAgentEventQuery {
                        kind: Some("metric_active_agents".to_string()),
                        limit: 1,
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|err| panic!("query active metric: {err}"));
            let idle_events = event_repo
                .query(
                    forge_db::persistent_agent_event_repository::PersistentAgentEventQuery {
                        kind: Some("metric_idle_agents".to_string()),
                        limit: 1,
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|err| panic!("query idle metric: {err}"));

            let active_detail = parse_metric_detail(&active[0]);
            let idle_detail = parse_metric_detail(&idle_events[0]);
            assert_eq!(active_detail["type"], "gauge");
            assert_eq!(active_detail["value"], 2);
            assert_eq!(idle_detail["type"], "gauge");
            assert_eq!(idle_detail["value"], 1);
        });
    }

    #[test]
    fn metric_latency_hook_persists_value_ms() {
        with_observability_db_path("metric-latency", |db_path| {
            observe_latency_metric("send_to_idle_duration", "success", Some("ag-lat"), 321);

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let repo = PersistentAgentEventRepository::new(&db);
            let events = repo
                .list_by_agent("ag-lat", 4)
                .unwrap_or_else(|err| panic!("list latency metrics: {err}"));
            assert_eq!(events[0].kind, "metric_send_to_idle_duration");
            let detail = parse_metric_detail(&events[0]);
            assert_eq!(detail["type"], "latency_ms");
            assert_eq!(detail["value_ms"], 321);
        });
    }

    #[test]
    fn wait_timeout_detector_matches_expected_shapes() {
        assert!(is_wait_timeout_error("wait timeout: idle"));
        assert!(is_wait_timeout_error("timed out waiting for idle"));
        assert!(!is_wait_timeout_error("agent entered terminal state"));
    }

    #[test]
    fn summary_snapshot_marks_blocked_when_blockers_present() {
        let transcript = Transcript {
            id: 1,
            agent_id: "ag-1".to_string(),
            content: "Investigated root cause
Blocked waiting on forge-fbs
Will resume after unblock"
                .to_string(),
            content_hash: "hash".to_string(),
            captured_at: "2026-02-12T00:00:00Z".to_string(),
        };
        let events = vec![PersistentAgentEvent {
            id: 7,
            agent_id: Some("ag-1".to_string()),
            kind: "wait_state".to_string(),
            outcome: "error: blocked by forge-fbs".to_string(),
            detail: Some("waiting for dependency".to_string()),
            timestamp: "2026-02-12T00:00:05Z".to_string(),
        }];

        let snapshot = summarize_agent_snapshot("ag-1", Some(&transcript), &events);
        assert_eq!(snapshot.concise_status, "blocked");
        assert!(snapshot.latest_task_outcome.contains("wait_state"));
        assert!(!snapshot.unresolved_blockers.is_empty());
        assert!(snapshot
            .unresolved_blockers
            .iter()
            .any(|line| line.to_ascii_lowercase().contains("blocked")));
        assert!(snapshot.transcript_excerpt.len() <= SUMMARY_EXCERPT_LIMIT);
    }

    #[test]
    fn summary_snapshot_without_inputs_is_unknown() {
        let snapshot = summarize_agent_snapshot("ag-empty", None, &[]);
        assert_eq!(snapshot.concise_status, "unknown");
        assert_eq!(snapshot.latest_task_outcome, "unknown");
        assert!(snapshot.unresolved_blockers.is_empty());
        assert!(snapshot.transcript_excerpt.is_empty());
    }

    #[test]
    fn agent_summary_json_persists_summary_snapshot_event() {
        let temp = temp_dir("summary-json");
        let db_path = temp.join("forge.db");
        setup_migrated_db(&db_path);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        let event_repo = PersistentAgentEventRepository::new(&db);
        let mut seed = PersistentAgentEvent {
            id: 0,
            agent_id: Some("ag-summary".to_string()),
            kind: "wait_state".to_string(),
            outcome: "error: blocked by forge-fbs".to_string(),
            detail: Some("blocked by forge-fbs".to_string()),
            timestamp: String::new(),
        };
        event_repo
            .append(&mut seed)
            .unwrap_or_else(|err| panic!("append seed event: {err}"));

        let args = vec![
            "agent".to_string(),
            "--json".to_string(),
            "summary".to_string(),
            "ag-summary".to_string(),
        ];
        let parsed = parse_ok(&args);
        let summary_args = match &parsed.subcommand {
            Subcommand::Summary(value) => value.clone(),
            other => panic!("expected Summary, got {other:?}"),
        };

        let mut stdout = Vec::new();
        exec_summary_with_db_path(&summary_args, &parsed, &mut stdout, &db_path)
            .unwrap_or_else(|err| panic!("exec summary: {err}"));

        let output = String::from_utf8(stdout).unwrap_or_else(|err| panic!("utf8 output: {err}"));
        let json = parse_json(&output);
        assert_eq!(json["agent_id"], "ag-summary");
        assert_eq!(json["concise_status"], "blocked");
        assert!(json["snapshot_event_id"].as_i64().unwrap_or(0) > seed.id);
        assert!(json["recent_events_considered"].as_u64().unwrap_or(0) >= 1);

        let verify_db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open verify db {}: {err}", db_path.display()));
        let verify_repo = PersistentAgentEventRepository::new(&verify_db);
        let events = verify_repo
            .list_by_agent("ag-summary", 20)
            .unwrap_or_else(|err| panic!("list events: {err}"));
        let latest = &events[0];
        assert_eq!(latest.kind, "summary_snapshot");
        assert_eq!(latest.outcome, "blocked");
        let detail = latest.detail.as_ref().expect("summary detail");
        assert!(detail.contains("\"agent_id\":\"ag-summary\""));

        std::fs::remove_dir_all(&temp)
            .unwrap_or_else(|err| panic!("remove temp dir {}: {err}", temp.display()));
    }

    #[test]
    fn eviction_candidate_prefers_ttl_over_other_thresholds() {
        let now = chrono::Utc::now();
        let agent = forge_db::persistent_agent_repository::PersistentAgent {
            id: "ag-ttl".to_string(),
            workspace_id: "ws-1".to_string(),
            harness: "codex".to_string(),
            mode: "continuous".to_string(),
            state: "idle".to_string(),
            ttl_seconds: Some(30),
            created_at: (now - chrono::Duration::seconds(120)).to_rfc3339(),
            last_activity_at: (now - chrono::Duration::seconds(120)).to_rfc3339(),
            updated_at: now.to_rfc3339(),
            ..Default::default()
        };
        let args = GcArgs {
            workspace_id: String::new(),
            idle_timeout_seconds: Some(10),
            max_age_seconds: Some(60),
            dry_run: false,
            limit: 100,
        };

        let candidate = evaluate_eviction_candidate(&agent, &args, &now).expect("candidate");
        assert_eq!(candidate.reason, EvictionReason::Ttl);
        assert_eq!(candidate.ttl_seconds, Some(30));
    }

    #[test]
    fn eviction_candidate_ignores_non_parked_states() {
        let now = chrono::Utc::now();
        let agent = forge_db::persistent_agent_repository::PersistentAgent {
            id: "ag-running".to_string(),
            workspace_id: "ws-1".to_string(),
            harness: "codex".to_string(),
            mode: "continuous".to_string(),
            state: "running".to_string(),
            ttl_seconds: Some(1),
            created_at: (now - chrono::Duration::seconds(600)).to_rfc3339(),
            last_activity_at: (now - chrono::Duration::seconds(600)).to_rfc3339(),
            updated_at: now.to_rfc3339(),
            ..Default::default()
        };
        let args = GcArgs {
            workspace_id: String::new(),
            idle_timeout_seconds: Some(10),
            max_age_seconds: Some(60),
            dry_run: false,
            limit: 100,
        };

        let candidate = evaluate_eviction_candidate(&agent, &args, &now);
        assert!(candidate.is_none());
    }

    #[test]
    fn agent_gc_evicts_expired_parked_agents_and_emits_events() {
        let temp = temp_dir("agent-gc");
        let db_path = temp.join("forge.db");
        setup_migrated_db(&db_path);

        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::seconds(600)).to_rfc3339();
        let recent = (now - chrono::Duration::seconds(5)).to_rfc3339();

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        let repo = forge_db::persistent_agent_repository::PersistentAgentRepository::new(&db);

        let mut stale_idle = forge_db::persistent_agent_repository::PersistentAgent {
            id: "ag-stale-idle".to_string(),
            workspace_id: "ws-1".to_string(),
            harness: "codex".to_string(),
            mode: "continuous".to_string(),
            state: "idle".to_string(),
            ttl_seconds: Some(30),
            created_at: old.clone(),
            last_activity_at: old.clone(),
            updated_at: old.clone(),
            ..Default::default()
        };
        repo.create(&mut stale_idle)
            .unwrap_or_else(|err| panic!("create stale idle: {err}"));

        let mut active_running = forge_db::persistent_agent_repository::PersistentAgent {
            id: "ag-active-running".to_string(),
            workspace_id: "ws-1".to_string(),
            harness: "codex".to_string(),
            mode: "continuous".to_string(),
            state: "running".to_string(),
            ttl_seconds: Some(30),
            created_at: old.clone(),
            last_activity_at: recent,
            updated_at: old,
            ..Default::default()
        };
        repo.create(&mut active_running)
            .unwrap_or_else(|err| panic!("create active running: {err}"));

        let gc_args = GcArgs {
            workspace_id: "ws-1".to_string(),
            idle_timeout_seconds: Some(60),
            max_age_seconds: None,
            dry_run: false,
            limit: 100,
        };
        let parsed = ParsedArgs {
            json: true,
            jsonl: false,
            quiet: false,
            subcommand: Subcommand::Help,
        };

        let mut stdout = Vec::new();
        exec_gc_with_db_path(&gc_args, &parsed, &mut stdout, &db_path)
            .unwrap_or_else(|err| panic!("exec gc: {err}"));

        let output = String::from_utf8(stdout).unwrap_or_else(|err| panic!("utf8 output: {err}"));
        let json = parse_json(&output);
        assert_eq!(json["evicted"], 1);
        assert_eq!(json["scanned"], 1);
        assert_eq!(json["evictions"][0]["agent_id"], "ag-stale-idle");

        let verify_db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open verify db {}: {err}", db_path.display()));
        let verify_repo =
            forge_db::persistent_agent_repository::PersistentAgentRepository::new(&verify_db);
        assert!(verify_repo.get("ag-stale-idle").is_err());
        assert!(verify_repo.get("ag-active-running").is_ok());

        let event_repo = PersistentAgentEventRepository::new(&verify_db);
        let events = event_repo
            .list_by_agent("ag-stale-idle", 10)
            .unwrap_or_else(|err| panic!("list gc events: {err}"));
        assert_eq!(events[0].kind, "gc_evict_done");
        assert_eq!(events[0].outcome, "success");
        assert_eq!(events[1].kind, "gc_evict_start");

        std::fs::remove_dir_all(&temp)
            .unwrap_or_else(|err| panic!("remove temp dir {}: {err}", temp.display()));
    }

    // ── Execution tests with InMemory backend ────────────────────────────

    #[test]
    fn agent_help_output() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Manage persistent agents"));
        assert!(out.stdout.contains("run"));
        assert!(out.stdout.contains("spawn"));
        assert!(out.stdout.contains("send"));
        assert!(out.stdout.contains("wait"));
        assert!(out.stdout.contains("ps"));
        assert!(out.stdout.contains("show"));
        assert!(out.stdout.contains("summary"));
        assert!(out.stdout.contains("gc"));
        assert!(out.stdout.contains("kill"));
    }

    #[test]
    fn agent_ps_empty_json() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "--json", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 0);
        assert!(parsed["agents"].as_array().unwrap().is_empty());
    }

    #[test]
    fn agent_ps_empty_human() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "No agents found\n");
    }

    #[test]
    fn agent_ps_with_agents_json() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 2);
        let agents = parsed["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn agent_ps_filters_by_state() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "ps", "--state", "idle"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 1);
        let agents = parsed["agents"].as_array().unwrap();
        assert_eq!(agents[0]["id"], "ag-001");
        assert_eq!(agents[0]["state"], "idle");
    }

    #[test]
    fn agent_ps_jsonl() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--jsonl", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        let first = parse_json(lines[0]);
        let second = parse_json(lines[1]);
        assert_eq!(first["id"], "ag-001");
        assert_eq!(second["id"], "ag-002");
    }

    #[test]
    fn agent_show_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "show", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "ag-001");
        assert_eq!(parsed["state"], "idle");
        assert_eq!(parsed["command"], "claude");
    }

    #[test]
    fn agent_show_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "show", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_show_missing_id() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "show"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent ID is required"));
    }

    #[test]
    fn agent_validate_json_success() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "validate"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["valid"], true);
        assert_eq!(parsed["checked"], 1);
        assert!(parsed["errors"]
            .as_array()
            .is_some_and(|items| items.is_empty()));
    }

    #[test]
    fn agent_validate_returns_nonzero_for_invalid_snapshot() {
        let mut bad = test_snapshot("ag-bad", AgentState::Idle);
        bad.workspace_id = String::new();
        let backend = InMemoryAgentBackend::new().with_agent(bad);

        let out = run_for_test(&["agent", "--json", "validate"], &backend);
        assert_eq!(out.exit_code, 1);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["valid"], false);
        assert_eq!(parsed["checked"], 1);
        assert_eq!(parsed["errors"][0]["agent_id"], "ag-bad");
        assert_eq!(parsed["errors"][0]["message"], "workspace_id is empty");
        assert!(out.stderr.contains("agent validation failed"));
    }

    #[test]
    fn agent_spawn_json() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(
            &[
                "agent",
                "--json",
                "spawn",
                "my-agent",
                "--command",
                "claude",
                "--workspace",
                "ws-1",
                "--adapter",
                "claude_code",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "my-agent");
        assert_eq!(parsed["state"], "starting");
        assert_eq!(parsed["command"], "claude");
    }

    #[test]
    fn agent_spawn_missing_command() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "spawn", "my-agent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--command is required"));
    }

    #[test]
    fn agent_spawn_capability_mismatch_is_actionable() {
        let backend =
            InMemoryAgentBackend::new().with_spawn_error(AgentServiceError::CapabilityMismatch {
                adapter: "codex".to_string(),
                requested_mode: "continuous".to_string(),
                command_mode: "one-shot".to_string(),
                hint: "switch to interactive command".to_string(),
            });
        let out = run_for_test(
            &["agent", "spawn", "ag-cap", "--command", "codex exec"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("capability mismatch"));
        assert!(out.stderr.contains("one-shot"));
    }

    #[test]
    fn agent_spawn_transport_unavailable_surfaces_fallback_error() {
        let backend =
            InMemoryAgentBackend::new().with_spawn_error(AgentServiceError::TransportUnavailable {
                message: "dial tcp 127.0.0.1:50051: connect: connection refused".to_string(),
            });
        let out = run_for_test(
            &["agent", "spawn", "ag-offline", "--command", "codex"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("forged daemon unavailable"));
        assert!(out.stderr.contains("connection refused"));
    }

    #[test]
    fn agent_send_with_multiple_agents_keeps_peer_visible_in_ps() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-a", AgentState::Idle))
            .with_agent(test_snapshot("ag-b", AgentState::Idle));

        let send_out = run_for_test(&["agent", "send", "ag-a", "ping"], &backend);
        assert_eq!(send_out.exit_code, 0, "stderr: {}", send_out.stderr);

        let ps_out = run_for_test(&["agent", "--json", "ps"], &backend);
        assert_eq!(ps_out.exit_code, 0, "stderr: {}", ps_out.stderr);
        let parsed = parse_json(&ps_out.stdout);
        assert_eq!(parsed["total"], 2);
    }

    #[test]
    fn agent_run_inherits_parent_context_into_spawn_env() {
        let backend = InMemoryAgentBackend::new();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let agent_id = format!("ag-context-{nonce}");

        with_temp_env(
            &[
                (APPROVAL_POLICY_ENV_KEY, "plan"),
                (ACCOUNT_ID_ENV_KEY, "acct-42"),
                (PROFILE_ENV_KEY, "ops"),
            ],
            || {
                let args = vec![
                    "agent",
                    "run",
                    "delegate audit",
                    "--agent",
                    agent_id.as_str(),
                ];
                let out = run_for_test(&args, &backend);
                assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
            },
        );

        let calls = backend.mock.calls();
        let spawn = calls
            .iter()
            .find_map(|call| match call {
                MockCall::Spawn(params) => Some(params.clone()),
                _ => None,
            })
            .expect("spawn call");
        assert_eq!(
            spawn.env.get(APPROVAL_POLICY_ENV_KEY),
            Some(&"plan".to_string())
        );
        assert_eq!(
            spawn.env.get(ACCOUNT_ID_ENV_KEY),
            Some(&"acct-42".to_string())
        );
        assert_eq!(spawn.env.get(PROFILE_ENV_KEY), Some(&"ops".to_string()));
    }

    #[test]
    fn agent_run_create_path_json() {
        let backend = InMemoryAgentBackend::new();
        with_observability_db_path("run-create-json", |_| {
            let out = run_for_test(
                &[
                    "agent",
                    "--json",
                    "run",
                    "ship m10 helper",
                    "--agent",
                    "ag-run-1",
                    "--type",
                    "claude",
                    "--task-id",
                    "forge-45p",
                ],
                &backend,
            );
            assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
            let parsed = parse_json(&out.stdout);
            assert_eq!(parsed["agent_id"], "ag-run-1");
            assert_eq!(parsed["reused"], false);
            assert_eq!(parsed["revived"], false);
            assert_eq!(parsed["observed_state"], "starting");
            assert_eq!(parsed["task_id"], "forge-45p");
        });
    }

    #[test]
    fn agent_run_reuse_path_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-reuse", AgentState::Idle));
        let out = run_for_test(
            &[
                "agent",
                "--json",
                "run",
                "continue prior work",
                "--agent",
                "ag-reuse",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["agent_id"], "ag-reuse");
        assert_eq!(parsed["reused"], true);
        assert_eq!(parsed["revived"], false);
    }

    #[test]
    fn agent_run_revive_path_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-dead", AgentState::Stopped));
        let out = run_for_test(
            &[
                "agent",
                "--json",
                "run",
                "restart and continue",
                "--agent",
                "ag-dead",
                "--revive",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["agent_id"], "ag-dead");
        assert_eq!(parsed["reused"], false);
        assert_eq!(parsed["revived"], true);
    }

    #[test]
    fn agent_send_risky_payload_denied_under_strict_policy() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-risk", AgentState::Idle));
        let out = run_for_test(&["agent", "send", "ag-risk", "rm -rf /tmp/demo"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("blocked risky send"));
    }

    #[test]
    fn agent_send_risky_payload_allowed_with_override() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-risk", AgentState::Idle));
        let out = run_for_test(
            &[
                "agent",
                "send",
                "ag-risk",
                "rm -rf /tmp/demo",
                "--allow-risky",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    }

    #[test]
    fn agent_send_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "send", "ag-001", "hello"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_send_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "send", "nonexistent", "hello"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_kill_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "kill", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_kill_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "kill", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_kill_force() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "kill", "ag-001", "--force"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_interrupt_waiting_approval_denied_under_strict_policy() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-approval", AgentState::WaitingApproval));
        let out = run_for_test(&["agent", "interrupt", "ag-approval"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("blocked risky interrupt"));
    }

    #[test]
    fn agent_interrupt_waiting_approval_allowed_with_override() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-approval", AgentState::WaitingApproval));
        let out = run_for_test(
            &["agent", "interrupt", "ag-approval", "--allow-risky"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    }

    #[test]
    fn agent_interrupt_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "interrupt", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_interrupt_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "interrupt", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_wait_already_in_state() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(
            &["agent", "--json", "wait", "ag-001", "--until", "idle"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "ag-001");
        assert_eq!(parsed["state"], "idle");
    }

    #[test]
    fn agent_wait_timeout() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Running));
        let out = run_for_test(
            &[
                "agent",
                "wait",
                "ag-001",
                "--until",
                "idle",
                "--timeout",
                "1",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("timeout"));
    }

    #[test]
    fn agent_run_terminal_without_auto_revive_policy_errors() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-dead", AgentState::Stopped));
        let out = run_for_test(
            &["agent", "run", "continue", "--agent", "ag-dead"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("revive policy is never"));
    }

    #[test]
    fn agent_run_missing_process_auto_policy_revives_with_audit_events() {
        with_observability_db_path("run-revive-missing", |db_path| {
            seed_persistent_agent(
                db_path,
                "ag-missing",
                "ws-revive",
                "claude_code",
                Some("parent-7"),
                "failed",
            );

            let backend = InMemoryAgentBackend::new().with_get_error(AgentServiceError::NotFound {
                agent_id: "ag-missing".to_string(),
            });
            let out = run_for_test(
                &[
                    "agent",
                    "--json",
                    "run",
                    "continue delegated task",
                    "--agent",
                    "ag-missing",
                    "--revive-policy",
                    "auto",
                ],
                &backend,
            );
            assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
            let parsed = parse_json(&out.stdout);
            assert_eq!(parsed["agent_id"], "ag-missing");
            assert_eq!(parsed["revived"], true);

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let events = PersistentAgentEventRepository::new(&db)
                .list_by_agent("ag-missing", 16)
                .unwrap_or_else(|err| panic!("list revive events: {err}"));
            assert!(events.iter().any(|event| event.kind == "revive_start"));
            assert!(events
                .iter()
                .any(|event| event.kind == "revive_done" && event.outcome == "success"));
        });
    }

    #[test]
    fn agent_revive_terminal_success_json() {
        with_observability_db_path("revive-terminal", |db_path| {
            seed_persistent_agent(
                db_path,
                "ag-revive",
                "ws-revive",
                "claude_code",
                Some("parent-11"),
                "stopped",
            );

            let backend = InMemoryAgentBackend::new()
                .with_agent(test_snapshot("ag-revive", AgentState::Stopped));
            let out = run_for_test(&["agent", "--json", "revive", "ag-revive"], &backend);
            assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
            let parsed = parse_json(&out.stdout);
            assert_eq!(parsed["id"], "ag-revive");
            assert_eq!(parsed["state"], "starting");

            let db = forge_db::Db::open(forge_db::Config::new(db_path))
                .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
            let events = PersistentAgentEventRepository::new(&db)
                .list_by_agent("ag-revive", 16)
                .unwrap_or_else(|err| panic!("list revive events: {err}"));
            assert!(events
                .iter()
                .any(|event| event.kind == "revive_done" && event.outcome == "success"));
        });
    }

    #[test]
    fn agent_revive_unknown_agent_returns_actionable_error() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "revive", "ag-missing"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("no persistent record exists"));
        assert!(out.stderr.contains("forge agent spawn ag-missing"));
    }

    #[test]
    fn agent_quiet_suppresses_output() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--quiet", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn agent_ps_human_table() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("ID"));
        assert!(out.stdout.contains("STATE"));
        assert!(out.stdout.contains("COMMAND"));
        assert!(out.stdout.contains("ag-001"));
        assert!(out.stdout.contains("idle"));
    }

    #[test]
    fn agent_subcommand_help_texts() {
        let backend = InMemoryAgentBackend::new();

        // Each subcommand's --help returns help text via stderr (exit 1).
        let run_out = run_for_test(&["agent", "run", "--help"], &backend);
        assert_eq!(run_out.exit_code, 1);
        assert!(run_out.stderr.contains("Reuse/spawn an agent"));

        let spawn_out = run_for_test(&["agent", "spawn", "--help"], &backend);
        assert_eq!(spawn_out.exit_code, 1);
        assert!(spawn_out.stderr.contains("Spawn a new persistent agent"));

        let send_out = run_for_test(&["agent", "send", "--help"], &backend);
        assert_eq!(send_out.exit_code, 1);
        assert!(send_out.stderr.contains("Send a message to an agent"));

        let wait_out = run_for_test(&["agent", "wait", "--help"], &backend);
        assert_eq!(wait_out.exit_code, 1);
        assert!(wait_out.stderr.contains("Wait for an agent"));

        let ps_out = run_for_test(&["agent", "ps", "--help"], &backend);
        assert_eq!(ps_out.exit_code, 1);
        assert!(ps_out.stderr.contains("List agents"));

        let show_out = run_for_test(&["agent", "show", "--help"], &backend);
        assert_eq!(show_out.exit_code, 1);
        assert!(show_out.stderr.contains("Show agent details"));

        let summary_out = run_for_test(&["agent", "summary", "--help"], &backend);
        assert_eq!(summary_out.exit_code, 1);
        assert!(summary_out
            .stderr
            .contains("Generate concise summary for parent rehydration"));

        let gc_out = run_for_test(&["agent", "gc", "--help"], &backend);
        assert_eq!(gc_out.exit_code, 1);
        assert!(gc_out
            .stderr
            .contains("Evict stale parked persistent agents"));

        let kill_out = run_for_test(&["agent", "kill", "--help"], &backend);
        assert_eq!(kill_out.exit_code, 1);
        assert!(kill_out.stderr.contains("Kill an agent"));
    }
}

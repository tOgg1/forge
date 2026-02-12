use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunArgs {
    text: String,
    agent_id: String,
    workspace_id: String,
    agent_type: String,
    command: String,
    wait_for: String,
    wait_timeout: u64,
    revive: bool,
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
    let detail = serde_json::to_string(snapshot).map_err(|err| err.to_string())?;
    let mut event = PersistentAgentEvent {
        id: 0,
        agent_id: Some(snapshot.agent_id.clone()),
        kind: "summary_snapshot".to_string(),
        outcome: snapshot.concise_status.clone(),
        detail: Some(detail),
        timestamp: String::new(),
    };
    repo.append(&mut event).map_err(|err| err.to_string())?;
    Ok(event.id)
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
            event_repo
                .append(&mut start_event)
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
                let _ = event_repo.append(&mut failed_event);
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
            event_repo
                .append(&mut done_event)
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
    let workspace_id = if args.workspace_id.is_empty() {
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

    let existing = match backend.get_agent(&agent_id) {
        Ok(snapshot) => Some(snapshot),
        Err(err) if err.contains("not found") => None,
        Err(err) => return Err(err),
    };

    let start_snapshot = if let Some(snapshot) = existing {
        if snapshot.state.is_terminal() {
            if !args.revive {
                return Err(format!(
                    "agent '{}' is in terminal state '{}'; use --revive to restart it",
                    snapshot.id, snapshot.state
                ));
            }
            backend.kill_agent(KillAgentParams {
                agent_id: snapshot.id.clone(),
                force: true,
                grace_period: None,
            })?;
            revived = true;
            spawn_for_run(
                backend,
                &agent_id,
                &workspace_id,
                &command,
                default_adapter,
                &args.agent_type,
            )?
        } else {
            reused = true;
            snapshot
        }
    } else {
        if args.revive && !args.agent_id.is_empty() {
            revived = true;
        }
        spawn_for_run(
            backend,
            &agent_id,
            &workspace_id,
            &command,
            default_adapter,
            &args.agent_type,
        )?
    };

    let run_text = build_run_message(args);
    backend.send_message(SendMessageParams {
        agent_id: agent_id.clone(),
        text: run_text.clone(),
        send_enter: true,
        keys: Vec::new(),
    })?;

    let observed = if let Some(target) = wait_target {
        backend.wait_state(WaitStateParams {
            agent_id: agent_id.clone(),
            target_states: vec![target],
            timeout: Duration::from_secs(args.wait_timeout),
            poll_interval: Duration::from_millis(500),
        })?
    } else {
        backend.get_agent(&agent_id).unwrap_or(start_snapshot)
    };

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
            "{}\t{}\t{}\t{}",
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
    let snapshot = backend.spawn_agent(params)?;
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
    let params = SendMessageParams {
        agent_id: args.agent_id.clone(),
        text: args.text.clone(),
        send_enter: args.send_enter,
        keys: args.keys.clone(),
    };
    let ok = backend.send_message(params)?;
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
    let snapshot = backend.wait_state(params)?;
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

fn exec_interrupt(
    backend: &dyn AgentBackend,
    args: &InterruptArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent interrupt".to_string());
    }
    let ok = backend.interrupt_agent(&args.agent_id)?;
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
    let ok = backend.kill_agent(params)?;
    write_bool_output(ok, parsed, stdout)
}

fn exec_revive(
    _backend: &dyn AgentBackend,
    args: &ReviveArgs,
    _parsed: &ParsedArgs,
    _stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent revive".to_string());
    }
    // Revive is a composite operation: spawn with the same parameters.
    // This requires the agent to be in a terminal state. For now, this
    // returns an error because the underlying service does not yet support
    // revive directly. M10.5 will add full revive semantics.
    Err(format!(
        "agent revive is not yet implemented (agent '{}'). Use `agent spawn` with the same parameters to restart.",
        args.agent_id
    ))
}

fn spawn_for_run(
    backend: &dyn AgentBackend,
    agent_id: &str,
    workspace_id: &str,
    command: &str,
    default_adapter: &str,
    agent_type: &str,
) -> Result<AgentSnapshot, String> {
    let working_dir = std::env::current_dir()
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| ".".to_string());
    let (_, mapped_adapter) = defaults_for_agent_type(agent_type);
    let adapter = if mapped_adapter.is_empty() {
        default_adapter.to_string()
    } else {
        mapped_adapter.to_string()
    };
    backend.spawn_agent(SpawnAgentParams {
        agent_id: agent_id.to_string(),
        workspace_id: workspace_id.to_string(),
        command: command.to_string(),
        args: Vec::new(),
        env: HashMap::new(),
        working_dir,
        session_name: agent_id.to_string(),
        adapter,
        requested_mode: AgentRequestMode::Continuous,
        allow_oneshot_fallback: false,
    })
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
        Some("ps") | Some("list") => {
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

fn parse_run_args(args: &[String], mut index: usize) -> Result<RunArgs, String> {
    let mut result = RunArgs {
        text: String::new(),
        agent_id: String::new(),
        workspace_id: String::new(),
        agent_type: "codex".to_string(),
        command: String::new(),
        wait_for: String::new(),
        wait_timeout: 300,
        revive: false,
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
                result.revive = true;
                index += 1;
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

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!(
                "error: unknown flag for agent interrupt: '{token}'"
            ));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
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
      --revive             restart terminal/missing agent id before send
      --task-id string     correlation id for parent task
      --tag string         correlation tag (repeatable)
      --label string       correlation label KEY=VALUE (repeatable)
  -t, --text string        task text";

const SEND_HELP: &str = "\
Send a message to an agent

Usage:
  forge agent send <agent-id> [text] [flags]

Flags:
  -t, --text string   message text
      --no-enter       do not send Enter after text
      --key string     send a key (repeatable)";

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
  ps, list

Flags:
  -w, --workspace string   filter by workspace
      --state string        filter by state";

const SHOW_HELP: &str = "\
Show agent details

Usage:
  forge agent show <agent-id>

Aliases:
  show, get";

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
  forge agent interrupt <agent-id>";

const KILL_HELP: &str = "\
Kill an agent

Usage:
  forge agent kill <agent-id> [flags]

Flags:
  -f, --force   force kill without grace period";

const REVIVE_HELP: &str = "\
Revive a stopped or failed agent

Usage:
  forge agent revive <agent-id>";

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_agent::mock::test_snapshot;
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
    fn parse_run_requires_text() {
        let args = vec!["agent".to_string(), "run".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("task text is required"));
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
    fn agent_run_create_path_json() {
        let backend = InMemoryAgentBackend::new();
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
    fn agent_revive_not_implemented() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Stopped));
        let out = run_for_test(&["agent", "revive", "ag-001"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not yet implemented"));
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

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use tabwriter::TabWriter;

const STATUS_ALERT_LIMIT: usize = 5;

/// Alert severity levels matching Go's `models.AlertSeverity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl AlertSeverity {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Critical => 4,
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
        }
    }
}

/// Alert type matching Go's `models.AlertType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertType {
    ApprovalNeeded,
    Cooldown,
    Error,
    RateLimit,
    UsageLimit,
}

impl AlertType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ApprovalNeeded => "approval_needed",
            Self::Cooldown => "cooldown",
            Self::Error => "error",
            Self::RateLimit => "rate_limit",
            Self::UsageLimit => "usage_limit",
        }
    }
}

/// Agent state matching Go's `models.AgentState`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

/// Fixed display order for agent states, matching Go.
const AGENT_STATE_ORDER: &[AgentState] = &[
    AgentState::Working,
    AgentState::Idle,
    AgentState::AwaitingApproval,
    AgentState::RateLimited,
    AgentState::Error,
    AgentState::Paused,
    AgentState::Starting,
    AgentState::Stopped,
];

/// Node status matching Go's `models.NodeStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Online,
    Offline,
    Unknown,
}

/// A single alert record.
#[derive(Debug, Clone)]
pub struct Alert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub agent_id: String,
    pub created_at: String,
}

/// Node summary counts.
#[derive(Debug, Clone, Default)]
pub struct NodeSummary {
    pub total: u64,
    pub online: u64,
    pub offline: u64,
    pub unknown: u64,
}

/// Agent summary with per-state counts.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub total: u64,
    pub by_state: Vec<(AgentState, u64)>,
}

impl Default for AgentSummary {
    fn default() -> Self {
        Self {
            total: 0,
            by_state: AGENT_STATE_ORDER
                .iter()
                .map(|state| (state.clone(), 0))
                .collect(),
        }
    }
}

/// Alert summary with top items.
#[derive(Debug, Clone, Default)]
pub struct AlertSummary {
    pub total: u64,
    pub items: Vec<Alert>,
}

/// Full status summary matching Go's `StatusSummary`.
#[derive(Debug, Clone)]
pub struct StatusSummary {
    pub timestamp: String,
    pub nodes: NodeSummary,
    pub workspaces: u64,
    pub agents: AgentSummary,
    pub alerts: AlertSummary,
}

/// Backend trait for fetching status data.
pub trait StatusBackend {
    fn get_status(&self) -> Result<StatusSummary, String>;
}

#[derive(Debug, Clone)]
pub struct SqliteStatusBackend {
    db_path: PathBuf,
}

impl SqliteStatusBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

/// In-memory backend for testing.
#[derive(Debug, Clone, Default)]
pub struct InMemoryStatusBackend {
    summary: Option<StatusSummary>,
}

impl InMemoryStatusBackend {
    pub fn with_summary(summary: StatusSummary) -> Self {
        Self {
            summary: Some(summary),
        }
    }
}

impl StatusBackend for InMemoryStatusBackend {
    fn get_status(&self) -> Result<StatusSummary, String> {
        match &self.summary {
            Some(summary) => Ok(summary.clone()),
            None => Ok(StatusSummary {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                nodes: NodeSummary::default(),
                workspaces: 0,
                agents: AgentSummary::default(),
                alerts: AlertSummary::default(),
            }),
        }
    }
}

impl StatusBackend for SqliteStatusBackend {
    fn get_status(&self) -> Result<StatusSummary, String> {
        let now = Utc::now();
        let timestamp = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        if !self.db_path.exists() {
            return Ok(empty_summary(timestamp));
        }

        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let loops = match loop_repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => {
                return Ok(empty_summary(timestamp));
            }
            Err(err) => return Err(err.to_string()),
        };

        let profile_cooldowns = load_profile_cooldowns(&profile_repo)?;
        let mut queue_table_missing = false;

        let mut nodes = NodeSummary::default();
        let mut workspaces = HashSet::new();
        let mut state_counts: HashMap<AgentState, u64> = HashMap::new();
        let mut alerts = Vec::new();
        let created_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);

        for loop_entry in &loops {
            if !loop_entry.repo_path.is_empty() {
                workspaces.insert(loop_entry.repo_path.clone());
            }

            let pending_queue =
                count_pending_queue(&queue_repo, &loop_entry.id, &mut queue_table_missing)?;
            let cooldown_active = profile_cooldowns
                .get(&loop_entry.profile_id)
                .map(|until| is_cooldown_active(until, &now))
                .unwrap_or(false);

            let health = runner_health(loop_entry.metadata.as_ref());
            match health {
                RunnerHealth::Online => {
                    nodes.total += 1;
                    nodes.online += 1;
                }
                RunnerHealth::Offline => {
                    nodes.total += 1;
                    nodes.offline += 1;
                }
                RunnerHealth::Unknown => {
                    nodes.total += 1;
                    nodes.unknown += 1;
                }
            }

            let state = classify_agent_state(&loop_entry.state, pending_queue, cooldown_active);
            increment_state_count(&mut state_counts, state);

            let loop_ref = loop_short_id(loop_entry);
            if matches!(
                loop_entry.state,
                forge_db::loop_repository::LoopState::Error
            ) {
                let message = if loop_entry.last_error.trim().is_empty() {
                    format!("Loop '{}' error", loop_entry.name)
                } else {
                    format!(
                        "Loop '{}' error: {}",
                        loop_entry.name, loop_entry.last_error
                    )
                };
                alerts.push(Alert {
                    alert_type: AlertType::Error,
                    severity: AlertSeverity::Error,
                    message,
                    agent_id: loop_ref.to_string(),
                    created_at: created_at.clone(),
                });
            }

            if cooldown_active {
                alerts.push(Alert {
                    alert_type: AlertType::Cooldown,
                    severity: AlertSeverity::Info,
                    message: format!("Loop '{}' profile cooldown active", loop_entry.name),
                    agent_id: loop_ref.to_string(),
                    created_at: created_at.clone(),
                });
            }

            if pending_queue > 0
                && matches!(
                    loop_entry.state,
                    forge_db::loop_repository::LoopState::Waiting
                )
            {
                alerts.push(Alert {
                    alert_type: AlertType::ApprovalNeeded,
                    severity: AlertSeverity::Warning,
                    message: format!(
                        "Loop '{}' waiting with {pending_queue} pending queue item(s)",
                        loop_entry.name
                    ),
                    agent_id: loop_ref.to_string(),
                    created_at: created_at.clone(),
                });
            }

            if matches!(health, RunnerHealth::Offline)
                && !matches!(
                    loop_entry.state,
                    forge_db::loop_repository::LoopState::Stopped
                )
            {
                alerts.push(Alert {
                    alert_type: AlertType::Error,
                    severity: AlertSeverity::Error,
                    message: format!("Loop '{}' runner health check failed", loop_entry.name),
                    agent_id: loop_ref.to_string(),
                    created_at: created_at.clone(),
                });
            }
        }

        let by_state = AGENT_STATE_ORDER
            .iter()
            .map(|state| (state.clone(), *state_counts.get(state).unwrap_or(&0)))
            .collect();
        let top_alerts = select_top_alerts(&alerts, STATUS_ALERT_LIMIT);

        Ok(StatusSummary {
            timestamp,
            nodes,
            workspaces: workspaces.len() as u64,
            agents: AgentSummary {
                total: loops.len() as u64,
                by_state,
            },
            alerts: AlertSummary {
                total: alerts.len() as u64,
                items: top_alerts,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
}

// --- JSON serialization types ---

#[derive(Debug, Serialize)]
struct StatusJson<'a> {
    timestamp: &'a str,
    nodes: NodeSummaryJson,
    workspaces: u64,
    agents: AgentSummaryJson,
    alerts: AlertSummaryJson<'a>,
}

#[derive(Debug, Serialize)]
struct NodeSummaryJson {
    total: u64,
    online: u64,
    offline: u64,
    unknown: u64,
}

#[derive(Debug, Serialize)]
struct AgentSummaryJson {
    total: u64,
    by_state: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AlertSummaryJson<'a> {
    total: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    items: Vec<AlertJson<'a>>,
}

#[derive(Debug, Serialize)]
struct AlertJson<'a> {
    #[serde(rename = "type")]
    alert_type: &'a str,
    severity: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    agent_id: &'a str,
    created_at: &'a str,
}

pub fn run_for_test(args: &[&str], backend: &dyn StatusBackend) -> CommandOutput {
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
    backend: &dyn StatusBackend,
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

fn execute(
    args: &[String],
    backend: &dyn StatusBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let summary = backend.get_status()?;

    if parsed.json || parsed.jsonl {
        let json_summary = build_json_summary(&summary);
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &json_summary).map_err(|err| err.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &json_summary)
                .map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    write_human(&summary, stdout)
}

fn build_json_summary(summary: &StatusSummary) -> StatusJson<'_> {
    let mut by_state = serde_json::Map::new();
    for (state, count) in &summary.agents.by_state {
        by_state.insert(
            state.as_str().to_string(),
            serde_json::Value::Number(serde_json::Number::from(*count)),
        );
    }

    StatusJson {
        timestamp: &summary.timestamp,
        nodes: NodeSummaryJson {
            total: summary.nodes.total,
            online: summary.nodes.online,
            offline: summary.nodes.offline,
            unknown: summary.nodes.unknown,
        },
        workspaces: summary.workspaces,
        agents: AgentSummaryJson {
            total: summary.agents.total,
            by_state,
        },
        alerts: AlertSummaryJson {
            total: summary.alerts.total,
            items: summary
                .alerts
                .items
                .iter()
                .map(|alert| AlertJson {
                    alert_type: alert.alert_type.as_str(),
                    severity: alert.severity.as_str(),
                    message: &alert.message,
                    agent_id: &alert.agent_id,
                    created_at: &alert.created_at,
                })
                .collect(),
        },
    }
}

fn write_human(summary: &StatusSummary, stdout: &mut dyn Write) -> Result<(), String> {
    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(tw, "Timestamp:\t{}", summary.timestamp).map_err(|err| err.to_string())?;
    writeln!(
        tw,
        "Nodes:\t{} (online {}, offline {}, unknown {})",
        summary.nodes.total, summary.nodes.online, summary.nodes.offline, summary.nodes.unknown,
    )
    .map_err(|err| err.to_string())?;
    writeln!(tw, "Workspaces:\t{}", summary.workspaces).map_err(|err| err.to_string())?;
    writeln!(tw, "Agents:\t{}", summary.agents.total).map_err(|err| err.to_string())?;
    writeln!(
        tw,
        "Agent states:\t{}",
        format_agent_state_counts(&summary.agents.by_state)
    )
    .map_err(|err| err.to_string())?;
    writeln!(tw, "Alerts:\t{}", summary.alerts.total).map_err(|err| err.to_string())?;
    tw.flush().map_err(|err| err.to_string())?;

    if !summary.alerts.items.is_empty() {
        writeln!(stdout, "Top alerts:").map_err(|err| err.to_string())?;
        for alert in &summary.alerts.items {
            write!(stdout, "- [{}] {}", alert.severity.as_str(), alert.message,)
                .map_err(|err| err.to_string())?;
            if !alert.agent_id.is_empty() {
                write!(stdout, " (agent {})", alert.agent_id).map_err(|err| err.to_string())?;
            }
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn format_agent_state_counts(by_state: &[(AgentState, u64)]) -> String {
    let parts: Vec<String> = by_state
        .iter()
        .map(|(state, count)| format!("{}={count}", state.as_str()))
        .collect();
    parts.join(" ")
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

fn empty_summary(timestamp: String) -> StatusSummary {
    StatusSummary {
        timestamp,
        nodes: NodeSummary::default(),
        workspaces: 0,
        agents: AgentSummary::default(),
        alerts: AlertSummary::default(),
    }
}

fn load_profile_cooldowns(
    profile_repo: &forge_db::profile_repository::ProfileRepository<'_>,
) -> Result<HashMap<String, String>, String> {
    let profiles = match profile_repo.list() {
        Ok(profiles) => profiles,
        Err(err) if err.to_string().contains("no such table: profiles") => {
            return Ok(HashMap::new())
        }
        Err(err) => return Err(err.to_string()),
    };

    let mut out = HashMap::new();
    for profile in profiles {
        let Some(until) = profile.cooldown_until else {
            continue;
        };
        if until.trim().is_empty() {
            continue;
        }
        out.insert(profile.id, until);
    }
    Ok(out)
}

fn count_pending_queue(
    queue_repo: &forge_db::loop_queue_repository::LoopQueueRepository<'_>,
    loop_id: &str,
    queue_table_missing: &mut bool,
) -> Result<u64, String> {
    if *queue_table_missing {
        return Ok(0);
    }
    match queue_repo.list(loop_id) {
        Ok(items) => Ok(items.iter().filter(|item| item.status == "pending").count() as u64),
        Err(err) if err.to_string().contains("no such table: loop_queue_items") => {
            *queue_table_missing = true;
            Ok(0)
        }
        Err(err) => Err(format!("list queue items for loop {loop_id}: {err}")),
    }
}

fn is_cooldown_active(until: &str, now: &DateTime<Utc>) -> bool {
    chrono::DateTime::parse_from_rfc3339(until)
        .map(|value| value.with_timezone(&Utc) > *now)
        .unwrap_or(false)
}

fn classify_agent_state(
    loop_state: &forge_db::loop_repository::LoopState,
    pending_queue: u64,
    cooldown_active: bool,
) -> AgentState {
    match loop_state {
        forge_db::loop_repository::LoopState::Error => AgentState::Error,
        forge_db::loop_repository::LoopState::Stopped => AgentState::Stopped,
        forge_db::loop_repository::LoopState::Waiting => {
            if cooldown_active {
                AgentState::RateLimited
            } else {
                AgentState::AwaitingApproval
            }
        }
        forge_db::loop_repository::LoopState::Sleeping => {
            if cooldown_active {
                AgentState::RateLimited
            } else {
                AgentState::Paused
            }
        }
        forge_db::loop_repository::LoopState::Running => {
            if cooldown_active {
                AgentState::RateLimited
            } else if pending_queue > 0 {
                AgentState::Working
            } else {
                AgentState::Idle
            }
        }
    }
}

fn increment_state_count(counts: &mut HashMap<AgentState, u64>, state: AgentState) {
    let current = counts.entry(state).or_insert(0);
    *current += 1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunnerHealth {
    Online,
    Offline,
    Unknown,
}

fn runner_health(metadata: Option<&HashMap<String, Value>>) -> RunnerHealth {
    let pid_alive = metadata_nested_bool(metadata, "runner_liveness", "pid_alive");
    let daemon_alive = metadata_nested_bool(metadata, "runner_liveness", "daemon_runner_alive")
        .or_else(|| metadata_nested_bool(metadata, "runner_liveness", "daemon_alive"));

    if matches!(pid_alive, Some(false)) || matches!(daemon_alive, Some(false)) {
        return RunnerHealth::Offline;
    }
    if matches!(pid_alive, Some(true)) || matches!(daemon_alive, Some(true)) {
        return RunnerHealth::Online;
    }
    RunnerHealth::Unknown
}

fn metadata_nested_bool(
    metadata: Option<&HashMap<String, Value>>,
    outer: &str,
    inner: &str,
) -> Option<bool> {
    metadata
        .and_then(|meta| meta.get(outer))
        .and_then(Value::as_object)
        .and_then(|nested| nested.get(inner))
        .and_then(Value::as_bool)
}

fn loop_short_id(loop_entry: &forge_db::loop_repository::Loop) -> &str {
    if loop_entry.short_id.is_empty() {
        return loop_entry.id.as_str();
    }
    loop_entry.short_id.as_str()
}

/// Select top alerts sorted by severity (desc) then by created_at (desc).
/// This matches Go's `selectTopAlerts`.
pub fn select_top_alerts(alerts: &[Alert], limit: usize) -> Vec<Alert> {
    if alerts.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut sorted: Vec<Alert> = alerts.to_vec();
    sorted.sort_by(|a, b| {
        let sev_cmp = b.severity.rank().cmp(&a.severity.rank());
        if sev_cmp != std::cmp::Ordering::Equal {
            return sev_cmp;
        }
        // Reverse string comparison — later timestamps sort first.
        b.created_at.cmp(&a.created_at)
    });

    sorted.truncate(limit);
    sorted
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "status") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;

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
            "--quiet" => {
                quiet = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for status: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: status takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs { json, jsonl, quiet })
}

const HELP_TEXT: &str = "\
Show fleet status summary

Usage:
  forge status [flags]

Flags:
  -h, --help    help for status";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

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
            Err(err) => panic!("expected valid json: {err}"),
        }
    }

    fn sample_summary() -> StatusSummary {
        StatusSummary {
            timestamp: "2026-01-15T12:00:00Z".to_string(),
            nodes: NodeSummary {
                total: 5,
                online: 4,
                offline: 1,
                unknown: 0,
            },
            workspaces: 12,
            agents: AgentSummary {
                total: 42,
                by_state: vec![
                    (AgentState::Working, 8),
                    (AgentState::Idle, 25),
                    (AgentState::AwaitingApproval, 5),
                    (AgentState::RateLimited, 2),
                    (AgentState::Error, 2),
                    (AgentState::Paused, 0),
                    (AgentState::Starting, 0),
                    (AgentState::Stopped, 0),
                ],
            },
            alerts: AlertSummary {
                total: 2,
                items: vec![
                    Alert {
                        alert_type: AlertType::Error,
                        severity: AlertSeverity::Error,
                        message: "Agent error".to_string(),
                        agent_id: "agent-001".to_string(),
                        created_at: "2026-01-15T11:58:00Z".to_string(),
                    },
                    Alert {
                        alert_type: AlertType::ApprovalNeeded,
                        severity: AlertSeverity::Warning,
                        message: "Approval needed".to_string(),
                        agent_id: "agent-002".to_string(),
                        created_at: "2026-01-15T11:59:00Z".to_string(),
                    },
                ],
            },
        }
    }

    fn empty_summary() -> StatusSummary {
        StatusSummary {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            nodes: NodeSummary::default(),
            workspaces: 0,
            agents: AgentSummary::default(),
            alerts: AlertSummary::default(),
        }
    }

    // --- parse_args tests ---

    #[test]
    fn parse_accepts_no_args() {
        let args = vec!["status".to_string()];
        let parsed = parse_ok(&args);
        assert!(!parsed.json);
        assert!(!parsed.jsonl);
        assert!(!parsed.quiet);
    }

    #[test]
    fn parse_accepts_json_flag() {
        let args = vec!["status".to_string(), "--json".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.json);
    }

    #[test]
    fn parse_accepts_jsonl_flag() {
        let args = vec!["status".to_string(), "--jsonl".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.jsonl);
    }

    #[test]
    fn parse_accepts_quiet_flag() {
        let args = vec!["status".to_string(), "--quiet".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.quiet);
    }

    #[test]
    fn parse_rejects_json_and_jsonl_together() {
        let args = vec![
            "status".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let args = vec!["status".to_string(), "--bogus".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("unknown argument for status"));
    }

    #[test]
    fn parse_rejects_positional_args() {
        let args = vec!["status".to_string(), "extra".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("no positional arguments"));
    }

    #[test]
    fn parse_help_returns_usage() {
        let args = vec!["status".to_string(), "--help".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("Show fleet status summary"));
        assert!(err.contains("forge status"));
    }

    // --- command output tests ---

    #[test]
    fn status_default_backend_returns_empty() {
        let backend = InMemoryStatusBackend::default();
        let out = run_for_test(&["status"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("Timestamp:"));
        assert!(out.stdout.contains("Nodes:"));
        assert!(out.stdout.contains("Workspaces:"));
        assert!(out.stdout.contains("Agents:"));
        assert!(out.stdout.contains("Agent states:"));
        assert!(out.stdout.contains("Alerts:"));
    }

    #[test]
    fn status_human_output_with_data() {
        let backend = InMemoryStatusBackend::with_summary(sample_summary());
        let out = run_for_test(&["status"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("2026-01-15T12:00:00Z"));
        assert!(out.stdout.contains("5 (online 4, offline 1, unknown 0)"));
        assert!(out.stdout.contains("Workspaces:"));
        assert!(out.stdout.contains("12"));
        assert!(out.stdout.contains("Agents:"));
        assert!(out.stdout.contains("42"));
        assert!(out
            .stdout
            .contains("working=8 idle=25 awaiting_approval=5 rate_limited=2 error=2 paused=0 starting=0 stopped=0"));
        assert!(out.stdout.contains("Top alerts:"));
        assert!(out
            .stdout
            .contains("- [error] Agent error (agent agent-001)"));
        assert!(out
            .stdout
            .contains("- [warning] Approval needed (agent agent-002)"));
    }

    #[test]
    fn status_human_output_no_alerts() {
        let backend = InMemoryStatusBackend::with_summary(empty_summary());
        let out = run_for_test(&["status"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(!out.stdout.contains("Top alerts:"));
    }

    #[test]
    fn status_json_output() {
        let backend = InMemoryStatusBackend::with_summary(sample_summary());
        let out = run_for_test(&["status", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["timestamp"], "2026-01-15T12:00:00Z");
        assert_eq!(parsed["nodes"]["total"], 5);
        assert_eq!(parsed["nodes"]["online"], 4);
        assert_eq!(parsed["nodes"]["offline"], 1);
        assert_eq!(parsed["nodes"]["unknown"], 0);
        assert_eq!(parsed["workspaces"], 12);
        assert_eq!(parsed["agents"]["total"], 42);
        assert_eq!(parsed["agents"]["by_state"]["working"], 8);
        assert_eq!(parsed["agents"]["by_state"]["idle"], 25);
        assert_eq!(parsed["agents"]["by_state"]["awaiting_approval"], 5);
        assert_eq!(parsed["agents"]["by_state"]["rate_limited"], 2);
        assert_eq!(parsed["agents"]["by_state"]["error"], 2);
        assert_eq!(parsed["agents"]["by_state"]["paused"], 0);
        assert_eq!(parsed["agents"]["by_state"]["starting"], 0);
        assert_eq!(parsed["agents"]["by_state"]["stopped"], 0);
        assert_eq!(parsed["alerts"]["total"], 2);
        let items = parsed["alerts"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "error");
        assert_eq!(items[0]["severity"], "error");
        assert_eq!(items[0]["message"], "Agent error");
        assert_eq!(items[0]["agent_id"], "agent-001");
        assert_eq!(items[1]["type"], "approval_needed");
        assert_eq!(items[1]["severity"], "warning");
    }

    #[test]
    fn status_json_empty_alerts_omits_items() {
        let backend = InMemoryStatusBackend::with_summary(empty_summary());
        let out = run_for_test(&["status", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["alerts"]["total"], 0);
        // items key should be absent when empty
        assert!(parsed["alerts"]["items"].is_null());
    }

    #[test]
    fn status_jsonl_output() {
        let backend = InMemoryStatusBackend::with_summary(sample_summary());
        let out = run_for_test(&["status", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);
        let parsed = parse_json(lines[0]);
        assert_eq!(parsed["timestamp"], "2026-01-15T12:00:00Z");
        assert_eq!(parsed["nodes"]["total"], 5);
        assert_eq!(parsed["agents"]["total"], 42);
    }

    #[test]
    fn status_quiet_suppresses_output() {
        let backend = InMemoryStatusBackend::with_summary(sample_summary());
        let out = run_for_test(&["status", "--quiet"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn status_help_returns_usage() {
        let backend = InMemoryStatusBackend::default();
        let out = run_for_test(&["status", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Show fleet status summary"));
        assert!(out.stderr.contains("forge status"));
    }

    // --- select_top_alerts tests ---

    #[test]
    fn select_top_alerts_empty_input() {
        let result = select_top_alerts(&[], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn select_top_alerts_zero_limit() {
        let alerts = vec![Alert {
            alert_type: AlertType::Error,
            severity: AlertSeverity::Error,
            message: "test".to_string(),
            agent_id: String::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }];
        let result = select_top_alerts(&alerts, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn select_top_alerts_sorts_by_severity() {
        let alerts = vec![
            Alert {
                alert_type: AlertType::ApprovalNeeded,
                severity: AlertSeverity::Warning,
                message: "warning".to_string(),
                agent_id: String::new(),
                created_at: "2026-01-01T00:00:01Z".to_string(),
            },
            Alert {
                alert_type: AlertType::Error,
                severity: AlertSeverity::Critical,
                message: "critical".to_string(),
                agent_id: String::new(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            Alert {
                alert_type: AlertType::Error,
                severity: AlertSeverity::Error,
                message: "error".to_string(),
                agent_id: String::new(),
                created_at: "2026-01-01T00:00:02Z".to_string(),
            },
        ];
        let result = select_top_alerts(&alerts, 5);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].message, "critical");
        assert_eq!(result[1].message, "error");
        assert_eq!(result[2].message, "warning");
    }

    #[test]
    fn select_top_alerts_sorts_by_recency_within_same_severity() {
        let alerts = vec![
            Alert {
                alert_type: AlertType::Error,
                severity: AlertSeverity::Error,
                message: "older".to_string(),
                agent_id: String::new(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            Alert {
                alert_type: AlertType::Error,
                severity: AlertSeverity::Error,
                message: "newer".to_string(),
                agent_id: String::new(),
                created_at: "2026-01-01T00:01:00Z".to_string(),
            },
        ];
        let result = select_top_alerts(&alerts, 5);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].message, "newer");
        assert_eq!(result[1].message, "older");
    }

    #[test]
    fn select_top_alerts_truncates_to_limit() {
        let alerts: Vec<Alert> = (0..10)
            .map(|i| Alert {
                alert_type: AlertType::Error,
                severity: AlertSeverity::Error,
                message: format!("alert-{i}"),
                agent_id: String::new(),
                created_at: format!("2026-01-01T00:00:{i:02}Z"),
            })
            .collect();
        let result = select_top_alerts(&alerts, 5);
        assert_eq!(result.len(), 5);
    }

    // --- format_agent_state_counts tests ---

    #[test]
    fn format_agent_state_counts_all_zeros() {
        let by_state = AgentSummary::default().by_state;
        let formatted = format_agent_state_counts(&by_state);
        assert_eq!(
            formatted,
            "working=0 idle=0 awaiting_approval=0 rate_limited=0 error=0 paused=0 starting=0 stopped=0"
        );
    }

    #[test]
    fn format_agent_state_counts_with_values() {
        let by_state = vec![
            (AgentState::Working, 3),
            (AgentState::Idle, 10),
            (AgentState::AwaitingApproval, 1),
            (AgentState::RateLimited, 0),
            (AgentState::Error, 2),
            (AgentState::Paused, 0),
            (AgentState::Starting, 1),
            (AgentState::Stopped, 0),
        ];
        let formatted = format_agent_state_counts(&by_state);
        assert_eq!(
            formatted,
            "working=3 idle=10 awaiting_approval=1 rate_limited=0 error=2 paused=0 starting=1 stopped=0"
        );
    }

    // --- golden test: JSON output structure ---

    #[test]
    fn status_json_golden_structure() {
        let backend = InMemoryStatusBackend::with_summary(StatusSummary {
            timestamp: "2026-02-01T10:00:00Z".to_string(),
            nodes: NodeSummary {
                total: 3,
                online: 2,
                offline: 1,
                unknown: 0,
            },
            workspaces: 5,
            agents: AgentSummary {
                total: 10,
                by_state: vec![
                    (AgentState::Working, 3),
                    (AgentState::Idle, 5),
                    (AgentState::AwaitingApproval, 1),
                    (AgentState::RateLimited, 0),
                    (AgentState::Error, 1),
                    (AgentState::Paused, 0),
                    (AgentState::Starting, 0),
                    (AgentState::Stopped, 0),
                ],
            },
            alerts: AlertSummary {
                total: 1,
                items: vec![Alert {
                    alert_type: AlertType::Error,
                    severity: AlertSeverity::Error,
                    message: "Agent error".to_string(),
                    agent_id: "agent-x".to_string(),
                    created_at: "2026-02-01T09:58:00Z".to_string(),
                }],
            },
        });
        let out = run_for_test(&["status", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);

        // Verify all top-level keys present
        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("workspaces").is_some());
        assert!(parsed.get("agents").is_some());
        assert!(parsed.get("alerts").is_some());

        // Verify nested structure
        assert!(parsed["nodes"].get("total").is_some());
        assert!(parsed["nodes"].get("online").is_some());
        assert!(parsed["nodes"].get("offline").is_some());
        assert!(parsed["nodes"].get("unknown").is_some());

        assert!(parsed["agents"].get("total").is_some());
        assert!(parsed["agents"].get("by_state").is_some());

        assert!(parsed["alerts"].get("total").is_some());
        let items = parsed["alerts"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "error");
        assert_eq!(items[0]["severity"], "error");
        assert_eq!(items[0]["message"], "Agent error");
        assert_eq!(items[0]["agent_id"], "agent-x");
        assert_eq!(items[0]["created_at"], "2026-02-01T09:58:00Z");
    }

    // --- alert without agent_id omits field in JSON ---

    #[test]
    fn status_json_alert_without_agent_id() {
        let backend = InMemoryStatusBackend::with_summary(StatusSummary {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            nodes: NodeSummary::default(),
            workspaces: 0,
            agents: AgentSummary::default(),
            alerts: AlertSummary {
                total: 1,
                items: vec![Alert {
                    alert_type: AlertType::Cooldown,
                    severity: AlertSeverity::Info,
                    message: "Cooldown active".to_string(),
                    agent_id: String::new(),
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                }],
            },
        });
        let out = run_for_test(&["status", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);
        let items = parsed["alerts"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        // agent_id should be omitted when empty
        assert!(items[0].get("agent_id").is_none());
        assert_eq!(items[0]["type"], "cooldown");
        assert_eq!(items[0]["severity"], "info");
    }

    // --- human output alert without agent_id ---

    #[test]
    fn status_human_alert_without_agent_id() {
        let backend = InMemoryStatusBackend::with_summary(StatusSummary {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            nodes: NodeSummary::default(),
            workspaces: 0,
            agents: AgentSummary::default(),
            alerts: AlertSummary {
                total: 1,
                items: vec![Alert {
                    alert_type: AlertType::UsageLimit,
                    severity: AlertSeverity::Warning,
                    message: "Usage limit approaching".to_string(),
                    agent_id: String::new(),
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                }],
            },
        });
        let out = run_for_test(&["status"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("- [warning] Usage limit approaching"));
        assert!(!out.stdout.contains("(agent"));
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-status-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    fn find_state_count(summary: &StatusSummary, state: AgentState) -> u64 {
        summary
            .agents
            .by_state
            .iter()
            .find(|(entry_state, _)| *entry_state == state)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    }

    #[test]
    fn status_sqlite_backend_summarizes_loops_queue_cooldown_and_runner_health() {
        let db_path = temp_db_path("sqlite-summary");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let mut cooled_profile = forge_db::profile_repository::Profile {
            name: "cooled".to_string(),
            command_template: "echo run".to_string(),
            cooldown_until: Some("2999-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        profile_repo
            .create(&mut cooled_profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

        let mut running_metadata = HashMap::new();
        running_metadata.insert(
            "runner_liveness".to_string(),
            json!({
                "pid_alive": true,
                "daemon_runner_alive": true
            }),
        );
        let mut running_loop = forge_db::loop_repository::Loop {
            name: "running-loop".to_string(),
            repo_path: "/repo/a".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            metadata: Some(running_metadata),
            ..Default::default()
        };
        loop_repo
            .create(&mut running_loop)
            .unwrap_or_else(|err| panic!("create running loop: {err}"));

        let mut sleeping_metadata = HashMap::new();
        sleeping_metadata.insert(
            "runner_liveness".to_string(),
            json!({
                "daemon_alive": true
            }),
        );
        let mut sleeping_loop = forge_db::loop_repository::Loop {
            name: "cooldown-loop".to_string(),
            repo_path: "/repo/b".to_string(),
            profile_id: cooled_profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Sleeping,
            metadata: Some(sleeping_metadata),
            ..Default::default()
        };
        loop_repo
            .create(&mut sleeping_loop)
            .unwrap_or_else(|err| panic!("create sleeping loop: {err}"));

        let mut waiting_loop = forge_db::loop_repository::Loop {
            name: "waiting-loop".to_string(),
            repo_path: "/repo/b".to_string(),
            state: forge_db::loop_repository::LoopState::Waiting,
            ..Default::default()
        };
        loop_repo
            .create(&mut waiting_loop)
            .unwrap_or_else(|err| panic!("create waiting loop: {err}"));

        let mut error_metadata = HashMap::new();
        error_metadata.insert(
            "runner_liveness".to_string(),
            json!({
                "pid_alive": false
            }),
        );
        let mut error_loop = forge_db::loop_repository::Loop {
            name: "error-loop".to_string(),
            repo_path: "/repo/c".to_string(),
            state: forge_db::loop_repository::LoopState::Error,
            last_error: "runner crashed".to_string(),
            metadata: Some(error_metadata),
            ..Default::default()
        };
        loop_repo
            .create(&mut error_loop)
            .unwrap_or_else(|err| panic!("create error loop: {err}"));

        let mut running_queue_items = vec![
            forge_db::loop_queue_repository::LoopQueueItem {
                item_type: "message_append".to_string(),
                payload: "{\"text\":\"hello\"}".to_string(),
                ..Default::default()
            },
            forge_db::loop_queue_repository::LoopQueueItem {
                item_type: "message_append".to_string(),
                payload: "{\"text\":\"world\"}".to_string(),
                ..Default::default()
            },
        ];
        queue_repo
            .enqueue(&running_loop.id, &mut running_queue_items)
            .unwrap_or_else(|err| panic!("enqueue running queue items: {err}"));

        let mut waiting_queue_items = vec![forge_db::loop_queue_repository::LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: "{\"text\":\"needs approval\"}".to_string(),
            ..Default::default()
        }];
        queue_repo
            .enqueue(&waiting_loop.id, &mut waiting_queue_items)
            .unwrap_or_else(|err| panic!("enqueue waiting queue item: {err}"));

        let backend = SqliteStatusBackend::new(db_path.clone());
        let summary = backend
            .get_status()
            .unwrap_or_else(|err| panic!("get status summary: {err}"));

        assert_eq!(summary.nodes.total, 4);
        assert_eq!(summary.nodes.online, 2);
        assert_eq!(summary.nodes.offline, 1);
        assert_eq!(summary.nodes.unknown, 1);
        assert_eq!(summary.workspaces, 3);

        assert_eq!(summary.agents.total, 4);
        assert_eq!(find_state_count(&summary, AgentState::Working), 1);
        assert_eq!(find_state_count(&summary, AgentState::RateLimited), 1);
        assert_eq!(find_state_count(&summary, AgentState::AwaitingApproval), 1);
        assert_eq!(find_state_count(&summary, AgentState::Error), 1);

        assert!(summary.alerts.total >= 4);
        assert!(summary
            .alerts
            .items
            .iter()
            .any(|item| item.alert_type == AlertType::Cooldown));
        assert!(summary
            .alerts
            .items
            .iter()
            .any(|item| item.alert_type == AlertType::ApprovalNeeded));
        assert!(summary
            .alerts
            .items
            .iter()
            .any(|item| item.alert_type == AlertType::Error));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn status_sqlite_backend_missing_db_returns_empty_summary() {
        let db_path = temp_db_path("sqlite-missing");
        let _ = std::fs::remove_file(&db_path);

        let backend = SqliteStatusBackend::new(db_path);
        let summary = backend
            .get_status()
            .unwrap_or_else(|err| panic!("summary from missing db: {err}"));

        assert!(!summary.timestamp.is_empty());
        assert_eq!(summary.nodes.total, 0);
        assert_eq!(summary.workspaces, 0);
        assert_eq!(summary.agents.total, 0);
        assert_eq!(summary.alerts.total, 0);
        assert!(summary.alerts.items.is_empty());
    }
}

use std::io::Write;

use serde::Serialize;
use tabwriter::TabWriter;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A simplified node record for export status output, matching Go's `models.Node`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportNode {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_target: Option<String>,
    pub is_local: bool,
    pub agent_count: usize,
}

/// A simplified workspace record for export status output, matching Go's `models.Workspace`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportWorkspace {
    pub id: String,
    pub name: String,
    pub node_id: String,
    pub status: String,
    pub agent_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<ExportAlert>,
}

/// A simplified agent record for export status output, matching Go's `models.Agent`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportAgent {
    pub id: String,
    pub workspace_id: String,
    pub state: String,
    #[serde(rename = "type")]
    pub agent_type: String,
    pub queue_length: usize,
}

/// A queue item for export status output, matching Go's `models.QueueItem`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportQueueItem {
    pub id: String,
    pub agent_id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub position: usize,
    pub status: String,
}

/// An alert for export status output, matching Go's `models.Alert`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportAlert {
    #[serde(rename = "type")]
    pub alert_type: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Full export status payload, matching Go's `ExportStatus`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportStatus {
    pub nodes: Vec<ExportNode>,
    pub workspaces: Vec<ExportWorkspace>,
    pub agents: Vec<ExportAgent>,
    pub queues: Vec<ExportQueueItem>,
    pub alerts: Vec<ExportAlert>,
}

/// An event record for export events output, matching Go's `models.Event`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportEvent {
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub entity_type: String,
    pub entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// Backend trait abstracting data access for testability.
pub trait ExportBackend {
    /// Build the full export status (nodes, workspaces, agents, queues, alerts).
    fn build_status(&self) -> Result<ExportStatus, String>;

    /// Query events with optional filters and cursor-based pagination.
    /// Returns (events, next_cursor). Empty next_cursor means no more pages.
    fn query_events(
        &self,
        cursor: &str,
        since: Option<&str>,
        until: Option<&str>,
        event_types: &[String],
        entity_types: &[String],
        entity_id: &str,
        limit: usize,
    ) -> Result<(Vec<ExportEvent>, String), String>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

/// In-memory backend for testing.
#[derive(Debug, Default)]
pub struct InMemoryExportBackend {
    pub status: Option<ExportStatus>,
    pub events: Vec<ExportEvent>,
    pub status_error: Option<String>,
    pub events_error: Option<String>,
}

impl InMemoryExportBackend {
    pub fn with_status(mut self, status: ExportStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_events(mut self, events: Vec<ExportEvent>) -> Self {
        self.events = events;
        self
    }

    pub fn with_status_error(mut self, err: &str) -> Self {
        self.status_error = Some(err.to_string());
        self
    }

    pub fn with_events_error(mut self, err: &str) -> Self {
        self.events_error = Some(err.to_string());
        self
    }
}

impl ExportBackend for InMemoryExportBackend {
    fn build_status(&self) -> Result<ExportStatus, String> {
        if let Some(ref err) = self.status_error {
            return Err(err.clone());
        }
        Ok(self.status.clone().unwrap_or(ExportStatus {
            nodes: Vec::new(),
            workspaces: Vec::new(),
            agents: Vec::new(),
            queues: Vec::new(),
            alerts: Vec::new(),
        }))
    }

    fn query_events(
        &self,
        _cursor: &str,
        since: Option<&str>,
        until: Option<&str>,
        event_types: &[String],
        _entity_types: &[String],
        entity_id: &str,
        _limit: usize,
    ) -> Result<(Vec<ExportEvent>, String), String> {
        if let Some(ref err) = self.events_error {
            return Err(err.clone());
        }
        let mut filtered: Vec<ExportEvent> = self.events.clone();

        // Filter by event types (client-side, matching Go's filterEventsByType).
        if !event_types.is_empty() {
            filtered.retain(|e| event_types.iter().any(|t| t == &e.event_type));
        }

        // Filter by entity_id if set.
        if !entity_id.is_empty() {
            filtered.retain(|e| e.entity_id == entity_id);
        }

        // Filter by since/until on timestamp strings (lexicographic for ISO-8601).
        if let Some(s) = since {
            filtered.retain(|e| e.timestamp.as_str() >= s);
        }
        if let Some(u) = until {
            filtered.retain(|e| e.timestamp.as_str() <= u);
        }

        // In-memory returns all at once (no pagination).
        Ok((filtered, String::new()))
    }
}

// ---------------------------------------------------------------------------
// Test-only command output
// ---------------------------------------------------------------------------

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn ExportBackend) -> CommandOutput {
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

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run_with_backend(
    args: &[String],
    backend: &dyn ExportBackend,
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
// Core execution
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn ExportBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.subcommand {
        Subcommand::Status => execute_status(backend, &parsed, stdout),
        Subcommand::Events => execute_events(backend, &parsed, stdout),
    }
}

fn execute_status(
    backend: &dyn ExportBackend,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let status = backend.build_status()?;

    if parsed.json || parsed.jsonl {
        write_json_output(stdout, &status, parsed.jsonl)?;
        return Ok(());
    }

    // Human-readable summary (matching Go's tabwriter output).
    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(tw, "Nodes:\t{}", status.nodes.len()).map_err(|e| e.to_string())?;
    writeln!(tw, "Workspaces:\t{}", status.workspaces.len()).map_err(|e| e.to_string())?;
    writeln!(tw, "Agents:\t{}", status.agents.len()).map_err(|e| e.to_string())?;
    writeln!(tw, "Queue items:\t{}", status.queues.len()).map_err(|e| e.to_string())?;
    writeln!(tw, "Alerts:\t{}", status.alerts.len()).map_err(|e| e.to_string())?;
    tw.flush().map_err(|e| e.to_string())?;

    writeln!(stdout, "Use --json or --jsonl for full export output.").map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_events(
    backend: &dyn ExportBackend,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let event_types = parse_event_types(&parsed.type_filter)?;
    let agent_id = parsed.agent_filter.trim().to_string();

    let mut entity_types: Vec<String> = Vec::new();
    if !agent_id.is_empty() {
        entity_types.push("agent".to_string());
    }

    // Validate since/until ordering.
    if let (Some(ref s), Some(ref u)) = (&parsed.since, &parsed.until) {
        if s.as_str() > u.as_str() {
            return Err("--since must be before --until".to_string());
        }
    }

    // Watch mode requires --jsonl (matching Go's MustBeJSONLForWatch).
    if parsed.watch {
        if !parsed.jsonl {
            return Err("--watch requires --jsonl output format".to_string());
        }
        if parsed.until.is_some() {
            return Err("--until cannot be used with --watch".to_string());
        }
    }

    if parsed.jsonl {
        return stream_export_events(
            backend,
            parsed,
            stdout,
            &event_types,
            &entity_types,
            &agent_id,
        );
    }

    let events = collect_export_events(backend, parsed, &event_types, &entity_types, &agent_id)?;

    if parsed.json {
        write_json_output(stdout, &events, false)?;
        return Ok(());
    }

    // Human-readable summary.
    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(tw, "Events:\t{}", events.len()).map_err(|e| e.to_string())?;
    tw.flush().map_err(|e| e.to_string())?;

    writeln!(stdout, "Use --json or --jsonl for full export output.").map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pagination helpers (matching Go's exportEventsPaginated)
// ---------------------------------------------------------------------------

const EVENTS_PAGE_SIZE: usize = 500;

fn stream_export_events(
    backend: &dyn ExportBackend,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
    event_types: &[String],
    entity_types: &[String],
    entity_id: &str,
) -> Result<(), String> {
    paginate_events(
        backend,
        parsed,
        event_types,
        entity_types,
        entity_id,
        |events| {
            if events.is_empty() {
                return Ok(());
            }
            for event in events {
                serde_json::to_writer(&mut *stdout, event).map_err(|e| e.to_string())?;
                writeln!(stdout).map_err(|e| e.to_string())?;
            }
            Ok(())
        },
    )
}

fn collect_export_events(
    backend: &dyn ExportBackend,
    parsed: &ParsedArgs,
    event_types: &[String],
    entity_types: &[String],
    entity_id: &str,
) -> Result<Vec<ExportEvent>, String> {
    let mut collected: Vec<ExportEvent> = Vec::new();
    paginate_events(
        backend,
        parsed,
        event_types,
        entity_types,
        entity_id,
        |events| {
            if !events.is_empty() {
                collected.extend(events.iter().cloned());
            }
            Ok(())
        },
    )?;
    Ok(collected)
}

fn paginate_events<F>(
    backend: &dyn ExportBackend,
    parsed: &ParsedArgs,
    event_types: &[String],
    entity_types: &[String],
    entity_id: &str,
    mut handle: F,
) -> Result<(), String>
where
    F: FnMut(&[ExportEvent]) -> Result<(), String>,
{
    let mut cursor = String::new();
    let mut since = parsed.since.clone();
    loop {
        let (events, next_cursor) = backend.query_events(
            &cursor,
            since.as_deref(),
            parsed.until.as_deref(),
            event_types,
            entity_types,
            entity_id,
            EVENTS_PAGE_SIZE,
        )?;

        // Client-side multi-type filtering (matching Go: filterEventsByType skips
        // for <=1 types since the DB query already handles a single type).
        let filtered = filter_events_by_type(&events, event_types);
        handle(&filtered)?;

        if next_cursor.is_empty() {
            break;
        }
        cursor = next_cursor;
        since = None;
    }
    Ok(())
}

fn filter_events_by_type(events: &[ExportEvent], event_types: &[String]) -> Vec<ExportEvent> {
    if event_types.len() <= 1 {
        return events.to_vec();
    }
    events
        .iter()
        .filter(|e| event_types.iter().any(|t| t == &e.event_type))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Event type parsing (matching Go's parseEventTypes)
// ---------------------------------------------------------------------------

fn parse_event_types(raw: &str) -> Result<Vec<String>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let types: Vec<String> = raw
        .split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();

    if types.is_empty() {
        return Err("event type filter cannot be empty".to_string());
    }

    Ok(types)
}

// ---------------------------------------------------------------------------
// JSON output helpers
// ---------------------------------------------------------------------------

fn write_json_output<T: Serialize>(
    stdout: &mut dyn Write,
    value: &T,
    compact: bool,
) -> Result<(), String> {
    if compact {
        serde_json::to_writer(&mut *stdout, value).map_err(|e| e.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, value).map_err(|e| e.to_string())?;
    }
    writeln!(stdout).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subcommand {
    Status,
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    subcommand: Subcommand,
    json: bool,
    jsonl: bool,
    watch: bool,
    since: Option<String>,
    until: Option<String>,
    type_filter: String,
    agent_filter: String,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;

    // Skip the command name "export" if present.
    if args.get(index).is_some_and(|token| token == "export") {
        index += 1;
    }

    // Parse subcommand or help.
    let sub_token = args.get(index).map(|s| s.as_str());
    let subcommand = match sub_token {
        Some("status") => {
            index += 1;
            Subcommand::Status
        }
        Some("events") => {
            index += 1;
            Subcommand::Events
        }
        Some("-h") | Some("--help") | Some("help") => {
            return Err(HELP_TEXT.to_string());
        }
        None => {
            return Err(HELP_TEXT.to_string());
        }
        Some(other) => {
            return Err(format!(
                "error: unknown export subcommand '{other}'. Expected 'status' or 'events'."
            ));
        }
    };

    let mut json = false;
    let mut jsonl = false;
    let mut watch = false;
    let mut since: Option<String> = None;
    let mut until: Option<String> = None;
    let mut type_filter = String::new();
    let mut agent_filter = String::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return match subcommand {
                    Subcommand::Status => Err(HELP_STATUS.to_string()),
                    Subcommand::Events => Err(HELP_EVENTS.to_string()),
                };
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--watch" => {
                watch = true;
                index += 1;
            }
            "--since" => {
                index += 1;
                since = Some(
                    args.get(index)
                        .ok_or("error: --since requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--until" => {
                if subcommand == Subcommand::Status {
                    return Err("error: --until is only valid for 'export events'".to_string());
                }
                index += 1;
                until = Some(
                    args.get(index)
                        .ok_or("error: --until requires a value")?
                        .clone(),
                );
                index += 1;
            }
            "--type" => {
                if subcommand == Subcommand::Status {
                    return Err("error: --type is only valid for 'export events'".to_string());
                }
                index += 1;
                type_filter = args
                    .get(index)
                    .ok_or("error: --type requires a value")?
                    .clone();
                index += 1;
            }
            "--agent" => {
                if subcommand == Subcommand::Status {
                    return Err("error: --agent is only valid for 'export events'".to_string());
                }
                index += 1;
                agent_filter = args
                    .get(index)
                    .ok_or("error: --agent requires a value")?
                    .clone();
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for export: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: export takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs {
        subcommand,
        json,
        jsonl,
        watch,
        since,
        until,
        type_filter,
        agent_filter,
    })
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

const HELP_TEXT: &str = "\
Export Forge state for automation or reporting.

Usage:
  forge export [command]

Available Commands:
  events      Export events
  status      Export full status

Flags:
  -h, --help   help for export";

const HELP_STATUS: &str = "\
Export full status as JSON: nodes, workspaces, agents, queues, alerts.

Usage:
  forge export status [flags]

Examples:
  forge export status --json
  forge export status --jsonl

Flags:
  -h, --help   help for status";

const HELP_EVENTS: &str = "\
Export the event log as JSON or JSONL, optionally filtered by type, time range, or agent.

Usage:
  forge export events [flags]

Examples:
  forge export events --json
  forge export events --jsonl --type agent.spawned
  forge export events --jsonl --agent my-agent --since 1h

Flags:
      --type string    filter by event type (comma-separated)
      --until string   filter events before a time (same format as --since)
      --agent string   filter by agent ID
  -h, --help           help for events";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn default_backend() -> InMemoryExportBackend {
        InMemoryExportBackend::default()
    }

    fn run(args: &[&str], backend: &dyn ExportBackend) -> CommandOutput {
        run_for_test(args, backend)
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    fn to_args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| (*s).to_string()).collect()
    }

    fn sample_status() -> ExportStatus {
        ExportStatus {
            nodes: vec![ExportNode {
                id: "node-1".to_string(),
                name: "local".to_string(),
                status: "online".to_string(),
                ssh_target: None,
                is_local: true,
                agent_count: 2,
            }],
            workspaces: vec![ExportWorkspace {
                id: "ws-1".to_string(),
                name: "my-repo".to_string(),
                node_id: "node-1".to_string(),
                status: "active".to_string(),
                agent_count: 2,
                alerts: Vec::new(),
            }],
            agents: vec![
                ExportAgent {
                    id: "agent-1".to_string(),
                    workspace_id: "ws-1".to_string(),
                    state: "working".to_string(),
                    agent_type: "claude".to_string(),
                    queue_length: 3,
                },
                ExportAgent {
                    id: "agent-2".to_string(),
                    workspace_id: "ws-1".to_string(),
                    state: "idle".to_string(),
                    agent_type: "claude".to_string(),
                    queue_length: 0,
                },
            ],
            queues: vec![ExportQueueItem {
                id: "qi-1".to_string(),
                agent_id: "agent-1".to_string(),
                item_type: "message".to_string(),
                position: 0,
                status: "pending".to_string(),
            }],
            alerts: vec![ExportAlert {
                alert_type: "cooldown".to_string(),
                severity: "warning".to_string(),
                message: "Rate limit approaching".to_string(),
                agent_id: Some("agent-1".to_string()),
            }],
        }
    }

    fn sample_events() -> Vec<ExportEvent> {
        vec![
            ExportEvent {
                id: "evt-1".to_string(),
                timestamp: "2026-02-09T10:00:00Z".to_string(),
                event_type: "agent.spawned".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-1".to_string(),
                payload: None,
                metadata: None,
            },
            ExportEvent {
                id: "evt-2".to_string(),
                timestamp: "2026-02-09T10:05:00Z".to_string(),
                event_type: "agent.state_changed".to_string(),
                entity_type: "agent".to_string(),
                entity_id: "agent-1".to_string(),
                payload: Some(serde_json::json!({"new_state": "working"})),
                metadata: None,
            },
            ExportEvent {
                id: "evt-3".to_string(),
                timestamp: "2026-02-09T10:10:00Z".to_string(),
                event_type: "node.online".to_string(),
                entity_type: "node".to_string(),
                entity_id: "node-1".to_string(),
                payload: None,
                metadata: None,
            },
        ]
    }

    // --- parse_args tests ---

    #[test]
    fn parse_no_subcommand_shows_help() {
        let args = to_args(&["export"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Export Forge state"));
        assert!(err.contains("Available Commands"));
    }

    #[test]
    fn parse_status_subcommand() {
        let args = to_args(&["export", "status"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.subcommand, Subcommand::Status);
        assert!(!parsed.json);
        assert!(!parsed.jsonl);
    }

    #[test]
    fn parse_events_subcommand() {
        let args = to_args(&["export", "events"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.subcommand, Subcommand::Events);
    }

    #[test]
    fn parse_status_json() {
        let args = to_args(&["export", "status", "--json"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.subcommand, Subcommand::Status);
        assert!(parsed.json);
    }

    #[test]
    fn parse_status_jsonl() {
        let args = to_args(&["export", "status", "--jsonl"]);
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.jsonl);
    }

    #[test]
    fn parse_events_with_type_filter() {
        let args = to_args(&["export", "events", "--type", "agent.spawned,node.online"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.type_filter, "agent.spawned,node.online");
    }

    #[test]
    fn parse_events_with_agent_filter() {
        let args = to_args(&["export", "events", "--agent", "agent-1"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.agent_filter, "agent-1");
    }

    #[test]
    fn parse_events_with_until() {
        let args = to_args(&["export", "events", "--until", "2026-02-09T12:00:00Z"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.until, Some("2026-02-09T12:00:00Z".to_string()));
    }

    #[test]
    fn parse_events_with_since() {
        let args = to_args(&["export", "events", "--since", "1h"]);
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.since, Some("1h".to_string()));
    }

    #[test]
    fn parse_events_with_watch() {
        let args = to_args(&["export", "events", "--jsonl", "--watch"]);
        let parsed = parse_args(&args).unwrap();
        assert!(parsed.watch);
        assert!(parsed.jsonl);
    }

    #[test]
    fn parse_rejects_json_and_jsonl_together() {
        let args = to_args(&["export", "status", "--json", "--jsonl"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_rejects_unknown_subcommand() {
        let args = to_args(&["export", "bogus"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown export subcommand 'bogus'"));
    }

    #[test]
    fn parse_rejects_unknown_flag() {
        let args = to_args(&["export", "status", "--bogus"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown flag for export"));
    }

    #[test]
    fn parse_rejects_positional_args() {
        let args = to_args(&["export", "status", "extra"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("no positional arguments"));
    }

    #[test]
    fn parse_help_returns_root_help() {
        let args = to_args(&["export", "--help"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Export Forge state"));
    }

    #[test]
    fn parse_status_help_returns_status_help() {
        let args = to_args(&["export", "status", "--help"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Export full status as JSON"));
    }

    #[test]
    fn parse_events_help_returns_events_help() {
        let args = to_args(&["export", "events", "--help"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("Export the event log"));
    }

    #[test]
    fn parse_type_missing_value() {
        let args = to_args(&["export", "events", "--type"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--type requires a value"));
    }

    #[test]
    fn parse_until_missing_value() {
        let args = to_args(&["export", "events", "--until"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--until requires a value"));
    }

    #[test]
    fn parse_agent_missing_value() {
        let args = to_args(&["export", "events", "--agent"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--agent requires a value"));
    }

    #[test]
    fn parse_since_missing_value() {
        let args = to_args(&["export", "events", "--since"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--since requires a value"));
    }

    #[test]
    fn parse_type_on_status_rejected() {
        let args = to_args(&["export", "status", "--type", "foo"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--type is only valid for 'export events'"));
    }

    #[test]
    fn parse_agent_on_status_rejected() {
        let args = to_args(&["export", "status", "--agent", "a"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--agent is only valid for 'export events'"));
    }

    #[test]
    fn parse_until_on_status_rejected() {
        let args = to_args(&["export", "status", "--until", "1h"]);
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--until is only valid for 'export events'"));
    }

    // --- parse_event_types tests ---

    #[test]
    fn parse_event_types_empty() {
        let types = parse_event_types("").unwrap();
        assert!(types.is_empty());
    }

    #[test]
    fn parse_event_types_single() {
        let types = parse_event_types("agent.spawned").unwrap();
        assert_eq!(types, vec!["agent.spawned"]);
    }

    #[test]
    fn parse_event_types_multiple() {
        let types = parse_event_types("agent.spawned, node.online").unwrap();
        assert_eq!(types, vec!["agent.spawned", "node.online"]);
    }

    #[test]
    fn parse_event_types_trims_whitespace() {
        let types = parse_event_types("  agent.spawned , node.online  ").unwrap();
        assert_eq!(types, vec!["agent.spawned", "node.online"]);
    }

    #[test]
    fn parse_event_types_skips_empty_parts() {
        let types = parse_event_types("agent.spawned,,node.online").unwrap();
        assert_eq!(types, vec!["agent.spawned", "node.online"]);
    }

    #[test]
    fn parse_event_types_only_commas_fails() {
        let err = parse_event_types(",,,").unwrap_err();
        assert!(err.contains("event type filter cannot be empty"));
    }

    // --- export status command output tests ---

    #[test]
    fn status_human_output_shows_counts() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Nodes:"));
        assert!(out.stdout.contains("1"));
        assert!(out.stdout.contains("Workspaces:"));
        assert!(out.stdout.contains("Agents:"));
        assert!(out.stdout.contains("2"));
        assert!(out.stdout.contains("Queue items:"));
        assert!(out.stdout.contains("Alerts:"));
        assert!(out
            .stdout
            .contains("Use --json or --jsonl for full export output."));
    }

    #[test]
    fn status_empty_human_output() {
        let backend = default_backend();
        let out = run(&["export", "status"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Nodes:"));
        assert!(out.stdout.contains("0"));
    }

    #[test]
    fn status_json_output() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("workspaces").is_some());
        assert!(parsed.get("agents").is_some());
        assert!(parsed.get("queues").is_some());
        assert!(parsed.get("alerts").is_some());
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["agents"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["queues"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["alerts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn status_json_keys_match_go() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let mut keys: Vec<&str> = parsed
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["agents", "alerts", "nodes", "queues", "workspaces"]
        );
    }

    #[test]
    fn status_jsonl_output() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--jsonl"], &backend);
        assert_success(&out);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert!(parsed.get("nodes").is_some());
    }

    #[test]
    fn status_json_empty() {
        let backend = default_backend();
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["workspaces"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["agents"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["queues"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["alerts"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn status_json_node_fields() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let node = &parsed["nodes"][0];
        assert_eq!(node["id"], "node-1");
        assert_eq!(node["name"], "local");
        assert_eq!(node["status"], "online");
        assert_eq!(node["is_local"], true);
        assert_eq!(node["agent_count"], 2);
        // ssh_target is None → should be absent (skip_serializing_if)
        assert!(node.get("ssh_target").is_none());
    }

    #[test]
    fn status_json_agent_fields() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let agent = &parsed["agents"][0];
        assert_eq!(agent["id"], "agent-1");
        assert_eq!(agent["workspace_id"], "ws-1");
        assert_eq!(agent["state"], "working");
        assert_eq!(agent["type"], "claude");
        assert_eq!(agent["queue_length"], 3);
    }

    #[test]
    fn status_json_alert_fields() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let alert = &parsed["alerts"][0];
        assert_eq!(alert["type"], "cooldown");
        assert_eq!(alert["severity"], "warning");
        assert_eq!(alert["message"], "Rate limit approaching");
        assert_eq!(alert["agent_id"], "agent-1");
    }

    #[test]
    fn status_error_propagated() {
        let backend = default_backend().with_status_error("database unavailable");
        let out = run(&["export", "status"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("database unavailable"));
    }

    // --- export events command output tests ---

    #[test]
    fn events_human_output_shows_count() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events"], &backend);
        assert_success(&out);
        assert!(out.stdout.contains("Events:"));
        assert!(out.stdout.contains("3"));
        assert!(out
            .stdout
            .contains("Use --json or --jsonl for full export output."));
    }

    #[test]
    fn events_json_output() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 3);
    }

    #[test]
    fn events_json_event_fields() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let event = &parsed[0];
        assert_eq!(event["id"], "evt-1");
        assert_eq!(event["timestamp"], "2026-02-09T10:00:00Z");
        assert_eq!(event["type"], "agent.spawned");
        assert_eq!(event["entity_type"], "agent");
        assert_eq!(event["entity_id"], "agent-1");
        // payload is None → absent
        assert!(event.get("payload").is_none());
    }

    #[test]
    fn events_json_event_with_payload() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let event = &parsed[1];
        assert_eq!(event["payload"]["new_state"], "working");
    }

    #[test]
    fn events_jsonl_output() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events", "--jsonl"], &backend);
        assert_success(&out);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 3);
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("id").is_some());
            assert!(parsed.get("type").is_some());
        }
    }

    #[test]
    fn events_empty_json() {
        let backend = default_backend();
        let out = run(&["export", "events", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[test]
    fn events_empty_jsonl() {
        let backend = default_backend();
        let out = run(&["export", "events", "--jsonl"], &backend);
        assert_success(&out);
        assert!(out.stdout.trim().is_empty());
    }

    #[test]
    fn events_type_filter() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &["export", "events", "--json", "--type", "agent.spawned"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        assert_eq!(parsed[0]["type"], "agent.spawned");
    }

    #[test]
    fn events_type_filter_multiple() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &[
                "export",
                "events",
                "--json",
                "--type",
                "agent.spawned,node.online",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_agent_filter() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &["export", "events", "--json", "--agent", "agent-1"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        // agent-1 has 2 events (evt-1, evt-2)
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_agent_filter_no_match() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &["export", "events", "--json", "--agent", "nonexistent"],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[test]
    fn events_since_filter() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &[
                "export",
                "events",
                "--json",
                "--since",
                "2026-02-09T10:05:00Z",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        // Should include evt-2 (10:05) and evt-3 (10:10)
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_until_filter() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &[
                "export",
                "events",
                "--json",
                "--until",
                "2026-02-09T10:05:00Z",
            ],
            &backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        // Should include evt-1 (10:00) and evt-2 (10:05)
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn events_since_before_until_error() {
        let backend = default_backend().with_events(sample_events());
        let out = run(
            &[
                "export",
                "events",
                "--json",
                "--since",
                "2026-02-09T12:00:00Z",
                "--until",
                "2026-02-09T10:00:00Z",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--since must be before --until"));
    }

    #[test]
    fn events_watch_requires_jsonl() {
        let backend = default_backend();
        let out = run(&["export", "events", "--watch"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--watch requires --jsonl"));
    }

    #[test]
    fn events_watch_rejects_until() {
        let backend = default_backend();
        let out = run(
            &["export", "events", "--jsonl", "--watch", "--until", "1h"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--until cannot be used with --watch"));
    }

    #[test]
    fn events_error_propagated() {
        let backend = default_backend().with_events_error("query failed");
        let out = run(&["export", "events", "--json"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("query failed"));
    }

    // --- help output tests ---

    #[test]
    fn help_on_export() {
        let backend = default_backend();
        let out = run(&["export", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Export Forge state"));
        assert!(out.stderr.contains("Available Commands"));
        assert!(out.stderr.contains("events"));
        assert!(out.stderr.contains("status"));
    }

    #[test]
    fn help_on_export_no_subcommand() {
        let backend = default_backend();
        let out = run(&["export"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Export Forge state"));
    }

    #[test]
    fn help_on_status_subcommand() {
        let backend = default_backend();
        let out = run(&["export", "status", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Export full status as JSON"));
    }

    #[test]
    fn help_on_events_subcommand() {
        let backend = default_backend();
        let out = run(&["export", "events", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Export the event log"));
        assert!(out.stderr.contains("--type"));
        assert!(out.stderr.contains("--agent"));
        assert!(out.stderr.contains("--until"));
    }

    // --- filter_events_by_type tests ---

    #[test]
    fn filter_events_by_type_no_filter_returns_all() {
        let events = sample_events();
        let result = filter_events_by_type(&events, &[]);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_events_by_type_single_returns_all() {
        let events = sample_events();
        let result = filter_events_by_type(&events, &["agent.spawned".to_string()]);
        // Single type: no client-side filtering (handled by DB query).
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_events_by_type_multi_filters() {
        let events = sample_events();
        let result = filter_events_by_type(
            &events,
            &["agent.spawned".to_string(), "node.online".to_string()],
        );
        assert_eq!(result.len(), 2);
    }

    // --- JSON golden structure tests ---

    #[test]
    fn status_json_golden_structure() {
        let backend = default_backend().with_status(sample_status());
        let out = run(&["export", "status", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

        // Top-level keys
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("workspaces").is_some());
        assert!(parsed.get("agents").is_some());
        assert!(parsed.get("queues").is_some());
        assert!(parsed.get("alerts").is_some());

        // Node structure
        let node = &parsed["nodes"][0];
        assert!(node.get("id").is_some());
        assert!(node.get("name").is_some());
        assert!(node.get("status").is_some());
        assert!(node.get("is_local").is_some());
        assert!(node.get("agent_count").is_some());

        // Workspace structure
        let ws = &parsed["workspaces"][0];
        assert!(ws.get("id").is_some());
        assert!(ws.get("name").is_some());
        assert!(ws.get("node_id").is_some());
        assert!(ws.get("status").is_some());
        assert!(ws.get("agent_count").is_some());

        // Agent structure
        let agent = &parsed["agents"][0];
        assert!(agent.get("id").is_some());
        assert!(agent.get("workspace_id").is_some());
        assert!(agent.get("state").is_some());
        assert!(agent.get("type").is_some());
        assert!(agent.get("queue_length").is_some());

        // Queue item structure
        let qi = &parsed["queues"][0];
        assert!(qi.get("id").is_some());
        assert!(qi.get("agent_id").is_some());
        assert!(qi.get("type").is_some());
        assert!(qi.get("position").is_some());
        assert!(qi.get("status").is_some());

        // Alert structure
        let alert = &parsed["alerts"][0];
        assert!(alert.get("type").is_some());
        assert!(alert.get("severity").is_some());
        assert!(alert.get("message").is_some());
    }

    #[test]
    fn events_json_golden_structure() {
        let backend = default_backend().with_events(sample_events());
        let out = run(&["export", "events", "--json"], &backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

        assert!(parsed.is_array());
        let event = &parsed[0];
        assert!(event.get("id").is_some());
        assert!(event.get("timestamp").is_some());
        assert!(event.get("type").is_some());
        assert!(event.get("entity_type").is_some());
        assert!(event.get("entity_id").is_some());
    }
}

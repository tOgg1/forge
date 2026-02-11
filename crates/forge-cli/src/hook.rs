use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Output from running the hook command (test helper).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// ---------------------------------------------------------------------------
// Hook types
// ---------------------------------------------------------------------------

/// How a hook is executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookKind {
    Command,
    Webhook,
}

/// A registered event hook, matching Go's `hooks.Hook`.
#[derive(Debug, Clone, Serialize)]
pub struct Hook {
    pub id: String,
    pub kind: HookKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub event_types: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entity_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Backend trait (dependency injection for testing)
// ---------------------------------------------------------------------------

/// Abstraction over hook-store persistence.
pub trait HookBackend {
    fn home_dir(&self) -> Result<PathBuf, String>;
    fn store_path(&self) -> Option<String>;
    fn read_store(&self, path: &Path) -> Result<Option<String>, String>;
    fn write_store(&self, path: &Path, contents: &str) -> Result<(), String>;
    fn generate_id(&self) -> String;
    fn now_rfc3339(&self) -> String;
}

/// Real filesystem backend.
pub struct FilesystemHookBackend;

impl HookBackend for FilesystemHookBackend {
    fn home_dir(&self) -> Result<PathBuf, String> {
        std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| "failed to get home directory".to_string())
    }

    fn store_path(&self) -> Option<String> {
        None
    }

    fn read_store(&self, path: &Path) -> Result<Option<String>, String> {
        match std::fs::read_to_string(path) {
            Ok(data) => Ok(Some(data)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(format!("failed to read hook store: {err}")),
        }
    }

    fn write_store(&self, path: &Path, contents: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create hook store directory: {err}"))?;
        }
        std::fs::write(path, contents).map_err(|err| format!("failed to write hook store: {err}"))
    }

    fn generate_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn now_rfc3339(&self) -> String {
        // chrono not available; use a simple approach
        // The Go code uses time.Now().UTC() which produces RFC3339.
        // We'll delegate to the backend so tests can inject deterministic values.
        // For production we need a real timestamp.
        // Since we don't have chrono, use a system call approach.
        use std::time::SystemTime;
        let now = SystemTime::now();
        let duration = match now.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(d) => d,
            Err(_) => return "1970-01-01T00:00:00Z".to_string(),
        };
        let secs = duration.as_secs();
        // Simple UTC timestamp formatting
        let days = secs / 86400;
        let time_secs = secs % 86400;
        let hours = time_secs / 3600;
        let minutes = (time_secs % 3600) / 60;
        let seconds = time_secs % 60;

        // Convert days since epoch to date
        let (year, month, day) = days_to_date(days);
        format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
    }
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant
    let z = days_since_epoch + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// In-memory backend for testing.
#[derive(Default)]
pub struct InMemoryHookBackend {
    pub home: Option<PathBuf>,
    pub config_store_path: Option<String>,
    pub store_contents: std::cell::RefCell<Option<String>>,
    pub written_path: std::cell::RefCell<Option<PathBuf>>,
    pub written_contents: std::cell::RefCell<Option<String>>,
    pub fixed_id: Option<String>,
    pub fixed_timestamp: Option<String>,
}

impl HookBackend for InMemoryHookBackend {
    fn home_dir(&self) -> Result<PathBuf, String> {
        self.home
            .clone()
            .ok_or_else(|| "failed to get home directory".to_string())
    }

    fn store_path(&self) -> Option<String> {
        self.config_store_path.clone()
    }

    fn read_store(&self, _path: &Path) -> Result<Option<String>, String> {
        Ok(self.store_contents.borrow().clone())
    }

    fn write_store(&self, path: &Path, contents: &str) -> Result<(), String> {
        *self.written_path.borrow_mut() = Some(path.to_path_buf());
        *self.written_contents.borrow_mut() = Some(contents.to_string());
        // Also update the in-memory store so subsequent reads reflect the write
        *self.store_contents.borrow_mut() = Some(contents.to_string());
        Ok(())
    }

    fn generate_id(&self) -> String {
        self.fixed_id
            .clone()
            .unwrap_or_else(|| "test-hook-id".to_string())
    }

    fn now_rfc3339(&self) -> String {
        self.fixed_timestamp
            .clone()
            .unwrap_or_else(|| "2026-01-15T12:00:00Z".to_string())
    }
}

// ---------------------------------------------------------------------------
// Internal store data structure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct HookFile {
    hooks: Vec<StoredHook>,
}

/// On-disk representation (uses snake_case JSON like Go).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct StoredHook {
    id: String,
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    event_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entity_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entity_id: Option<String>,
    #[serde(default)]
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

const VALID_ENTITY_TYPES: &[&str] = &["node", "workspace", "agent", "queue", "account", "system"];

const DEFAULT_TIMEOUT: &str = "30s";

#[derive(Debug, Clone)]
struct ParsedOnEventArgs {
    cmd: String,
    url: String,
    headers: Vec<String>,
    event_types: String,
    entity_type: String,
    entity_id: String,
    timeout: String,
    disabled: bool,
    json: bool,
    jsonl: bool,
}

#[derive(Debug, Clone)]
enum SubCommand {
    Help,
    OnEvent(ParsedOnEventArgs),
}

#[derive(Debug, Clone)]
struct ParsedArgs {
    sub: SubCommand,
}

fn take_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(|v| v.as_str())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let start = if args.first().is_some_and(|a| a == "hook") {
        1
    } else {
        0
    };

    let subcommand = args.get(start).map(|s| s.as_str());

    match subcommand {
        None | Some("help") | Some("-h") | Some("--help") => Ok(ParsedArgs {
            sub: SubCommand::Help,
        }),
        Some("on-event") => parse_on_event_args(&args[start + 1..]),
        Some(other) => Err(format!("unknown hook subcommand: {other}")),
    }
}

fn parse_on_event_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut cmd = String::new();
    let mut url = String::new();
    let mut headers: Vec<String> = Vec::new();
    let mut event_types = String::new();
    let mut entity_type = String::new();
    let mut entity_id = String::new();
    let mut timeout = DEFAULT_TIMEOUT.to_string();
    let mut disabled = false;
    let mut json = false;
    let mut jsonl = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--cmd" => {
                cmd = take_value(args, idx, "--cmd")?.to_string();
                idx += 2;
            }
            "--url" => {
                url = take_value(args, idx, "--url")?.to_string();
                idx += 2;
            }
            "--header" => {
                headers.push(take_value(args, idx, "--header")?.to_string());
                idx += 2;
            }
            "--type" => {
                event_types = take_value(args, idx, "--type")?.to_string();
                idx += 2;
            }
            "--entity-type" => {
                entity_type = take_value(args, idx, "--entity-type")?.to_string();
                idx += 2;
            }
            "--entity-id" => {
                entity_id = take_value(args, idx, "--entity-id")?.to_string();
                idx += 2;
            }
            "--timeout" => {
                timeout = take_value(args, idx, "--timeout")?.to_string();
                idx += 2;
            }
            "--disabled" => {
                disabled = true;
                idx += 1;
            }
            "--json" => {
                json = true;
                idx += 1;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
            }
            other => {
                return Err(format!("unexpected argument: {other}"));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs {
        sub: SubCommand::OnEvent(ParsedOnEventArgs {
            cmd,
            url,
            headers,
            event_types,
            entity_type,
            entity_id,
            timeout,
            disabled,
            json,
            jsonl,
        }),
    })
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn parse_event_types(raw: &str) -> Result<Vec<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let types: Vec<String> = trimmed
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if types.is_empty() {
        return Err("event type filter cannot be empty".to_string());
    }

    Ok(types)
}

fn parse_entity_types(raw: &str) -> Result<Vec<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if !VALID_ENTITY_TYPES.contains(&trimmed) {
        return Err(format!("invalid entity type: {trimmed}"));
    }

    Ok(vec![trimmed.to_string()])
}

fn parse_headers(values: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut headers = BTreeMap::new();
    for raw in values {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(format!("invalid header {raw:?} (expected key=value)"));
        }
        let key = parts[0].trim();
        let value = parts[1].trim();
        if key.is_empty() {
            return Err(format!("invalid header {raw:?} (empty key)"));
        }
        headers.insert(key.to_string(), value.to_string());
    }
    Ok(headers)
}

fn validate_timeout(timeout: &str) -> Result<(), String> {
    let trimmed = timeout.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Ok(());
    }

    // Parse Go-style duration: digits followed by unit (s, m, h)
    parse_duration_str(trimmed)?;
    Ok(())
}

fn parse_duration_str(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("invalid --timeout value: empty string".to_string());
    }

    let mut total_ms: u64 = 0;
    let mut current_num = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            current_num.push(ch);
        } else {
            if current_num.is_empty() {
                return Err(format!("invalid --timeout value: {s}"));
            }
            let num: f64 = current_num
                .parse()
                .map_err(|_| format!("invalid --timeout value: {s}"))?;
            current_num.clear();

            let multiplier: f64 = match ch {
                'h' => 3_600_000.0,
                'm' => 60_000.0,
                's' => 1_000.0,
                _ => return Err(format!("invalid --timeout value: {s}")),
            };
            total_ms += (num * multiplier) as u64;
        }
    }

    // If there's a trailing number with no unit, it's invalid for Go durations
    if !current_num.is_empty() {
        return Err(format!("invalid --timeout value: {s}"));
    }

    Ok(total_ms)
}

// ---------------------------------------------------------------------------
// Store path resolution
// ---------------------------------------------------------------------------

fn resolve_store_path(backend: &dyn HookBackend) -> Result<PathBuf, String> {
    // If the backend provides a configured store path, use it
    if let Some(path) = backend.store_path() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    // Default: ~/.config/forge/hooks.json
    let home = backend.home_dir()?;
    Ok(home.join(".config").join("forge").join("hooks.json"))
}

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn HookBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.sub {
        SubCommand::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        SubCommand::OnEvent(on_event) => execute_on_event(&on_event, backend, stdout),
    }
}

fn execute_on_event(
    args: &ParsedOnEventArgs,
    backend: &dyn HookBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let command = args.cmd.trim().to_string();
    let url = args.url.trim().to_string();

    // Exactly one of --cmd or --url is required
    if command.is_empty() == url.is_empty() {
        return Err("exactly one of --cmd or --url is required".to_string());
    }

    // Validate timeout
    validate_timeout(&args.timeout)?;

    // Parse filters
    let event_types = parse_event_types(&args.event_types)?;
    let entity_types = parse_entity_types(&args.entity_type)?;
    let headers = parse_headers(&args.headers)?;

    // Determine kind
    let kind = if !url.is_empty() {
        HookKind::Webhook
    } else {
        HookKind::Command
    };

    let entity_id_trimmed = args.entity_id.trim().to_string();
    let timeout_trimmed = args.timeout.trim().to_string();

    let now = backend.now_rfc3339();
    let id = backend.generate_id();

    let hook = Hook {
        id: id.clone(),
        kind,
        command: if command.is_empty() {
            None
        } else {
            Some(command)
        },
        url: if url.is_empty() { None } else { Some(url) },
        headers: headers.clone(),
        event_types: event_types.clone(),
        entity_types: entity_types.clone(),
        entity_id: if entity_id_trimmed.is_empty() {
            None
        } else {
            Some(entity_id_trimmed.clone())
        },
        enabled: !args.disabled,
        timeout: if timeout_trimmed.is_empty() {
            None
        } else {
            Some(timeout_trimmed.clone())
        },
        created_at: Some(now.clone()),
        updated_at: Some(now.clone()),
    };

    // Load existing store, append, save
    let store_path = resolve_store_path(backend)?;
    let mut hooks = load_hooks(backend, &store_path)?;

    let stored = StoredHook {
        id: hook.id.clone(),
        kind: match &hook.kind {
            HookKind::Command => "command".to_string(),
            HookKind::Webhook => "webhook".to_string(),
        },
        command: hook.command.clone(),
        url: hook.url.clone(),
        headers,
        event_types,
        entity_types,
        entity_id: if entity_id_trimmed.is_empty() {
            None
        } else {
            Some(entity_id_trimmed)
        },
        enabled: hook.enabled,
        timeout: if timeout_trimmed.is_empty() {
            None
        } else {
            Some(timeout_trimmed)
        },
        created_at: Some(now.clone()),
        updated_at: Some(now),
    };

    hooks.push(stored);
    save_hooks(backend, &store_path, &hooks)?;

    // Output
    if args.json || args.jsonl {
        write_json_output(stdout, &hook, args.jsonl)?;
    } else {
        writeln!(stdout, "Hook registered: {id}").map_err(|err| err.to_string())?;
        writeln!(stdout, "Store: {}", store_path.display()).map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn load_hooks(backend: &dyn HookBackend, path: &Path) -> Result<Vec<StoredHook>, String> {
    match backend.read_store(path)? {
        None => Ok(Vec::new()),
        Some(data) => {
            let file: HookFile = serde_json::from_str(&data)
                .map_err(|err| format!("failed to parse hook store: {err}"))?;
            Ok(file.hooks)
        }
    }
}

fn save_hooks(backend: &dyn HookBackend, path: &Path, hooks: &[StoredHook]) -> Result<(), String> {
    let file = HookFile {
        hooks: hooks.to_vec(),
    };
    let data = serde_json::to_string_pretty(&file)
        .map_err(|err| format!("failed to serialize hook store: {err}"))?;
    backend.write_store(path, &data)
}

fn write_json_output(
    output: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        let line = serde_json::to_string(value).map_err(|err| err.to_string())?;
        writeln!(output, "{line}").map_err(|err| err.to_string())?;
    } else {
        let text = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
        writeln!(output, "{text}").map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Manage event hooks")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Register scripts or webhooks that run when Forge emits events."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  on-event  Register a hook for events")?;
    writeln!(stdout)?;
    writeln!(stdout, "on-event Flags:")?;
    writeln!(
        stdout,
        "      --cmd string          command to execute for matching events"
    )?;
    writeln!(
        stdout,
        "      --url string          webhook URL to POST matching events"
    )?;
    writeln!(
        stdout,
        "      --header key=value    webhook header (repeatable)"
    )?;
    writeln!(
        stdout,
        "      --type string         filter by event type (comma-separated)"
    )?;
    writeln!(
        stdout,
        "      --entity-type string  filter by entity type (node, workspace, agent, queue, account, system)"
    )?;
    writeln!(stdout, "      --entity-id string    filter by entity ID")?;
    writeln!(
        stdout,
        "      --timeout string      hook execution timeout (default: 30s, 0 to disable)"
    )?;
    writeln!(
        stdout,
        "      --disabled            register hook as disabled"
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn run_with_backend(
    args: &[String],
    backend: &dyn HookBackend,
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

pub fn run_for_test(args: &[&str], backend: &dyn HookBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    let stdout = match String::from_utf8(stdout) {
        Ok(value) => value,
        Err(err) => panic!("stdout should be utf-8: {err}"),
    };
    let stderr = match String::from_utf8(stderr) {
        Ok(value) => value,
        Err(err) => panic!("stderr should be utf-8: {err}"),
    };
    CommandOutput {
        stdout,
        stderr,
        exit_code,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_backend() -> InMemoryHookBackend {
        InMemoryHookBackend {
            home: Some(PathBuf::from("/home/user")),
            fixed_id: Some("test-uuid-1234".to_string()),
            fixed_timestamp: Some("2026-01-15T12:00:00Z".to_string()),
            ..Default::default()
        }
    }

    // -- help --

    #[test]
    fn help_no_args() {
        let backend = test_backend();
        let out = run_for_test(&[], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
        assert!(out.stdout.contains("on-event"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn help_explicit() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn help_dash_h() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "-h"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    // -- on-event with --cmd --

    #[test]
    fn on_event_cmd_text_output() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "on-event", "--cmd", "echo hello"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Hook registered: test-uuid-1234"));
        assert!(out.stdout.contains("Store:"));
        assert!(out.stdout.contains("hooks.json"));
        assert!(out.stderr.is_empty());

        // Verify store was written
        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["hooks"][0]["id"], "test-uuid-1234");
        assert_eq!(parsed["hooks"][0]["kind"], "command");
        assert_eq!(parsed["hooks"][0]["command"], "echo hello");
        assert_eq!(parsed["hooks"][0]["enabled"], true);
    }

    #[test]
    fn on_event_cmd_json_output() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--json", "--cmd", "echo test"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["id"], "test-uuid-1234");
        assert_eq!(parsed["kind"], "command");
        assert_eq!(parsed["command"], "echo test");
        assert_eq!(parsed["enabled"], true);
        assert_eq!(parsed["created_at"], "2026-01-15T12:00:00Z");
    }

    #[test]
    fn on_event_cmd_jsonl_output() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--jsonl", "--cmd", "echo test"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(parsed["id"], "test-uuid-1234");
        assert_eq!(parsed["kind"], "command");
    }

    // -- on-event with --url --

    #[test]
    fn on_event_url_webhook() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--url",
                "https://example.com/webhook",
                "--header",
                "Authorization=Bearer token123",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Hook registered: test-uuid-1234"));

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["hooks"][0]["kind"], "webhook");
        assert_eq!(parsed["hooks"][0]["url"], "https://example.com/webhook");
        assert_eq!(
            parsed["hooks"][0]["headers"]["Authorization"],
            "Bearer token123"
        );
    }

    // -- filters --

    #[test]
    fn on_event_with_filters() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--cmd",
                "notify",
                "--type",
                "agent.spawned,agent.terminated",
                "--entity-type",
                "agent",
                "--entity-id",
                "agent-42",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        let hook = &parsed["hooks"][0];
        assert_eq!(hook["event_types"][0], "agent.spawned");
        assert_eq!(hook["event_types"][1], "agent.terminated");
        assert_eq!(hook["entity_types"][0], "agent");
        assert_eq!(hook["entity_id"], "agent-42");
    }

    // -- disabled flag --

    #[test]
    fn on_event_disabled() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--cmd", "echo", "--disabled"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["hooks"][0]["enabled"], false);
    }

    // -- timeout --

    #[test]
    fn on_event_custom_timeout() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--cmd", "echo", "--timeout", "5m"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["hooks"][0]["timeout"], "5m");
    }

    #[test]
    fn on_event_zero_timeout() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--cmd", "echo", "--timeout", "0"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn on_event_invalid_timeout() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--cmd", "echo", "--timeout", "xyz"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("invalid --timeout value"));
    }

    // -- error cases --

    #[test]
    fn no_cmd_or_url() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "on-event"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("exactly one of --cmd or --url is required"));
    }

    #[test]
    fn both_cmd_and_url() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--cmd",
                "echo",
                "--url",
                "https://example.com",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("exactly one of --cmd or --url is required"));
    }

    #[test]
    fn invalid_entity_type() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--cmd",
                "echo",
                "--entity-type",
                "foobar",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("invalid entity type: foobar"));
    }

    #[test]
    fn invalid_header_format() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--cmd",
                "echo",
                "--header",
                "no-equals-sign",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("invalid header"));
        assert!(out.stderr.contains("expected key=value"));
    }

    #[test]
    fn empty_header_key() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--cmd", "echo", "--header", "=value"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("empty key"));
    }

    #[test]
    fn unknown_subcommand() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown hook subcommand: foobar"));
    }

    #[test]
    fn json_and_jsonl_conflict() {
        let backend = test_backend();
        let out = run_for_test(
            &["hook", "on-event", "--json", "--jsonl", "--cmd", "echo"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    // -- store path resolution --

    #[test]
    fn default_store_path() {
        let backend = test_backend();
        let out = run_for_test(&["hook", "on-event", "--cmd", "echo hello"], &backend);
        assert_eq!(out.exit_code, 0);
        let written_path = backend.written_path.borrow();
        let path = written_path.as_ref().unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/home/user/.config/forge/hooks.json"
        );
    }

    #[test]
    fn custom_store_path() {
        let backend = InMemoryHookBackend {
            home: Some(PathBuf::from("/home/user")),
            config_store_path: Some("/custom/hooks.json".to_string()),
            fixed_id: Some("test-uuid-1234".to_string()),
            fixed_timestamp: Some("2026-01-15T12:00:00Z".to_string()),
            ..Default::default()
        };
        let out = run_for_test(&["hook", "on-event", "--cmd", "echo hello"], &backend);
        assert_eq!(out.exit_code, 0);
        let written_path = backend.written_path.borrow();
        let path = written_path.as_ref().unwrap();
        assert_eq!(path.to_str().unwrap(), "/custom/hooks.json");
    }

    // -- appending to existing store --

    #[test]
    fn appends_to_existing_hooks() {
        let existing = r#"{"hooks":[{"id":"existing-1","kind":"command","command":"echo old","headers":{},"enabled":true}]}"#;
        let backend = InMemoryHookBackend {
            home: Some(PathBuf::from("/home/user")),
            store_contents: std::cell::RefCell::new(Some(existing.to_string())),
            fixed_id: Some("new-hook-id".to_string()),
            fixed_timestamp: Some("2026-01-15T12:00:00Z".to_string()),
            ..Default::default()
        };
        let out = run_for_test(&["hook", "on-event", "--cmd", "echo new"], &backend);
        assert_eq!(out.exit_code, 0);

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        let hooks = parsed["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0]["id"], "existing-1");
        assert_eq!(hooks[1]["id"], "new-hook-id");
    }

    // -- parse helpers --

    #[test]
    fn parse_event_types_comma_separated() {
        let types = parse_event_types("agent.spawned, agent.terminated").unwrap();
        assert_eq!(types, vec!["agent.spawned", "agent.terminated"]);
    }

    #[test]
    fn parse_event_types_empty() {
        let types = parse_event_types("").unwrap();
        assert!(types.is_empty());
    }

    #[test]
    fn parse_entity_types_valid() {
        for entity in VALID_ENTITY_TYPES {
            let types = parse_entity_types(entity).unwrap();
            assert_eq!(types, vec![entity.to_string()]);
        }
    }

    #[test]
    fn parse_entity_types_invalid() {
        let result = parse_entity_types("foobar");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid entity type"));
    }

    #[test]
    fn parse_headers_valid() {
        let headers = parse_headers(&[
            "Content-Type=application/json".to_string(),
            "Authorization=Bearer abc".to_string(),
        ])
        .unwrap();
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer abc");
    }

    #[test]
    fn parse_headers_empty() {
        let headers = parse_headers(&[]).unwrap();
        assert!(headers.is_empty());
    }

    #[test]
    fn validate_timeout_valid_durations() {
        assert!(validate_timeout("30s").is_ok());
        assert!(validate_timeout("5m").is_ok());
        assert!(validate_timeout("1h").is_ok());
        assert!(validate_timeout("1h30m").is_ok());
        assert!(validate_timeout("0").is_ok());
        assert!(validate_timeout("").is_ok());
    }

    #[test]
    fn validate_timeout_invalid() {
        assert!(validate_timeout("xyz").is_err());
        assert!(validate_timeout("123").is_err());
    }

    // -- multiple headers --

    #[test]
    fn on_event_multiple_headers() {
        let backend = test_backend();
        let out = run_for_test(
            &[
                "hook",
                "on-event",
                "--url",
                "https://example.com",
                "--header",
                "X-Custom=one",
                "--header",
                "X-Other=two",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);

        let contents = backend.written_contents.borrow();
        let data = contents.as_ref().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["hooks"][0]["headers"]["X-Custom"], "one");
        assert_eq!(parsed["hooks"][0]["headers"]["X-Other"], "two");
    }
}

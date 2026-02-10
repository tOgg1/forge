//! fmail-cli: command-line interface surface for fmail.

use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "fmail-cli"
}

static VERSION: OnceLock<String> = OnceLock::new();

/// Set the version string for `--version` output.
pub fn set_version(version: &str) {
    let _ = VERSION.set(version.to_string());
}

fn get_version() -> &'static str {
    VERSION.get().map(|s| s.as_str()).unwrap_or("dev")
}

fn help_text() -> String {
    "\
fmail sends and receives messages via .fmail/ files.

Usage:
  fmail [command]

Available Commands:
  completion  Generate the autocompletion script for the specified shell
  gc          Remove old messages
  help        Help about any command
  init        Initialize a project mailbox
  log         View recent messages
  messages    View all public messages (topics and direct messages)
  register    Request a unique agent name
  send        Send a message to a topic or agent
  status      Show or set your status
  topics      List topics with activity
  watch       Stream messages as they arrive
  who         List known agents

Flags:
  -h, --help         help for fmail
      --robot-help   Machine-readable help output
  -v, --version      version for fmail

Use \"fmail [command] --help\" for more information about a command.\n"
        .to_string()
}

fn robot_help_json() -> String {
    let version = get_version();
    let normalized = normalize_robot_help_version(version);
    // Build the robot-help JSON matching Go output structure.
    let payload = serde_json::json!({
        "name": "fmail",
        "version": normalized,
        "description": "Agent-to-agent messaging via .fmail/ files",
        "setup": "export FMAIL_AGENT=<your-name>",
        "commands": {
            "send": {
                "usage": "fmail send <topic|@agent> <message>",
                "flags": ["-f FILE", "--reply-to ID", "--priority low|normal|high"],
                "examples": [
                    "fmail send task 'implement auth'",
                    "fmail send @reviewer 'check PR #42'"
                ]
            },
            "log": {
                "usage": "fmail log [topic|@agent] [-n N] [--since TIME]",
                "flags": ["-n LIMIT", "--since TIME", "--from AGENT", "--json", "-f/--follow"],
                "examples": [
                    "fmail log task -n 5",
                    "fmail log @$FMAIL_AGENT --since 1h"
                ]
            },
            "messages": {
                "usage": "fmail messages [-n N] [--since TIME]",
                "flags": ["-n LIMIT", "--since TIME", "--from AGENT", "--json", "-f/--follow"],
                "examples": [
                    "fmail messages -n 50",
                    "fmail messages --since 30m --json"
                ],
                "description": "View all public messages across topics and direct messages"
            },
            "watch": {
                "usage": "fmail watch [topic|@agent] [--timeout T] [--count N]",
                "flags": ["--timeout DURATION", "--count N", "--json"],
                "examples": [
                    "fmail watch task",
                    "fmail watch @$FMAIL_AGENT --count 1 --timeout 2m"
                ]
            },
            "who": {
                "usage": "fmail who [--json]",
                "description": "List agents in project"
            },
            "status": {
                "usage": "fmail status [message] [--clear]",
                "examples": [
                    "fmail status 'working on auth'",
                    "fmail status --clear"
                ]
            },
            "register": {
                "usage": "fmail register [name]",
                "flags": ["--json"],
                "examples": [
                    "fmail register",
                    "fmail register agent-42"
                ]
            },
            "topics": {
                "usage": "fmail topics [--json]",
                "description": "List topics with activity"
            },
            "gc": {
                "usage": "fmail gc [--days N] [--dry-run]"
            }
        },
        "patterns": {
            "request_response": [
                "fmail send @worker 'analyze src/auth.go'",
                "response=$(fmail watch @$FMAIL_AGENT --count 1 --timeout 2m)"
            ],
            "broadcast": "fmail send status 'starting work'",
            "coordinate": [
                "fmail send editing 'src/auth.go'",
                "fmail log editing --since 5m --json | grep -q 'auth.go'"
            ]
        },
        "env": {
            "FMAIL_AGENT": "Your agent name (strongly recommended)",
            "FMAIL_PROJECT": "Project ID for cross-host sync",
            "FMAIL_ROOT": "Project directory (auto-detected)"
        },
        "message_format": {
            "body": "string or JSON object",
            "from": "sender agent name",
            "id": "YYYYMMDD-HHMMSS-NNNN",
            "time": "ISO 8601 timestamp",
            "to": "topic or @agent"
        },
        "storage": ".fmail/topics/<topic>/<id>.json and .fmail/dm/<agent>/<id>.json"
    });
    // serde_json::Value serialization is infallible for to_string_pretty.
    #[allow(clippy::expect_used)]
    let mut s = serde_json::to_string_pretty(&payload).expect("Value serialization is infallible");
    s.push('\n');
    s
}

fn normalize_robot_help_version(version: &str) -> String {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("dev") {
        return "2.2.0".to_string();
    }
    trimmed.strip_prefix('v').unwrap_or(trimmed).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait FmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String>;
    fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String>;
    fn now_utc(&self) -> DateTime<Utc>;
    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String>;
    fn set_agent_status(&self, name: &str, status: &str, host: &str)
        -> Result<AgentRecord, String>;
    fn hostname(&self) -> String;
    fn agent_name(&self) -> Result<String, String>;
    fn save_message(&self, message: &mut Message) -> Result<String, String>;
    fn read_file(&self, path: &str) -> Result<String, String>;
    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String>;
    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String>;
    fn read_message_at(&self, path: &std::path::Path) -> Result<Message, String>;
    /// Initialize the project (create .fmail + project.json).
    fn init_project(&self, project_id: Option<&str>) -> Result<(), String>;
    /// Garbage-collect old messages. Returns dry-run output or empty string.
    fn gc_messages(&self, days: i64, dry_run: bool) -> Result<String, String>;
}

pub struct FilesystemFmailBackend;

impl FmailBackend for FilesystemFmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.list_agent_records()
    }

    fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.read_agent_record(name)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        chrono::Utc::now()
    }

    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.register_agent_record(name, host, now)
    }

    fn set_agent_status(
        &self,
        name: &str,
        status: &str,
        host: &str,
    ) -> Result<AgentRecord, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.set_agent_status(name, status, host, now)
    }

    fn hostname(&self) -> String {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    fn agent_name(&self) -> Result<String, String> {
        std::env::var(fmail_core::constants::ENV_AGENT)
            .map_err(|_| "FMAIL_AGENT not set".to_string())
    }

    fn save_message(&self, message: &mut Message) -> Result<String, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.save_message(message, now)
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("read file: {e}"))
    }

    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let topics_dir = store.root().join("topics");
        if !topics_dir.exists() {
            return Ok(None);
        }
        store.list_topics().map(Some)
    }

    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        match target {
            None => store.list_all_message_files(),
            Some(t) if t.starts_with('@') => {
                let agent = t.strip_prefix('@').unwrap_or(t);
                store.list_dm_message_files(agent)
            }
            Some(topic) => store.list_topic_message_files(topic),
        }
    }

    fn read_message_at(&self, path: &std::path::Path) -> Result<Message, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.read_message(path)
    }

    fn init_project(&self, project_id: Option<&str>) -> Result<(), String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.ensure_root()?;

        let existing = store.read_project()?;

        match project_id {
            None => {
                // No explicit project ID: if file exists, done.
                if existing.is_some() {
                    return Ok(());
                }
                let id = fmail_core::project::derive_project_id(&root)?;
                store.ensure_project(&id, chrono::Utc::now())?;
                Ok(())
            }
            Some(id) => {
                // Explicit project ID provided.
                if let Some(ref ex) = existing {
                    if ex.id.trim() == id {
                        return Ok(());
                    }
                }
                let created = existing
                    .and_then(|e| {
                        if e.created == DateTime::<Utc>::default() {
                            None
                        } else {
                            Some(e.created)
                        }
                    })
                    .unwrap_or_else(chrono::Utc::now);

                let project = fmail_core::project::Project {
                    id: id.to_string(),
                    created,
                };
                store.write_project(&project)?;
                Ok(())
            }
        }
    }

    fn gc_messages(&self, days: i64, dry_run: bool) -> Result<String, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let files = store.list_gc_files()?;
        let mut output = String::new();

        for file in &files {
            let mut file_time = file.mod_time;
            if let Some(stem) = file.path.file_stem().and_then(|s| s.to_str()) {
                if let Some(ts) = fmail_core::store::parse_message_time(stem) {
                    file_time = ts;
                }
            }

            if file_time == DateTime::<Utc>::default() || file_time >= cutoff {
                continue;
            }

            if dry_run {
                let display_path = file
                    .path
                    .strip_prefix(store.root())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.path.to_string_lossy().to_string());
                output.push_str(&display_path);
                output.push('\n');
                continue;
            }

            match std::fs::remove_file(&file.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("remove {}: {e}", file.path.display())),
            }
        }

        Ok(output)
    }
}

pub mod completion;
pub(crate) mod duration;
pub mod gc;
pub mod init;
pub mod log;
pub mod messages;
pub mod register;
pub mod send;
pub mod status;
pub mod topics;
pub mod watch;
pub mod who;

#[cfg(test)]
pub(crate) mod tests_common;

pub fn run_cli_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let mut out = CommandOutput {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
    };

    // Check for --robot-help anywhere in args (matches Go behavior).
    if args.contains(&"--robot-help") {
        out.stdout = robot_help_json();
        return out;
    }

    let Some((cmd, rest)) = args.split_first() else {
        // No args: show help (exit 0).
        out.stdout = help_text();
        return out;
    };

    match *cmd {
        "--help" | "-h" | "help" => {
            out.stdout = help_text();
            out
        }
        "--version" | "-v" => {
            out.stdout = format!("fmail version {}\n", get_version());
            out
        }
        "completion" => completion::run_completion_for_test(rest),
        "gc" => gc::run_gc_for_test(rest, backend),
        "init" => init::run_init_for_test(rest, backend),
        "log" | "logs" => log::run_log_for_test(rest, backend),
        "messages" => messages::run_messages_for_test(rest, backend),
        "register" => register::run_register_for_test(rest, backend),
        "send" => send::run_send_for_test(rest, backend),
        "status" => status::run_status_for_test(rest, backend),
        "topics" | "topic" => topics::run_topics_for_test(rest, backend),
        "watch" => watch::run_watch_for_test(rest, backend),
        "who" => who::run_who_for_test(rest, backend),
        _ => {
            out.stderr = format!("Error: unknown command \"{cmd}\" for \"fmail\"\n");
            out.exit_code = 1;
            out
        }
    }
}

pub fn run_cli(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_cli_for_test(&refs, backend)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tests_common::MockFmailBackend;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "fmail-cli");
    }

    #[test]
    fn no_args_shows_help_exit_0() {
        let backend = MockFmailBackend::new();
        let out = run_cli_for_test(&[], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("fmail sends and receives messages"));
        assert!(out.stdout.contains("Available Commands:"));
        assert!(out.stdout.contains("send"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn help_flag_shows_help_exit_0() {
        let backend = MockFmailBackend::new();
        for flag in &["--help", "-h", "help"] {
            let out = run_cli_for_test(&[flag], &backend);
            assert_eq!(out.exit_code, 0, "flag={flag}");
            assert!(
                out.stdout.contains("fmail sends and receives messages"),
                "flag={flag}"
            );
            assert!(out.stderr.is_empty(), "flag={flag}");
        }
    }

    #[test]
    fn version_flag_shows_version_exit_0() {
        let backend = MockFmailBackend::new();
        for flag in &["--version", "-v"] {
            let out = run_cli_for_test(&[flag], &backend);
            assert_eq!(out.exit_code, 0, "flag={flag}");
            assert!(
                out.stdout.starts_with("fmail version "),
                "flag={flag}: got {:?}",
                out.stdout
            );
            assert!(out.stderr.is_empty(), "flag={flag}");
        }
    }

    #[test]
    fn unknown_command_error_format_exit_1() {
        let backend = MockFmailBackend::new();
        let out = run_cli_for_test(&["nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(
            out.stderr,
            "Error: unknown command \"nonexistent\" for \"fmail\"\n"
        );
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn robot_help_returns_json_exit_0() {
        let backend = MockFmailBackend::new();
        let out = run_cli_for_test(&["--robot-help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_str(&out.stdout).expect("robot-help should be valid JSON");
        assert_eq!(parsed["name"], "fmail");
        assert!(parsed["commands"]["send"].is_object());
        assert!(parsed["commands"]["log"].is_object());
    }

    #[test]
    fn robot_help_anywhere_in_args() {
        let backend = MockFmailBackend::new();
        let out = run_cli_for_test(&["send", "--robot-help"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value =
            serde_json::from_str(&out.stdout).expect("robot-help should be valid JSON");
        assert_eq!(parsed["name"], "fmail");
    }

    #[test]
    fn normalize_robot_help_version_cases() {
        assert_eq!(normalize_robot_help_version(""), "2.2.0");
        assert_eq!(normalize_robot_help_version("dev"), "2.2.0");
        assert_eq!(normalize_robot_help_version("DEV"), "2.2.0");
        assert_eq!(normalize_robot_help_version("v1.2.3"), "1.2.3");
        assert_eq!(normalize_robot_help_version("1.2.3"), "1.2.3");
        assert_eq!(
            normalize_robot_help_version("  v0.2.0-14-gf2169e2  "),
            "0.2.0-14-gf2169e2"
        );
    }
}

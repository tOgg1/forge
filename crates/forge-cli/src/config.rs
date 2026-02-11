use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait ConfigBackend {
    fn home_dir(&self) -> Result<PathBuf, String>;
    fn file_exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String>;
}

pub struct FilesystemConfigBackend;

impl ConfigBackend for FilesystemConfigBackend {
    fn home_dir(&self) -> Result<PathBuf, String> {
        env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| "failed to get home directory".to_string())
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|err| format!("failed to create config directory: {err}"))
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        fs::write(path, contents).map_err(|err| format!("failed to write config file: {err}"))
    }
}

#[derive(Default)]
pub struct InMemoryConfigBackend {
    pub home: Option<PathBuf>,
    pub existing_files: Vec<PathBuf>,
    pub created_dirs: std::cell::RefCell<Vec<PathBuf>>,
    pub written_files: std::cell::RefCell<Vec<(PathBuf, String)>>,
}

impl ConfigBackend for InMemoryConfigBackend {
    fn home_dir(&self) -> Result<PathBuf, String> {
        self.home
            .clone()
            .ok_or_else(|| "failed to get home directory".to_string())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.existing_files.iter().any(|p| p == path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        self.created_dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        self.written_files
            .borrow_mut()
            .push((path.to_path_buf(), contents.to_string()));
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    Init { force: bool },
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: Command,
    json: bool,
    jsonl: bool,
}

pub fn run_from_env_with_backend(backend: &dyn ConfigBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &dyn ConfigBackend) -> CommandOutput {
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

pub fn run_with_backend(
    args: &[String],
    backend: &dyn ConfigBackend,
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
    backend: &dyn ConfigBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Path => {
            let home = backend.home_dir()?;
            let config_path = home.join(".config").join("forge").join("config.yaml");

            if parsed.json || parsed.jsonl {
                write_json_output(
                    stdout,
                    &ConfigPathResult {
                        path: config_path.display().to_string(),
                    },
                    parsed.jsonl,
                )?;
            } else {
                writeln!(stdout, "{}", config_path.display()).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        Command::Init { force } => {
            let home = backend.home_dir()?;
            let config_dir = home.join(".config").join("forge");
            let config_path = config_dir.join("config.yaml");

            if !force && backend.file_exists(&config_path) {
                let result = ConfigInitResult {
                    path: config_path.display().to_string(),
                    created: false,
                    message: Some(
                        "config file already exists (use --force to overwrite)".to_string(),
                    ),
                };
                if parsed.json || parsed.jsonl {
                    write_json_output(stdout, &result, parsed.jsonl)?;
                    return Ok(());
                }
                writeln!(
                    stdout,
                    "Config file already exists: {}",
                    config_path.display()
                )
                .map_err(|err| err.to_string())?;
                writeln!(stdout, "Use --force to overwrite.").map_err(|err| err.to_string())?;
                return Ok(());
            }

            backend.create_dir_all(&config_dir)?;
            backend.write_file(&config_path, DEFAULT_GLOBAL_CONFIG)?;

            let result = ConfigInitResult {
                path: config_path.display().to_string(),
                created: true,
                message: None,
            };

            if parsed.json || parsed.jsonl {
                write_json_output(stdout, &result, parsed.jsonl)?;
                return Ok(());
            }

            writeln!(stdout, "Created config file: {}", config_path.display())
                .map_err(|err| err.to_string())?;
            writeln!(stdout, "\nEdit this file to customize Forge behavior.")
                .map_err(|err| err.to_string())?;
            Ok(())
        }
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            command: Command::Help,
            json: false,
            jsonl: false,
        });
    }

    let start = if args.first().is_some_and(|arg| arg == "config") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut force = false;
    let mut subcommand: Option<String> = None;

    let mut idx = start;
    while idx < args.len() {
        match args[idx].as_str() {
            "--json" => {
                json = true;
                idx += 1;
                continue;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
                continue;
            }
            "--force" | "-f" => {
                force = true;
                idx += 1;
                continue;
            }
            _ => {}
        }

        if subcommand.is_none() {
            subcommand = Some(args[idx].clone());
        } else {
            return Err(format!("unexpected argument: {}", args[idx]));
        }
        idx += 1;
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let command = match subcommand.as_deref() {
        None | Some("help") | Some("-h") | Some("--help") => Command::Help,
        Some("init") => Command::Init { force },
        Some("path") => Command::Path,
        Some(other) => return Err(format!("unknown config subcommand: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
}

#[derive(Debug, Serialize)]
struct ConfigPathResult {
    path: String,
}

#[derive(Debug, Serialize)]
struct ConfigInitResult {
    path: String,
    created: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
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
    writeln!(
        stdout,
        "Manage Forge global configuration at ~/.config/forge/config.yaml."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  init    Create a default global config file")?;
    writeln!(stdout, "  path    Print the global config file path")?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "  -f, --force   Overwrite existing config file (init only)"
    )?;
    Ok(())
}

const DEFAULT_GLOBAL_CONFIG: &str = r#"# Forge Global Configuration
# Location: ~/.config/forge/config.yaml
#
# This file configures global Forge behavior. All settings have sensible defaults,
# so you only need to uncomment and modify the ones you want to change.
#
# Forge also supports environment variables with the FORGE_ prefix:
#   FORGE_LOGGING_LEVEL=debug
#   FORGE_DATABASE_PATH=/custom/path/forge.db

# =============================================================================
# Global Settings
# =============================================================================
global:
  # Where Forge stores runtime data (database, logs, etc.)
  # Default: ~/.local/share/forge
  # data_dir: ~/.local/share/forge

  # Where config files are stored
  # Default: ~/.config/forge
  # config_dir: ~/.config/forge

  # Automatically register the local machine as a node
  # Default: true
  # auto_register_local_node: true

# =============================================================================
# Database Settings
# =============================================================================
database:
  # SQLite database file path (empty = data_dir/forge.db)
  # path: ""

  # Maximum number of database connections
  # Default: 10
  # max_connections: 10

  # How long to wait for a locked database (milliseconds)
  # Default: 5000
  # busy_timeout_ms: 5000

# =============================================================================
# Logging Settings
# =============================================================================
logging:
  # Minimum log level: debug, info, warn, error
  # Default: info
  # level: info

  # Output format: console, json
  # Default: console
  # format: console

  # Optional log file path (empty = stdout only)
  # file: ""

  # Add caller information (file:line) to logs
  # Default: false
  # enable_caller: false

# =============================================================================
# Loop Defaults
# =============================================================================
loop_defaults:
  # Sleep duration between loop iterations
  # Default: 30s
  # interval: 30s

  # Default base prompt path (relative to repo root)
  # prompt: ""

  # Default base prompt message content
  # prompt_msg: ""

# =============================================================================
# Scheduler Settings
# =============================================================================
scheduler:
  # How often the scheduler runs
  # Default: 1s
  # dispatch_interval: 1s

  # Maximum dispatch retry count
  # Default: 3
  # max_retries: 3

  # Base backoff duration for retries
  # Default: 5s
  # retry_backoff: 5s

  # Default cooldown after rate limiting
  # Default: 5m
  # default_cooldown_duration: 5m

  # Automatically rotate accounts on rate limit
  # Default: true
  # auto_rotate_on_rate_limit: true

# =============================================================================
# TUI Settings
# =============================================================================
tui:
  # How often to refresh the display
  # Default: 500ms
  # refresh_interval: 500ms

  # Color theme: default, high-contrast, ocean, sunset
  # Default: default
  # theme: default

  # Show timestamps in the UI
  # Default: true
  # show_timestamps: true

  # Use a more compact layout
  # Default: false
  # compact_mode: false

# =============================================================================
# Node Defaults (for remote SSH nodes)
# =============================================================================
node_defaults:
  # SSH backend: native (Go), system (ssh command), auto
  # Default: auto
  # ssh_backend: auto

  # SSH connection timeout
  # Default: 30s
  # ssh_timeout: 30s

  # Default SSH private key path
  # ssh_key_path: ~/.ssh/id_rsa

  # How often to check node health
  # Default: 60s
  # health_check_interval: 60s

# =============================================================================
# Workspace Defaults
# =============================================================================
workspace_defaults:
  # Prefix for generated tmux session names
  # Default: forge
  # tmux_prefix: forge

  # Default agent type: opencode, claude-code, codex, gemini, generic
  # Default: opencode
  # default_agent_type: opencode

  # Automatically import existing tmux sessions
  # Default: false
  # auto_import_existing: false

# =============================================================================
# Agent Defaults
# =============================================================================
agent_defaults:
  # Default agent type
  # Default: opencode
  # default_type: opencode

  # How often to poll agent state
  # Default: 2s
  # state_polling_interval: 2s

  # How long of no activity before considering agent idle
  # Default: 10s
  # idle_timeout: 10s

  # Max lines to keep in transcript buffer
  # Default: 10000
  # transcript_buffer_size: 10000

  # Approval policy: strict, permissive, custom
  # Default: strict
  # approval_policy: strict

# =============================================================================
# Event Retention
# =============================================================================
event_retention:
  # Enable automatic event cleanup
  # Default: true
  # enabled: true

  # Maximum age of events to keep (e.g., 720h = 30 days)
  # Default: 720h (30 days)
  # max_age: 720h

  # Maximum number of events to keep (0 = no limit)
  # Default: 0
  # max_count: 0

  # How often to run cleanup
  # Default: 1h
  # cleanup_interval: 1h

  # Archive events before deletion
  # Default: false
  # archive_before_delete: false

  # Events per cleanup batch
  # Default: 1000
  # batch_size: 1000

# =============================================================================
# Profiles (harness + auth combinations)
# =============================================================================
# Profiles are typically managed via 'forge profile' commands or imported
# from shell aliases via 'forge profile init'.
#
# Example profile definition:
# profiles:
#   - name: claude-main
#     harness: claude
#     auth_home: ~/.claude
#     prompt_mode: env
#     command_template: claude --dangerously-skip-permissions --verbose --output-format stream-json --include-partial-messages -p "$FORGE_PROMPT_CONTENT"
#     max_concurrency: 1

# =============================================================================
# Pools (groups of profiles for load balancing)
# =============================================================================
# Pools let you group profiles and distribute work across them.
#
# Example pool definition:
# pools:
#   - name: anthropic
#     strategy: round-robin
#     profiles:
#       - claude-main
#       - claude-backup
#
# default_pool: anthropic
"#;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn backend_with_home(home: &str) -> InMemoryConfigBackend {
        InMemoryConfigBackend {
            home: Some(PathBuf::from(home)),
            ..Default::default()
        }
    }

    fn backend_with_existing_config(home: &str) -> InMemoryConfigBackend {
        let home_path = PathBuf::from(home);
        let config_path = home_path.join(".config").join("forge").join("config.yaml");
        InMemoryConfigBackend {
            home: Some(home_path),
            existing_files: vec![config_path],
            ..Default::default()
        }
    }

    // -- help --

    #[test]
    fn help_no_args() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&[], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
        assert!(out.stdout.contains("init"));
        assert!(out.stdout.contains("path"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn help_explicit() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn help_dash_h() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "-h"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    // -- path --

    #[test]
    fn path_text_output() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "path"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("/home/user/.config/forge/config.yaml"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn path_json_output() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "--json", "path"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            parsed["path"].as_str().unwrap(),
            "/home/user/.config/forge/config.yaml"
        );
    }

    #[test]
    fn path_jsonl_output() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "--jsonl", "path"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(
            parsed["path"].as_str().unwrap(),
            "/home/user/.config/forge/config.yaml"
        );
    }

    // -- init --

    #[test]
    fn init_creates_config_text() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "init"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Created config file:"));
        assert!(out.stdout.contains("/home/user/.config/forge/config.yaml"));
        assert!(out.stdout.contains("Edit this file to customize"));
        assert!(out.stderr.is_empty());

        let dirs = backend.created_dirs.borrow();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with(".config/forge"));

        let files = backend.written_files.borrow();
        assert_eq!(files.len(), 1);
        assert!(files[0].0.ends_with(".config/forge/config.yaml"));
        assert!(files[0].1.contains("Forge Global Configuration"));
    }

    #[test]
    fn init_creates_config_json() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "--json", "init"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["created"], true);
        assert_eq!(
            parsed["path"].as_str().unwrap(),
            "/home/user/.config/forge/config.yaml"
        );
        assert!(parsed.get("message").is_none());
    }

    #[test]
    fn init_existing_no_force_text() {
        let backend = backend_with_existing_config("/home/user");
        let out = run_for_test(&["config", "init"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("already exists"));
        assert!(out.stdout.contains("--force"));

        let files = backend.written_files.borrow();
        assert!(files.is_empty());
    }

    #[test]
    fn init_existing_no_force_json() {
        let backend = backend_with_existing_config("/home/user");
        let out = run_for_test(&["config", "--json", "init"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["created"], false);
        assert!(parsed["message"]
            .as_str()
            .unwrap()
            .contains("already exists"));
    }

    #[test]
    fn init_existing_with_force() {
        let backend = backend_with_existing_config("/home/user");
        let out = run_for_test(&["config", "init", "--force"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Created config file:"));

        let files = backend.written_files.borrow();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn init_existing_with_force_short() {
        let backend = backend_with_existing_config("/home/user");
        let out = run_for_test(&["config", "init", "-f"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Created config file:"));

        let files = backend.written_files.borrow();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn init_force_json() {
        let backend = backend_with_existing_config("/home/user");
        let out = run_for_test(&["config", "--json", "init", "--force"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["created"], true);
    }

    // -- error cases --

    #[test]
    fn unknown_subcommand() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown config subcommand: foobar"));
    }

    #[test]
    fn json_and_jsonl_conflict() {
        let backend = backend_with_home("/home/user");
        let out = run_for_test(&["config", "--json", "--jsonl", "path"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn no_home_dir_error() {
        let backend = InMemoryConfigBackend::default();
        let out = run_for_test(&["config", "path"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("failed to get home directory"));
    }

    #[test]
    fn default_config_content_matches_go() {
        // Verify the embedded config template contains expected sections
        assert!(DEFAULT_GLOBAL_CONFIG.contains("# Forge Global Configuration"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("global:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("database:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("logging:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("loop_defaults:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("scheduler:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("tui:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("node_defaults:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("workspace_defaults:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("agent_defaults:"));
        assert!(DEFAULT_GLOBAL_CONFIG.contains("event_retention:"));
    }
}

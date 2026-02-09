use std::env;
use std::io::Write;
use std::sync::OnceLock;

pub mod audit;
pub mod clean;
pub mod completion;
pub mod config;
pub mod error_envelope;
pub mod init;
pub mod kill;
pub mod logs;
pub mod loop_internal;
pub mod mem;
pub mod migrate;
pub mod msg;
pub mod pool;
pub mod profile;
pub mod prompt;
pub mod ps;
pub mod queue;
pub mod resume;
pub mod rm;
pub mod run;
pub mod scale;
pub mod stop;
pub mod up;
pub mod work;

use error_envelope::{handle_cli_error, parse_global_flags, GlobalFlags};

/// Version information set at build time.
static VERSION_STRING: OnceLock<String> = OnceLock::new();

pub fn crate_label() -> &'static str {
    "forge-cli"
}

/// Set the version string for `--version` output.
/// Must be called before `run_from_env`. Format: `"<version> (commit: <hash>, built: <date>)"`.
pub fn set_version(version: &str, commit: &str, date: &str) {
    let formatted = format!("{version} (commit: {commit}, built: {date})");
    let _ = VERSION_STRING.set(formatted);
}

fn get_version() -> &'static str {
    VERSION_STRING
        .get()
        .map(|value| value.as_str())
        .unwrap_or("dev (commit: none, built: unknown)")
}

pub fn run_from_env() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_args(&args, &mut stdout, &mut stderr)
}

pub fn run_with_args(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let (flags, index) = parse_global_flags(args);

    if flags.version {
        let _ = writeln!(stdout, "forge version {}", get_version());
        return 0;
    }

    let remaining = &args[index..];
    let command = remaining.first().map(|arg| arg.as_str());
    match command {
        None | Some("help") | Some("-h") | Some("--help") => {
            if let Err(err) = write_root_help(stdout) {
                let _ = writeln!(stderr, "{err}");
                return 1;
            }
            0
        }
        Some("init") => {
            let backend = init::FilesystemInitBackend;
            let forwarded = forward_args(remaining, &flags);
            init::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("audit") => {
            let backend = audit::InMemoryAuditBackend::default();
            let forwarded = forward_args(remaining, &flags);
            audit::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("kill") => {
            let mut backend = kill::InMemoryKillBackend::default();
            let forwarded = forward_args(remaining, &flags);
            kill::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("loop") => {
            let mut backend = loop_internal::InMemoryLoopInternalBackend::default();
            let forwarded = remaining.to_vec();
            loop_internal::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("logs") | Some("log") => {
            let mut backend = logs::InMemoryLogsBackend::default();
            let forwarded = remaining.to_vec();
            logs::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("clean") => {
            let mut backend = clean::InMemoryLoopBackend::default();
            let forwarded = forward_args(remaining, &flags);
            clean::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("completion") => completion::run(remaining, stdout, stderr),
        Some("config") => {
            let backend = config::FilesystemConfigBackend;
            let forwarded = forward_args(remaining, &flags);
            config::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("migrate") => {
            let forwarded = forward_args(remaining, &flags);
            let mut backend = match migrate::SqliteMigrationBackend::open_from_env() {
                Ok(backend) => backend,
                Err(message) => {
                    return handle_cli_error(&message, &flags, stdout, stderr);
                }
            };
            migrate::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("work") => {
            let mut backend = work::InMemoryWorkBackend::default();
            let forwarded = forward_args(remaining, &flags);
            work::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("prompt") => {
            let mut backend = prompt::FilesystemPromptBackend;
            let forwarded = forward_args(remaining, &flags);
            prompt::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("queue") => {
            let mut backend = queue::InMemoryQueueBackend::default();
            let mut forwarded = remaining.to_vec();
            if flags.json {
                let insertion = if forwarded.len() > 2 {
                    2
                } else {
                    forwarded.len()
                };
                forwarded.insert(insertion, "--json".to_string());
            }
            queue::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("mem") => {
            let mut backend = mem::InMemoryMemBackend::default();
            let forwarded = forward_args(remaining, &flags);
            mem::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("msg") => {
            let mut backend = msg::InMemoryMsgBackend::default();
            let forwarded = forward_args(remaining, &flags);
            msg::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("pool") => {
            let mut backend = pool::InMemoryPoolBackend::default();
            let forwarded = forward_args(remaining, &flags);
            pool::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("profile") => {
            let mut backend = profile::InMemoryProfileBackend::default();
            let forwarded = forward_args(remaining, &flags);
            profile::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("resume") => {
            let mut backend = resume::InMemoryResumeBackend::default();
            let forwarded = forward_args(remaining, &flags);
            resume::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("rm") => {
            let mut backend = rm::InMemoryLoopBackend::default();
            let forwarded = forward_args(remaining, &flags);
            rm::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("run") => {
            let mut backend = run::InMemoryRunBackend::default();
            let forwarded = remaining.to_vec();
            run::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("scale") => {
            let mut backend = scale::InMemoryScaleBackend::default();
            let forwarded = forward_args(remaining, &flags);
            scale::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("ps") | Some("ls") => {
            let backend = ps::InMemoryPsBackend::default();
            let forwarded = forward_args(remaining, &flags);
            ps::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("stop") => {
            let mut backend = stop::InMemoryStopBackend::default();
            let forwarded = forward_args(remaining, &flags);
            stop::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("up") => {
            let mut backend = up::InMemoryUpBackend::default();
            let forwarded = forward_args(remaining, &flags);
            up::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some(other) => {
            let message = format!("unknown forge command: {other}");
            let code = handle_cli_error(&message, &flags, stdout, stderr);
            if !flags.json && !flags.jsonl {
                let _ = write_root_help(stderr);
            }
            code
        }
    }
}

fn forward_args(remaining: &[String], flags: &GlobalFlags) -> Vec<String> {
    let mut out = remaining.to_vec();
    if out.is_empty() {
        return out;
    }

    // Most command parsers accept these flags anywhere; keep deterministic ordering.
    if flags.json {
        out.insert(1, "--json".to_string());
    }
    if flags.jsonl {
        out.insert(1, "--jsonl".to_string());
    }
    if flags.quiet {
        out.insert(1, "--quiet".to_string());
    }
    out
}

fn write_root_help(out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "Control plane for AI coding agents")?;
    writeln!(out)?;
    writeln!(
        out,
        "Forge is a control plane for running and supervising AI coding agents"
    )?;
    writeln!(out, "across multiple repositories and servers.")?;
    writeln!(out)?;
    writeln!(out, "It provides:")?;
    writeln!(
        out,
        "  - A fast TUI dashboard for monitoring agent progress"
    )?;
    writeln!(out, "  - A CLI for automation and scripting")?;
    writeln!(out, "  - Deep integration with tmux and SSH")?;
    writeln!(
        out,
        "  - Multi-account orchestration with cooldown management"
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "Run 'forge' without arguments to launch the TUI dashboard."
    )?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(out, "  forge <command> [options]")?;
    writeln!(out)?;
    writeln!(out, "Commands:")?;
    writeln!(out, "  audit     View the Forge audit log")?;
    writeln!(out, "  clean     Remove inactive loops")?;
    writeln!(out, "  completion  Generate shell completion scripts")?;
    writeln!(out, "  config    Manage global configuration")?;
    writeln!(out, "  init      Initialize a repo for Forge loops")?;
    writeln!(out, "  kill      Kill loops immediately")?;
    writeln!(out, "  logs      Tail loop logs")?;
    writeln!(out, "  migrate   Database migration command family")?;
    writeln!(out, "  mem       Loop memory command family")?;
    writeln!(out, "  msg       Queue a message for loop(s)")?;
    writeln!(out, "  pool      Profile pool command family")?;
    writeln!(out, "  profile   Harness profile command family")?;
    writeln!(out, "  prompt    Loop prompt command family")?;
    writeln!(out, "  ps        List loops")?;
    writeln!(out, "  queue     Manage loop queues")?;
    writeln!(out, "  resume    Resume loop execution")?;
    writeln!(out, "  rm        Remove loop records")?;
    writeln!(out, "  run       Run a single loop iteration")?;
    writeln!(out, "  scale     Scale loops to target count")?;
    writeln!(out, "  stop      Stop loops after current iteration")?;
    writeln!(out, "  up        Start loop(s) for a repo")?;
    writeln!(out, "  work      Loop work-context command family")?;
    writeln!(out)?;
    writeln!(out, "Global Flags:")?;
    writeln!(
        out,
        "      --config string   config file (default is $HOME/.config/forge/config.yaml)"
    )?;
    writeln!(out, "      --json            output in JSON format")?;
    writeln!(
        out,
        "      --jsonl           output in JSON Lines format (for streaming)"
    )?;
    writeln!(
        out,
        "      --watch           watch for changes and stream updates"
    )?;
    writeln!(
        out,
        "      --since string    replay events since duration (e.g., 1h, 30m, 24h) or timestamp"
    )?;
    writeln!(out, "  -v, --verbose         enable verbose output")?;
    writeln!(out, "      --quiet           suppress non-essential output")?;
    writeln!(out, "      --no-color        disable colored output")?;
    writeln!(out, "      --no-progress     disable progress output")?;
    writeln!(
        out,
        "      --non-interactive run without prompts, use defaults"
    )?;
    writeln!(out, "  -y, --yes             skip confirmation prompts")?;
    writeln!(
        out,
        "      --log-level string  override logging level (debug, info, warn, error)"
    )?;
    writeln!(
        out,
        "      --log-format string override logging format (json, console)"
    )?;
    writeln!(
        out,
        "  -C, --chdir string    change working directory for this command"
    )?;
    writeln!(
        out,
        "      --robot-help      show agent-oriented help and exit"
    )?;
    writeln!(out, "      --version         show version information")?;
    Ok(())
}

/// Test-only helper: run CLI with string slices and capture output.
pub fn run_for_test(args: &[&str]) -> RootCommandOutput {
    let owned_args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_args(&owned_args, &mut stdout, &mut stderr);
    RootCommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub struct RootCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{
        audit, clean, completion, config, crate_label, init, kill, logs, loop_internal, mem,
        migrate, msg, pool, profile, prompt, ps, queue, resume, rm, run, run_for_test,
        run_with_args, scale, stop, up, work,
    };

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-cli");
    }

    #[test]
    fn init_module_is_accessible() {
        let _ = init::FilesystemInitBackend;
    }

    #[test]
    fn audit_module_is_accessible() {
        let _ = audit::InMemoryAuditBackend::default();
    }

    #[test]
    fn kill_module_is_accessible() {
        let _ = kill::InMemoryKillBackend::default();
    }

    #[test]
    fn logs_module_is_accessible() {
        let _ = logs::InMemoryLogsBackend::default();
    }

    #[test]
    fn loop_internal_module_is_accessible() {
        let _ = loop_internal::InMemoryLoopInternalBackend::default();
    }

    #[test]
    fn clean_module_is_accessible() {
        let _ = clean::InMemoryLoopBackend::default();
    }

    #[test]
    fn completion_module_is_accessible() {
        let out = completion::run_for_test(&["completion", "bash"]);
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn config_module_is_accessible() {
        let _ = config::InMemoryConfigBackend::default();
    }

    #[test]
    fn migrate_module_is_accessible() {
        let _ = migrate::InMemoryMigrationBackend::default();
    }

    #[test]
    fn work_module_is_accessible() {
        let _ = work::InMemoryWorkBackend::default();
    }

    #[test]
    fn prompt_module_is_accessible() {
        let _ = prompt::FilesystemPromptBackend;
    }

    #[test]
    fn mem_module_is_accessible() {
        let _ = mem::InMemoryMemBackend::default();
    }

    #[test]
    fn msg_module_is_accessible() {
        let _ = msg::InMemoryMsgBackend::default();
    }

    #[test]
    fn pool_module_is_accessible() {
        let _ = pool::InMemoryPoolBackend::default();
    }

    #[test]
    fn profile_module_is_accessible() {
        let _ = profile::InMemoryProfileBackend::default();
    }

    #[test]
    fn queue_module_is_accessible() {
        let _ = queue::InMemoryQueueBackend::default();
    }

    #[test]
    fn rm_module_is_accessible() {
        let _ = rm::InMemoryLoopBackend::default();
    }

    #[test]
    fn run_module_is_accessible() {
        let _ = run::InMemoryRunBackend::default();
    }

    #[test]
    fn scale_module_is_accessible() {
        let _ = scale::InMemoryScaleBackend::default();
    }

    #[test]
    fn resume_module_is_accessible() {
        let _ = resume::InMemoryResumeBackend::default();
    }

    #[test]
    fn ps_module_is_accessible() {
        let _ = ps::InMemoryPsBackend::default();
    }

    #[test]
    fn stop_module_is_accessible() {
        let _ = stop::InMemoryStopBackend::default();
    }

    #[test]
    fn up_module_is_accessible() {
        let _ = up::InMemoryUpBackend::default();
    }

    #[test]
    fn root_help_renders_when_no_command() {
        let args: Vec<String> = Vec::new();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with_args(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let rendered = match String::from_utf8(stdout) {
            Ok(value) => value,
            Err(err) => panic!("stdout should be utf-8: {err}"),
        };
        assert!(rendered.contains("Control plane for AI coding agents"));
        assert!(rendered.contains("audit"));
        assert!(rendered.contains("clean"));
        assert!(rendered.contains("config"));
        assert!(rendered.contains("init"));
        assert!(rendered.contains("kill"));
        assert!(rendered.contains("logs"));
        assert!(rendered.contains("msg"));
        assert!(rendered.contains("pool"));
        assert!(rendered.contains("profile"));
        assert!(rendered.contains("prompt"));
        assert!(rendered.contains("ps"));
        assert!(rendered.contains("queue"));
        assert!(rendered.contains("resume"));
        assert!(rendered.contains("rm"));
        assert!(rendered.contains("run"));
        assert!(rendered.contains("scale"));
        assert!(rendered.contains("stop"));
        assert!(rendered.contains("up"));
        assert!(rendered.contains("work"));
        assert!(rendered.contains("Global Flags:"));
        assert!(rendered.contains("--json"));
        assert!(rendered.contains("--version"));
        assert!(!rendered.contains("loop run"));
    }

    #[test]
    fn version_flag_prints_version() {
        let out = run_for_test(&["--version"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.starts_with("forge version "));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn unknown_command_returns_error() {
        let out = run_for_test(&["nonexistent"]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown forge command: nonexistent"));
        assert!(out.stderr.contains("Commands:"));
    }

    #[test]
    fn unknown_command_json_returns_envelope() {
        let out = run_for_test(&["--json", "nonexistent"]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.is_empty());
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["error"]["code"], "ERR_UNKNOWN");
        assert!(parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent"));
    }

    #[test]
    fn help_flag_returns_help() {
        let out = run_for_test(&["--help"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Control plane for AI coding agents"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn global_flags_parsed_before_command() {
        let out = run_for_test(&["--verbose", "--quiet", "--help"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }
}

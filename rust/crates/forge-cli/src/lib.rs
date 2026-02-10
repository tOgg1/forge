use std::env;
use std::io::Write;
use std::sync::OnceLock;

pub mod audit;
pub mod clean;
pub mod completion;
pub mod config;
pub mod context;
pub mod doctor;
pub mod error_envelope;
pub mod explain;
pub mod export;
pub mod hook;
pub mod init;
pub mod inject;
pub mod kill;
pub mod lock;
pub mod logs;
pub mod loop_internal;
pub mod mail;
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
pub mod send;
pub mod seq;
pub mod skills;
pub mod status;
pub mod stop;
pub mod template;
pub mod tui;
pub mod up;
pub mod wait;
pub mod work;
pub mod workflow;

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
        Some("hook") => {
            let backend = hook::FilesystemHookBackend;
            let forwarded = forward_args(remaining, &flags);
            hook::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("inject") => {
            let mut backend = inject::SqliteInjectBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            inject::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("init") => {
            let backend = init::FilesystemInitBackend;
            let forwarded = forward_args(remaining, &flags);
            init::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("audit") => {
            let backend = audit::SqliteAuditBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            audit::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("kill") => {
            let mut backend = kill::InMemoryKillBackend::default();
            let forwarded = forward_args(remaining, &flags);
            kill::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("lock") => {
            let backend = lock::FilesystemLockBackend::default();
            let forwarded = forward_args(remaining, &flags);
            lock::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("loop") => {
            let mut backend = loop_internal::InMemoryLoopInternalBackend::default();
            let forwarded = remaining.to_vec();
            loop_internal::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("logs") | Some("log") => {
            let mut backend = logs::SqliteLogsBackend::open_from_env();
            let forwarded = remaining.to_vec();
            logs::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("clean") => {
            let mut backend = clean::SqliteCleanBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            clean::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("completion") => completion::run(remaining, stdout, stderr),
        Some("context") => {
            let backend = context::FilesystemContextBackend::default();
            let forwarded = forward_args(remaining, &flags);
            context::run_context(&forwarded, &backend, stdout, stderr)
        }
        Some("use") => {
            let backend = context::FilesystemContextBackend::default();
            let forwarded = forward_args(remaining, &flags);
            context::run_use(&forwarded, &backend, stdout, stderr)
        }
        Some("config") => {
            let backend = config::FilesystemConfigBackend;
            let forwarded = forward_args(remaining, &flags);
            config::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("doctor") => {
            let backend = doctor::FilesystemDoctorBackend::default();
            let forwarded = forward_args(remaining, &flags);
            doctor::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("explain") => {
            let backend = explain::SqliteExplainBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            explain::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("export") => {
            let backend = export::InMemoryExportBackend::default();
            let forwarded = forward_args(remaining, &flags);
            export::run_with_backend(&forwarded, &backend, stdout, stderr)
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
            let mut backend = work::SqliteWorkBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            work::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("prompt") => {
            let mut backend = prompt::FilesystemPromptBackend;
            let forwarded = forward_args(remaining, &flags);
            prompt::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("queue") => {
            let mut backend = queue::SqliteQueueBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            queue::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("mail") => {
            let backend = mail::SqliteMailBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            mail::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("mem") => {
            let mut backend = mem::SqliteMemBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            mem::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("msg") => {
            let mut backend = msg::SqliteMsgBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            msg::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("pool") => {
            let mut backend = pool::InMemoryPoolBackend::default();
            let forwarded = forward_args(remaining, &flags);
            pool::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("profile") => {
            let mut backend = profile::SqliteProfileBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            profile::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("resume") => {
            let mut backend = resume::SqliteResumeBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            resume::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("rm") => {
            let mut backend = rm::SqliteLoopBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            rm::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("run") => {
            let mut backend = run::SqliteRunBackend::open_from_env();
            let forwarded = remaining.to_vec();
            run::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("scale") => {
            let mut backend = scale::SqliteScaleBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            scale::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("ps") | Some("ls") => {
            let backend = ps::SqlitePsBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            ps::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("send") => {
            let mut backend = send::SqliteSendBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            send::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("seq") | Some("sequence") => {
            let mut backend = seq::FilesystemSeqBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            seq::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("skills") => {
            let backend = skills::FilesystemSkillsBackend;
            let forwarded = forward_args(remaining, &flags);
            skills::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("status") => {
            let backend = status::SqliteStatusBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            status::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("template") | Some("tmpl") => {
            let backend = template::FilesystemTemplateBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            template::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("tui") | Some("ui") => {
            let forwarded = forward_args(remaining, &flags);
            tui::run_from_env(&forwarded, stdout, stderr)
        }
        Some("stop") => {
            let mut backend = stop::SqliteStopBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            stop::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("up") => {
            let mut backend = up::InMemoryUpBackend::default();
            let forwarded = forward_args(remaining, &flags);
            up::run_with_backend(&forwarded, &mut backend, stdout, stderr)
        }
        Some("wait") => {
            let backend = wait::SqliteWaitBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            wait::run_with_backend(&forwarded, &backend, stdout, stderr)
        }
        Some("workflow") | Some("wf") => {
            let backend = workflow::FilesystemWorkflowBackend::open_from_env();
            let forwarded = forward_args(remaining, &flags);
            workflow::run_with_backend(&forwarded, &backend, stdout, stderr)
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
    writeln!(out, "  context   Show current context")?;
    writeln!(out, "  config    Manage global configuration")?;
    writeln!(out, "  doctor    Run environment diagnostics")?;
    writeln!(out, "  explain   Explain agent or queue item status")?;
    writeln!(out, "  export    Export Forge data")?;
    writeln!(out, "  hook      Manage event hooks")?;
    writeln!(out, "  inject    Inject message directly into agent")?;
    writeln!(out, "  init      Initialize a repo for Forge loops")?;
    writeln!(out, "  kill      Kill loops immediately")?;
    writeln!(out, "  lock      Manage advisory file locks")?;
    writeln!(out, "  logs      Tail loop logs")?;
    writeln!(out, "  mail      Forge Mail messaging")?;
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
    writeln!(out, "  send      Queue a message for an agent")?;
    writeln!(out, "  skills    Manage workspace skills")?;
    writeln!(out, "  status    Show fleet status summary")?;
    writeln!(out, "  stop      Stop loops after current iteration")?;
    writeln!(out, "  template  Manage message templates")?;
    writeln!(out, "  tui       Launch the Forge TUI")?;
    writeln!(out, "  up        Start loop(s) for a repo")?;
    writeln!(out, "  use       Set current workspace or agent context")?;
    writeln!(out, "  work      Loop work-context command family")?;
    writeln!(out, "  workflow  Manage workflows")?;
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
        audit, clean, completion, config, context, crate_label, doctor, explain, export, hook,
        init, inject, kill, lock, logs, loop_internal, mail, mem, migrate, msg, pool, profile,
        prompt, ps, queue, resume, rm, run, run_for_test, run_with_args, scale, send, seq, skills,
        status, stop, template, tui, up, wait, work, workflow,
    };

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-cli");
    }

    #[test]
    fn context_module_is_accessible() {
        let _ = context::InMemoryContextBackend::default();
    }

    #[test]
    fn doctor_module_is_accessible() {
        let _ = doctor::InMemoryDoctorBackend::default();
    }

    #[test]
    fn explain_module_is_accessible() {
        let _ = explain::SqliteExplainBackend::open_from_env();
    }

    #[test]
    fn export_module_is_accessible() {
        let _ = export::InMemoryExportBackend::default();
    }

    #[test]
    fn hook_module_is_accessible() {
        let _ = hook::InMemoryHookBackend::default();
    }

    #[test]
    fn inject_module_is_accessible() {
        let _ = inject::InMemoryInjectBackend::default();
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
    fn lock_module_is_accessible() {
        let _ = lock::InMemoryLockBackend::default();
    }

    #[test]
    fn logs_module_is_accessible() {
        let _ = logs::InMemoryLogsBackend::default();
    }

    #[test]
    fn mail_module_is_accessible() {
        let _ = mail::InMemoryMailBackend::default();
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
    fn send_module_is_accessible() {
        let _ = send::InMemorySendBackend::default();
    }

    #[test]
    fn skills_module_is_accessible() {
        let _ = skills::InMemorySkillsBackend::default();
    }

    #[test]
    fn status_module_is_accessible() {
        let _ = status::InMemoryStatusBackend::default();
    }

    #[test]
    fn template_module_is_accessible() {
        let _ = template::Template {
            name: "demo".to_string(),
            description: String::new(),
            message: "hello".to_string(),
            variables: Vec::new(),
            tags: Vec::new(),
            source: String::new(),
        };
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
    fn seq_module_is_accessible() {
        let _ = seq::InMemorySeqBackend::default();
    }

    #[test]
    fn tui_module_is_accessible() {
        let _ = tui::InMemoryTuiBackend::default();
    }

    #[test]
    fn wait_module_is_accessible() {
        let _ = wait::InMemoryWaitBackend::default();
    }

    #[test]
    fn workflow_module_is_accessible() {
        let _ = workflow::InMemoryWorkflowBackend::default();
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
        assert!(rendered.contains("doctor"));
        assert!(rendered.contains("explain"));
        assert!(rendered.contains("export"));
        assert!(rendered.contains("hook"));
        assert!(rendered.contains("inject"));
        assert!(rendered.contains("init"));
        assert!(rendered.contains("kill"));
        assert!(rendered.contains("lock"));
        assert!(rendered.contains("logs"));
        assert!(rendered.contains("mail"));
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
        assert!(rendered.contains("send"));
        assert!(rendered.contains("status"));
        assert!(rendered.contains("stop"));
        assert!(rendered.contains("tui"));
        assert!(rendered.contains("up"));
        assert!(rendered.contains("use"));
        assert!(rendered.contains("work"));
        assert!(rendered.contains("workflow"));
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

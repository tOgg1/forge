use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;

use crate::config::load_config;
use crate::runner::Runner;
use crate::sink::{DatabaseEventSink, EventSink, SocketEventSink};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub workspace_id: String,
    pub agent_id: String,
    pub event_socket: String,
    pub prompt_regex: String,
    pub busy_regex: String,
    pub heartbeat: Duration,
    pub tail_lines: usize,
    pub db_path: String,
    pub config_file: String,
    pub log_level: String,
    pub log_format: String,
    pub command: Vec<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            workspace_id: String::new(),
            agent_id: String::new(),
            event_socket: String::new(),
            prompt_regex: String::new(),
            busy_regex: String::new(),
            heartbeat: Duration::from_secs(5),
            tail_lines: 50,
            db_path: String::new(),
            config_file: String::new(),
            log_level: String::new(),
            log_format: String::new(),
            command: Vec::new(),
        }
    }
}

pub fn run_from_env() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_with_args(&args)
}

pub fn run_with_args(argv: &[String]) -> i32 {
    let parsed = match parse_args(argv) {
        Ok(args) => args,
        Err(err) => {
            eprint!("{err}");
            return 2;
        }
    };

    if parsed.workspace_id.is_empty() || parsed.agent_id.is_empty() {
        eprint!("{}", usage(Some("--workspace and --agent are required")));
        return 2;
    }
    if parsed.command.is_empty() {
        eprint!("{}", usage(Some("agent command is required after --")));
        return 2;
    }

    let (mut cfg, _used_path) = match load_config(if parsed.config_file.is_empty() {
        None
    } else {
        Some(parsed.config_file.as_str())
    }) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error loading config: {err}");
            return 1;
        }
    };

    if !parsed.log_level.trim().is_empty() {
        cfg.logging.level = parsed.log_level.trim().to_string();
    }
    if !parsed.log_format.trim().is_empty() {
        cfg.logging.format = parsed.log_format.trim().to_string();
    }

    if let Err(err) = cfg.ensure_directories() {
        eprintln!("Warning: failed to create directories: {err}");
    }

    let sink = match build_event_sink(
        &cfg,
        &parsed.workspace_id,
        &parsed.agent_id,
        &parsed.event_socket,
        &parsed.db_path,
    ) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to initialize event sink: {err}");
            return 1;
        }
    };

    let prompt_re = if parsed.prompt_regex.trim().is_empty() {
        None
    } else {
        match Regex::new(parsed.prompt_regex.trim()) {
            Ok(re) => Some(re),
            Err(err) => {
                eprint!("{}", usage(Some(&format!("invalid --prompt-regex: {err}"))));
                return 2;
            }
        }
    };
    let busy_re = if parsed.busy_regex.trim().is_empty() {
        None
    } else {
        match Regex::new(parsed.busy_regex.trim()) {
            Ok(re) => Some(re),
            Err(err) => {
                eprint!("{}", usage(Some(&format!("invalid --busy-regex: {err}"))));
                return 2;
            }
        }
    };

    let mut runner = Runner::new(
        &parsed.workspace_id,
        &parsed.agent_id,
        parsed.command.clone(),
    );
    runner.prompt_regex = prompt_re;
    runner.busy_regex = busy_re;
    runner.heartbeat_interval = parsed.heartbeat;
    runner.tail_lines = parsed.tail_lines;
    runner.event_sink = sink;
    runner.control_reader = Some(Box::new(std::io::stdin()));
    runner.output_writer = Box::new(std::io::stdout());

    if let Err(err) = runner.run() {
        eprintln!("{err}");
        return 1;
    }
    0
}

fn build_event_sink(
    cfg: &crate::config::Config,
    workspace_id: &str,
    agent_id: &str,
    event_socket: &str,
    db_path: &str,
) -> Result<Arc<dyn EventSink>, String> {
    if !event_socket.trim().is_empty() {
        return Ok(Arc::new(SocketEventSink::connect(event_socket)?));
    }

    let path = if db_path.trim().is_empty() {
        cfg.database_path()
    } else {
        PathBuf::from(db_path.trim())
    };

    Ok(Arc::new(DatabaseEventSink::open(
        &path,
        cfg.database.busy_timeout_ms,
        workspace_id,
        agent_id,
    )?))
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut idx = 0usize;

    while idx < argv.len() {
        let token = &argv[idx];
        if token == "--" {
            out.command = argv[idx + 1..].to_vec();
            return Ok(out);
        }

        // Stop flag parsing at first non-flag (Go flag package behavior).
        if !token.starts_with('-') {
            out.command = argv[idx..].to_vec();
            return Ok(out);
        }

        let (key, inline) = if let Some((k, v)) = token.split_once('=') {
            (k.to_string(), Some(v.to_string()))
        } else {
            (token.to_string(), None)
        };

        match key.as_str() {
            "--workspace" => {
                out.workspace_id = take_value(argv, &mut idx, inline, "--workspace")?;
            }
            "--agent" => {
                out.agent_id = take_value(argv, &mut idx, inline, "--agent")?;
            }
            "--event-socket" => {
                out.event_socket = take_value(argv, &mut idx, inline, "--event-socket")?;
            }
            "--prompt-regex" => {
                out.prompt_regex = take_value(argv, &mut idx, inline, "--prompt-regex")?;
            }
            "--busy-regex" => {
                out.busy_regex = take_value(argv, &mut idx, inline, "--busy-regex")?;
            }
            "--heartbeat" => {
                let raw = take_value(argv, &mut idx, inline, "--heartbeat")?;
                out.heartbeat = parse_duration(&raw)
                    .map_err(|err| usage_with_message(&format!("invalid --heartbeat: {err}")))?;
            }
            "--tail-lines" => {
                let raw = take_value(argv, &mut idx, inline, "--tail-lines")?;
                out.tail_lines = raw
                    .parse::<usize>()
                    .map_err(|_| usage_with_message("invalid --tail-lines"))?;
            }
            "--db-path" => {
                out.db_path = take_value(argv, &mut idx, inline, "--db-path")?;
            }
            "--config" => {
                out.config_file = take_value(argv, &mut idx, inline, "--config")?;
            }
            "--log-level" => {
                out.log_level = take_value(argv, &mut idx, inline, "--log-level")?;
            }
            "--log-format" => {
                out.log_format = take_value(argv, &mut idx, inline, "--log-format")?;
            }
            "-h" | "--help" => return Err(usage(None)),
            other => return Err(usage_with_message(&format!("unknown flag: {other}"))),
        }
        idx += 1;
    }

    Ok(out)
}

fn take_value(
    argv: &[String],
    idx: &mut usize,
    inline: Option<String>,
    flag: &str,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    *idx += 1;
    argv.get(*idx)
        .cloned()
        .ok_or_else(|| usage_with_message(&format!("missing value for {flag}")))
}

fn parse_duration(raw: &str) -> Result<Duration, String> {
    let nanos = crate::runner::parse_go_duration_to_nanos(raw)?;
    if nanos <= 0 {
        return Err("duration must be positive".to_string());
    }
    Ok(Duration::from_nanos(nanos as u64))
}

fn usage_with_message(message: &str) -> String {
    usage(Some(message))
}

fn usage(message: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(msg) = message {
        if !msg.trim().is_empty() {
            out.push_str(&format!("Error: {msg}\n\n"));
        }
    }
    out.push_str("Usage: forge-agent-runner --workspace W --agent A [options] -- <command>\n\n");
    out.push_str("Options:\n");
    out.push_str("  --workspace string      workspace id (required)\n");
    out.push_str("  --agent string          agent id (required)\n");
    out.push_str("  --event-socket string   unix socket path for runner events\n");
    out.push_str("  --prompt-regex string   regex to detect prompt readiness\n");
    out.push_str("  --busy-regex string     regex to detect busy output\n");
    out.push_str("  --heartbeat duration    heartbeat interval (default 5s)\n");
    out.push_str("  --tail-lines int        output lines included in heartbeat (default 50)\n");
    out.push_str("  --db-path string        database path (defaults to config)\n");
    out.push_str(
        "  --config string         config file (default is $HOME/.config/forge/config.yaml)\n",
    );
    out.push_str("  --log-level string      override logging level (debug, info, warn, error)\n");
    out.push_str("  --log-format string     override logging format (json, console)\n");
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Args};

    #[test]
    fn parse_rejects_unknown_flag() {
        let argv = vec!["--bogus".to_string()];
        assert!(parse_args(&argv).is_err());
    }

    #[test]
    fn parse_collects_command_after_double_dash() {
        let argv = vec![
            "--workspace".to_string(),
            "ws".to_string(),
            "--agent".to_string(),
            "ag".to_string(),
            "--".to_string(),
            "echo".to_string(),
            "hi".to_string(),
        ];
        let args = match parse_args(&argv) {
            Ok(value) => value,
            Err(err) => panic!("parse: {err}"),
        };
        assert_eq!(args.workspace_id, "ws");
        assert_eq!(args.agent_id, "ag");
        assert_eq!(args.command, vec!["echo".to_string(), "hi".to_string()]);
    }

    #[test]
    fn parse_stops_at_first_non_flag() {
        let argv = vec![
            "--workspace".to_string(),
            "ws".to_string(),
            "--agent".to_string(),
            "ag".to_string(),
            "echo".to_string(),
            "hi".to_string(),
        ];
        let args = match parse_args(&argv) {
            Ok(value) => value,
            Err(err) => panic!("parse: {err}"),
        };
        assert_eq!(args.command, vec!["echo".to_string(), "hi".to_string()]);
    }

    #[test]
    fn args_defaults_match_go_shape() {
        let args = Args::default();
        assert_eq!(args.tail_lines, 50);
        assert_eq!(args.heartbeat, std::time::Duration::from_secs(5));
    }
}

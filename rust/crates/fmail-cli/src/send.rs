//! fmail send command ported from Go `internal/fmail/send.go`.

use fmail_core::message::{parse_message_body, Message};
use fmail_core::validate::{normalize_tags, normalize_target, validate_priority};

use crate::{CommandOutput, FmailBackend};

/// Run the send command from test arguments.
pub fn run_send_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_send(&owned, backend)
}

fn run_send(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_send(args, backend) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

fn execute_send(
    args: &[String],
    backend: &dyn FmailBackend,
) -> Result<CommandOutput, (i32, String)> {
    let parsed = parse_send_args(args)?;
    let reply_to = parsed.reply_to.trim().to_string();

    let agent = backend.agent_name().map_err(|e| (1, e))?;

    let (normalized_target, _is_dm) =
        normalize_target(&parsed.target).map_err(|e| (1, format!("invalid target: {e}")))?;

    let body = resolve_body(&parsed, backend)?;

    let priority = if parsed.priority_set {
        let p = parsed.priority.trim().to_lowercase();
        let p = if p.is_empty() {
            "normal".to_string()
        } else {
            p
        };
        validate_priority(&p).map_err(|e| (1, e))?;
        p
    } else {
        String::new()
    };

    let normalized_tags =
        normalize_tags(&parsed.tags).map_err(|e| (1, format!("invalid tags: {e}")))?;

    let now = backend.now_utc();
    let host = backend.hostname();

    let mut message = Message {
        id: String::new(),
        from: agent,
        to: normalized_target,
        time: now,
        body,
        reply_to,
        priority,
        host,
        tags: normalized_tags,
    };

    let id = backend.save_message(&mut message).map_err(|e| (1, e))?;

    if parsed.json {
        let json = serde_json::to_string_pretty(&message)
            .map_err(|e| (1, format!("encode message: {e}")))?;
        Ok(CommandOutput {
            stdout: format!("{json}\n"),
            stderr: String::new(),
            exit_code: 0,
        })
    } else {
        Ok(CommandOutput {
            stdout: format!("{id}\n"),
            stderr: String::new(),
            exit_code: 0,
        })
    }
}

fn resolve_body(
    parsed: &ParsedSendArgs,
    backend: &dyn FmailBackend,
) -> Result<serde_json::Value, (i32, String)> {
    let body_arg = parsed.body_arg.trim();
    let file_path = parsed.file.trim();

    if !file_path.is_empty() && !body_arg.is_empty() {
        return Err((
            2,
            "provide either a message argument or --file, not both".to_string(),
        ));
    }

    let raw = if !file_path.is_empty() {
        backend
            .read_file(file_path)
            .map_err(|e| (1, format!("read file: {e}")))?
    } else if !body_arg.is_empty() {
        parsed.body_arg.clone()
    } else {
        String::new()
    };

    if raw.trim().is_empty() {
        return Err((2, "message body is required".to_string()));
    }

    parse_message_body(&raw).map_err(|e| (1, e))
}

#[derive(Debug, Default)]
struct ParsedSendArgs {
    target: String,
    body_arg: String,
    file: String,
    reply_to: String,
    priority: String,
    priority_set: bool,
    tags: Vec<String>,
    json: bool,
}

fn parse_send_args(args: &[String]) -> Result<ParsedSendArgs, (i32, String)> {
    let mut parsed = ParsedSendArgs::default();
    let mut idx = 0usize;
    let mut positional_count = 0u32;

    while idx < args.len() {
        let token = &args[idx];
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err((0, HELP_TEXT.to_string()));
            }
            "--json" => {
                parsed.json = true;
            }
            flag if flag.starts_with("--file=") || flag.starts_with("-f=") => {
                parsed.file = inline_flag_value(flag);
            }
            "-f" | "--file" => {
                idx += 1;
                parsed.file = take_flag_value(args, idx, "--file")?;
            }
            flag if flag.starts_with("--reply-to=") || flag.starts_with("-r=") => {
                parsed.reply_to = inline_flag_value(flag);
            }
            "-r" | "--reply-to" => {
                idx += 1;
                parsed.reply_to = take_flag_value(args, idx, "--reply-to")?;
            }
            flag if flag.starts_with("--priority=") || flag.starts_with("-p=") => {
                parsed.priority = inline_flag_value(flag);
                parsed.priority_set = true;
            }
            "-p" | "--priority" => {
                idx += 1;
                parsed.priority = take_flag_value(args, idx, "--priority")?;
                parsed.priority_set = true;
            }
            flag if flag.starts_with("--tag=") || flag.starts_with("-t=") => {
                add_tag_values(&mut parsed.tags, inline_flag_value(flag));
            }
            "-t" | "--tag" => {
                idx += 1;
                add_tag_values(&mut parsed.tags, take_flag_value(args, idx, "--tag")?);
            }
            flag if flag.starts_with('-') => {
                return Err((2, format!("unknown flag: {flag}")));
            }
            positional => {
                match positional_count {
                    0 => parsed.target = positional.to_string(),
                    1 => parsed.body_arg = positional.to_string(),
                    _ => return Err((2, "too many arguments".to_string())),
                }
                positional_count += 1;
            }
        }
        idx += 1;
    }

    if parsed.target.is_empty() {
        return Err((2, "target is required".to_string()));
    }

    Ok(parsed)
}

fn take_flag_value(args: &[String], idx: usize, flag: &str) -> Result<String, (i32, String)> {
    args.get(idx)
        .cloned()
        .ok_or_else(|| (2, format!("missing value for {flag}")))
}

fn inline_flag_value(flag: &str) -> String {
    flag.split_once('=')
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
}

fn add_tag_values(tags: &mut Vec<String>, raw: String) {
    // Support comma-separated tags like Go.
    for part in raw.split(',') {
        let trimmed = part.trim().to_string();
        if !trimmed.is_empty() {
            tags.push(trimmed);
        }
    }
}

const HELP_TEXT: &str = "\
Send a message to a topic or agent

Usage:
  fmail send <target> [message] [flags]

Arguments:
  target    Topic name or @agent for direct message
  message   Message body (optional if --file is used)

Flags:
  -f, --file string       Read message body from file
  -r, --reply-to string   Reference a previous message ID
  -p, --priority string   Set priority (low, normal, high)
  -t, --tag string        Add tag (repeatable, comma-separated)
      --json              Output result as JSON
  -h, --help              Help for send";

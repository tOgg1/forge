//! fmail log command ported from Go `internal/fmail/log.go`.

use chrono::{DateTime, Utc};
use fmail_core::message::Message;
use fmail_core::validate::normalize_agent_name;

use crate::{CommandOutput, FmailBackend};

/// Run the log command from test arguments.
pub fn run_log_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_log(&owned, backend)
}

fn run_log(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_log(args, backend, false) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

/// Shared implementation for both `log` and `messages` commands.
pub fn execute_log(
    args: &[String],
    backend: &dyn FmailBackend,
    all_messages: bool,
) -> Result<CommandOutput, (i32, String)> {
    let parsed = parse_log_args(args, all_messages)?;

    let now = backend.now_utc();
    let since = parse_since(&parsed.since, now)?;
    let from = normalize_from_filter(&parsed.from)?;

    // Determine target for listing files
    let target = if all_messages {
        None // list all messages (topics + DMs)
    } else {
        parsed.target.as_deref()
    };

    let files = backend
        .list_message_files(target)
        .map_err(|e| (1, format!("list messages: {e}")))?;

    // Load and filter messages
    let mut messages: Vec<Message> = Vec::new();
    for path in &files {
        match backend.read_message_at(path) {
            Ok(msg) => {
                if filter_message(&msg, &since, &from) {
                    messages.push(msg);
                }
            }
            Err(_) => continue, // Skip unreadable messages
        }
    }

    // Sort by ID, then time
    messages.sort_by(|a, b| a.id.cmp(&b.id));

    // Apply limit (keep last N)
    if parsed.limit > 0 && messages.len() > parsed.limit {
        let start = messages.len() - parsed.limit;
        messages = messages[start..].to_vec();
    }

    // Output
    if parsed.json {
        let mut out = String::new();
        for msg in &messages {
            let json = serde_json::to_string_pretty(msg)
                .map_err(|e| (1, format!("encode message: {e}")))?;
            out.push_str(&json);
            out.push('\n');
        }
        return Ok(CommandOutput {
            stdout: out,
            stderr: String::new(),
            exit_code: 0,
        });
    }

    // Text output
    let mut out = String::new();
    for msg in &messages {
        let body_str = format_body(&msg.body);
        out.push_str(&format!(
            "{} {} -> {}: {}\n",
            msg.id, msg.from, msg.to, body_str
        ));
    }

    Ok(CommandOutput {
        stdout: out,
        stderr: String::new(),
        exit_code: 0,
    })
}

fn filter_message(msg: &Message, since: &Option<DateTime<Utc>>, from: &Option<String>) -> bool {
    if let Some(since_time) = since {
        if msg.time < *since_time {
            return false;
        }
    }
    if let Some(from_filter) = from {
        if msg.from.to_lowercase() != *from_filter {
            return false;
        }
    }
    true
}

fn format_body(body: &serde_json::Value) -> String {
    match body {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

#[derive(Debug)]
struct ParsedLogArgs {
    target: Option<String>,
    limit: usize,
    since: String,
    from: String,
    json: bool,
}

fn parse_log_args(args: &[String], no_positional: bool) -> Result<ParsedLogArgs, (i32, String)> {
    let mut parsed = ParsedLogArgs {
        target: None,
        limit: 20,
        since: String::new(),
        from: String::new(),
        json: false,
    };

    let mut idx = 0usize;
    while idx < args.len() {
        let token = &args[idx];
        match token.as_str() {
            "-h" | "--help" | "help" => return Err((0, HELP_TEXT.to_string())),
            "--json" => parsed.json = true,
            "-n" | "--limit" => {
                idx += 1;
                let val = take_flag_value(args, idx, "--limit")?;
                parsed.limit = val
                    .parse::<usize>()
                    .map_err(|_| (2, "limit must be a non-negative integer".to_string()))?;
            }
            "--since" => {
                idx += 1;
                parsed.since = take_flag_value(args, idx, "--since")?;
            }
            "--from" => {
                idx += 1;
                parsed.from = take_flag_value(args, idx, "--from")?;
            }
            "-f" | "--follow" => {
                // Follow mode is acknowledged but not implemented in test mode
            }
            flag if flag.starts_with('-') => {
                return Err((2, format!("unknown flag: {flag}")));
            }
            positional => {
                if no_positional {
                    return Err((2, "messages takes no arguments".to_string()));
                }
                if parsed.target.is_some() {
                    return Err((2, "log takes at most one argument".to_string()));
                }
                parsed.target = Some(positional.to_string());
            }
        }
        idx += 1;
    }

    Ok(parsed)
}

fn take_flag_value(args: &[String], idx: usize, flag: &str) -> Result<String, (i32, String)> {
    args.get(idx)
        .cloned()
        .ok_or_else(|| (2, format!("missing value for {flag}")))
}

/// Parse the `--since` value into a DateTime filter.
fn parse_since(value: &str, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, (i32, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed == "now" {
        return Ok(Some(now));
    }

    // Try duration format: e.g. "1h", "2d", "30m", "3.5d"
    if let Some(dur) = parse_duration_with_days(trimmed) {
        return Ok(Some(now - dur));
    }

    // Try RFC3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }

    // Try date only: YYYY-MM-DD
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        if let Some(ndt) = date.and_hms_opt(0, 0, 0) {
            return Ok(Some(ndt.and_utc()));
        }
    }

    // Try datetime without timezone: YYYY-MM-DDTHH:MM:SS
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Some(ndt.and_utc()));
    }

    Err((1, format!("invalid --since value: {trimmed}")))
}

/// Parse a duration string with days support.
fn parse_duration_with_days(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(num_str) = s.strip_suffix('d') {
        if let Ok(days) = num_str.parse::<f64>() {
            let secs = (days * 86400.0) as i64;
            return chrono::Duration::try_seconds(secs);
        }
    }
    if let Some(num_str) = s.strip_suffix('h') {
        if let Ok(hours) = num_str.parse::<f64>() {
            let secs = (hours * 3600.0) as i64;
            return chrono::Duration::try_seconds(secs);
        }
    }
    if let Some(num_str) = s.strip_suffix('m') {
        if let Ok(mins) = num_str.parse::<f64>() {
            let secs = (mins * 60.0) as i64;
            return chrono::Duration::try_seconds(secs);
        }
    }
    if let Some(num_str) = s.strip_suffix('s') {
        if let Ok(seconds) = num_str.parse::<f64>() {
            return chrono::Duration::try_seconds(seconds as i64);
        }
    }

    None
}

fn normalize_from_filter(from: &str) -> Result<Option<String>, (i32, String)> {
    let trimmed = from.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let raw = trimmed.strip_prefix('@').unwrap_or(trimmed);
    let normalized =
        normalize_agent_name(raw).map_err(|e| (1, format!("invalid --from value: {e}")))?;
    Ok(Some(normalized))
}

const HELP_TEXT: &str = "\
View recent messages

Usage:
  fmail log [topic|@agent] [flags]

Arguments:
  topic|@agent  Topic name or @agent for DM (optional, defaults to all topics)

Flags:
  -n, --limit int       Max messages to show (default: 20)
      --since string    Filter by time window (e.g. 1h, 2d, 2024-01-15T10:30:00Z)
      --from string     Filter by sender
  -f, --follow          Stream new messages (poll-based)
      --json            Output as JSON
  -h, --help            Help for log";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_empty() {
        let now = Utc::now();
        let result = parse_since("", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_none());
    }

    #[test]
    fn parse_since_now() {
        let now = Utc::now();
        let result = parse_since("now", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some());
    }

    #[test]
    fn parse_since_duration_hours() {
        let now = Utc::now();
        let result = parse_since("2h", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some());
    }

    #[test]
    fn parse_since_duration_days() {
        let now = Utc::now();
        let result = parse_since("1d", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some());
    }

    #[test]
    fn parse_since_rfc3339() {
        let now = Utc::now();
        let result = parse_since("2026-02-09T12:00:00Z", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some());
    }

    #[test]
    fn parse_since_date_only() {
        let now = Utc::now();
        let result = parse_since("2026-02-09", now);
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_some());
    }

    #[test]
    fn parse_since_invalid() {
        let now = Utc::now();
        assert!(parse_since("invalid", now).is_err());
    }

    #[test]
    fn parse_duration_with_days_values() {
        assert!(parse_duration_with_days("1d").is_some());
        assert!(parse_duration_with_days("2h").is_some());
        assert!(parse_duration_with_days("30m").is_some());
        assert!(parse_duration_with_days("3.5d").is_some());
        assert!(parse_duration_with_days("invalid").is_none());
    }

    #[test]
    fn format_body_string() {
        let body = serde_json::Value::String("hello".to_string());
        assert_eq!(format_body(&body), "hello");
    }

    #[test]
    fn format_body_object() {
        let body = serde_json::json!({"key": "value"});
        let formatted = format_body(&body);
        assert!(formatted.contains("key"));
    }

    #[test]
    fn normalize_from_empty() {
        let result = normalize_from_filter("");
        assert!(result.is_ok());
        assert!(result.unwrap_or(None).is_none());
    }

    #[test]
    fn normalize_from_strips_at() {
        let result = normalize_from_filter("@alice");
        assert!(result.is_ok());
        assert_eq!(result.unwrap_or(None).as_deref(), Some("alice"));
    }

    #[test]
    fn normalize_from_lowercases() {
        let result = normalize_from_filter("Alice");
        assert!(result.is_ok());
        assert_eq!(result.unwrap_or(None).as_deref(), Some("alice"));
    }
}

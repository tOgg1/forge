//! fmail log/messages command behavior from Go `internal/fmail/log.go`.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fmail_core::message::Message;
use fmail_core::validate::{normalize_agent_name, normalize_topic};

use crate::duration::parse_go_duration_seconds;
use crate::{CommandOutput, FmailBackend};

const LOG_FOLLOW_POLL_INTERVAL: Duration = Duration::from_millis(100);

type MessageEntry = (Message, String);
type MessageEntriesWithSeen = (Vec<MessageEntry>, HashSet<String>);

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

    let (mut messages, mut seen) = load_message_entries(backend, &parsed.target, &since, &from)?;
    if parsed.limit > 0 && messages.len() > parsed.limit {
        let start = messages.len() - parsed.limit;
        messages = messages[start..].to_vec();
    }

    let mut out = String::new();
    for (message, _) in &messages {
        write_message(&mut out, message, parsed.json).map_err(|e| (1, format!("output: {e}")))?;
    }

    if !parsed.follow {
        return Ok(CommandOutput {
            stdout: out,
            stderr: String::new(),
            exit_code: 0,
        });
    }

    loop {
        thread::sleep(LOG_FOLLOW_POLL_INTERVAL);
        let updates = scan_new_messages(backend, &parsed.target, &since, &from, &mut seen)?;
        for (message, _) in &updates {
            write_message(&mut out, message, parsed.json)
                .map_err(|e| (1, format!("output: {e}")))?;
        }
    }
}

#[derive(Debug, Clone)]
enum LogTarget {
    AllTopics,
    AllMessages,
    Topic(String),
    Dm(String),
}

#[derive(Debug)]
struct ParsedLogArgs {
    target: LogTarget,
    limit: usize,
    since: String,
    from: String,
    json: bool,
    follow: bool,
}

fn parse_log_args(args: &[String], all_messages: bool) -> Result<ParsedLogArgs, (i32, String)> {
    let mut target = if all_messages {
        LogTarget::AllMessages
    } else {
        LogTarget::AllTopics
    };

    let mut target_set = false;
    let mut limit = 20usize;
    let mut since = String::new();
    let mut from = String::new();
    let mut json = false;
    let mut follow = false;

    let mut idx = 0usize;
    while idx < args.len() {
        let token = &args[idx];
        match token.as_str() {
            "-h" | "--help" | "help" => {
                let help = if all_messages {
                    HELP_TEXT_MESSAGES
                } else {
                    HELP_TEXT_LOG
                };
                return Err((0, help.to_string()));
            }
            "--json" => json = true,
            "-n" | "--limit" => {
                idx += 1;
                let raw = take_flag_value(args, idx, "--limit")?;
                let parsed = raw
                    .parse::<i64>()
                    .map_err(|_| (2, "limit must be an integer".to_string()))?;
                if parsed < 0 {
                    return Err((2, "limit must be >= 0".to_string()));
                }
                limit =
                    usize::try_from(parsed).map_err(|_| (2, "limit out of range".to_string()))?;
            }
            "--since" => {
                idx += 1;
                since = take_flag_value(args, idx, "--since")?;
            }
            "--from" => {
                idx += 1;
                from = take_flag_value(args, idx, "--from")?;
            }
            "-f" | "--follow" => {
                follow = true;
            }
            flag if flag.starts_with('-') => {
                return Err((2, format!("unknown flag: {flag}")));
            }
            positional => {
                if all_messages {
                    return Err((2, "messages takes no arguments".to_string()));
                }
                if target_set {
                    return Err((2, "log takes at most one argument".to_string()));
                }
                target = parse_log_target(positional)?;
                target_set = true;
            }
        }
        idx += 1;
    }

    Ok(ParsedLogArgs {
        target,
        limit,
        since,
        from,
        json,
        follow,
    })
}

fn parse_log_target(raw: &str) -> Result<LogTarget, (i32, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(LogTarget::AllTopics);
    }
    if let Some(agent_raw) = trimmed.strip_prefix('@') {
        let agent = normalize_agent_name(agent_raw)
            .map_err(|e| (1, format!("invalid target \"{trimmed}\": {e}")))?;
        return Ok(LogTarget::Dm(agent));
    }
    let topic =
        normalize_topic(trimmed).map_err(|e| (1, format!("invalid target \"{trimmed}\": {e}")))?;
    Ok(LogTarget::Topic(topic))
}

fn take_flag_value(args: &[String], idx: usize, flag: &str) -> Result<String, (i32, String)> {
    args.get(idx)
        .cloned()
        .ok_or_else(|| (2, format!("missing value for {flag}")))
}

fn load_message_entries(
    backend: &dyn FmailBackend,
    target: &LogTarget,
    since: &Option<DateTime<Utc>>,
    from: &Option<String>,
) -> Result<MessageEntriesWithSeen, (i32, String)> {
    let files = collect_target_files(backend, target).map_err(|e| (1, format!("log: {e}")))?;
    let mut seen = HashSet::with_capacity(files.len());
    let mut messages = Vec::new();

    for path in &files {
        let key = path.to_string_lossy().to_string();
        seen.insert(key.clone());

        let message = backend
            .read_message_at(path)
            .map_err(|e| (1, format!("log: read message {}: {e}", path.display())))?;
        if matches_log_target(&message, target) && filter_message(&message, since, from) {
            messages.push((message, key));
        }
    }

    sort_message_entries(&mut messages);
    Ok((messages, seen))
}

fn scan_new_messages(
    backend: &dyn FmailBackend,
    target: &LogTarget,
    since: &Option<DateTime<Utc>>,
    from: &Option<String>,
    seen: &mut HashSet<String>,
) -> Result<Vec<MessageEntry>, (i32, String)> {
    let files = collect_target_files(backend, target).map_err(|e| (1, format!("follow: {e}")))?;
    let mut updates = Vec::new();

    for path in &files {
        let key = path.to_string_lossy().to_string();
        if seen.contains(&key) {
            continue;
        }

        seen.insert(key.clone());
        let message = backend
            .read_message_at(path)
            .map_err(|e| (1, format!("follow: read message {}: {e}", path.display())))?;
        if matches_log_target(&message, target) && filter_message(&message, since, from) {
            updates.push((message, key));
        }
    }

    sort_message_entries(&mut updates);
    Ok(updates)
}

fn collect_target_files(
    backend: &dyn FmailBackend,
    target: &LogTarget,
) -> Result<Vec<PathBuf>, String> {
    match target {
        LogTarget::AllTopics | LogTarget::AllMessages => backend.list_message_files(None),
        LogTarget::Topic(topic) => backend.list_message_files(Some(topic)),
        LogTarget::Dm(agent) => {
            let dm = format!("@{agent}");
            backend.list_message_files(Some(&dm))
        }
    }
}

fn matches_log_target(message: &Message, target: &LogTarget) -> bool {
    match target {
        LogTarget::AllMessages => true,
        LogTarget::AllTopics => !message.to.starts_with('@'),
        LogTarget::Topic(topic) => message.to.eq_ignore_ascii_case(topic),
        LogTarget::Dm(agent) => message
            .to
            .strip_prefix('@')
            .is_some_and(|name| name.eq_ignore_ascii_case(agent)),
    }
}

fn sort_message_entries(entries: &mut [MessageEntry]) {
    entries.sort_by(
        |(left, left_path), (right, right_path)| match left.id.cmp(&right.id) {
            Ordering::Equal => match left.time.cmp(&right.time) {
                Ordering::Equal => left_path.cmp(right_path),
                other => other,
            },
            other => other,
        },
    );
}

fn filter_message(msg: &Message, since: &Option<DateTime<Utc>>, from: &Option<String>) -> bool {
    if let Some(since_time) = since {
        if msg.time < *since_time {
            return false;
        }
    }
    if let Some(from_filter) = from {
        if !msg.from.eq_ignore_ascii_case(from_filter) {
            return false;
        }
    }
    true
}

fn write_message(out: &mut String, message: &Message, json_output: bool) -> Result<(), String> {
    if json_output {
        let encoded = serde_json::to_string(message).map_err(|e| format!("encode message: {e}"))?;
        out.push_str(&encoded);
        out.push('\n');
        return Ok(());
    }

    let body = format_body(&message.body);
    out.push_str(&format!(
        "{} {} -> {}: {}\n",
        message.id, message.from, message.to, body
    ));
    Ok(())
}

fn format_body(body: &serde_json::Value) -> String {
    match body {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// Parse the `--since` value into a DateTime filter.
fn parse_since(value: &str, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, (i32, String)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.eq_ignore_ascii_case("now") {
        return Ok(Some(now));
    }

    if let Some(dur) = parse_duration_with_days(trimmed) {
        return Ok(Some(now - dur));
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }

    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        if let Some(ndt) = date.and_hms_opt(0, 0, 0) {
            return Ok(Some(ndt.and_utc()));
        }
    }

    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Some(ndt.and_utc()));
    }

    Err((
        2,
        "invalid --since value: use duration like '1h' or timestamp like '2024-01-15T10:30:00Z'"
            .to_string(),
    ))
}

/// Parse a duration string with `d` (day) support plus Go-style durations.
fn parse_duration_with_days(raw: &str) -> Option<chrono::Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(days_raw) = trimmed.strip_suffix('d') {
        if let Ok(days) = days_raw.parse::<f64>() {
            return seconds_to_chrono(days * 86400.0);
        }
    }

    let seconds = parse_go_duration_seconds(trimmed).ok()?;
    seconds_to_chrono(seconds)
}

fn seconds_to_chrono(seconds: f64) -> Option<chrono::Duration> {
    if !seconds.is_finite() {
        return None;
    }

    let negative = seconds < 0.0;
    let base = std::time::Duration::try_from_secs_f64(seconds.abs()).ok()?;
    let chrono = chrono::Duration::from_std(base).ok()?;
    Some(if negative { -chrono } else { chrono })
}

fn normalize_from_filter(from: &str) -> Result<Option<String>, (i32, String)> {
    let trimmed = from.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let raw = trimmed.strip_prefix('@').unwrap_or(trimmed);
    let normalized =
        normalize_agent_name(raw).map_err(|e| (2, format!("invalid --from value: {e}")))?;
    Ok(Some(normalized))
}

const HELP_TEXT_LOG: &str = "\
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

const HELP_TEXT_MESSAGES: &str = "\
View all public messages (topics and direct messages)

Usage:
  fmail messages [flags]

Flags:
  -n, --limit int       Max messages to show (default: 20)
      --since string    Filter by time window (e.g. 1h, 2d, 2024-01-15T10:30:00Z)
      --from string     Filter by sender
  -f, --follow          Stream new messages (poll-based)
      --json            Output as JSON
  -h, --help            Help for messages";

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_since_empty() {
        let now = Utc::now();
        let result = parse_since("", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn parse_since_now_case_insensitive() {
        let now = Utc::now();
        let result = parse_since("NoW", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_duration_hours() {
        let now = Utc::now();
        let result = parse_since("2h", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_duration_days() {
        let now = Utc::now();
        let result = parse_since("1d", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_duration_chain() {
        let now = Utc::now();
        let result = parse_since("1h30m", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_rfc3339() {
        let now = Utc::now();
        let result = parse_since("2026-02-09T12:00:00Z", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_date_only() {
        let now = Utc::now();
        let result = parse_since("2026-02-09", now);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn parse_since_invalid_is_usage_error() {
        let now = Utc::now();
        let err = parse_since("invalid", now).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("invalid --since value"), "{}", err.1);
    }

    #[test]
    fn parse_duration_with_days_values() {
        assert!(parse_duration_with_days("1d").is_some());
        assert!(parse_duration_with_days("2h").is_some());
        assert!(parse_duration_with_days("30m").is_some());
        assert!(parse_duration_with_days("3.5d").is_some());
        assert!(parse_duration_with_days("1h30m").is_some());
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
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn normalize_from_strips_at() {
        let result = normalize_from_filter("@alice");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_deref(), Some("alice"));
    }

    #[test]
    fn normalize_from_lowercases() {
        let result = normalize_from_filter("Alice");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_deref(), Some("alice"));
    }

    #[test]
    fn parse_log_target_validates() {
        assert!(matches!(
            parse_log_target("Task").unwrap(),
            LogTarget::Topic(topic) if topic == "task"
        ));
        assert!(matches!(
            parse_log_target("@Bob").unwrap(),
            LogTarget::Dm(agent) if agent == "bob"
        ));
    }

    #[test]
    fn parse_log_target_invalid_is_failure() {
        let err = parse_log_target("bad target!").unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("invalid target"), "{}", err.1);
    }
}

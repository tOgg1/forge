//! fmail watch command ported from Go `internal/fmail/watch.go`.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use fmail_core::message::Message;
use fmail_core::validate::{normalize_agent_name, normalize_topic};

use crate::duration::parse_go_duration_seconds;
use crate::{CommandOutput, FmailBackend};

const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
enum WatchTarget {
    AllTopics,
    Topic(String),
    Dm(String),
}

#[derive(Debug, Clone)]
struct ParsedWatchArgs {
    target: WatchTarget,
    count: usize,
    json: bool,
    timeout: Option<Duration>,
}

/// Run the watch command from test arguments.
pub fn run_watch_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    run_watch(&owned, backend)
}

fn run_watch(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    match execute_watch(args, backend) {
        Ok(output) => output,
        Err((exit_code, message)) => CommandOutput {
            stdout: String::new(),
            stderr: format!("{message}\n"),
            exit_code,
        },
    }
}

fn execute_watch(
    args: &[String],
    backend: &dyn FmailBackend,
) -> Result<CommandOutput, (i32, String)> {
    let parsed = parse_watch_args(args)?;

    let mut seen =
        initialize_seen(backend, &parsed.target).map_err(|e| (1, format!("watch: {e}")))?;

    let mut stdout = String::new();
    let deadline = parsed.timeout.map(|timeout| Instant::now() + timeout);
    let mut remaining = if parsed.count == 0 {
        None
    } else {
        Some(parsed.count)
    };

    'watch: loop {
        if let Some(remaining_count) = remaining {
            if remaining_count == 0 {
                break 'watch;
            }
        }

        if let Some(limit) = deadline {
            if Instant::now() >= limit {
                break 'watch;
            }
        }

        thread::sleep(WATCH_POLL_INTERVAL);

        let messages = scan_new_messages(backend, &parsed.target, &mut seen)
            .map_err(|e| (1, format!("watch: {e}")))?;

        for message in &messages {
            write_watch_message(&mut stdout, message, parsed.json)
                .map_err(|e| (1, format!("output: {e}")))?;

            if let Some(ref mut remaining_count) = remaining {
                if *remaining_count > 0 {
                    *remaining_count -= 1;
                    if *remaining_count == 0 {
                        break 'watch;
                    }
                }
            }
        }
    }

    Ok(CommandOutput {
        stdout,
        stderr: String::new(),
        exit_code: 0,
    })
}

fn parse_watch_args(args: &[String]) -> Result<ParsedWatchArgs, (i32, String)> {
    let mut json = false;
    let mut count_raw: i64 = 0;
    let mut timeout_raw: Option<String> = None;
    let mut target_raw: Option<String> = None;

    let mut idx = 0usize;
    while idx < args.len() {
        let token = &args[idx];
        match token.as_str() {
            "-h" | "--help" | "help" => return Err((0, HELP_TEXT.to_string())),
            "--json" => json = true,
            "-c" | "--count" => {
                idx += 1;
                let raw = take_flag_value(args, idx, "--count")?;
                count_raw = raw
                    .parse::<i64>()
                    .map_err(|_| (2, "count must be an integer".to_string()))?;
            }
            "--timeout" => {
                idx += 1;
                timeout_raw = Some(take_flag_value(args, idx, "--timeout")?);
            }
            flag if flag.starts_with('-') => {
                return Err((2, format!("unknown flag: {flag}")));
            }
            positional => {
                if target_raw.is_some() {
                    return Err((2, "watch takes at most one argument".to_string()));
                }
                target_raw = Some(positional.to_string());
            }
        }
        idx += 1;
    }

    if count_raw < 0 {
        return Err((2, "count must be >= 0".to_string()));
    }

    let timeout = match timeout_raw {
        None => None,
        Some(raw) => {
            let seconds = parse_go_duration_seconds(&raw)
                .map_err(|_| (2, format!("invalid timeout duration: {raw}")))?;
            if seconds < 0.0 {
                return Err((2, "timeout must be >= 0".to_string()));
            }
            let parsed = Duration::try_from_secs_f64(seconds)
                .map_err(|_| (2, format!("invalid timeout duration: {raw}")))?;
            if parsed.is_zero() {
                None
            } else {
                Some(parsed)
            }
        }
    };

    let target = parse_watch_target(target_raw.as_deref().unwrap_or(""))?;

    Ok(ParsedWatchArgs {
        target,
        count: usize::try_from(count_raw).map_err(|_| (2, "count out of range".to_string()))?,
        json,
        timeout,
    })
}

fn parse_watch_target(raw: &str) -> Result<WatchTarget, (i32, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(WatchTarget::AllTopics);
    }

    if let Some(agent) = trimmed.strip_prefix('@') {
        let normalized = normalize_agent_name(agent)
            .map_err(|e| (1, format!("invalid target \"{trimmed}\": {e}")))?;
        return Ok(WatchTarget::Dm(normalized));
    }

    let normalized =
        normalize_topic(trimmed).map_err(|e| (1, format!("invalid target \"{trimmed}\": {e}")))?;
    Ok(WatchTarget::Topic(normalized))
}

fn initialize_seen(
    backend: &dyn FmailBackend,
    target: &WatchTarget,
) -> Result<HashSet<String>, String> {
    let files = collect_target_files(backend, target)?;
    let mut seen = HashSet::with_capacity(files.len());
    for path in &files {
        seen.insert(path.to_string_lossy().to_string());
    }
    Ok(seen)
}

fn scan_new_messages(
    backend: &dyn FmailBackend,
    target: &WatchTarget,
    seen: &mut HashSet<String>,
) -> Result<Vec<Message>, String> {
    let files = collect_target_files(backend, target)?;

    let mut updates: Vec<(Message, String)> = Vec::new();
    for path in &files {
        let key = path.to_string_lossy().to_string();
        if seen.contains(&key) {
            continue;
        }

        seen.insert(key.clone());
        let message = backend
            .read_message_at(path)
            .map_err(|e| format!("read message {}: {e}", path.display()))?;
        updates.push((message, key));
    }

    updates.sort_by(|a, b| {
        let left = &a.0;
        let right = &b.0;

        match left.id.cmp(&right.id) {
            Ordering::Equal => match left.time.cmp(&right.time) {
                Ordering::Equal => a.1.cmp(&b.1),
                other => other,
            },
            other => other,
        }
    });

    Ok(updates.into_iter().map(|(message, _)| message).collect())
}

fn collect_target_files(
    backend: &dyn FmailBackend,
    target: &WatchTarget,
) -> Result<Vec<PathBuf>, String> {
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();

    match target {
        WatchTarget::AllTopics => {
            let mut topic_names: BTreeSet<String> = BTreeSet::new();
            for topic in backend.list_topics()?.unwrap_or_default() {
                topic_names.insert(topic.name);
            }

            for topic in &topic_names {
                for path in backend.list_message_files(Some(topic))? {
                    files.insert(path);
                }
            }
        }
        WatchTarget::Topic(topic) => {
            for path in backend.list_message_files(Some(topic))? {
                files.insert(path);
            }
        }
        WatchTarget::Dm(agent) => {
            let target = format!("@{agent}");
            for path in backend.list_message_files(Some(&target))? {
                files.insert(path);
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn write_watch_message(
    out: &mut String,
    message: &Message,
    json_output: bool,
) -> Result<(), String> {
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
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

fn take_flag_value(args: &[String], idx: usize, flag: &str) -> Result<String, (i32, String)> {
    args.get(idx)
        .cloned()
        .ok_or_else(|| (2, format!("missing value for {flag}")))
}

const HELP_TEXT: &str = "\
Stream messages as they arrive

Usage:
  fmail watch [topic|@agent] [flags]

Flags:
  -c, --count int         Exit after receiving N messages
      --json              Output as JSON
      --timeout duration  Maximum wait time before exiting
  -h, --help              Help for watch";

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parse_watch_target_all_topics_when_empty() {
        assert_eq!(parse_watch_target("").unwrap(), WatchTarget::AllTopics);
        assert_eq!(parse_watch_target("   ").unwrap(), WatchTarget::AllTopics);
    }

    #[test]
    fn parse_watch_target_topic_and_dm() {
        assert_eq!(
            parse_watch_target("Task").unwrap(),
            WatchTarget::Topic("task".to_string())
        );
        assert_eq!(
            parse_watch_target("@Bob").unwrap(),
            WatchTarget::Dm("bob".to_string())
        );
    }

    #[test]
    fn parse_watch_target_invalid() {
        let err = parse_watch_target("bad topic!").unwrap_err();
        assert_eq!(err.0, 1);
        assert!(err.1.contains("invalid target"), "{}", err.1);
    }

    #[test]
    fn parse_duration_supports_common_units() {
        assert_eq!(
            crate::duration::parse_go_duration("100ms").unwrap(),
            Duration::from_millis(100)
        );
        assert_eq!(
            crate::duration::parse_go_duration("2s").unwrap(),
            Duration::from_secs(2)
        );
        assert_eq!(
            crate::duration::parse_go_duration("2m").unwrap(),
            Duration::from_secs(120)
        );
        assert_eq!(
            crate::duration::parse_go_duration("1h").unwrap(),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn parse_duration_supports_composite_values() {
        assert_eq!(
            crate::duration::parse_go_duration("1m30s").unwrap(),
            Duration::from_secs(90)
        );
        assert_eq!(
            crate::duration::parse_go_duration("2h15m").unwrap(),
            Duration::from_secs(8100)
        );
    }

    #[test]
    fn parse_duration_rejects_invalid() {
        assert!(crate::duration::parse_go_duration("").is_err());
        assert!(crate::duration::parse_go_duration("2").is_err());
        assert!(crate::duration::parse_go_duration("abc").is_err());
        assert!(crate::duration::parse_go_duration("1d").is_err());
    }

    #[test]
    fn parse_watch_args_rejects_negative_count_and_timeout() {
        let args = vec!["--count".to_string(), "-1".to_string()];
        let err = parse_watch_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("count must be >= 0"), "{}", err.1);

        let args = vec!["--timeout".to_string(), "-1s".to_string()];
        let err = parse_watch_args(&args).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("timeout must be >= 0"), "{}", err.1);
    }
}

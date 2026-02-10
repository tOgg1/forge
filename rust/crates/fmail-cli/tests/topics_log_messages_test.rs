#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

/// In-memory backend for topics/log/messages tests.
struct TopicsLogBackend {
    now: DateTime<Utc>,
    topics: Option<Vec<TopicSummary>>,
    messages: Vec<Message>,
}

impl TopicsLogBackend {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            topics: Some(Vec::new()),
            messages: Vec::new(),
        }
    }

    fn with_topics(mut self, topics: Vec<TopicSummary>) -> Self {
        self.topics = Some(topics);
        self
    }

    fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }
}

impl FmailBackend for TopicsLogBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(Some(vec![]))
    }

    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
        Ok(None)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }

    fn register_agent_record(&self, _name: &str, _host: &str) -> Result<AgentRecord, String> {
        Err("not implemented".to_string())
    }

    fn set_agent_status(
        &self,
        _name: &str,
        _status: &str,
        _host: &str,
    ) -> Result<AgentRecord, String> {
        Err("not implemented".to_string())
    }

    fn hostname(&self) -> String {
        "test-host".to_string()
    }

    fn agent_name(&self) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn save_message(&self, _message: &mut Message) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String> {
        Ok(self.topics.clone())
    }

    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        let normalized_target = target.map(|value| value.trim().to_lowercase());
        Ok(self
            .messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| match normalized_target.as_deref() {
                None => true,
                Some(filter) => msg.to.eq_ignore_ascii_case(filter),
            })
            .map(|(i, _)| PathBuf::from(format!("/fake/{i}.json")))
            .collect())
    }

    fn read_message_at(&self, path: &Path) -> Result<Message, String> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("bad path")?;
        let idx: usize = stem.parse().map_err(|_| "bad index".to_string())?;
        self.messages
            .get(idx)
            .cloned()
            .ok_or_else(|| "message not found".to_string())
    }

    fn init_project(&self, _project_id: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn gc_messages(&self, _days: i64, _dry_run: bool) -> Result<String, String> {
        Ok(String::new())
    }
}

fn rfc3339(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("parse")
        .with_timezone(&Utc)
}

fn make_msg(id: &str, from: &str, to: &str, body: &str, time: &str) -> Message {
    Message {
        id: id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        time: rfc3339(time),
        body: serde_json::Value::String(body.to_string()),
        reply_to: String::new(),
        priority: String::new(),
        host: String::new(),
        tags: Vec::new(),
    }
}

// ===== TOPICS GOLDEN TESTS =====

#[test]
fn topics_empty_text_matches_golden() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/topics/empty_text.txt"));
}

#[test]
fn topics_nonempty_text_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_topics(vec![
        TopicSummary {
            name: "bugs".to_string(),
            messages: 5,
            last_activity: Some(rfc3339("2026-02-09T11:30:00Z")),
        },
        TopicSummary {
            name: "tasks".to_string(),
            messages: 12,
            last_activity: Some(rfc3339("2026-02-08T10:00:00Z")),
        },
    ]);
    let out = run_cli_for_test(&["topics"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/topics/nonempty_text.txt"));
}

#[test]
fn topics_json_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_topics(vec![TopicSummary {
        name: "bugs".to_string(),
        messages: 3,
        last_activity: Some(rfc3339("2026-02-09T11:00:00Z")),
    }]);
    let out = run_cli_for_test(&["topics", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/topics/json.txt"));
}

#[test]
fn topics_json_missing_topics_dir_is_null() {
    let mut backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    backend.topics = None;
    let out = run_cli_for_test(&["topics", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "null\n");
}

#[test]
fn topics_help_matches_golden() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stderr, include_str!("golden/topics/help.txt"));
}

#[test]
fn topics_alias_works() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topic"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/topics/empty_text.txt"));
}

#[test]
fn topics_unknown_flag_fails() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics", "--verbose"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn topics_rejects_positional_args() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics", "extra"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("takes no arguments"),
        "stderr: {}",
        out.stderr
    );
}

// ===== LOG GOLDEN TESTS =====

#[test]
fn log_empty_outputs_nothing() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn log_text_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "first task",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "bob",
            "tasks",
            "second task",
            "2026-02-09T11:30:00Z",
        ),
    ]);
    let out = run_cli_for_test(&["log"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/text.txt"));
}

#[test]
fn log_default_excludes_direct_messages() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "topic message",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "bob",
            "@alice",
            "dm should be hidden",
            "2026-02-09T11:30:00Z",
        ),
    ]);

    let out = run_cli_for_test(&["log"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("topic message"),
        "stdout: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("dm should be hidden"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn log_dm_target_filters_to_agent_mailbox() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "@bob",
            "for bob",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "alice",
            "@eve",
            "for eve",
            "2026-02-09T11:05:00Z",
        ),
    ]);

    let out = run_cli_for_test(&["log", "@Bob"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("for bob"), "stdout: {}", out.stdout);
    assert!(!out.stdout.contains("for eve"), "stdout: {}", out.stdout);
}

#[test]
fn log_json_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![make_msg(
        "msg-001",
        "alice",
        "tasks",
        "hello",
        "2026-02-09T11:00:00Z",
    )]);
    let out = run_cli_for_test(&["log", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/json.txt"));
}

#[test]
fn log_help_matches_golden() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stderr, include_str!("golden/log/help.txt"));
}

#[test]
fn log_alias_logs_works() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["logs"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
}

#[test]
fn log_limit_flag() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "first", "2026-02-09T10:00:00Z"),
        make_msg(
            "msg-002",
            "alice",
            "tasks",
            "second",
            "2026-02-09T11:00:00Z",
        ),
        make_msg("msg-003", "alice", "tasks", "third", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["log", "-n", "1"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/limit_one.txt"));
}

#[test]
fn log_limit_long_flag() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "first", "2026-02-09T10:00:00Z"),
        make_msg(
            "msg-002",
            "alice",
            "tasks",
            "second",
            "2026-02-09T11:00:00Z",
        ),
    ]);
    let out = run_cli_for_test(&["log", "--limit", "1"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1, "should show 1 message: {}", out.stdout);
}

#[test]
fn log_from_filter() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "from alice",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "bob",
            "tasks",
            "from bob",
            "2026-02-09T11:30:00Z",
        ),
    ]);
    let out = run_cli_for_test(&["log", "--from", "alice"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/from_filter.txt"));
}

#[test]
fn log_from_filter_with_at_prefix() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "from alice",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "bob",
            "tasks",
            "from bob",
            "2026-02-09T11:30:00Z",
        ),
    ]);
    let out = run_cli_for_test(&["log", "--from", "@alice"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    // Same output as --from alice (@ is stripped)
    assert_eq!(out.stdout, include_str!("golden/log/from_filter.txt"));
}

#[test]
fn log_since_duration_filter() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "old", "2026-02-08T10:00:00Z"),
        make_msg("msg-002", "bob", "tasks", "recent", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["log", "--since", "1h"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/since_filter.txt"));
}

#[test]
fn log_since_rfc3339_filter() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "old", "2026-02-08T10:00:00Z"),
        make_msg("msg-002", "bob", "tasks", "recent", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["log", "--since", "2026-02-09T00:00:00Z"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    // Same output as since=1h (both filter out the old message)
    assert_eq!(out.stdout, include_str!("golden/log/since_filter.txt"));
}

#[test]
fn log_since_invalid() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "--since", "bogus"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("invalid --since"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn log_unknown_flag() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "--verbose"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn log_invalid_limit() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "-n", "abc"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("limit must be"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn log_too_many_positionals() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "a", "b"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("at most one argument"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn log_invalid_target_fails() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "bad target!"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid target"),
        "stderr: {}",
        out.stderr
    );
}

// ===== MESSAGES GOLDEN TESTS =====

#[test]
fn messages_empty_outputs_nothing() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["messages"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn messages_text_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "task msg",
            "2026-02-09T11:00:00Z",
        ),
        make_msg("msg-002", "bob", "@alice", "dm msg", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["messages"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/messages/text.txt"));
}

#[test]
fn messages_json_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![make_msg(
        "msg-001",
        "alice",
        "tasks",
        "hello",
        "2026-02-09T11:00:00Z",
    )]);
    let out = run_cli_for_test(&["messages", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/messages/json.txt"));
}

#[test]
fn messages_rejects_positional_args() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["messages", "extra"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("takes no arguments"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn messages_from_filter_matches_golden() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg(
            "msg-001",
            "alice",
            "tasks",
            "from alice",
            "2026-02-09T11:00:00Z",
        ),
        make_msg(
            "msg-002",
            "bob",
            "tasks",
            "from bob",
            "2026-02-09T11:30:00Z",
        ),
    ]);
    let out = run_cli_for_test(&["messages", "--from", "alice"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    // Same as log --from alice (from_filter.txt)
    assert_eq!(out.stdout, include_str!("golden/log/from_filter.txt"));
}

#[test]
fn messages_since_filter() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "old", "2026-02-08T10:00:00Z"),
        make_msg("msg-002", "bob", "tasks", "recent", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["messages", "--since", "1h"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/log/since_filter.txt"));
}

#[test]
fn messages_limit() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "first", "2026-02-09T10:00:00Z"),
        make_msg(
            "msg-002",
            "alice",
            "tasks",
            "second",
            "2026-02-09T11:00:00Z",
        ),
        make_msg("msg-003", "alice", "tasks", "third", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["messages", "-n", "2"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2, "should show 2 messages: {}", out.stdout);
}

#[test]
fn messages_help_matches_golden() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["messages", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stderr, include_str!("golden/messages/help.txt"));
}

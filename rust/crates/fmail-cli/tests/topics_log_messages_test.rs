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
    topics: Vec<TopicSummary>,
    /// Map from (target filter) -> list of message file paths
    /// None key = all messages, Some(key) = filtered
    messages: Vec<Message>,
}

impl TopicsLogBackend {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            topics: Vec::new(),
            messages: Vec::new(),
        }
    }

    fn with_topics(mut self, topics: Vec<TopicSummary>) -> Self {
        self.topics = topics;
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

    fn list_topics(&self) -> Result<Vec<TopicSummary>, String> {
        Ok(self.topics.clone())
    }

    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        // Return a fake path per message
        Ok(self
            .messages
            .iter()
            .enumerate()
            .map(|(i, _)| PathBuf::from(format!("/fake/{i}.json")))
            .collect())
    }

    fn read_message_at(&self, path: &Path) -> Result<Message, String> {
        // Extract index from path
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

// ===== TOPICS TESTS =====

#[test]
fn topics_empty_outputs_header_only() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("TOPIC"),
        "should have header: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("MESSAGES"),
        "should have header: {}",
        out.stdout
    );
}

#[test]
fn topics_lists_topics_text() {
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
    assert!(out.stdout.contains("bugs"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("tasks"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("5"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("12"), "stdout: {}", out.stdout);
    // Last activity should show relative time
    assert!(out.stdout.contains("30m ago"), "stdout: {}", out.stdout);
}

#[test]
fn topics_json_output() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_topics(vec![TopicSummary {
        name: "bugs".to_string(),
        messages: 3,
        last_activity: Some(rfc3339("2026-02-09T11:00:00Z")),
    }]);
    let out = run_cli_for_test(&["topics", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);

    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).expect("parse json");
    assert!(parsed.is_array(), "should be array: {parsed}");
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "bugs");
    assert_eq!(arr[0]["messages"], 3);
}

#[test]
fn topics_alias_works() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topic"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("TOPIC"), "stdout: {}", out.stdout);
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

#[test]
fn topics_help() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["topics", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.contains("List topics"), "stderr: {}", out.stderr);
}

// ===== LOG TESTS =====

#[test]
fn log_empty_outputs_nothing() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn log_shows_messages_text() {
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
    assert!(
        out.stdout.contains("msg-001"),
        "should contain id: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("alice -> tasks"),
        "should contain from->to: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("first task"),
        "should contain body: {}",
        out.stdout
    );
    assert!(out.stdout.contains("msg-002"), "stdout: {}", out.stdout);
}

#[test]
fn log_json_output() {
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

    let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("parse json");
    assert_eq!(parsed["id"], "msg-001");
    assert_eq!(parsed["from"], "alice");
    assert_eq!(parsed["to"], "tasks");
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
    // Should only show 1 message (the last one after sorting by ID)
    let lines: Vec<&str> = out.stdout.trim().lines().collect();
    assert_eq!(lines.len(), 1, "should show 1 message: {}", out.stdout);
    assert!(
        out.stdout.contains("msg-003"),
        "should show last message: {}",
        out.stdout
    );
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
    assert!(
        out.stdout.contains("from alice"),
        "should have alice's msg: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("from bob"),
        "should not have bob's msg: {}",
        out.stdout
    );
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
    assert!(
        out.stdout.contains("from alice"),
        "should have alice's msg: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("from bob"),
        "should not have bob's msg: {}",
        out.stdout
    );
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
    assert!(
        !out.stdout.contains("old"),
        "should not have old msg: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("recent"),
        "should have recent msg: {}",
        out.stdout
    );
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
    assert!(
        !out.stdout.contains("old"),
        "should not have old msg: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("recent"),
        "should have recent msg: {}",
        out.stdout
    );
}

#[test]
fn log_since_invalid() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "--since", "bogus"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid --since"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn log_positional_target() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![make_msg(
        "msg-001",
        "alice",
        "bugs",
        "bug report",
        "2026-02-09T11:00:00Z",
    )]);
    let out = run_cli_for_test(&["log", "bugs"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    // The positional argument is passed to list_message_files as target
    // In our mock, all messages are returned regardless, but the command should succeed
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
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
fn log_help() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["log", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(
        out.stderr.contains("View recent messages"),
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

// ===== MESSAGES TESTS =====

#[test]
fn messages_empty_outputs_nothing() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["messages"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn messages_shows_all_messages() {
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
    assert!(out.stdout.contains("msg-001"), "stdout: {}", out.stdout);
    assert!(out.stdout.contains("msg-002"), "stdout: {}", out.stdout);
}

#[test]
fn messages_json_output() {
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

    let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("parse json");
    assert_eq!(parsed["id"], "msg-001");
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
fn messages_supports_from_filter() {
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
    assert!(
        out.stdout.contains("from alice"),
        "should have alice's msg: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("from bob"),
        "should not have bob's msg: {}",
        out.stdout
    );
}

#[test]
fn messages_supports_since_filter() {
    let now = rfc3339("2026-02-09T12:00:00Z");
    let backend = TopicsLogBackend::new(now).with_messages(vec![
        make_msg("msg-001", "alice", "tasks", "old", "2026-02-08T10:00:00Z"),
        make_msg("msg-002", "bob", "tasks", "recent", "2026-02-09T11:30:00Z"),
    ]);
    let out = run_cli_for_test(&["messages", "--since", "1h"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(
        !out.stdout.contains("old"),
        "should not have old msg: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("recent"),
        "should have recent msg: {}",
        out.stdout
    );
}

#[test]
fn messages_supports_limit() {
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
fn messages_help() {
    let backend = TopicsLogBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["messages", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    // Help is shown via the log command's help text
    assert!(
        out.stderr.contains("View recent messages"),
        "stderr: {}",
        out.stderr
    );
}

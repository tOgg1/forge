#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::{generate_message_id, Message};

struct SendBackend {
    now: DateTime<Utc>,
    agent: String,
    host: String,
    messages: RefCell<Vec<Message>>,
    files: HashMap<String, String>,
}

impl SendBackend {
    fn new(now: DateTime<Utc>, agent: &str) -> Self {
        Self {
            now,
            agent: agent.to_string(),
            host: "test-host".to_string(),
            messages: RefCell::new(Vec::new()),
            files: HashMap::new(),
        }
    }
}

impl FmailBackend for SendBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(Some(vec![]))
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }

    fn register_agent_record(&self, _name: &str, _host: &str) -> Result<AgentRecord, String> {
        Err("not implemented".to_string())
    }

    fn hostname(&self) -> String {
        self.host.clone()
    }

    fn agent_name(&self) -> Result<String, String> {
        Ok(self.agent.clone())
    }

    fn save_message(&self, message: &mut Message) -> Result<String, String> {
        if message.id.is_empty() {
            message.id = generate_message_id(self.now);
        }
        let id = message.id.clone();
        self.messages.borrow_mut().push(message.clone());
        Ok(id)
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not found: {path}"))
    }
    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
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
    fn list_topics(&self) -> Result<Vec<fmail_core::store::TopicSummary>, String> {
        Err("not implemented".to_string())
    }
    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<std::path::PathBuf>, String> {
        Err("not implemented".to_string())
    }
    fn read_message_at(&self, _path: &std::path::Path) -> Result<Message, String> {
        Err("not implemented".to_string())
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

// --- Basic send tests ---

#[test]
fn send_to_topic_outputs_id() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "hello world"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    let id = out.stdout.trim();
    assert!(!id.is_empty(), "id should not be empty");
    assert!(id.starts_with("20260209-"), "id format: {id}");
}

#[test]
fn send_to_dm_outputs_id() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "@bob", "hello bob"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    let id = out.stdout.trim();
    assert!(!id.is_empty());
}

#[test]
fn send_stores_message_fields() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "hello"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.from, "alice");
    assert_eq!(msg.to, "task");
    assert_eq!(msg.host, "test-host");
    assert!(msg.body.is_string());
}

#[test]
fn send_dm_normalizes_target() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "@Bob", "hi"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].to, "@bob");
}

// --- JSON output ---

#[test]
fn send_json_outputs_message() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "hello", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);

    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).expect("parse json");
    assert_eq!(parsed["from"], "alice");
    assert_eq!(parsed["to"], "task");
    assert!(parsed["id"].is_string());
    assert!(parsed["body"].is_string());
}

// --- JSON body parsing ---

#[test]
fn send_json_body_is_parsed() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", r#"{"key": "value"}"#, "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).expect("parse json");
    assert!(parsed["body"].is_object(), "body: {}", parsed["body"]);
    assert_eq!(parsed["body"]["key"], "value");
}

// --- File input ---

#[test]
fn send_from_file() {
    let mut backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    backend
        .files
        .insert("/tmp/msg.txt".to_string(), "file content".to_string());

    let out = run_cli_for_test(&["send", "task", "--file", "/tmp/msg.txt"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].body.is_string());
}

#[test]
fn send_body_and_file_conflict() {
    let mut backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    backend
        .files
        .insert("/tmp/msg.txt".to_string(), "file content".to_string());

    let out = run_cli_for_test(
        &["send", "task", "inline body", "--file", "/tmp/msg.txt"],
        &backend,
    );
    assert_eq!(out.exit_code, 2);
    assert!(out.stderr.contains("not both"), "stderr: {}", out.stderr);
}

// --- Priority ---

#[test]
fn send_with_priority() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "urgent!", "--priority", "high"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].priority, "high");
}

#[test]
fn send_invalid_priority_fails() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "msg", "--priority", "urgent"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid priority"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_priority_short_flag() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "msg", "-p", "low"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].priority, "low");
}

// --- Tags ---

#[test]
fn send_with_tags() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(
        &["send", "task", "msg", "--tag", "bug", "--tag", "p1"],
        &backend,
    );
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].tags, vec!["bug", "p1"]);
}

#[test]
fn send_comma_separated_tags() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "msg", "-t", "bug,feature,p1"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].tags, vec!["bug", "feature", "p1"]);
}

// --- Reply-to ---

#[test]
fn send_with_reply_to() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(
        &["send", "task", "reply here", "--reply-to", "prev-msg-id"],
        &backend,
    );
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].reply_to, "prev-msg-id");
}

#[test]
fn send_reply_to_short_flag() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "reply", "-r", "old-id"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert_eq!(messages[0].reply_to, "old-id");
}

// --- Error cases ---

#[test]
fn send_missing_target() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("target is required"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_missing_body() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("body is required"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_invalid_target() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "BAD TARGET!", "msg"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid target"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_unknown_flag() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "msg", "--verbose"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_too_many_args() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "body", "extra"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("too many arguments"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn send_help_matches_golden() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stderr, include_str!("golden/send/help.txt"));
}

// --- Priority not set leaves field empty ---

#[test]
fn send_without_priority_flag_leaves_empty() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "msg"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let messages = backend.messages.borrow();
    assert!(messages[0].priority.is_empty());
}

// --- JSON output omits empty fields ---

#[test]
fn send_json_omits_empty_optional_fields() {
    let backend = SendBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice");
    let out = run_cli_for_test(&["send", "task", "hello", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).expect("parse json");
    // Optional empty fields should not appear
    assert!(
        parsed.get("reply_to").is_none(),
        "reply_to should be omitted"
    );
    assert!(
        parsed.get("priority").is_none(),
        "priority should be omitted"
    );
    assert!(parsed.get("tags").is_none(), "tags should be omitted");
}

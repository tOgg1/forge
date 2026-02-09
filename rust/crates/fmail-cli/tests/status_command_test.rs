#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

struct StatusBackend {
    now: DateTime<Utc>,
    agent: String,
    host: String,
    record: RefCell<Option<AgentRecord>>,
    last_set: RefCell<Option<(String, String, String)>>,
}

impl StatusBackend {
    fn new(now: DateTime<Utc>, agent: &str, host: &str, record: Option<AgentRecord>) -> Self {
        Self {
            now,
            agent: agent.to_string(),
            host: host.to_string(),
            record: RefCell::new(record),
            last_set: RefCell::new(None),
        }
    }
}

impl FmailBackend for StatusBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(Some(vec![]))
    }

    fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String> {
        let record = self.record.borrow().clone();
        Ok(record.filter(|r| r.name == name))
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }

    fn register_agent_record(&self, _name: &str, _host: &str) -> Result<AgentRecord, String> {
        Err("not implemented".to_string())
    }

    fn set_agent_status(
        &self,
        name: &str,
        status: &str,
        host: &str,
    ) -> Result<AgentRecord, String> {
        self.last_set.borrow_mut().replace((
            name.to_string(),
            status.to_string(),
            host.to_string(),
        ));
        let mut record = self.record.borrow().clone().unwrap_or(AgentRecord {
            name: name.to_string(),
            host: None,
            status: None,
            first_seen: self.now,
            last_seen: self.now,
        });
        record.status = if status.trim().is_empty() {
            None
        } else {
            Some(status.trim().to_string())
        };
        record.host = if host.trim().is_empty() {
            None
        } else {
            Some(host.trim().to_string())
        };
        *self.record.borrow_mut() = Some(record.clone());
        Ok(record)
    }

    fn hostname(&self) -> String {
        self.host.clone()
    }

    fn agent_name(&self) -> Result<String, String> {
        Ok(self.agent.clone())
    }

    fn save_message(&self, _message: &mut Message) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn list_topics(&self) -> Result<Vec<TopicSummary>, String> {
        Err("not implemented".to_string())
    }

    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        Err("not implemented".to_string())
    }

    fn read_message_at(&self, _path: &Path) -> Result<Message, String> {
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

#[test]
fn status_show_missing_is_empty() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn status_show_prints_trimmed_status() {
    let backend = StatusBackend::new(
        rfc3339("2026-02-09T12:00:00Z"),
        "alice",
        "test-host",
        Some(AgentRecord {
            name: "alice".to_string(),
            host: Some("h1".to_string()),
            status: Some("  working on auth  ".to_string()),
            first_seen: rfc3339("2026-02-09T11:00:00Z"),
            last_seen: rfc3339("2026-02-09T11:59:00Z"),
        }),
    );
    let out = run_cli_for_test(&["status"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/status/show.txt"));
}

#[test]
fn status_set_calls_backend() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status", "working on auth"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty());

    let last = backend.last_set.borrow().clone().expect("last_set");
    assert_eq!(last.0, "alice");
    assert_eq!(last.1, "working on auth");
    assert_eq!(last.2, "test-host");
}

#[test]
fn status_clear_calls_backend_with_empty_status() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status", "--clear"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty());

    let last = backend.last_set.borrow().clone().expect("last_set");
    assert_eq!(last.1, "");
}

#[test]
fn status_clear_rejects_message() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status", "--clear", "x"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr
            .contains("status does not take a message with --clear"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn status_message_required() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status", "   "], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "status message is required\n");
}

#[test]
fn status_unknown_flag() {
    let backend = StatusBackend::new(rfc3339("2026-02-09T12:00:00Z"), "alice", "test-host", None);
    let out = run_cli_for_test(&["status", "--verbose"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

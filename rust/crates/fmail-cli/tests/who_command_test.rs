#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

struct InMemoryBackend {
    now: DateTime<Utc>,
    records: Option<Vec<AgentRecord>>,
}

impl FmailBackend for InMemoryBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(self.records.clone())
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
        Err("not implemented".to_string())
    }

    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<PathBuf>, String> {
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

#[test]
fn who_empty_text_matches_golden() {
    let backend = InMemoryBackend {
        now: rfc3339("2026-02-09T00:00:00Z"),
        records: Some(vec![]),
    };
    let out = run_cli_for_test(&["who"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/who/empty.txt"));
}

#[test]
fn who_nonempty_text_matches_golden() {
    let now = rfc3339("2026-02-09T00:00:00Z");
    let backend = InMemoryBackend {
        now,
        records: Some(vec![
            AgentRecord {
                name: "alice".to_string(),
                host: None,
                status: None,
                first_seen: rfc3339("2026-02-08T00:00:00Z"),
                last_seen: rfc3339("2026-02-08T23:59:30Z"),
            },
            AgentRecord {
                name: "bob".to_string(),
                host: None,
                status: None,
                first_seen: rfc3339("2026-02-08T00:00:00Z"),
                last_seen: rfc3339("2026-02-08T22:00:00Z"),
            },
            AgentRecord {
                name: "carol".to_string(),
                host: Some("macbook-pro".to_string()),
                status: Some("working on fmail".to_string()),
                first_seen: rfc3339("2026-02-07T00:00:00Z"),
                last_seen: rfc3339("2026-02-06T23:00:00Z"),
            },
        ]),
    };
    let out = run_cli_for_test(&["who"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/who/nonempty.txt"));
}

#[test]
fn who_json_null_matches_golden() {
    let backend = InMemoryBackend {
        now: rfc3339("2026-02-09T00:00:00Z"),
        records: None,
    };
    let out = run_cli_for_test(&["who", "--json"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/who/json_null.txt"));
}

#[test]
fn who_json_nonempty_matches_golden() {
    let backend = InMemoryBackend {
        now: rfc3339("2026-02-09T00:00:00Z"),
        records: Some(vec![AgentRecord {
            name: "alice".to_string(),
            host: Some("macbook-pro".to_string()),
            status: None,
            first_seen: rfc3339("2026-02-08T00:00:00Z"),
            last_seen: rfc3339("2026-02-09T00:00:00Z"),
        }]),
    };
    let out = run_cli_for_test(&["who", "--json"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/who/json_nonempty.txt"));
}

#[test]
fn who_help_matches_go_shape() {
    let backend = InMemoryBackend {
        now: rfc3339("2026-02-09T00:00:00Z"),
        records: Some(vec![]),
    };
    let out = run_cli_for_test(&["who", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("List known agents"),
        "stdout: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("fmail who [flags]"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn who_rejects_positional_args_with_argsmax_message() {
    let backend = InMemoryBackend {
        now: rfc3339("2026-02-09T00:00:00Z"),
        records: Some(vec![]),
    };
    let out = run_cli_for_test(&["who", "extra"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
    assert_eq!(out.stderr, "expected at most 0 args, got 1\n");
}

#![allow(clippy::expect_used, clippy::unwrap_used)]

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;

struct InMemoryBackend {
    now: DateTime<Utc>,
    records: Option<Vec<AgentRecord>>,
}

impl FmailBackend for InMemoryBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(self.records.clone())
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }

    fn register_agent_record(&self, _name: &str, _host: &str) -> Result<AgentRecord, String> {
        Err("not implemented".to_string())
    }

    fn hostname(&self) -> String {
        "test-host".to_string()
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

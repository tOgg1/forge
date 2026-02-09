#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::HashSet;

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::store::ERR_AGENT_EXISTS;

struct RegisterBackend {
    now: DateTime<Utc>,
    host: String,
    registered: RefCell<HashSet<String>>,
}

impl RegisterBackend {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            host: "test-host".to_string(),
            registered: RefCell::new(HashSet::new()),
        }
    }
}

impl FmailBackend for RegisterBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(Some(vec![]))
    }

    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }

    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String> {
        let mut set = self.registered.borrow_mut();
        if set.contains(name) {
            return Err(ERR_AGENT_EXISTS.to_string());
        }
        set.insert(name.to_string());
        Ok(AgentRecord {
            name: name.to_string(),
            host: if host.trim().is_empty() {
                None
            } else {
                Some(host.to_string())
            },
            status: None,
            first_seen: self.now,
            last_seen: self.now,
        })
    }

    fn hostname(&self) -> String {
        self.host.clone()
    }
}

fn rfc3339(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("parse")
        .with_timezone(&Utc)
}

#[test]
fn register_named_text_matches_golden() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "my-agent"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/register/named_text.txt"));
}

#[test]
fn register_named_json_matches_golden() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "my-agent", "--json"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/register/named_json.txt"));
}

#[test]
fn register_normalizes_name() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "My-Agent"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "my-agent");
}

#[test]
fn register_rejects_invalid_name() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "bad name!"], &backend);
    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid agent name"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn register_duplicate_fails() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out1 = run_cli_for_test(&["register", "alice"], &backend);
    assert_eq!(out1.exit_code, 0);

    let out2 = run_cli_for_test(&["register", "alice"], &backend);
    assert_eq!(out2.exit_code, 1);
    assert!(
        out2.stderr.contains("agent name already registered"),
        "stderr: {}",
        out2.stderr
    );
}

#[test]
fn register_auto_generates_name() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register"], &backend);
    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    let name = out.stdout.trim();
    assert!(!name.is_empty(), "name should not be empty");
    // Name should be kebab-case: lowercase + digits + hyphens
    assert!(
        name.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "name should be kebab-case: {name}"
    );
    // Should have at least 2 parts (adjective-name)
    assert!(name.contains('-'), "name should have parts: {name}");
}

#[test]
fn register_too_many_args() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "a", "b"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("at most 1 argument"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn register_unknown_flag() {
    let backend = RegisterBackend::new(rfc3339("2026-01-10T18:00:00Z"));
    let out = run_cli_for_test(&["register", "--verbose"], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

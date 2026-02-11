#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

#[derive(Clone)]
struct WatchBackend {
    now: DateTime<Utc>,
    topics: Arc<Mutex<Vec<TopicSummary>>>,
    messages: Arc<Mutex<BTreeMap<PathBuf, Message>>>,
    seq: Arc<AtomicUsize>,
}

impl WatchBackend {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            topics: Arc::new(Mutex::new(Vec::new())),
            messages: Arc::new(Mutex::new(BTreeMap::new())),
            seq: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn with_topics(self, names: &[&str]) -> Self {
        let mut topics = self.topics.lock().expect("topics lock");
        *topics = names
            .iter()
            .map(|name| TopicSummary {
                name: (*name).to_string(),
                messages: 0,
                last_activity: None,
            })
            .collect();
        drop(topics);
        self
    }

    fn push_message(&self, message: Message) {
        let idx = self.seq.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(format!("/fake/{idx:04}-{}.json", message.id));
        self.messages
            .lock()
            .expect("messages lock")
            .insert(path, message);
    }
}

impl FmailBackend for WatchBackend {
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
        Ok("alice".to_string())
    }

    fn save_message(&self, _message: &mut Message) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("not implemented".to_string())
    }

    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String> {
        Ok(Some(self.topics.lock().expect("topics lock").clone()))
    }

    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        let target_filter = target.map(|value| value.trim().to_lowercase());

        let guard = self.messages.lock().expect("messages lock");
        let mut out = Vec::new();

        for (path, message) in guard.iter() {
            let include = match target_filter.as_deref() {
                None => true,
                Some(filter) => message.to.to_lowercase() == filter,
            };
            if include {
                out.push(path.clone());
            }
        }

        Ok(out)
    }

    fn read_message_at(&self, path: &Path) -> Result<Message, String> {
        self.messages
            .lock()
            .expect("messages lock")
            .get(path)
            .cloned()
            .ok_or_else(|| format!("message not found: {}", path.display()))
    }

    fn init_project(&self, _project_id: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn gc_messages(&self, _days: i64, _dry_run: bool) -> Result<String, String> {
        Ok(String::new())
    }
}

fn rfc3339(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("parse rfc3339")
        .with_timezone(&Utc)
}

fn make_message(id: &str, to: &str, body: &str) -> Message {
    Message {
        id: id.to_string(),
        from: "alice".to_string(),
        to: to.to_string(),
        time: rfc3339("2026-02-09T12:00:00Z"),
        body: serde_json::Value::String(body.to_string()),
        reply_to: String::new(),
        priority: String::new(),
        host: String::new(),
        tags: Vec::new(),
    }
}

fn golden(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/golden/watch/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read golden")
}

#[test]
fn watch_help_matches_golden() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["watch", "--help"], &backend);

    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, golden("help.txt"));
}

#[test]
fn watch_emits_new_topic_message_json_and_exits_on_count() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z")).with_topics(&["task"]);
    let producer = backend.clone();

    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        producer.push_message(make_message(
            "20260209-120000-0001",
            "task",
            "hello from watch",
        ));
    });

    let out = run_cli_for_test(
        &["watch", "task", "--json", "--count", "1", "--timeout", "2s"],
        &backend,
    );

    sender.join().expect("join sender");

    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);

    let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("parse json");
    assert_eq!(parsed["id"], "20260209-120000-0001");
    assert_eq!(parsed["to"], "task");
    assert_eq!(parsed["body"], "hello from watch");
}

#[test]
fn watch_default_targets_topics_only_not_dm() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z")).with_topics(&["task"]);
    let producer = backend.clone();

    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(40));
        producer.push_message(make_message(
            "20260209-120000-0001",
            "@bob",
            "dm should not appear",
        ));
        thread::sleep(Duration::from_millis(90));
        producer.push_message(make_message(
            "20260209-120000-0002",
            "task",
            "topic should appear",
        ));
    });

    let out = run_cli_for_test(&["watch", "--count", "1", "--timeout", "2s"], &backend);

    sender.join().expect("join sender");

    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("topic should appear"),
        "stdout: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("dm should not appear"),
        "stdout: {}",
        out.stdout
    );
}

#[test]
fn watch_dm_target_normalizes_agent_name() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z")).with_topics(&["task"]);
    let producer = backend.clone();

    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(110));
        producer.push_message(make_message("20260209-120000-0003", "@bob", "hello bob"));
    });

    let out = run_cli_for_test(
        &["watch", "@Bob", "--count", "1", "--timeout", "2s"],
        &backend,
    );

    sender.join().expect("join sender");

    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, golden("dm_text.txt"));
}

#[test]
fn watch_timeout_exits_cleanly_when_no_messages() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z")).with_topics(&["task"]);
    let out = run_cli_for_test(
        &["watch", "task", "--count", "1", "--timeout", "120ms"],
        &backend,
    );

    assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert!(out.stdout.is_empty(), "stdout: {}", out.stdout);
}

#[test]
fn watch_rejects_negative_count() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["watch", "--count", "-1"], &backend);

    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("count must be >= 0"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn watch_rejects_negative_timeout() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["watch", "--timeout", "-1s"], &backend);

    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("timeout must be >= 0"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn watch_rejects_invalid_target() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["watch", "bad target!"], &backend);

    assert_eq!(out.exit_code, 1);
    assert!(
        out.stderr.contains("invalid target"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn watch_unknown_flag_fails() {
    let backend = WatchBackend::new(rfc3339("2026-02-09T12:00:00Z"));
    let out = run_cli_for_test(&["watch", "--jsonl"], &backend);

    assert_eq!(out.exit_code, 2);
    assert!(
        out.stderr.contains("unknown flag"),
        "stderr: {}",
        out.stderr
    );
}

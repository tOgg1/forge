#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

struct MockBackend {
    gc_result: Option<String>,
}

impl MockBackend {
    fn new() -> Self {
        Self { gc_result: None }
    }
}

impl FmailBackend for MockBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(None)
    }

    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
        Ok(None)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
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
        Ok("test-agent".to_string())
    }

    fn save_message(&self, message: &mut Message) -> Result<String, String> {
        if message.id.is_empty() {
            message.id = "20260101-120000-0001".to_string();
        }
        Ok(message.id.clone())
    }

    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("not found".to_string())
    }

    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String> {
        Ok(Some(vec![]))
    }

    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        Ok(vec![])
    }

    fn read_message_at(&self, _path: &Path) -> Result<Message, String> {
        Err("not found".to_string())
    }

    fn init_project(&self, _project_id: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn gc_messages(&self, _days: i64, _dry_run: bool) -> Result<String, String> {
        Ok(self.gc_result.clone().unwrap_or_default())
    }
}

#[test]
fn init_help_matches_golden() {
    let backend = MockBackend::new();
    let out = run_cli_for_test(&["init", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/init/help.txt"));
}

#[test]
fn init_empty_project_flag_errors() {
    let backend = MockBackend::new();
    let out = run_cli_for_test(&["init", "--project", "   "], &backend);
    assert_eq!(out.exit_code, 2);
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.contains("project id is required"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn gc_help_matches_golden() {
    let backend = MockBackend::new();
    let out = run_cli_for_test(&["gc", "--help"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/gc/help.txt"));
}

#[test]
fn gc_dry_run_matches_golden() {
    let mut backend = MockBackend::new();
    backend.gc_result = Some(include_str!("golden/gc/dry_run.txt").to_string());
    let out = run_cli_for_test(&["gc", "--dry-run"], &backend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/gc/dry_run.txt"));
}

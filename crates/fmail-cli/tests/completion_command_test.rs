#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fmail_cli::{run_cli_for_test, FmailBackend};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

/// Minimal backend stub — completion never touches the backend.
struct StubBackend;

impl FmailBackend for StubBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Err("not implemented".to_string())
    }
    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
        Err("not implemented".to_string())
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

#[test]
fn completion_bash_matches_golden() {
    let out = run_cli_for_test(&["completion", "bash"], &StubBackend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/completion/bash.txt"));
}

#[test]
fn completion_zsh_matches_golden() {
    let out = run_cli_for_test(&["completion", "zsh"], &StubBackend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/completion/zsh.txt"));
}

#[test]
fn completion_fish_matches_golden() {
    let out = run_cli_for_test(&["completion", "fish"], &StubBackend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/completion/fish.txt"));
}

#[test]
fn completion_unsupported_shell_errors() {
    let out = run_cli_for_test(&["completion", "tcsh"], &StubBackend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: unsupported shell: tcsh\n");
}

#[test]
fn completion_requires_one_argument() {
    let out = run_cli_for_test(&["completion"], &StubBackend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: accepts exactly 1 argument: [bash|zsh|fish]\n"
    );
}

#[test]
fn completion_help_matches_golden() {
    let out = run_cli_for_test(&["completion", "--help"], &StubBackend);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, include_str!("golden/completion/help.txt"));
}

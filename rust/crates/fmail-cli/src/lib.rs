//! fmail-cli: command-line interface surface for fmail.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "fmail-cli"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait FmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String>;
    fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String>;
    fn now_utc(&self) -> DateTime<Utc>;
    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String>;
    fn set_agent_status(&self, name: &str, status: &str, host: &str)
        -> Result<AgentRecord, String>;
    fn hostname(&self) -> String;
    fn agent_name(&self) -> Result<String, String>;
    fn save_message(&self, message: &mut Message) -> Result<String, String>;
    fn read_file(&self, path: &str) -> Result<String, String>;
    fn list_topics(&self) -> Result<Vec<TopicSummary>, String>;
    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String>;
    fn read_message_at(&self, path: &std::path::Path) -> Result<Message, String>;
    /// Initialize the project (create .fmail + project.json).
    fn init_project(&self, project_id: Option<&str>) -> Result<(), String>;
    /// Garbage-collect old messages. Returns dry-run output or empty string.
    fn gc_messages(&self, days: i64, dry_run: bool) -> Result<String, String>;
}

pub struct FilesystemFmailBackend;

impl FmailBackend for FilesystemFmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.list_agent_records()
    }

    fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.read_agent_record(name)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        chrono::Utc::now()
    }

    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.register_agent_record(name, host, now)
    }

    fn set_agent_status(
        &self,
        name: &str,
        status: &str,
        host: &str,
    ) -> Result<AgentRecord, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.set_agent_status(name, status, host, now)
    }

    fn hostname(&self) -> String {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    fn agent_name(&self) -> Result<String, String> {
        std::env::var(fmail_core::constants::ENV_AGENT)
            .map_err(|_| "FMAIL_AGENT not set".to_string())
    }

    fn save_message(&self, message: &mut Message) -> Result<String, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let now = chrono::Utc::now();
        store.save_message(message, now)
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| format!("read file: {e}"))
    }

    fn list_topics(&self) -> Result<Vec<TopicSummary>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.list_topics()
    }

    fn list_message_files(&self, target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        match target {
            None => store.list_all_message_files(),
            Some(t) if t.starts_with('@') => {
                let agent = t.strip_prefix('@').unwrap_or(t);
                store.list_dm_message_files(agent)
            }
            Some(topic) => store.list_topic_message_files(topic),
        }
    }

    fn read_message_at(&self, path: &std::path::Path) -> Result<Message, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.read_message(path)
    }

    fn init_project(&self, project_id: Option<&str>) -> Result<(), String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.ensure_root()?;

        let existing = store.read_project()?;

        match project_id {
            None => {
                // No explicit project ID: if file exists, done.
                if existing.is_some() {
                    return Ok(());
                }
                let id = fmail_core::project::derive_project_id(&root)?;
                store.ensure_project(&id, chrono::Utc::now())?;
                Ok(())
            }
            Some(id) => {
                // Explicit project ID provided.
                if let Some(ref ex) = existing {
                    if ex.id.trim() == id {
                        return Ok(());
                    }
                }
                let created = existing
                    .and_then(|e| {
                        if e.created == DateTime::<Utc>::default() {
                            None
                        } else {
                            Some(e.created)
                        }
                    })
                    .unwrap_or_else(chrono::Utc::now);

                let project = fmail_core::project::Project {
                    id: id.to_string(),
                    created,
                };
                store.write_project(&project)?;
                Ok(())
            }
        }
    }

    fn gc_messages(&self, days: i64, dry_run: bool) -> Result<String, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
        let files = store.list_gc_files()?;
        let mut output = String::new();

        for file in &files {
            let mut file_time = file.mod_time;
            if let Some(stem) = file.path.file_stem().and_then(|s| s.to_str()) {
                if let Some(ts) = fmail_core::store::parse_message_time(stem) {
                    file_time = ts;
                }
            }

            if file_time == DateTime::<Utc>::default() || file_time >= cutoff {
                continue;
            }

            if dry_run {
                let display_path = file
                    .path
                    .strip_prefix(store.root())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| file.path.to_string_lossy().to_string());
                output.push_str(&display_path);
                output.push('\n');
                continue;
            }

            match std::fs::remove_file(&file.path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("remove {}: {e}", file.path.display())),
            }
        }

        Ok(output)
    }
}

pub mod completion;
pub mod gc;
pub mod init;
pub mod log;
pub mod messages;
pub mod register;
pub mod send;
pub mod status;
pub mod topics;
pub mod watch;
pub mod who;

#[cfg(test)]
pub(crate) mod tests_common;

pub fn run_cli_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let mut out = CommandOutput {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
    };

    let Some((cmd, rest)) = args.split_first() else {
        out.stderr = "missing command\n".to_string();
        out.exit_code = 2;
        return out;
    };

    match *cmd {
        "completion" => completion::run_completion_for_test(rest),
        "gc" => gc::run_gc_for_test(rest, backend),
        "init" => init::run_init_for_test(rest, backend),
        "log" | "logs" => log::run_log_for_test(rest, backend),
        "messages" => messages::run_messages_for_test(rest, backend),
        "register" => register::run_register_for_test(rest, backend),
        "send" => send::run_send_for_test(rest, backend),
        "status" => status::run_status_for_test(rest, backend),
        "topics" | "topic" => topics::run_topics_for_test(rest, backend),
        "watch" => watch::run_watch_for_test(rest, backend),
        "who" => who::run_who_for_test(rest, backend),
        _ => {
            out.stderr = format!("unknown command: {cmd}\n");
            out.exit_code = 2;
            out
        }
    }
}

pub fn run_cli(args: &[String], backend: &dyn FmailBackend) -> CommandOutput {
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_cli_for_test(&refs, backend)
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "fmail-cli");
    }
}

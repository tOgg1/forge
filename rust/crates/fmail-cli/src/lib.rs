//! fmail-cli: command-line interface surface for fmail.

use chrono::{DateTime, Utc};
use fmail_core::agent_registry::AgentRecord;

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
    fn now_utc(&self) -> DateTime<Utc>;
    fn register_agent_record(&self, name: &str, host: &str) -> Result<AgentRecord, String>;
    fn hostname(&self) -> String;
}

pub struct FilesystemFmailBackend;

impl FmailBackend for FilesystemFmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        let root = fmail_core::root::discover_project_root(None)?;
        let store = fmail_core::store::Store::new(&root)?;
        store.list_agent_records()
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

    fn hostname(&self) -> String {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
}

pub mod register;
pub mod who;

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
        "register" => register::run_register_for_test(rest, backend),
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

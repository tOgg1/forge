//! Shared mock backend for CLI unit tests.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

use crate::FmailBackend;

pub struct MockFmailBackend {
    pub gc_result: Option<String>,
}

impl MockFmailBackend {
    pub fn new() -> Self {
        Self { gc_result: None }
    }
}

impl FmailBackend for MockFmailBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Ok(None)
    }

    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
        Ok(None)
    }

    fn now_utc(&self) -> DateTime<Utc> {
        chrono::Utc::now()
    }

    fn register_agent_record(&self, name: &str, _host: &str) -> Result<AgentRecord, String> {
        Ok(AgentRecord {
            name: name.to_string(),
            host: None,
            status: None,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
        })
    }

    fn set_agent_status(
        &self,
        name: &str,
        status: &str,
        _host: &str,
    ) -> Result<AgentRecord, String> {
        Ok(AgentRecord {
            name: name.to_string(),
            host: None,
            status: Some(status.to_string()),
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
        })
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

    fn read_message_at(&self, _path: &std::path::Path) -> Result<Message, String> {
        Err("not found".to_string())
    }

    fn init_project(&self, _project_id: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn gc_messages(&self, _days: i64, _dry_run: bool) -> Result<String, String> {
        Ok(self.gc_result.clone().unwrap_or_default())
    }
}

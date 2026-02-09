use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use forge_db::event_repository::{Event, EventRepository};
use forge_db::{Config, Db};

use crate::runner::RunnerEvent;

const RUNNER_ENTITY_TYPE: &str = "agent";

pub trait EventSink: Send + Sync {
    fn emit(&self, event: &RunnerEvent) -> Result<(), String>;
    fn close(&self) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct NoopSink;

impl EventSink for NoopSink {
    fn emit(&self, _event: &RunnerEvent) -> Result<(), String> {
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct SocketEventSink {
    inner: Mutex<SocketEventSinkInner>,
}

#[derive(Debug)]
struct SocketEventSinkInner {
    stream: Option<UnixStream>,
    closed: bool,
}

impl SocketEventSink {
    pub fn connect(path: &str) -> Result<Self, String> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err("event socket path is required".to_string());
        }

        let stream = UnixStream::connect(trimmed).map_err(|err| err.to_string())?;
        Ok(Self {
            inner: Mutex::new(SocketEventSinkInner {
                stream: Some(stream),
                closed: false,
            }),
        })
    }
}

impl EventSink for SocketEventSink {
    fn emit(&self, event: &RunnerEvent) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "event socket lock poisoned".to_string())?;
        if inner.closed {
            return Err("event socket closed".to_string());
        }
        let stream = inner
            .stream
            .as_mut()
            .ok_or_else(|| "event socket closed".to_string())?;
        serde_json::to_writer(&mut *stream, event).map_err(|err| err.to_string())?;
        stream.write_all(b"\n").map_err(|err| err.to_string())
    }

    fn close(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "event socket lock poisoned".to_string())?;
        if inner.closed {
            return Ok(());
        }
        inner.closed = true;
        let _ = inner.stream.take();
        Ok(())
    }
}

#[derive(Debug)]
pub struct DatabaseEventSink {
    inner: Mutex<DatabaseEventSinkInner>,
}

#[derive(Debug)]
struct DatabaseEventSinkInner {
    db: Option<Db>,
    workspace_id: String,
    agent_id: String,
}

impl DatabaseEventSink {
    pub fn open(
        path: &Path,
        busy_timeout_ms: u64,
        workspace_id: &str,
        agent_id: &str,
    ) -> Result<Self, String> {
        let mut db = Db::open(Config {
            path: path.to_path_buf(),
            busy_timeout_ms,
        })
        .map_err(|err| err.to_string())?;
        db.migrate_up().map_err(|err| err.to_string())?;

        Ok(Self {
            inner: Mutex::new(DatabaseEventSinkInner {
                db: Some(db),
                workspace_id: workspace_id.to_string(),
                agent_id: agent_id.to_string(),
            }),
        })
    }
}

impl EventSink for DatabaseEventSink {
    fn emit(&self, event: &RunnerEvent) -> Result<(), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "event database lock poisoned".to_string())?;
        let db = inner
            .db
            .as_ref()
            .ok_or_else(|| "event repository is required".to_string())?;

        let payload = serde_json::to_string(&event.data).map_err(|err| err.to_string())?;
        let mut metadata = HashMap::new();
        if !inner.workspace_id.is_empty() {
            metadata.insert("workspace_id".to_string(), inner.workspace_id.clone());
        }
        let timestamp = if event.timestamp.trim().is_empty() {
            Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        } else {
            event.timestamp.clone()
        };

        let mut repo_event = Event {
            id: String::new(),
            timestamp,
            event_type: runner_event_type(&event.event_type),
            entity_type: RUNNER_ENTITY_TYPE.to_string(),
            entity_id: inner.agent_id.clone(),
            payload,
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
        };

        let repo = EventRepository::new(db);
        repo.create(&mut repo_event).map_err(|err| err.to_string())
    }

    fn close(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "event database lock poisoned".to_string())?;
        inner.db = None;
        Ok(())
    }
}

fn runner_event_type(event_type: &str) -> String {
    let trimmed = event_type.trim();
    if let Some(rest) = trimmed.strip_prefix("runner.") {
        if rest.is_empty() {
            return "runner.unknown".to_string();
        }
        return trimmed.to_string();
    }
    if trimmed.is_empty() {
        return "runner.unknown".to_string();
    }
    format!("runner.{trimmed}")
}

#[cfg(test)]
mod tests {
    use forge_db::event_repository::EventRepository;
    use forge_db::{Config, Db};
    use tempfile::tempdir;

    use super::{runner_event_type, DatabaseEventSink, EventSink, SocketEventSink};
    use crate::runner::RunnerEvent;

    fn must<T, E: std::fmt::Display>(res: Result<T, E>) -> T {
        match res {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn runner_event_type_normalizes_prefix() {
        assert_eq!(runner_event_type("heartbeat"), "runner.heartbeat");
        assert_eq!(runner_event_type("runner.busy"), "runner.busy");
        assert_eq!(runner_event_type(""), "runner.unknown");
        assert_eq!(runner_event_type(" runner.pause "), "runner.pause");
    }

    #[test]
    fn socket_sink_requires_path() {
        let err = match SocketEventSink::connect("  ") {
            Ok(_) => panic!("expected path validation error"),
            Err(err) => err,
        };
        assert!(err.contains("event socket path is required"));
    }

    #[test]
    fn database_sink_persists_runner_event() {
        let tmp = must(tempdir());
        let db_path = tmp.path().join("runner.db");

        let sink = must(DatabaseEventSink::open(&db_path, 5000, "ws-1", "agent-1"));
        let event = RunnerEvent {
            event_type: "output_line".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            workspace_id: String::new(),
            agent_id: String::new(),
            data: Some(serde_json::json!({ "line": "hello" })),
        };
        must(sink.emit(&event));
        must(sink.close());

        let db = must(Db::open(Config::new(&db_path)));
        let repo = EventRepository::new(&db);
        let events = must(repo.list_by_entity("agent", "agent-1", 10));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "runner.output_line");
        assert!(events[0].payload.contains("hello"));
        assert_eq!(
            events[0]
                .metadata
                .as_ref()
                .and_then(|m| m.get("workspace_id"))
                .cloned(),
            Some("ws-1".to_string())
        );
    }
}

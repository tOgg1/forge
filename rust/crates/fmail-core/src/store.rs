//! fmail-core storage primitives.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::agent_registry::AgentRecord;
use crate::validate::normalize_agent_name;

/// Sentinel error message for agent-already-exists.
pub const ERR_AGENT_EXISTS: &str = "agent already exists";

/// Store rooted at `<project_root>/.fmail`.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Create a store rooted at `<project_root>/.fmail`.
    pub fn new(project_root: &Path) -> Result<Self, String> {
        let trimmed = project_root.to_string_lossy().trim().to_string();
        if trimmed.is_empty() {
            return Err("project root required".to_string());
        }
        let abs = project_root
            .canonicalize()
            .map_err(|e| format!("canonicalize project root: {e}"))?;
        Ok(Self {
            root: abs.join(".fmail"),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.root.join("agents")
    }

    /// Ensure the root `.fmail` directory exists.
    pub fn ensure_root(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| format!("create root dir: {e}"))
    }

    /// Register a new agent record, failing if the name is already taken.
    ///
    /// Go parity: `RegisterAgentRecord` in `agent_registry.go`.
    /// Uses exclusive file creation to prevent races.
    pub fn register_agent_record(
        &self,
        name: &str,
        host: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentRecord, String> {
        let (path, normalized) = self.agent_record_path(name)?;
        self.ensure_root()?;
        fs::create_dir_all(self.agents_dir()).map_err(|e| format!("create agents dir: {e}"))?;

        let host_trimmed = host.trim();
        let record = AgentRecord {
            name: normalized,
            host: if host_trimmed.is_empty() {
                None
            } else {
                Some(host_trimmed.to_string())
            },
            status: None,
            first_seen: now,
            last_seen: now,
        };

        let data =
            serde_json::to_string_pretty(&record).map_err(|e| format!("encode agent: {e}"))?;

        write_file_exclusive(&path, data.as_bytes())?;
        Ok(record)
    }

    /// Resolve the file path and normalized name for an agent.
    fn agent_record_path(&self, name: &str) -> Result<(PathBuf, String), String> {
        let normalized = normalize_agent_name(name)?;
        let path = self.agents_dir().join(format!("{normalized}.json"));
        Ok((path, normalized))
    }

    /// List all known agent records.
    ///
    /// Go parity: returns `None` when the agents directory is missing (nil slice => json `null`).
    pub fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        let dir = self.agents_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Ok(None);
                }
                return Err(format!("read agents dir: {err}"));
            }
        };

        let mut records: Vec<AgentRecord> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("read agents dir entry: {e}"))?;
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let data = fs::read_to_string(&path).map_err(|e| format!("read {:?}: {e}", path))?;
            let mut record: AgentRecord =
                serde_json::from_str(&data).map_err(|e| format!("parse {:?}: {e}", path))?;

            if record.name.trim().is_empty() {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    record.name = stem.to_string();
                }
            }
            records.push(record);
        }

        records.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Some(records))
    }
}

/// Write data to a file, failing if it already exists (O_EXCL).
///
/// Go parity: `writeFileExclusivePerm`.
fn write_file_exclusive(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ERR_AGENT_EXISTS.to_string()
            } else {
                format!("create agent file: {e}")
            }
        })?;
    file.write_all(data)
        .map_err(|e| format!("write agent file: {e}"))?;
    file.flush().map_err(|e| format!("flush agent file: {e}"))?;
    Ok(())
}

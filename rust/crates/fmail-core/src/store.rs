//! fmail-core storage primitives.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use serde::Serialize;

use crate::agent_registry::AgentRecord;
use crate::message::{generate_message_id, Message, MAX_MESSAGE_SIZE};
use crate::validate::{normalize_agent_name, normalize_target};

/// Summary info for a topic (used by `topics` command).
#[derive(Debug, Clone, Serialize)]
pub struct TopicSummary {
    pub name: String,
    pub messages: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<DateTime<Utc>>,
}

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

    /// Read an agent record by name.
    ///
    /// Go parity: `ReadAgentRecord` returns os.ErrNotExist when missing; Rust returns `Ok(None)`.
    pub fn read_agent_record(&self, name: &str) -> Result<Option<AgentRecord>, String> {
        let (path, normalized) = self.agent_record_path(name)?;
        let mut record = match read_agent_record_file(&path)? {
            Some(r) => r,
            None => return Ok(None),
        };

        if record.name.trim().is_empty() {
            record.name = normalized;
        }
        Ok(Some(record))
    }

    /// Set or clear an agent's status.
    ///
    /// Go parity: `SetAgentStatus` creates the record when missing.
    pub fn set_agent_status(
        &self,
        name: &str,
        status: &str,
        host: &str,
        now: DateTime<Utc>,
    ) -> Result<AgentRecord, String> {
        let (path, normalized) = self.agent_record_path(name)?;
        self.ensure_root()?;
        fs::create_dir_all(self.agents_dir()).map_err(|e| format!("create agents dir: {e}"))?;

        let mut record = read_agent_record_file(&path)?.unwrap_or(AgentRecord {
            name: normalized.clone(),
            host: None,
            status: None,
            first_seen: now,
            last_seen: now,
        });

        if record.name.trim().is_empty() {
            record.name = normalized;
        }
        if record.first_seen == DateTime::<Utc>::default() {
            record.first_seen = now;
        }
        record.last_seen = now;

        let trimmed = status.trim();
        record.status = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };

        let host_trimmed = host.trim();
        if !host_trimmed.is_empty() {
            record.host = Some(host_trimmed.to_string());
        }

        write_agent_record_file(&path, &record)?;
        Ok(record)
    }

    // -----------------------------------------------------------------
    // Message paths
    // -----------------------------------------------------------------

    /// Directory for a topic's messages.
    pub fn topic_dir(&self, topic: &str) -> PathBuf {
        self.root.join("topics").join(topic)
    }

    /// Directory for direct messages to an agent.
    pub fn dm_dir(&self, agent: &str) -> PathBuf {
        self.root.join("dm").join(agent)
    }

    /// File path for a topic message.
    pub fn topic_message_path(&self, topic: &str, id: &str) -> PathBuf {
        self.topic_dir(topic).join(format!("{id}.json"))
    }

    /// File path for a direct message.
    pub fn dm_message_path(&self, agent: &str, id: &str) -> PathBuf {
        self.dm_dir(agent).join(format!("{id}.json"))
    }

    // -----------------------------------------------------------------
    // Save / read messages
    // -----------------------------------------------------------------

    /// Save a message, generating an ID if empty. Retries on ID collision.
    ///
    /// Go parity: `Store.SaveMessage`.
    pub fn save_message(
        &self,
        message: &mut Message,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<String, String> {
        // Normalize from/to
        message.from = normalize_agent_name(&message.from)?;
        let (normalized_to, is_dm) = normalize_target(&message.to)?;
        message.to = normalized_to.clone();

        if message.time == chrono::DateTime::<chrono::Utc>::default() {
            message.time = now;
        }

        if message.id.is_empty() {
            message.id = generate_message_id(now);
        }

        message.validate().map_err(|e| format!("validate: {e}"))?;

        self.ensure_root()?;

        let (dir, _is_dm_dir) = if is_dm {
            let agent = normalized_to.strip_prefix('@').unwrap_or(&normalized_to);
            let d = self.dm_dir(agent);
            fs::create_dir_all(&d).map_err(|e| format!("create dm dir: {e}"))?;
            // Set restrictive permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&d, fs::Permissions::from_mode(0o700));
            }
            (d, true)
        } else {
            let d = self.topic_dir(&normalized_to);
            fs::create_dir_all(&d).map_err(|e| format!("create topic dir: {e}"))?;
            (d, false)
        };

        const MAX_ID_RETRIES: usize = 10;
        for _attempt in 0..MAX_ID_RETRIES {
            let data = serde_json::to_string_pretty(message)
                .map_err(|e| format!("encode message: {e}"))?;

            if data.len() > MAX_MESSAGE_SIZE {
                return Err("message exceeds 1MB limit".to_string());
            }

            let path = dir.join(format!("{}.json", message.id));
            match write_file_exclusive(&path, data.as_bytes()) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let perm = if is_dm { 0o600 } else { 0o644 };
                        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(perm));
                    }
                    return Ok(message.id.clone());
                }
                Err(e) if e.contains("already exists") || e.contains("AlreadyExists") => {
                    message.id = generate_message_id(now);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err("message id collision".to_string())
    }

    /// Read a message from a file path.
    pub fn read_message(&self, path: &Path) -> Result<Message, String> {
        let data = fs::read_to_string(path).map_err(|e| format!("read message: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("parse message: {e}"))
    }

    // -----------------------------------------------------------------
    // List topics
    // -----------------------------------------------------------------

    /// List all topics with message counts and last activity.
    ///
    /// Go parity: `Store.ListTopics`.
    pub fn list_topics(&self) -> Result<Vec<TopicSummary>, String> {
        let topics_dir = self.root.join("topics");
        let entries = match fs::read_dir(&topics_dir) {
            Ok(entries) => entries,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Ok(vec![]);
                }
                return Err(format!("read topics dir: {err}"));
            }
        };

        let mut topics = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("read topics dir entry: {e}"))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Validate topic name
            if crate::validate::normalize_topic(&name).is_err() {
                continue;
            }
            let (count, last_activity) = scan_topic_dir(&path)?;
            if count == 0 {
                continue;
            }
            topics.push(TopicSummary {
                name,
                messages: count,
                last_activity,
            });
        }
        topics.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(topics)
    }

    // -----------------------------------------------------------------
    // List message files
    // -----------------------------------------------------------------

    /// List all `.json` message file paths in a topic directory.
    pub fn list_topic_message_files(&self, topic: &str) -> Result<Vec<PathBuf>, String> {
        list_json_files(&self.topic_dir(topic))
    }

    /// List all `.json` message file paths in a DM directory.
    pub fn list_dm_message_files(&self, agent: &str) -> Result<Vec<PathBuf>, String> {
        list_json_files(&self.dm_dir(agent))
    }

    /// List all `.json` message file paths across all topics.
    pub fn list_all_topic_message_files(&self) -> Result<Vec<PathBuf>, String> {
        let topics_dir = self.root.join("topics");
        list_json_files_recursive(&topics_dir)
    }

    /// List all `.json` message file paths across all DMs.
    pub fn list_all_dm_message_files(&self) -> Result<Vec<PathBuf>, String> {
        let dm_dir = self.root.join("dm");
        list_json_files_recursive(&dm_dir)
    }

    /// List all message files (both topics and DMs).
    pub fn list_all_message_files(&self) -> Result<Vec<PathBuf>, String> {
        let mut files = self.list_all_topic_message_files()?;
        files.extend(self.list_all_dm_message_files()?);
        files.sort();
        Ok(files)
    }

    // -----------------------------------------------------------------
    // Project
    // -----------------------------------------------------------------

    /// Path to `project.json`.
    ///
    /// Go parity: `Store.ProjectFile()`.
    pub fn project_file(&self) -> PathBuf {
        self.root.join("project.json")
    }

    /// Ensure a project file exists, creating it if needed.
    ///
    /// Go parity: `Store.EnsureProject`.
    pub fn ensure_project(
        &self,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<crate::project::Project, String> {
        self.ensure_root()?;
        let path = self.project_file();

        // If file already exists, read and return it.
        if path.exists() {
            return read_project_file(&path);
        }

        let project = crate::project::Project {
            id: id.to_string(),
            created: now,
        };
        let data =
            serde_json::to_string_pretty(&project).map_err(|e| format!("encode project: {e}"))?;

        match write_file_exclusive(&path, data.as_bytes()) {
            Ok(()) => Ok(project),
            Err(e) if e == ERR_AGENT_EXISTS => {
                // Another process created it; read it.
                read_project_file(&path)
            }
            Err(e) => Err(format!("write project: {e}")),
        }
    }

    /// Read project.json if it exists, returning None if missing.
    ///
    /// Go parity: `readProjectIfExists`.
    pub fn read_project(&self) -> Result<Option<crate::project::Project>, String> {
        let path = self.project_file();
        match read_project_file(&path) {
            Ok(p) => Ok(Some(p)),
            Err(e) if e.contains("not found") || e.contains("No such file") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write a project to project.json (overwrite).
    pub fn write_project(&self, project: &crate::project::Project) -> Result<(), String> {
        self.ensure_root()?;
        let path = self.project_file();
        let data =
            serde_json::to_string_pretty(project).map_err(|e| format!("encode project: {e}"))?;
        fs::write(&path, data.as_bytes()).map_err(|e| format!("write project: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o644));
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // GC
    // -----------------------------------------------------------------

    /// List all message files eligible for GC (topics + DMs) with modification times.
    ///
    /// Go parity: `listGCFiles` in `gc.go`.
    pub fn list_gc_files(&self) -> Result<Vec<GcFile>, String> {
        let mut files = Vec::new();

        // Scan topics
        let topics_root = self.root.join("topics");
        for name in list_sub_dirs(&topics_root)? {
            if crate::validate::normalize_topic(&name).is_err() {
                continue;
            }
            files.extend(list_files_with_modtime(&topics_root.join(&name))?);
        }

        // Scan DMs
        let dm_root = self.root.join("dm");
        for name in list_sub_dirs(&dm_root)? {
            if crate::validate::normalize_agent_name(&name).is_err() {
                continue;
            }
            files.extend(list_files_with_modtime(&dm_root.join(&name))?);
        }

        Ok(files)
    }
}

/// A file with its path and modification time, used for GC.
///
/// Go parity: `messageFile` struct in `watch.go`.
#[derive(Debug, Clone)]
pub struct GcFile {
    pub path: PathBuf,
    pub mod_time: DateTime<Utc>,
}

/// Read and parse a project.json file.
fn read_project_file(path: &Path) -> Result<crate::project::Project, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("read project: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse project: {e}"))
}

/// List subdirectory names in a directory, sorted alphabetically.
///
/// Go parity: `listSubDirs` in `gc.go`.
fn list_sub_dirs(root: &Path) -> Result<Vec<String>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            }
            return Err(format!("read dir: {err}"));
        }
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        if entry
            .file_type()
            .map_err(|e| format!("file type: {e}"))?
            .is_dir()
        {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// List `.json` files in a directory with modification times, sorted by name.
///
/// Go parity: `listFilesInDir` in `watch.go`.
fn list_files_with_modtime(dir: &Path) -> Result<Vec<GcFile>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            }
            return Err(format!("read dir: {err}"));
        }
    };

    let mut files: Vec<(String, GcFile)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                return Err(format!("metadata: {e}"));
            }
        };
        let mod_time = meta.modified().map_err(|e| format!("mod time: {e}"))?;
        let mod_time_utc: DateTime<Utc> = mod_time.into();
        files.push((
            name,
            GcFile {
                path,
                mod_time: mod_time_utc,
            },
        ));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files.into_iter().map(|(_, f)| f).collect())
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

fn read_agent_record_file(path: &Path) -> Result<Option<AgentRecord>, String> {
    let data = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(format!("read agent file: {err}"));
        }
    };
    let record: AgentRecord =
        serde_json::from_str(&data).map_err(|e| format!("parse agent file: {e}"))?;
    Ok(Some(record))
}

fn write_agent_record_file(path: &Path, record: &AgentRecord) -> Result<(), String> {
    let data = serde_json::to_string_pretty(record).map_err(|e| format!("encode agent: {e}"))?;
    fs::write(path, data.as_bytes()).map_err(|e| format!("write agent file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o644));
    }

    Ok(())
}

/// Scan a topic directory for message count and last activity time.
///
/// Go parity: `scanTopic` in `store.go`.
fn scan_topic_dir(dir: &Path) -> Result<(usize, Option<DateTime<Utc>>), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok((0, None));
            }
            return Err(format!("read topic dir: {err}"));
        }
    };

    let mut count = 0usize;
    let mut last_activity: Option<DateTime<Utc>> = None;

    for entry in entries {
        let entry = entry.map_err(|e| format!("read topic entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        count += 1;

        // Try to extract timestamp from filename (YYYYMMDD-HHMMSS prefix)
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Some(ts) = parse_message_time(stem) {
                match &last_activity {
                    Some(prev) if ts > *prev => last_activity = Some(ts),
                    None => last_activity = Some(ts),
                    _ => {}
                }
            }
        }
    }
    Ok((count, last_activity))
}

/// Parse a message timestamp from a filename prefix.
///
/// Go parity: `parseMessageTime` - expects `YYYYMMDD-HHMMSS` (15 chars).
pub fn parse_message_time(filename: &str) -> Option<DateTime<Utc>> {
    if filename.len() < 15 {
        return None;
    }
    let prefix = &filename[..15];
    chrono::NaiveDateTime::parse_from_str(prefix, "%Y%m%d-%H%M%S")
        .ok()
        .map(|dt| dt.and_utc())
}

/// List all `.json` files in a directory (non-recursive, sorted).
fn list_json_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            }
            return Err(format!("read dir: {err}"));
        }
    };

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// List all `.json` files across all subdirectories (one level deep).
fn list_json_files_recursive(parent: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Ok(vec![]);
            }
            return Err(format!("read dir: {err}"));
        }
    };

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read dir entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(list_json_files(&path)?);
        }
    }
    files.sort();
    Ok(files)
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

use super::{RenderedQueueItem, SeqBackend, Sequence, SequenceStep, SequenceVar};
use crate::context::{ContextBackend, FilesystemContextBackend};

#[derive(Debug, Clone)]
pub struct FilesystemSeqBackend {
    db_path: PathBuf,
    context_backend: FilesystemContextBackend,
}

impl Default for FilesystemSeqBackend {
    fn default() -> Self {
        Self::open_from_env()
    }
}

impl FilesystemSeqBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
            context_backend: FilesystemContextBackend::default(),
        }
    }

    #[cfg(test)]
    pub fn new(db_path: PathBuf, context_backend: FilesystemContextBackend) -> Self {
        Self {
            db_path,
            context_backend,
        }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    fn resolve_project_dir(&self) -> Result<Option<PathBuf>, String> {
        // Prefer workspace context (repo_path from DB) when available.
        let ctx = self.context_backend.load_context().unwrap_or_default();
        if !ctx.workspace_id.is_empty() && self.db_path.exists() {
            let db = self.open_db()?;
            let conn = db.conn();
            let repo_path = conn
                .query_row(
                    "SELECT repo_path FROM workspaces WHERE id = ?1",
                    rusqlite::params![ctx.workspace_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|err| err.to_string())?;
            if let Some(repo_path) = repo_path {
                return Ok(Some(PathBuf::from(repo_path)));
            }
        }

        // Fall back to git root of cwd (Go: getGitRoot(cwd)).
        let root = fmail_core::root::discover_project_root(None)?;
        Ok(Some(root))
    }

    fn load_sequences_from_dir(dir: &Path) -> Result<Vec<Sequence>, String> {
        let entries = match std::fs::read_dir(dir) {
            Ok(v) => v,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("read sequences dir {}: {err}", dir.display())),
        };

        let mut sequences = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| err.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            let seq = load_sequence(&path)?;
            sequences.push(seq);
        }

        sequences.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(sequences)
    }

    fn load_builtin_sequences_from_repo(&self) -> Result<Vec<Sequence>, String> {
        // Best-effort: when running from the monorepo, load builtins from Go sources.
        // In installed binaries, this directory may not exist.
        let Some(repo_root) = discover_repo_root_containing("internal/sequences/builtin") else {
            return Ok(Vec::new());
        };
        let dir = repo_root.join("internal/sequences/builtin");
        let mut sequences = Self::load_sequences_from_dir(&dir)?;
        for seq in &mut sequences {
            seq.source = "builtin".to_string();
        }
        Ok(sequences)
    }

    fn load_sequences_from_search_paths(&self) -> Result<Vec<Sequence>, String> {
        let mut paths: Vec<PathBuf> = Vec::new();

        if let Ok(Some(project_dir)) = self.resolve_project_dir() {
            paths.push(project_dir.join(".forge").join("sequences"));
        }

        if let Some(home) = std::env::var_os("HOME") {
            paths.push(
                PathBuf::from(home)
                    .join(".config")
                    .join("forge")
                    .join("sequences"),
            );
        }

        paths.push(PathBuf::from("/usr/share/forge/sequences"));

        let mut seen: HashMap<String, Sequence> = HashMap::new();
        let mut order: Vec<String> = Vec::new();

        for path in paths {
            let seqs = Self::load_sequences_from_dir(&path)?;
            for seq in seqs {
                if seen.contains_key(&seq.name) {
                    continue;
                }
                order.push(seq.name.clone());
                seen.insert(seq.name.clone(), seq);
            }
        }

        for seq in self.load_builtin_sequences_from_repo()? {
            if seen.contains_key(&seq.name) {
                continue;
            }
            order.push(seq.name.clone());
            seen.insert(seq.name.clone(), seq);
        }

        Ok(order
            .into_iter()
            .filter_map(|name| seen.remove(&name))
            .collect())
    }

    fn ensure_agent_exists(conn: &rusqlite::Connection, agent_id: &str) -> Result<(), String> {
        let row = conn
            .query_row(
                "SELECT id FROM agents WHERE id = ?1",
                rusqlite::params![agent_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if row.is_some() {
            return Ok(());
        }
        Err(format!("agent not found: {agent_id}"))
    }

    fn now_rfc3339() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    fn next_item_id() -> String {
        format!("qi_{}", uuid::Uuid::new_v4().simple())
    }

    fn next_position(conn: &rusqlite::Connection, agent_id: &str) -> Result<i64, String> {
        let current = conn
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM queue_items WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| err.to_string())?;
        Ok(current + 1)
    }

    fn insert_item(
        conn: &rusqlite::Connection,
        id: &str,
        agent_id: &str,
        item_type: &str,
        position: i64,
        payload: &str,
    ) -> Result<(), String> {
        conn.execute(
            "INSERT INTO queue_items (
                id, agent_id, type, position, status, attempts, payload_json, created_at
            ) VALUES (
                ?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6
            )",
            rusqlite::params![
                id,
                agent_id,
                item_type,
                position,
                payload,
                Self::now_rfc3339()
            ],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    }
}

impl SeqBackend for FilesystemSeqBackend {
    fn load_sequences(&self) -> Result<Vec<Sequence>, String> {
        self.load_sequences_from_search_paths()
    }

    fn user_sequence_dir(&self) -> Result<PathBuf, String> {
        Ok(resolve_config_dir()?.join("sequences"))
    }

    fn project_sequence_dir(&self) -> Result<Option<PathBuf>, String> {
        let Some(project_dir) = self.resolve_project_dir()? else {
            return Ok(None);
        };
        Ok(Some(project_dir.join(".forge").join("sequences")))
    }

    fn file_exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|err| err.to_string())
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        std::fs::write(path, contents).map_err(|err| err.to_string())
    }

    fn remove_file(&self, path: &Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|err| err.to_string())
    }

    fn open_editor(&self, path: &Path) -> Result<(), String> {
        let editor = std::env::var("EDITOR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                std::env::var("VISUAL")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
            })
            .ok_or_else(|| "EDITOR is not set".to_string())?;

        let mut parts = editor.split_whitespace();
        let bin = parts
            .next()
            .ok_or_else(|| "EDITOR is empty".to_string())?
            .to_string();
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();

        let mut cmd = std::process::Command::new(&bin);
        cmd.args(args);
        cmd.arg(path);
        cmd.stdin(std::process::Stdio::inherit());
        cmd.stdout(std::process::Stdio::inherit());
        cmd.stderr(std::process::Stdio::inherit());
        let status = cmd
            .status()
            .map_err(|err| format!("failed to run editor {bin:?}: {err}"))?;
        if !status.success() {
            return Err(format!("failed to run editor {bin:?}"));
        }
        Ok(())
    }

    fn resolve_agent_id(&self, agent_flag: &str) -> Result<String, String> {
        let ctx = self.context_backend.load_context().unwrap_or_default();

        if agent_flag.trim().is_empty() {
            if !ctx.agent_id.is_empty() {
                return Ok(ctx.agent_id);
            }
            return Err("agent id required".to_string());
        }

        self.context_backend
            .resolve_agent(agent_flag.trim(), &ctx.workspace_id)
            .map(|a| a.id)
    }

    fn enqueue_sequence_items(
        &mut self,
        agent_id: &str,
        items: &[RenderedQueueItem],
    ) -> Result<Vec<String>, String> {
        if !self.db_path.exists() {
            return Err("database not found".to_string());
        }
        let db = self.open_db()?;
        let conn = db.conn();
        Self::ensure_agent_exists(conn, agent_id)?;

        let mut next = Self::next_position(conn, agent_id)?;
        let mut ids = Vec::with_capacity(items.len());
        for item in items {
            let id = Self::next_item_id();
            let (item_type, payload) = match item {
                RenderedQueueItem::Message { text } => {
                    ("message", serde_json::json!({ "text": text }).to_string())
                }
                RenderedQueueItem::Pause {
                    duration_seconds,
                    reason,
                } => {
                    let payload = if reason.trim().is_empty() {
                        serde_json::json!({ "duration_seconds": duration_seconds }).to_string()
                    } else {
                        serde_json::json!({
                            "duration_seconds": duration_seconds,
                            "reason": reason
                        })
                        .to_string()
                    };
                    ("pause", payload)
                }
                RenderedQueueItem::Conditional {
                    condition_type,
                    expression,
                    message,
                } => {
                    let payload = if expression.trim().is_empty() {
                        serde_json::json!({
                            "condition_type": condition_type,
                            "message": message
                        })
                        .to_string()
                    } else {
                        serde_json::json!({
                            "condition_type": condition_type,
                            "expression": expression,
                            "message": message
                        })
                        .to_string()
                    };
                    ("conditional", payload)
                }
            };
            Self::insert_item(conn, &id, agent_id, item_type, next, &payload)?;
            next += 1;
            ids.push(id);
        }
        Ok(ids)
    }
}

#[derive(Debug, serde::Deserialize)]
struct YamlSequence {
    name: String,
    #[serde(default)]
    description: String,
    steps: Vec<YamlStep>,
    #[serde(default)]
    variables: Vec<YamlVar>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct YamlStep {
    #[serde(rename = "type")]
    step_type: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    when: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    expression: String,
}

#[derive(Debug, serde::Deserialize, Default)]
struct YamlVar {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "default", default)]
    default_value: String,
    #[serde(default)]
    required: bool,
}

fn load_sequence(path: &Path) -> Result<Sequence, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| format!("read sequence {}: {err}", path.display()))?;
    let parsed: YamlSequence = serde_yaml::from_str(&raw)
        .map_err(|err| format!("parse sequence {}: {err}", path.display()))?;
    normalize_yaml_sequence(parsed, &path.to_string_lossy())
}

fn normalize_yaml_sequence(mut seq: YamlSequence, source: &str) -> Result<Sequence, String> {
    seq.name = seq.name.trim().to_string();
    if seq.name.is_empty() {
        return Err("sequence name is required".to_string());
    }
    seq.description = seq.description.trim().to_string();
    if seq.steps.is_empty() {
        return Err("sequence steps are required".to_string());
    }

    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut variables = Vec::new();
    for var in seq.variables {
        let name = var.name.trim().to_string();
        if name.is_empty() {
            return Err("sequence variable name is required".to_string());
        }
        if seen.contains_key(&name) {
            return Err(format!("duplicate sequence variable {name:?}"));
        }
        seen.insert(name.clone(), ());
        variables.push(SequenceVar {
            name,
            description: var.description.trim().to_string(),
            default_value: var.default_value.trim().to_string(),
            required: var.required,
        });
    }

    let mut steps = Vec::new();
    for (idx, mut step) in seq.steps.into_iter().enumerate() {
        step.step_type = step.step_type.trim().to_ascii_lowercase();
        step.content = step.content.trim().to_string();
        step.message = step.message.trim().to_string();
        step.duration = step.duration.trim().to_string();
        step.when = step.when.trim().to_string();
        step.reason = step.reason.trim().to_string();
        step.expression = step.expression.trim().to_string();

        if step.content.is_empty() && !step.message.is_empty() {
            step.content = step.message.clone();
        }
        if !step.content.is_empty() && !step.message.is_empty() && step.content != step.message {
            return Err(format!(
                "sequence step {}: content and message disagree",
                idx + 1
            ));
        }

        match step.step_type.as_str() {
            "message" => {
                if step.content.is_empty() {
                    return Err(format!(
                        "sequence step {}: message content is required",
                        idx + 1
                    ));
                }
            }
            "pause" => {
                if step.duration.is_empty() {
                    return Err(format!(
                        "sequence step {}: pause duration is required",
                        idx + 1
                    ));
                }
            }
            "conditional" => {
                if step.content.is_empty() {
                    return Err(format!(
                        "sequence step {}: conditional message is required",
                        idx + 1
                    ));
                }
            }
            other => {
                return Err(format!(
                    "sequence step {}: unknown step type {other:?}",
                    idx + 1
                ));
            }
        }

        steps.push(SequenceStep {
            step_type: step.step_type,
            content: step.content,
            message: step.message,
            duration: step.duration,
            when: step.when,
            reason: step.reason,
            expression: step.expression,
        });
    }

    let tags: Vec<String> = seq
        .tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    Ok(Sequence {
        name: seq.name,
        description: seq.description,
        steps,
        variables,
        tags,
        source: source.to_string(),
    })
}

fn discover_repo_root_containing(relative: &str) -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    let mut current = start.canonicalize().ok()?;
    loop {
        if current.join(relative).is_dir() {
            return Some(current);
        }
        let parent = current.parent()?.to_path_buf();
        if parent == current {
            return None;
        }
        current = parent;
    }
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}

fn resolve_config_dir() -> Result<PathBuf, String> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let mut path = PathBuf::from(xdg);
        path.push("forge");
        return Ok(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("forge");
        return Ok(path);
    }
    Ok(PathBuf::from(".config/forge"))
}

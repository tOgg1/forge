use std::io::Write;
use std::path::PathBuf;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Persisted CLI context (workspace + agent selection).
/// Serialization matches Go `config.Context` JSON output.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextRecord {
    #[serde(rename = "WorkspaceID")]
    pub workspace_id: String,
    #[serde(rename = "WorkspaceName")]
    pub workspace_name: String,
    #[serde(rename = "AgentID")]
    pub agent_id: String,
    #[serde(rename = "AgentName")]
    pub agent_name: String,
    #[serde(rename = "UpdatedAt")]
    pub updated_at: String,
}

impl Default for ContextRecord {
    fn default() -> Self {
        Self {
            workspace_id: String::new(),
            workspace_name: String::new(),
            agent_id: String::new(),
            agent_name: String::new(),
            updated_at: "0001-01-01T00:00:00Z".to_string(),
        }
    }
}

impl ContextRecord {
    pub fn is_empty(&self) -> bool {
        self.workspace_id.is_empty() && self.agent_id.is_empty()
    }

    pub fn has_workspace(&self) -> bool {
        !self.workspace_id.is_empty()
    }

    pub fn has_agent(&self) -> bool {
        !self.agent_id.is_empty()
    }

    pub fn set_workspace(&mut self, id: &str, name: &str) {
        self.workspace_id = id.to_string();
        self.workspace_name = name.to_string();
        // Clear agent when workspace changes (agent belongs to workspace).
        self.agent_id.clear();
        self.agent_name.clear();
    }

    pub fn set_agent(&mut self, id: &str, name: &str) {
        self.agent_id = id.to_string();
        self.agent_name = name.to_string();
    }

    /// Human-readable representation matching Go `Context.String()`.
    pub fn display_string(&self) -> String {
        if self.is_empty() {
            return "(none)".to_string();
        }

        let ws_label = if !self.workspace_name.is_empty() {
            &self.workspace_name
        } else if !self.workspace_id.is_empty() {
            &self.workspace_id
        } else {
            ""
        };

        let agent_label = if !self.agent_name.is_empty() {
            self.agent_name.clone()
        } else if !self.agent_id.is_empty() {
            short_id(&self.agent_id).to_string()
        } else {
            String::new()
        };

        if !ws_label.is_empty() && !agent_label.is_empty() {
            format!("{ws_label}:{agent_label}")
        } else if !ws_label.is_empty() {
            ws_label.to_string()
        } else if !agent_label.is_empty() {
            format!(":{agent_label}")
        } else {
            "(none)".to_string()
        }
    }
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

/// Minimal workspace info returned by the backend resolver.
#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
}

/// Minimal agent info returned by the backend resolver.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub workspace_id: String,
}

// ---------------------------------------------------------------------------
// Filesystem backend (production)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextYaml {
    #[serde(default, rename = "workspace")]
    workspace_id: String,
    #[serde(default, rename = "workspace_name")]
    workspace_name: String,
    #[serde(default, rename = "agent")]
    agent_id: String,
    #[serde(default, rename = "agent_name")]
    agent_name: String,
    #[serde(default, rename = "updated_at")]
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<ContextYaml> for ContextRecord {
    fn from(value: ContextYaml) -> Self {
        Self {
            workspace_id: value.workspace_id,
            workspace_name: value.workspace_name,
            agent_id: value.agent_id,
            agent_name: value.agent_name,
            updated_at: value
                .updated_at
                .map(|ts| ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                .unwrap_or_else(|| "0001-01-01T00:00:00Z".to_string()),
        }
    }
}

impl ContextRecord {
    fn to_yaml(&self) -> ContextYaml {
        ContextYaml {
            workspace_id: self.workspace_id.clone(),
            workspace_name: self.workspace_name.clone(),
            agent_id: self.agent_id.clone(),
            agent_name: self.agent_name.clone(),
            updated_at: parse_rfc3339_utc(&self.updated_at).ok(),
        }
    }
}

fn parse_rfc3339_utc(value: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).map_err(|err| err.to_string())?;
    Ok(parsed.with_timezone(&chrono::Utc))
}

fn now_rfc3339_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn resolve_context_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("forge");
        path.push("context.yaml");
        return path;
    }

    PathBuf::from(".config/forge/context.yaml")
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

#[derive(Debug, Clone)]
pub struct FilesystemContextBackend {
    path: PathBuf,
    db_path: PathBuf,
}

impl Default for FilesystemContextBackend {
    fn default() -> Self {
        Self {
            path: resolve_context_path(),
            db_path: resolve_database_path(),
        }
    }
}

impl FilesystemContextBackend {
    pub fn new(path: PathBuf, db_path: PathBuf) -> Self {
        Self { path, db_path }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        let cfg = forge_db::Config::new(&self.db_path);
        forge_db::Db::open(cfg).map_err(|err| err.to_string())
    }
}

impl ContextBackend for FilesystemContextBackend {
    fn load_context(&self) -> Result<ContextRecord, String> {
        let raw = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ContextRecord::default());
            }
            Err(err) => return Err(format!("failed to read context file: {err}")),
        };

        let parsed: ContextYaml = serde_yaml::from_str(&raw)
            .map_err(|err| format!("failed to parse context file: {err}"))?;
        Ok(parsed.into())
    }

    fn save_context(&self, ctx: &ContextRecord) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create context directory: {err}"))?;
        }
        let text = serde_yaml::to_string(&ctx.to_yaml())
            .map_err(|err| format!("failed to serialize context: {err}"))?;
        std::fs::write(&self.path, text)
            .map_err(|err| format!("failed to write context file: {err}"))?;
        Ok(())
    }

    fn clear_context(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("failed to remove context file: {err}")),
        }
    }

    fn resolve_workspace(&self, target: &str) -> Result<WorkspaceInfo, String> {
        let db = self.open_db()?;

        // Exact ID.
        if let Some(row) = db
            .conn()
            .query_row(
                "SELECT id, name FROM workspaces WHERE id = ?1",
                rusqlite::params![target],
                |r| {
                    Ok(WorkspaceInfo {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|err| err.to_string())?
        {
            return Ok(row);
        }

        // Name or prefix match.
        let mut stmt = db
            .conn()
            .prepare("SELECT id, name FROM workspaces ORDER BY id")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(WorkspaceInfo {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })
            .map_err(|err| err.to_string())?;

        for row in rows {
            let ws = row.map_err(|err| err.to_string())?;
            if ws.name == target || ws.id.starts_with(target) {
                return Ok(ws);
            }
        }

        Err(format!("workspace not found: {target}"))
    }

    fn resolve_agent(&self, target: &str, workspace_id: &str) -> Result<AgentInfo, String> {
        let db = self.open_db()?;

        // Exact ID.
        if let Some(row) = db
            .conn()
            .query_row(
                "SELECT id, workspace_id FROM agents WHERE id = ?1",
                rusqlite::params![target],
                |r| {
                    Ok(AgentInfo {
                        id: r.get(0)?,
                        workspace_id: r.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|err| err.to_string())?
        {
            if !workspace_id.is_empty() && row.workspace_id != workspace_id {
                return Err(format!(
                    "agent {target} does not belong to workspace {workspace_id}"
                ));
            }
            return Ok(row);
        }

        if workspace_id.is_empty() {
            let mut stmt = db
                .conn()
                .prepare("SELECT id, workspace_id FROM agents ORDER BY id")
                .map_err(|err| err.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(AgentInfo {
                        id: r.get(0)?,
                        workspace_id: r.get(1)?,
                    })
                })
                .map_err(|err| err.to_string())?;
            for row in rows {
                let agent = row.map_err(|err| err.to_string())?;
                if agent.id.starts_with(target) {
                    return Ok(agent);
                }
            }
        } else {
            let mut stmt = db
                .conn()
                .prepare("SELECT id, workspace_id FROM agents WHERE workspace_id = ?1 ORDER BY id")
                .map_err(|err| err.to_string())?;
            let rows = stmt
                .query_map([workspace_id], |r| {
                    Ok(AgentInfo {
                        id: r.get(0)?,
                        workspace_id: r.get(1)?,
                    })
                })
                .map_err(|err| err.to_string())?;
            for row in rows {
                let agent = row.map_err(|err| err.to_string())?;
                if agent.id.starts_with(target) {
                    return Ok(agent);
                }
            }
        }

        Err(format!("agent not found: {target}"))
    }

    fn get_workspace(&self, id: &str) -> Result<WorkspaceInfo, String> {
        let db = self.open_db()?;
        db.conn()
            .query_row(
                "SELECT id, name FROM workspaces WHERE id = ?1",
                rusqlite::params![id],
                |r| {
                    Ok(WorkspaceInfo {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                },
            )
            .map_err(|err| err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait ContextBackend {
    /// Load the current context from persistent storage.
    fn load_context(&self) -> Result<ContextRecord, String>;
    /// Save the context to persistent storage.
    fn save_context(&self, ctx: &ContextRecord) -> Result<(), String>;
    /// Clear all context (remove file).
    fn clear_context(&self) -> Result<(), String>;
    /// Resolve a workspace target (ID, name, or prefix) to workspace info.
    fn resolve_workspace(&self, target: &str) -> Result<WorkspaceInfo, String>;
    /// Resolve an agent target (ID or prefix), optionally within a workspace.
    fn resolve_agent(&self, target: &str, workspace_id: &str) -> Result<AgentInfo, String>;
    /// Look up a workspace by its exact ID.
    fn get_workspace(&self, id: &str) -> Result<WorkspaceInfo, String>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InMemoryContextBackend {
    pub context: std::cell::RefCell<ContextRecord>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub agents: Vec<AgentInfo>,
    pub load_error: Option<String>,
    pub save_error: Option<String>,
    pub clear_error: Option<String>,
}

impl ContextBackend for InMemoryContextBackend {
    fn load_context(&self) -> Result<ContextRecord, String> {
        if let Some(ref err) = self.load_error {
            return Err(err.clone());
        }
        Ok(self.context.borrow().clone())
    }

    fn save_context(&self, ctx: &ContextRecord) -> Result<(), String> {
        if let Some(ref err) = self.save_error {
            return Err(err.clone());
        }
        *self.context.borrow_mut() = ctx.clone();
        Ok(())
    }

    fn clear_context(&self) -> Result<(), String> {
        if let Some(ref err) = self.clear_error {
            return Err(err.clone());
        }
        *self.context.borrow_mut() = ContextRecord::default();
        Ok(())
    }

    fn resolve_workspace(&self, target: &str) -> Result<WorkspaceInfo, String> {
        for ws in &self.workspaces {
            if ws.id == target {
                return Ok(ws.clone());
            }
        }
        for ws in &self.workspaces {
            if ws.name == target {
                return Ok(ws.clone());
            }
        }
        for ws in &self.workspaces {
            if ws.id.starts_with(target) {
                return Ok(ws.clone());
            }
        }
        Err(format!("workspace not found: {target}"))
    }

    fn resolve_agent(&self, target: &str, workspace_id: &str) -> Result<AgentInfo, String> {
        for a in &self.agents {
            if a.id == target {
                if !workspace_id.is_empty() && a.workspace_id != workspace_id {
                    return Err(format!(
                        "agent {target} does not belong to workspace {workspace_id}"
                    ));
                }
                return Ok(a.clone());
            }
        }
        let candidates: Vec<&AgentInfo> = if workspace_id.is_empty() {
            self.agents.iter().collect()
        } else {
            self.agents
                .iter()
                .filter(|a| a.workspace_id == workspace_id)
                .collect()
        };
        for a in candidates {
            if a.id.starts_with(target) {
                return Ok(a.clone());
            }
        }
        Err(format!("agent not found: {target}"))
    }

    fn get_workspace(&self, id: &str) -> Result<WorkspaceInfo, String> {
        for ws in &self.workspaces {
            if ws.id == id {
                return Ok(ws.clone());
            }
        }
        Err(format!("workspace not found: {id}"))
    }
}

// ---------------------------------------------------------------------------
// `forge context` entry point (show-only, matches Go `contextCmd`)
// ---------------------------------------------------------------------------

pub fn run_context(
    args: &[String],
    backend: &dyn ContextBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute_context(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute_context(
    args: &[String],
    backend: &dyn ContextBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_context_args(args)?;

    if parsed.help {
        write_context_help(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let ctx = backend
        .load_context()
        .map_err(|e| format!("failed to load context: {e}"))?;

    if parsed.json || parsed.jsonl {
        write_json_output(stdout, &ctx, parsed.jsonl)?;
        return Ok(());
    }

    if ctx.is_empty() {
        writeln!(stdout, "No context set.").map_err(|e| e.to_string())?;
    } else {
        writeln!(stdout, "Context: {}", ctx.display_string()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct ContextParsed {
    json: bool,
    jsonl: bool,
    help: bool,
}

fn parse_context_args(args: &[String]) -> Result<ContextParsed, String> {
    let mut json = false;
    let mut jsonl = false;
    let mut help = false;

    let start = if args.first().is_some_and(|a| a == "context") {
        1
    } else {
        0
    };

    for arg in &args[start..] {
        match arg.as_str() {
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            "-h" | "--help" | "help" => help = true,
            "--quiet" => {}
            other => return Err(format!("unknown flag: {other}")),
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ContextParsed { json, jsonl, help })
}

fn write_context_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        stdout,
        "Show the current workspace and agent context. Alias for 'forge use --show'."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge context [flags]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(stdout, "      --json    output in JSON format")?;
    writeln!(stdout, "      --jsonl   output in JSON Lines format")?;
    writeln!(stdout, "  -h, --help    help for context")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// `forge use` entry point (set/show/clear context, matches Go `useCmd`)
// ---------------------------------------------------------------------------

pub fn run_use(
    args: &[String],
    backend: &dyn ContextBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute_use(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

#[derive(Debug)]
struct UseParsed {
    agent: String,
    workspace: String,
    clear: bool,
    show: bool,
    help: bool,
    json: bool,
    jsonl: bool,
    positional: Option<String>,
}

fn parse_use_args(args: &[String]) -> Result<UseParsed, String> {
    let mut agent = String::new();
    let mut workspace = String::new();
    let mut clear = false;
    let mut show = false;
    let mut help = false;
    let mut json = false;
    let mut jsonl = false;
    let mut positional: Option<String> = None;

    let start = if args.first().is_some_and(|a| a == "use") {
        1
    } else {
        0
    };

    let mut idx = start;
    while idx < args.len() {
        match args[idx].as_str() {
            "--agent" => {
                idx += 1;
                agent = args.get(idx).ok_or("--agent requires a value")?.clone();
            }
            "--workspace" => {
                idx += 1;
                workspace = args.get(idx).ok_or("--workspace requires a value")?.clone();
            }
            "--clear" => clear = true,
            "--show" => show = true,
            "--json" => json = true,
            "--jsonl" => jsonl = true,
            "--quiet" => {}
            "-h" | "--help" | "help" => help = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown flag: {other}"));
            }
            _ => {
                if positional.is_some() {
                    return Err(format!("unexpected argument: {}", args[idx]));
                }
                positional = Some(args[idx].clone());
            }
        }
        idx += 1;
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(UseParsed {
        agent,
        workspace,
        clear,
        show,
        help,
        json,
        jsonl,
        positional,
    })
}

fn execute_use(
    args: &[String],
    backend: &dyn ContextBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_use_args(args)?;

    if parsed.help {
        write_use_help(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Handle --clear
    if parsed.clear {
        backend
            .clear_context()
            .map_err(|e| format!("failed to clear context: {e}"))?;
        writeln!(stdout, "Context cleared.").map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Handle --show or no args (show mode)
    let has_setters =
        !parsed.agent.is_empty() || !parsed.workspace.is_empty() || parsed.positional.is_some();

    if parsed.show || !has_setters {
        let ctx = backend
            .load_context()
            .map_err(|e| format!("failed to load context: {e}"))?;

        if parsed.json || parsed.jsonl {
            write_json_output(stdout, &ctx, parsed.jsonl)?;
            return Ok(());
        }

        if ctx.is_empty() {
            writeln!(stdout, "No context set.").map_err(|e| e.to_string())?;
            writeln!(stdout).map_err(|e| e.to_string())?;
            writeln!(stdout, "Set context with:").map_err(|e| e.to_string())?;
            writeln!(stdout, "  forge use <workspace>       # Set workspace")
                .map_err(|e| e.to_string())?;
            writeln!(stdout, "  forge use --agent <agent>   # Set agent")
                .map_err(|e| e.to_string())?;
        } else {
            writeln!(stdout, "Current context: {}", ctx.display_string())
                .map_err(|e| e.to_string())?;
            if ctx.has_workspace() {
                if !ctx.workspace_name.is_empty() {
                    writeln!(
                        stdout,
                        "  Workspace: {} ({})",
                        ctx.workspace_id, ctx.workspace_name
                    )
                    .map_err(|e| e.to_string())?;
                } else {
                    writeln!(stdout, "  Workspace: {}", ctx.workspace_id)
                        .map_err(|e| e.to_string())?;
                }
            }
            if ctx.has_agent() {
                if !ctx.agent_name.is_empty() {
                    writeln!(stdout, "  Agent: {} ({})", ctx.agent_id, ctx.agent_name)
                        .map_err(|e| e.to_string())?;
                } else {
                    writeln!(stdout, "  Agent: {}", ctx.agent_id).map_err(|e| e.to_string())?;
                }
            }
        }
        return Ok(());
    }

    // Load existing context for modification.
    let mut ctx = backend
        .load_context()
        .map_err(|e| format!("failed to load context: {e}"))?;

    // Handle --workspace flag
    if !parsed.workspace.is_empty() {
        let ws = backend
            .resolve_workspace(&parsed.workspace)
            .map_err(|e| format!("failed to resolve workspace: {e}"))?;
        ctx.set_workspace(&ws.id, &ws.name);
        ctx.updated_at = now_rfc3339_utc();
        writeln!(
            stdout,
            "Workspace set to: {} ({})",
            ws.name,
            short_id(&ws.id)
        )
        .map_err(|e| e.to_string())?;
    }

    // Handle --agent flag
    if !parsed.agent.is_empty() {
        let ws_id = if ctx.workspace_id.is_empty() {
            ""
        } else {
            &ctx.workspace_id
        };
        let agent = backend
            .resolve_agent(&parsed.agent, ws_id)
            .map_err(|e| format!("failed to resolve agent: {e}"))?;
        if ctx.workspace_id.is_empty() || ctx.workspace_id != agent.workspace_id {
            if let Ok(ws) = backend.get_workspace(&agent.workspace_id) {
                ctx.set_workspace(&ws.id, &ws.name);
            }
        }
        ctx.set_agent(&agent.id, short_id(&agent.id));
        ctx.updated_at = now_rfc3339_utc();
        writeln!(stdout, "Agent set to: {}", short_id(&agent.id)).map_err(|e| e.to_string())?;
    }

    // Handle positional argument (workspace or workspace:agent)
    if let Some(ref target) = parsed.positional {
        if target.contains(':') {
            let parts: Vec<&str> = target.splitn(2, ':').collect();
            let ws_target = parts[0];
            let agent_target = parts[1];

            let ws = backend
                .resolve_workspace(ws_target)
                .map_err(|e| format!("failed to resolve workspace '{ws_target}': {e}"))?;
            ctx.set_workspace(&ws.id, &ws.name);

            let agent = backend
                .resolve_agent(agent_target, &ws.id)
                .map_err(|e| format!("failed to resolve agent '{agent_target}': {e}"))?;
            ctx.set_agent(&agent.id, short_id(&agent.id));
            ctx.updated_at = now_rfc3339_utc();

            writeln!(
                stdout,
                "Context set to: {}:{}",
                ws.name,
                short_id(&agent.id)
            )
            .map_err(|e| e.to_string())?;
        } else {
            match backend.resolve_workspace(target) {
                Ok(ws) => {
                    ctx.set_workspace(&ws.id, &ws.name);
                    ctx.updated_at = now_rfc3339_utc();
                    writeln!(
                        stdout,
                        "Workspace set to: {} ({})",
                        ws.name,
                        short_id(&ws.id)
                    )
                    .map_err(|e| e.to_string())?;
                }
                Err(_) => {
                    let ws_id = if ctx.workspace_id.is_empty() {
                        ""
                    } else {
                        &ctx.workspace_id
                    };
                    let agent = backend
                        .resolve_agent(target, ws_id)
                        .map_err(|_| format!("'{target}' is not a valid workspace or agent"))?;
                    if !agent.workspace_id.is_empty() {
                        if let Ok(ws) = backend.get_workspace(&agent.workspace_id) {
                            ctx.set_workspace(&ws.id, &ws.name);
                        }
                    }
                    ctx.set_agent(&agent.id, short_id(&agent.id));
                    ctx.updated_at = now_rfc3339_utc();
                    writeln!(stdout, "Agent set to: {}", short_id(&agent.id))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    // Save context.
    backend
        .save_context(&ctx)
        .map_err(|e| format!("failed to save context: {e}"))?;

    Ok(())
}

fn write_use_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Set the current workspace or agent context")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Context is persisted to ~/.config/forge/context.yaml and used by other commands"
    )?;
    writeln!(stdout, "when explicit flags are not provided.")?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge use [workspace|agent] [flags]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Examples:")?;
    writeln!(
        stdout,
        "  forge use my-project           # Set workspace context by name"
    )?;
    writeln!(
        stdout,
        "  forge use ws_abc123            # Set workspace context by ID"
    )?;
    writeln!(
        stdout,
        "  forge use --agent agent_xyz    # Set agent context (keeps workspace)"
    )?;
    writeln!(
        stdout,
        "  forge use --clear              # Clear all context"
    )?;
    writeln!(
        stdout,
        "  forge use --show               # Show current context"
    )?;
    writeln!(
        stdout,
        "  forge use                      # Show current context (same as --show)"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "      --agent string       set agent context (within current workspace)"
    )?;
    writeln!(stdout, "      --workspace string   set workspace context")?;
    writeln!(stdout, "      --clear              clear all context")?;
    writeln!(stdout, "      --show               show current context")?;
    writeln!(stdout, "      --json               output in JSON format")?;
    writeln!(
        stdout,
        "      --jsonl              output in JSON Lines format"
    )?;
    writeln!(stdout, "  -h, --help               help for use")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn write_json_output(
    output: &mut dyn Write,
    value: &ContextRecord,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
        writeln!(output, "{line}").map_err(|e| e.to_string())?;
    } else {
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        writeln!(output, "{text}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Test-only helper for `forge context`.
pub fn run_context_for_test(args: &[&str], backend: &dyn ContextBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_context(&owned, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

/// Test-only helper for `forge use`.
pub fn run_use_for_test(args: &[&str], backend: &dyn ContextBackend) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_use(&owned, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn empty_backend() -> InMemoryContextBackend {
        InMemoryContextBackend::default()
    }

    fn backend_with_context(ctx: ContextRecord) -> InMemoryContextBackend {
        InMemoryContextBackend {
            context: std::cell::RefCell::new(ctx),
            ..Default::default()
        }
    }

    fn backend_with_data() -> InMemoryContextBackend {
        InMemoryContextBackend {
            workspaces: vec![
                WorkspaceInfo {
                    id: "ws_abc12345".to_string(),
                    name: "my-project".to_string(),
                },
                WorkspaceInfo {
                    id: "ws_def67890".to_string(),
                    name: "other-project".to_string(),
                },
            ],
            agents: vec![
                AgentInfo {
                    id: "agent_xyz12345".to_string(),
                    workspace_id: "ws_abc12345".to_string(),
                },
                AgentInfo {
                    id: "agent_uvw98765".to_string(),
                    workspace_id: "ws_def67890".to_string(),
                },
            ],
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // ContextRecord model tests
    // -----------------------------------------------------------------------

    #[test]
    fn context_is_empty_default() {
        let ctx = ContextRecord::default();
        assert!(ctx.is_empty());
        assert!(!ctx.has_workspace());
        assert!(!ctx.has_agent());
    }

    #[test]
    fn context_is_empty_with_workspace() {
        let ctx = ContextRecord {
            workspace_id: "ws_123".to_string(),
            ..Default::default()
        };
        assert!(!ctx.is_empty());
        assert!(ctx.has_workspace());
    }

    #[test]
    fn context_is_empty_with_agent() {
        let ctx = ContextRecord {
            agent_id: "agent_123".to_string(),
            ..Default::default()
        };
        assert!(!ctx.is_empty());
        assert!(ctx.has_agent());
    }

    #[test]
    fn context_display_string_empty() {
        let ctx = ContextRecord::default();
        assert_eq!(ctx.display_string(), "(none)");
    }

    #[test]
    fn context_display_string_workspace_with_name() {
        let ctx = ContextRecord {
            workspace_id: "ws_123".to_string(),
            workspace_name: "my-project".to_string(),
            ..Default::default()
        };
        assert_eq!(ctx.display_string(), "my-project");
    }

    #[test]
    fn context_display_string_workspace_without_name() {
        let ctx = ContextRecord {
            workspace_id: "ws_123".to_string(),
            ..Default::default()
        };
        assert_eq!(ctx.display_string(), "ws_123");
    }

    #[test]
    fn context_display_string_agent_with_name() {
        let ctx = ContextRecord {
            agent_id: "agent_123".to_string(),
            agent_name: "agent_12".to_string(),
            ..Default::default()
        };
        assert_eq!(ctx.display_string(), ":agent_12");
    }

    #[test]
    fn context_display_string_both_with_names() {
        let ctx = ContextRecord {
            workspace_id: "ws_123".to_string(),
            workspace_name: "my-project".to_string(),
            agent_id: "agent_123".to_string(),
            agent_name: "agent_12".to_string(),
            ..Default::default()
        };
        assert_eq!(ctx.display_string(), "my-project:agent_12");
    }

    #[test]
    fn context_set_workspace_clears_agent() {
        let mut ctx = ContextRecord {
            workspace_id: "ws_old".to_string(),
            agent_id: "agent_old".to_string(),
            agent_name: "old_name".to_string(),
            ..Default::default()
        };
        ctx.set_workspace("ws_new", "new-project");
        assert_eq!(ctx.workspace_id, "ws_new");
        assert_eq!(ctx.workspace_name, "new-project");
        assert!(ctx.agent_id.is_empty());
        assert!(ctx.agent_name.is_empty());
    }

    #[test]
    fn context_set_agent() {
        let mut ctx = ContextRecord::default();
        ctx.set_agent("agent_123", "agent_12");
        assert_eq!(ctx.agent_id, "agent_123");
        assert_eq!(ctx.agent_name, "agent_12");
    }

    #[test]
    fn short_id_truncates() {
        assert_eq!(short_id("agent_xyz12345"), "agent_xy");
    }

    #[test]
    fn short_id_short_input() {
        assert_eq!(short_id("abc"), "abc");
    }

    // -----------------------------------------------------------------------
    // `forge context` command tests
    // -----------------------------------------------------------------------

    #[test]
    fn context_cmd_empty_text() {
        let backend = empty_backend();
        let out = run_context_for_test(&["context"], &backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "No context set.\n");
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn context_cmd_with_context_text() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc12345".to_string(),
            workspace_name: "my-project".to_string(),
            ..Default::default()
        });
        let out = run_context_for_test(&["context"], &backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Context: my-project\n");
    }

    #[test]
    fn context_cmd_json_empty() {
        let backend = empty_backend();
        let out = run_context_for_test(&["context", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_object());
        assert_eq!(parsed["WorkspaceID"], "");
        assert_eq!(parsed["AgentID"], "");
    }

    #[test]
    fn context_cmd_json_with_context() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc12345".to_string(),
            workspace_name: "my-project".to_string(),
            agent_id: "agent_xyz".to_string(),
            agent_name: "agent_xy".to_string(),
            ..Default::default()
        });
        let out = run_context_for_test(&["context", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["WorkspaceID"], "ws_abc12345");
        assert_eq!(parsed["WorkspaceName"], "my-project");
        assert_eq!(parsed["AgentID"], "agent_xyz");
        assert_eq!(parsed["AgentName"], "agent_xy");
    }

    #[test]
    fn context_cmd_jsonl() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc".to_string(),
            ..Default::default()
        });
        let out = run_context_for_test(&["context", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(parsed["WorkspaceID"], "ws_abc");
    }

    #[test]
    fn context_cmd_help() {
        let backend = empty_backend();
        let out = run_context_for_test(&["context", "--help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("forge context"));
        assert!(out.stdout.contains("--json"));
    }

    #[test]
    fn context_cmd_json_jsonl_conflict() {
        let backend = empty_backend();
        let out = run_context_for_test(&["context", "--json", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn context_cmd_unknown_flag() {
        let backend = empty_backend();
        let out = run_context_for_test(&["context", "--foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown flag: --foobar"));
    }

    // -----------------------------------------------------------------------
    // `forge use` command tests
    // -----------------------------------------------------------------------

    #[test]
    fn use_cmd_no_args_empty() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("No context set."));
        assert!(out.stdout.contains("forge use <workspace>"));
    }

    #[test]
    fn use_cmd_show_empty() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--show"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("No context set."));
    }

    #[test]
    fn use_cmd_show_with_context() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc12345".to_string(),
            workspace_name: "my-project".to_string(),
            agent_id: "agent_xyz12345".to_string(),
            agent_name: "agent_xy".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--show"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Current context: my-project:agent_xy"));
        assert!(out.stdout.contains("Workspace: ws_abc12345 (my-project)"));
        assert!(out.stdout.contains("Agent: agent_xyz12345 (agent_xy)"));
    }

    #[test]
    fn use_cmd_clear() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--clear"], &backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Context cleared.\n");
    }

    #[test]
    fn use_cmd_set_workspace_by_name() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "my-project"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workspace set to: my-project"));
        let saved = backend.context.borrow();
        assert_eq!(saved.workspace_id, "ws_abc12345");
        assert_eq!(saved.workspace_name, "my-project");
    }

    #[test]
    fn use_cmd_set_workspace_by_id() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "ws_abc12345"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workspace set to: my-project"));
    }

    #[test]
    fn use_cmd_set_workspace_by_prefix() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "ws_abc"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workspace set to: my-project"));
    }

    #[test]
    fn use_cmd_set_workspace_flag() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "--workspace", "other-project"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workspace set to: other-project"));
        let saved = backend.context.borrow();
        assert_eq!(saved.workspace_id, "ws_def67890");
    }

    #[test]
    fn use_cmd_set_agent_flag() {
        let mut backend = backend_with_data();
        backend.context = std::cell::RefCell::new(ContextRecord {
            workspace_id: "ws_abc12345".to_string(),
            workspace_name: "my-project".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--agent", "agent_xyz12345"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Agent set to: agent_xy"));
        let saved = backend.context.borrow();
        assert_eq!(saved.agent_id, "agent_xyz12345");
    }

    #[test]
    fn use_cmd_positional_workspace_colon_agent() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "my-project:agent_xyz12345"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Context set to: my-project:agent_xy"));
        let saved = backend.context.borrow();
        assert_eq!(saved.workspace_id, "ws_abc12345");
        assert_eq!(saved.agent_id, "agent_xyz12345");
    }

    #[test]
    fn use_cmd_positional_agent_fallback() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "agent_xyz12345"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Agent set to: agent_xy"));
        let saved = backend.context.borrow();
        assert_eq!(saved.workspace_id, "ws_abc12345");
    }

    #[test]
    fn use_cmd_positional_not_found() {
        let backend = backend_with_data();
        let out = run_use_for_test(&["use", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("'nonexistent' is not a valid workspace or agent"));
    }

    #[test]
    fn use_cmd_help() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("forge use"));
        assert!(out.stdout.contains("--agent"));
        assert!(out.stdout.contains("--workspace"));
        assert!(out.stdout.contains("--clear"));
    }

    #[test]
    fn use_cmd_unknown_flag() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown flag: --foobar"));
    }

    #[test]
    fn use_cmd_show_json() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc".to_string(),
            workspace_name: "my-project".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--show", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["WorkspaceID"], "ws_abc");
        assert_eq!(parsed["WorkspaceName"], "my-project");
    }

    #[test]
    fn use_cmd_agent_requires_value() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--agent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--agent requires a value"));
    }

    #[test]
    fn use_cmd_workspace_requires_value() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--workspace"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--workspace requires a value"));
    }

    #[test]
    fn use_cmd_show_workspace_no_name() {
        let backend = backend_with_context(ContextRecord {
            workspace_id: "ws_abc12345".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--show"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Current context: ws_abc12345"));
        assert!(out.stdout.contains("Workspace: ws_abc12345\n"));
    }

    #[test]
    fn use_cmd_show_agent_no_name() {
        let backend = backend_with_context(ContextRecord {
            agent_id: "agent_xyz12345".to_string(),
            ..Default::default()
        });
        let out = run_use_for_test(&["use", "--show"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Agent: agent_xyz12345\n"));
    }

    #[test]
    fn use_cmd_json_jsonl_conflict() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "--json", "--jsonl", "--show"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn use_cmd_unexpected_argument() {
        let backend = empty_backend();
        let out = run_use_for_test(&["use", "arg1", "arg2"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unexpected argument: arg2"));
    }

    #[test]
    fn context_cmd_load_error() {
        let backend = InMemoryContextBackend {
            load_error: Some("disk on fire".to_string()),
            ..Default::default()
        };
        let out = run_context_for_test(&["context"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("failed to load context: disk on fire"));
    }

    #[test]
    fn use_cmd_save_error() {
        let backend = InMemoryContextBackend {
            save_error: Some("disk full".to_string()),
            workspaces: vec![WorkspaceInfo {
                id: "ws_abc".to_string(),
                name: "project".to_string(),
            }],
            ..Default::default()
        };
        let out = run_use_for_test(&["use", "project"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("failed to save context: disk full"));
    }

    #[test]
    fn use_cmd_clear_error() {
        let backend = InMemoryContextBackend {
            clear_error: Some("permission denied".to_string()),
            ..Default::default()
        };
        let out = run_use_for_test(&["use", "--clear"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("failed to clear context: permission denied"));
    }
}

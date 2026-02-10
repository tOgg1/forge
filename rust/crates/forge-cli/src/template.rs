use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use crate::context::ContextBackend;
use crate::context::FilesystemContextBackend;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<TemplateVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Serialize)]
struct TemplateRunResult {
    template: String,
    agent_id: String,
    item_id: String,
}

#[derive(Debug, Serialize)]
struct TemplatePathResult {
    path: String,
}

#[derive(Debug, Serialize)]
struct TemplateDeleteResult {
    deleted: String,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait TemplateBackend {
    /// Return all templates (including builtins) for the current context.
    fn load_templates(&self) -> Result<Vec<Template>, String>;

    /// Return the user template directory path.
    fn user_template_dir(&self) -> Result<PathBuf, String>;

    /// Return the project template directory path (if in a project context).
    fn project_template_dir(&self) -> Result<Option<PathBuf>, String>;

    /// Check whether a file exists on disk.
    fn file_exists(&self, path: &Path) -> bool;

    /// Create all directories leading to `path`.
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;

    /// Write `contents` to `path`.
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String>;

    /// Remove the file at `path`.
    fn remove_file(&self, path: &Path) -> Result<(), String>;

    /// Open the file in the user's editor ($EDITOR).
    fn open_editor(&self, path: &Path) -> Result<(), String>;

    /// Enqueue a rendered template message for an agent.
    /// Returns `(agent_id, item_id)`.
    fn enqueue_template(&self, message: &str, agent_flag: &str)
        -> Result<(String, String), String>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct InMemoryTemplateBackend {
    pub templates: Vec<Template>,
    pub user_dir: Option<PathBuf>,
    pub project_dir: Option<PathBuf>,
    pub existing_files: Vec<PathBuf>,
    pub created_dirs: std::cell::RefCell<Vec<PathBuf>>,
    pub written_files: std::cell::RefCell<Vec<(PathBuf, String)>>,
    pub removed_files: std::cell::RefCell<Vec<PathBuf>>,
    pub editor_opened: std::cell::RefCell<Vec<PathBuf>>,
    pub enqueue_result: Option<(String, String)>,
}

impl TemplateBackend for InMemoryTemplateBackend {
    fn load_templates(&self) -> Result<Vec<Template>, String> {
        Ok(self.templates.clone())
    }

    fn user_template_dir(&self) -> Result<PathBuf, String> {
        self.user_dir
            .clone()
            .ok_or_else(|| "failed to get user template directory".to_string())
    }

    fn project_template_dir(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.project_dir.clone())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.existing_files.iter().any(|p| p == path)
            || self.written_files.borrow().iter().any(|(p, _)| p == path)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        self.created_dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        self.written_files
            .borrow_mut()
            .push((path.to_path_buf(), contents.to_string()));
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<(), String> {
        self.removed_files.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn open_editor(&self, path: &Path) -> Result<(), String> {
        self.editor_opened.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn enqueue_template(
        &self,
        _message: &str,
        _agent_flag: &str,
    ) -> Result<(String, String), String> {
        self.enqueue_result
            .clone()
            .ok_or_else(|| "enqueue not configured in test backend".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct FilesystemTemplateBackend {
    project_dir: PathBuf,
    db_path: PathBuf,
    context_backend: FilesystemContextBackend,
}

impl FilesystemTemplateBackend {
    pub fn open_from_env() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_dir = discover_project_dir(&cwd);
        let db_path = resolve_database_path();
        let context_backend = FilesystemContextBackend::default();
        Self {
            project_dir,
            db_path,
            context_backend,
        }
    }

    #[cfg(test)]
    pub fn for_paths(project_dir: PathBuf, db_path: PathBuf, context_path: PathBuf) -> Self {
        let context_backend = FilesystemContextBackend::new(context_path, db_path.clone());
        Self {
            project_dir,
            db_path,
            context_backend,
        }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    fn resolve_workspace_id(&self) -> Result<Option<String>, String> {
        let context = self.context_backend.load_context()?;
        if !context.workspace_id.is_empty() {
            return Ok(Some(context.workspace_id));
        }
        if !self.db_path.exists() {
            return Ok(None);
        }

        let db = self.open_db()?;
        let repo = self.project_dir.to_string_lossy().to_string();
        let workspace_id = db
            .conn()
            .query_row(
                "SELECT id FROM workspaces WHERE repo_path = ?1 ORDER BY id LIMIT 1",
                rusqlite::params![repo],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        Ok(workspace_id)
    }

    fn list_workspace_agents(&self) -> Result<Vec<String>, String> {
        let Some(workspace_id) = self.resolve_workspace_id()? else {
            return Ok(Vec::new());
        };
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let mut stmt = match conn
            .prepare("SELECT id FROM agents WHERE workspace_id = ?1 ORDER BY id")
        {
            Ok(value) => value,
            Err(err) if err.to_string().contains("no such table: agents") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        let rows = stmt
            .query_map(rusqlite::params![workspace_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|err| err.to_string())?);
        }
        Ok(items)
    }

    fn resolve_agent(&self, target: &str) -> Result<String, String> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Err("agent ID required".to_string());
        }
        if !self.db_path.exists() {
            return Err(format!("agent not found: {trimmed}"));
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let exact = conn
            .query_row(
                "SELECT id FROM agents WHERE id = ?1",
                rusqlite::params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .optional();
        let exact = match exact {
            Ok(value) => value,
            Err(err) if err.to_string().contains("no such table: agents") => {
                return Err(format!("agent not found: {trimmed}"));
            }
            Err(err) => return Err(err.to_string()),
        };
        if let Some(agent_id) = exact {
            return Ok(agent_id);
        }

        let mut stmt =
            match conn.prepare("SELECT id FROM agents WHERE id LIKE ?1 ORDER BY id LIMIT 2") {
                Ok(value) => value,
                Err(err) if err.to_string().contains("no such table: agents") => {
                    return Err(format!("agent not found: {trimmed}"));
                }
                Err(err) => return Err(err.to_string()),
            };
        let like = format!("{trimmed}%");
        let rows = stmt
            .query_map(rusqlite::params![like], |row| row.get::<_, String>(0))
            .map_err(|err| err.to_string())?;
        let mut matches = Vec::new();
        for row in rows {
            matches.push(row.map_err(|err| err.to_string())?);
        }
        match matches.len() {
            0 => Err(format!("agent not found: {trimmed}")),
            1 => Ok(matches.remove(0)),
            _ => Err(format!(
                "agent '{trimmed}' is ambiguous; use a longer prefix or full ID"
            )),
        }
    }

    fn resolve_agent_for_template(&self, agent_flag: &str) -> Result<String, String> {
        if !agent_flag.trim().is_empty() {
            return self.resolve_agent(agent_flag);
        }

        let context = self.context_backend.load_context()?;
        if !context.agent_id.is_empty() {
            return self.resolve_agent(&context.agent_id);
        }

        let agents = self.list_workspace_agents()?;
        if agents.len() == 1 {
            return Ok(agents[0].clone());
        }
        if agents.is_empty() {
            return Err(
                "no agents in workspace; spawn one with 'forge up' or 'forge agent spawn'"
                    .to_string(),
            );
        }
        Err(
            "agent required: pass --agent <id> or set context with 'forge use --agent <id>'"
                .to_string(),
        )
    }

    fn enqueue_message(&self, agent_id: &str, text: &str) -> Result<String, String> {
        if !self.db_path.exists() {
            return Err("database not found".to_string());
        }

        let db = self.open_db()?;
        let conn = db.conn();
        let exists = conn
            .query_row(
                "SELECT 1 FROM agents WHERE id = ?1",
                rusqlite::params![agent_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if exists.is_none() {
            return Err(format!("agent not found: {agent_id}"));
        }

        let position = conn
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM queue_items WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| err.to_string())?
            + 1;

        let uuid = uuid::Uuid::new_v4().simple().to_string();
        let item_id = format!("q-{}", &uuid[..8]);
        let payload = serde_json::json!({ "text": text }).to_string();
        conn.execute(
            "INSERT INTO queue_items (
                id, agent_id, type, position, status, attempts, payload_json, created_at
            ) VALUES (
                ?1, ?2, 'message', ?3, 'pending', 0, ?4, ?5
            )",
            rusqlite::params![
                item_id,
                agent_id,
                position,
                payload,
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ],
        )
        .map_err(|err| err.to_string())?;

        Ok(item_id)
    }
}

impl TemplateBackend for FilesystemTemplateBackend {
    fn load_templates(&self) -> Result<Vec<Template>, String> {
        let user_dir = self.user_template_dir()?;
        let project_dir = self.project_template_dir()?.unwrap_or_default();
        let search_paths = template_search_paths(&project_dir, &user_dir);

        let mut seen = HashSet::new();
        let mut items = Vec::new();

        for dir in &search_paths {
            for tmpl in load_templates_from_dir(dir)? {
                if seen.insert(tmpl.name.clone()) {
                    items.push(tmpl);
                }
            }
        }

        for builtin in load_builtin_templates()? {
            if seen.insert(builtin.name.clone()) {
                items.push(builtin);
            }
        }

        Ok(items)
    }

    fn user_template_dir(&self) -> Result<PathBuf, String> {
        if let Some(home) = env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            path.push(".config");
            path.push("forge");
            path.push("templates");
            return Ok(path);
        }
        Ok(PathBuf::from(".config/forge/templates"))
    }

    fn project_template_dir(&self) -> Result<Option<PathBuf>, String> {
        Ok(Some(self.project_dir.join(".forge").join("templates")))
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|err| err.to_string())
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        fs::write(path, contents).map_err(|err| err.to_string())
    }

    fn remove_file(&self, path: &Path) -> Result<(), String> {
        fs::remove_file(path).map_err(|err| err.to_string())
    }

    fn open_editor(&self, path: &Path) -> Result<(), String> {
        let editor = env::var("EDITOR")
            .or_else(|_| env::var("VISUAL"))
            .map_err(|_| "EDITOR is not set (set $EDITOR or use --file/--stdin)".to_string())?;
        let mut parts = editor.split_whitespace();
        let binary = parts.next().ok_or_else(|| "EDITOR is empty".to_string())?;
        let status = ProcessCommand::new(binary)
            .args(parts)
            .arg(path)
            .status()
            .map_err(|err| format!("failed to run editor {binary:?}: {err}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("failed to run editor {binary:?}: exit {status}"))
        }
    }

    fn enqueue_template(
        &self,
        message: &str,
        agent_flag: &str,
    ) -> Result<(String, String), String> {
        let agent_id = self.resolve_agent_for_template(agent_flag)?;
        let item_id = self.enqueue_message(&agent_id, message)?;
        Ok((agent_id, item_id))
    }
}

// ---------------------------------------------------------------------------
// Parsed arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum SubCommand {
    Help,
    List {
        tags: Vec<String>,
    },
    Show {
        name: String,
    },
    Add {
        name: String,
    },
    Edit {
        name: String,
    },
    Run {
        name: String,
        agent: String,
        vars: Vec<String>,
    },
    Delete {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    command: SubCommand,
    json: bool,
    jsonl: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn TemplateBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    let stdout = match String::from_utf8(stdout) {
        Ok(value) => value,
        Err(err) => panic!("stdout should be utf-8: {err}"),
    };
    let stderr = match String::from_utf8(stderr) {
        Ok(value) => value,
        Err(err) => panic!("stderr should be utf-8: {err}"),
    };
    CommandOutput {
        stdout,
        stderr,
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &dyn TemplateBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Core execution
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &dyn TemplateBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.command {
        SubCommand::Help => {
            write_help(stdout).map_err(|err| err.to_string())?;
            Ok(())
        }
        SubCommand::List { tags } => {
            execute_list(backend, &tags, parsed.json, parsed.jsonl, stdout)
        }
        SubCommand::Show { name } => {
            execute_show(backend, &name, parsed.json, parsed.jsonl, stdout)
        }
        SubCommand::Add { name } => execute_add(backend, &name, parsed.json, parsed.jsonl, stdout),
        SubCommand::Edit { name } => {
            execute_edit(backend, &name, parsed.json, parsed.jsonl, stdout)
        }
        SubCommand::Run { name, agent, vars } => execute_run(
            backend,
            &name,
            &agent,
            &vars,
            parsed.json,
            parsed.jsonl,
            stdout,
        ),
        SubCommand::Delete { name } => {
            execute_delete(backend, &name, parsed.json, parsed.jsonl, stdout)
        }
    }
}

fn execute_list(
    backend: &dyn TemplateBackend,
    tags: &[String],
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_templates()?;
    let filtered = filter_templates(&items, tags);

    if json || jsonl {
        return write_json_output(stdout, &filtered, jsonl);
    }

    if filtered.is_empty() {
        writeln!(stdout, "No templates found").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let user_dir = backend.user_template_dir().unwrap_or_default();
    let project_dir = backend.project_template_dir().unwrap_or(None);

    let user_dir_str = user_dir.to_string_lossy().to_string();
    let project_dir_str = project_dir
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut tw = tabwriter::TabWriter::new(Vec::new());
    let _ = writeln!(tw, "NAME\tDESCRIPTION\tSOURCE\tTAGS");
    for tmpl in &filtered {
        let source_label = template_source_label(&tmpl.source, &user_dir_str, &project_dir_str);
        let tags_str = tmpl.tags.join(",");
        let _ = writeln!(
            tw,
            "{}\t{}\t{}\t{}",
            tmpl.name, tmpl.description, source_label, tags_str
        );
    }
    let _ = tw.flush();
    let rendered = match tw.into_inner() {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return Err("failed to render table".to_string()),
    };
    write!(stdout, "{rendered}").map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_show(
    backend: &dyn TemplateBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_templates()?;
    let tmpl = find_template_by_name(&items, name)
        .ok_or_else(|| format!("template {name:?} not found"))?;

    if json || jsonl {
        return write_json_output(stdout, &tmpl, jsonl);
    }

    writeln!(stdout, "Template: {}", tmpl.name).map_err(|e| e.to_string())?;
    writeln!(stdout, "Source: {}", tmpl.source).map_err(|e| e.to_string())?;
    if !tmpl.description.is_empty() {
        writeln!(stdout, "Description: {}", tmpl.description).map_err(|e| e.to_string())?;
    }
    if !tmpl.tags.is_empty() {
        writeln!(stdout, "Tags: {}", tmpl.tags.join(",")).map_err(|e| e.to_string())?;
    }
    writeln!(stdout).map_err(|e| e.to_string())?;
    writeln!(stdout, "Message:").map_err(|e| e.to_string())?;
    writeln!(stdout, "{}", indent_block(&tmpl.message, "  ")).map_err(|e| e.to_string())?;

    if tmpl.variables.is_empty() {
        writeln!(stdout, "\nVariables: (none)").map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "\nVariables:").map_err(|e| e.to_string())?;
    for var in &tmpl.variables {
        let mut line = format!("- {}", var.name);
        if !var.description.is_empty() {
            line.push_str(": ");
            line.push_str(&var.description);
        }
        if var.required {
            line.push_str(" (required)");
        }
        if !var.default.is_empty() {
            line.push_str(&format!(" [default: {}]", var.default));
        }
        writeln!(stdout, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn execute_add(
    backend: &dyn TemplateBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_template_name(name)?;
    let user_dir = backend.user_template_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if backend.file_exists(&path) {
        return Err(format!(
            "template {normalized:?} already exists at {}",
            path.display()
        ));
    }

    let parent = path
        .parent()
        .ok_or_else(|| "failed to determine parent directory".to_string())?;
    backend.create_dir_all(parent)?;
    backend.write_file(&path, &template_skeleton(&normalized))?;
    backend.open_editor(&path)?;

    if json || jsonl {
        return write_json_output(
            stdout,
            &TemplatePathResult {
                path: path.display().to_string(),
            },
            jsonl,
        );
    }

    writeln!(stdout, "Template created: {}", path.display()).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_edit(
    backend: &dyn TemplateBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_template_name(name)?;
    let user_dir = backend.user_template_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if !backend.file_exists(&path) {
        return Err(format!(
            "template {normalized:?} not found in user templates"
        ));
    }

    backend.open_editor(&path)?;

    if json || jsonl {
        return write_json_output(
            stdout,
            &TemplatePathResult {
                path: path.display().to_string(),
            },
            jsonl,
        );
    }

    writeln!(stdout, "Template updated: {}", path.display()).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_run(
    backend: &dyn TemplateBackend,
    name: &str,
    agent: &str,
    var_args: &[String],
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_templates()?;
    let tmpl = find_template_by_name(&items, name)
        .ok_or_else(|| format!("template {name:?} not found"))?;

    let vars = parse_template_vars(var_args)?;
    let message = render_template(tmpl, &vars)?;

    let (agent_id, item_id) = backend.enqueue_template(&message, agent)?;

    let result = TemplateRunResult {
        template: tmpl.name.clone(),
        agent_id: agent_id.clone(),
        item_id: item_id.clone(),
    };

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    writeln!(
        stdout,
        "Queued template {:?} for agent {} (item {})",
        tmpl.name,
        short_id(&agent_id),
        short_id(&item_id)
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_delete(
    backend: &dyn TemplateBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_template_name(name)?;
    let user_dir = backend.user_template_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if !backend.file_exists(&path) {
        return Err(format!(
            "template {normalized:?} not found in user templates"
        ));
    }

    backend.remove_file(&path)?;

    if json || jsonl {
        return write_json_output(
            stdout,
            &TemplateDeleteResult {
                deleted: normalized,
            },
            jsonl,
        );
    }

    writeln!(stdout, "Deleted template {:?}", name).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            command: SubCommand::Help,
            json: false,
            jsonl: false,
        });
    }

    let start = if args.first().is_some_and(|a| a == "template" || a == "tmpl") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut tags: Vec<String> = Vec::new();
    let mut agent = String::new();
    let mut var_args: Vec<String> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();

    let mut idx = start;
    while idx < args.len() {
        let token = &args[idx];
        match token.as_str() {
            "--json" => {
                json = true;
                idx += 1;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
            }
            "--quiet" => {
                // accepted but ignored for template
                idx += 1;
            }
            "--tags" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| "missing value for --tags".to_string())?;
                tags.push(value.clone());
                idx += 1;
            }
            "--agent" | "-a" => {
                idx += 1;
                agent = args
                    .get(idx)
                    .ok_or_else(|| "missing value for --agent".to_string())?
                    .clone();
                idx += 1;
            }
            "--var" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| "missing value for --var".to_string())?;
                var_args.push(value.clone());
                idx += 1;
            }
            "-h" | "--help" => {
                positionals.push(token.clone());
                idx += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown argument: {flag}"));
            }
            _ => {
                positionals.push(token.clone());
                idx += 1;
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let subcmd = positionals.first().map(|s| s.as_str());
    let command = match subcmd {
        None | Some("help") | Some("-h") | Some("--help") => SubCommand::Help,
        Some("ls") | Some("list") => SubCommand::List { tags },
        Some("show") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge template show <name>".to_string())?
                .clone();
            SubCommand::Show { name }
        }
        Some("add") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge template add <name>".to_string())?
                .clone();
            SubCommand::Add { name }
        }
        Some("edit") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge template edit <name>".to_string())?
                .clone();
            SubCommand::Edit { name }
        }
        Some("run") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge template run <name>".to_string())?
                .clone();
            SubCommand::Run {
                name,
                agent,
                vars: var_args,
            }
        }
        Some("delete") | Some("rm") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge template delete <name>".to_string())?
                .clone();
            SubCommand::Delete { name }
        }
        Some(other) => return Err(format!("unknown template subcommand: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
}

// ---------------------------------------------------------------------------
// Template helpers (ported from Go)
// ---------------------------------------------------------------------------

fn filter_templates<'a>(items: &'a [Template], tags: &[String]) -> Vec<&'a Template> {
    if tags.is_empty() {
        return items.iter().collect();
    }

    let mut wanted = std::collections::HashSet::new();
    for entry in tags {
        for tag in split_comma_list(entry) {
            wanted.insert(tag.to_lowercase());
        }
    }

    items
        .iter()
        .filter(|tmpl| {
            if tmpl.tags.is_empty() {
                return false;
            }
            tmpl.tags
                .iter()
                .any(|tag| wanted.contains(&tag.to_lowercase()))
        })
        .collect()
}

fn find_template_by_name<'a>(items: &'a [Template], name: &str) -> Option<&'a Template> {
    items
        .iter()
        .find(|tmpl| tmpl.name.eq_ignore_ascii_case(name))
}

fn normalize_template_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("template name is required".to_string());
    }
    if trimmed.contains(std::path::MAIN_SEPARATOR) || trimmed.contains("..") {
        return Err(format!("invalid template name {trimmed:?}"));
    }
    // Also reject forward slash on all platforms for safety
    if trimmed.contains('/') {
        return Err(format!("invalid template name {trimmed:?}"));
    }
    Ok(trimmed.to_string())
}

fn template_skeleton(name: &str) -> String {
    format!("name: {name}\ndescription: Describe this template\nmessage: |\n  Write the instruction here.\n")
}

fn template_source_label(source: &str, user_dir: &str, project_dir: &str) -> &'static str {
    if source == "builtin" {
        return "builtin";
    }
    if !user_dir.is_empty() && is_within_dir(source, user_dir) {
        return "user";
    }
    if !project_dir.is_empty() && is_within_dir(source, project_dir) {
        return "project";
    }
    "file"
}

fn is_within_dir(path: &str, dir: &str) -> bool {
    if path.is_empty() || dir.is_empty() {
        return false;
    }
    match Path::new(dir).join("x").parent() {
        Some(_) => {
            // Use starts_with for simplicity and cross-platform correctness
            let normalized_dir = if dir.ends_with('/') {
                dir.to_string()
            } else {
                format!("{dir}/")
            };
            path.starts_with(&normalized_dir)
        }
        None => false,
    }
}

fn parse_template_vars(values: &[String]) -> Result<HashMap<String, String>, String> {
    let mut vars = HashMap::new();
    for entry in values {
        for part in split_comma_list(entry) {
            if part.is_empty() {
                continue;
            }
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| format!("invalid variable {part:?} (expected key=value)"))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(format!("invalid variable {part:?} (empty key)"));
            }
            vars.insert(key.to_string(), value.to_string());
        }
    }
    Ok(vars)
}

fn split_comma_list(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

fn discover_project_dir(cwd: &Path) -> PathBuf {
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    cwd.to_path_buf()
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}

fn template_search_paths(project_dir: &Path, user_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if !project_dir.as_os_str().is_empty() {
        paths.push(project_dir.to_path_buf());
    }
    if !user_dir.as_os_str().is_empty() {
        paths.push(user_dir.to_path_buf());
    }
    paths.push(PathBuf::from("/usr/share/forge/templates"));
    paths
}

fn load_templates_from_dir(dir: &Path) -> Result<Vec<Template>, String> {
    if dir.as_os_str().is_empty() {
        return Ok(Vec::new());
    }
    let entries = match fs::read_dir(dir) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("read templates dir {}: {err}", dir.display())),
    };

    let mut templates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("read templates dir {}: {err}", dir.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read templates dir {}: {err}", dir.display()))?;
        if file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !ext.eq_ignore_ascii_case("yaml") && !ext.eq_ignore_ascii_case("yml") {
            continue;
        }
        templates.push(load_template_from_path(&path)?);
    }

    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

fn load_template_from_path(path: &Path) -> Result<Template, String> {
    let source = path.to_string_lossy().to_string();
    let data = fs::read_to_string(path)
        .map_err(|err| format!("read template {}: {err}", path.display()))?;
    parse_template_yaml(&data, &source)
}

fn parse_template_yaml(data: &str, source: &str) -> Result<Template, String> {
    let mut tmpl: Template =
        serde_yaml::from_str(data).map_err(|err| format!("parse template {source}: {err}"))?;
    tmpl.name = tmpl.name.trim().to_string();
    if tmpl.name.is_empty() {
        return Err("template name is required".to_string());
    }
    if tmpl.message.trim().is_empty() {
        return Err("template message is required".to_string());
    }

    let mut seen = HashSet::new();
    for variable in &mut tmpl.variables {
        variable.name = variable.name.trim().to_string();
        if variable.name.is_empty() {
            return Err("template variable name is required".to_string());
        }
        if !seen.insert(variable.name.clone()) {
            return Err(format!("duplicate template variable {:?}", variable.name));
        }
    }

    tmpl.source = source.to_string();
    Ok(tmpl)
}

fn load_builtin_templates() -> Result<Vec<Template>, String> {
    let builtins = [
        (
            "commit",
            include_str!("../../../../internal/templates/builtin/commit.yaml"),
        ),
        (
            "continue",
            include_str!("../../../../internal/templates/builtin/continue.yaml"),
        ),
        (
            "explain",
            include_str!("../../../../internal/templates/builtin/explain.yaml"),
        ),
        (
            "review",
            include_str!("../../../../internal/templates/builtin/review.yaml"),
        ),
        (
            "test",
            include_str!("../../../../internal/templates/builtin/test.yaml"),
        ),
    ];

    let mut templates = Vec::new();
    for (_, body) in builtins {
        templates.push(parse_template_yaml(body, "builtin")?);
    }
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

fn render_template(tmpl: &Template, vars: &HashMap<String, String>) -> Result<String, String> {
    // Simple variable substitution: replace {{.VarName}} and {{ .VarName }} patterns.
    // This matches Go's text/template basic variable expansion for the common case.
    let mut data: HashMap<String, String> = vars.clone();

    // Apply defaults and check required variables
    for variable in &tmpl.variables {
        let value = data.get(&variable.name).map(|s| s.trim().to_string());
        let is_empty = value.as_ref().map_or(true, |v| v.is_empty());
        if is_empty {
            if !variable.default.is_empty() {
                data.insert(variable.name.clone(), variable.default.clone());
                continue;
            }
            if variable.required {
                return Err(format!("missing required variable {:?}", variable.name));
            }
        }
    }

    // Perform Go-style template substitution: {{.Key}} and {{ .Key }}
    let mut result = tmpl.message.clone();
    for (key, value) in &data {
        // Replace both {{.key}} and {{ .key }} variants
        let pattern_tight = format!("{{{{{}.{key}}}}}", "");
        let pattern_spaced = format!("{{{{ .{key} }}}}");
        result = result.replace(&pattern_tight, value);
        result = result.replace(&pattern_spaced, value);
    }

    Ok(result)
}

fn indent_block(text: &str, prefix: &str) -> String {
    let trimmed = text.trim_end_matches('\n');
    trimmed
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

fn write_json_output(
    output: &mut dyn Write,
    value: &impl Serialize,
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

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Create, edit, and run reusable message templates.")?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge template <command> [options]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Aliases:")?;
    writeln!(stdout, "  template, tmpl")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  ls          List templates")?;
    writeln!(stdout, "  show        Show template details")?;
    writeln!(stdout, "  add         Create a new template")?;
    writeln!(stdout, "  edit        Edit an existing template")?;
    writeln!(stdout, "  run         Queue a template message")?;
    writeln!(stdout, "  delete      Delete a user template")?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(stdout, "      --tags string   filter by tags (ls only)")?;
    writeln!(
        stdout,
        "  -a, --agent string  agent ID or prefix (run only)"
    )?;
    writeln!(
        stdout,
        "      --var string    template variable key=value (run only)"
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sample_templates() -> Vec<Template> {
        vec![
            Template {
                name: "commit".to_string(),
                description: "Commit changes".to_string(),
                message: "Commit your changes.".to_string(),
                variables: vec![],
                tags: vec!["git".to_string()],
                source: "builtin".to_string(),
            },
            Template {
                name: "review".to_string(),
                description: "Request a review".to_string(),
                message: "Review the changes.".to_string(),
                variables: vec![],
                tags: vec!["review".to_string()],
                source: "/home/user/.config/forge/templates/review.yaml".to_string(),
            },
            Template {
                name: "deploy".to_string(),
                description: "Deploy to staging".to_string(),
                message: "Deploy {{.target}} to staging.".to_string(),
                variables: vec![TemplateVar {
                    name: "target".to_string(),
                    description: "Deployment target".to_string(),
                    default: "main".to_string(),
                    required: false,
                }],
                tags: vec!["ops".to_string(), "git".to_string()],
                source: "/project/.forge/templates/deploy.yaml".to_string(),
            },
            Template {
                name: "empty-tags".to_string(),
                description: "No tags".to_string(),
                message: "Hello.".to_string(),
                variables: vec![],
                tags: vec![],
                source: "builtin".to_string(),
            },
        ]
    }

    fn test_backend() -> InMemoryTemplateBackend {
        InMemoryTemplateBackend {
            templates: sample_templates(),
            user_dir: Some(PathBuf::from("/home/user/.config/forge/templates")),
            project_dir: Some(PathBuf::from("/project/.forge/templates")),
            ..Default::default()
        }
    }

    // -- help --

    #[test]
    fn help_no_args() {
        let backend = test_backend();
        let out = run_for_test(&[], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
        assert!(out.stdout.contains("ls"));
        assert!(out.stdout.contains("show"));
        assert!(out.stdout.contains("add"));
        assert!(out.stdout.contains("edit"));
        assert!(out.stdout.contains("run"));
        assert!(out.stdout.contains("delete"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn help_explicit() {
        let backend = test_backend();
        let out = run_for_test(&["template", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn help_dash_h() {
        let backend = test_backend();
        let out = run_for_test(&["template", "-h"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn tmpl_alias_accepted() {
        let backend = test_backend();
        let out = run_for_test(&["tmpl", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    // -- ls / list --

    #[test]
    fn list_all_templates() {
        let backend = test_backend();
        let out = run_for_test(&["template", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("commit"));
        assert!(out.stdout.contains("review"));
        assert!(out.stdout.contains("deploy"));
        assert!(out.stdout.contains("NAME"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn list_alias_works() {
        let backend = test_backend();
        let out = run_for_test(&["template", "list"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("commit"));
    }

    #[test]
    fn list_filter_by_tags() {
        let backend = test_backend();
        let out = run_for_test(&["template", "ls", "--tags", "review"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("review"));
        assert!(!out.stdout.contains("commit"));
    }

    #[test]
    fn list_filter_by_multiple_tags() {
        let backend = test_backend();
        let out = run_for_test(&["template", "ls", "--tags", "review,ops"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("review"));
        assert!(out.stdout.contains("deploy"));
        assert!(!out.stdout.contains("commit\t")); // "commit" appears but as a name not a row
    }

    #[test]
    fn list_no_match() {
        let backend = test_backend();
        let out = run_for_test(&["template", "ls", "--tags", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("No templates found"));
    }

    #[test]
    fn list_empty_templates() {
        let backend = InMemoryTemplateBackend::default();
        let out = run_for_test(&["template", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("No templates found"));
    }

    #[test]
    fn list_json_output() {
        let backend = test_backend();
        let out = run_for_test(&["template", "--json", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 4);
    }

    #[test]
    fn list_jsonl_output() {
        let backend = test_backend();
        let out = run_for_test(&["template", "--jsonl", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert!(parsed.is_array());
    }

    // -- show --

    #[test]
    fn show_template() {
        let backend = test_backend();
        let out = run_for_test(&["template", "show", "commit"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Template: commit"));
        assert!(out.stdout.contains("Source: builtin"));
        assert!(out.stdout.contains("Message:"));
        assert!(out.stdout.contains("Variables: (none)"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn show_template_case_insensitive() {
        let backend = test_backend();
        let out = run_for_test(&["template", "show", "COMMIT"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Template: commit"));
    }

    #[test]
    fn show_template_with_variables() {
        let backend = test_backend();
        let out = run_for_test(&["template", "show", "deploy"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Variables:"));
        assert!(out.stdout.contains("- target"));
        assert!(out.stdout.contains("[default: main]"));
    }

    #[test]
    fn show_template_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["template", "show", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn show_template_json() {
        let backend = test_backend();
        let out = run_for_test(&["template", "--json", "show", "commit"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["name"], "commit");
        assert_eq!(parsed["source"], "builtin");
    }

    #[test]
    fn show_missing_name() {
        let backend = test_backend();
        let out = run_for_test(&["template", "show"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("usage:"));
    }

    // -- add --

    #[test]
    fn add_template() {
        let backend = test_backend();
        let out = run_for_test(&["template", "add", "my-template"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Template created:"));
        assert!(out.stdout.contains("my-template.yaml"));

        let files = backend.written_files.borrow();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.contains("name: my-template"));

        let editors = backend.editor_opened.borrow();
        assert_eq!(editors.len(), 1);
    }

    #[test]
    fn add_template_already_exists() {
        let mut backend = test_backend();
        backend.existing_files.push(PathBuf::from(
            "/home/user/.config/forge/templates/existing.yaml",
        ));
        let out = run_for_test(&["template", "add", "existing"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("already exists"));
    }

    #[test]
    fn add_template_json() {
        let backend = test_backend();
        let out = run_for_test(&["template", "--json", "add", "new-one"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed["path"].as_str().unwrap().contains("new-one.yaml"));
    }

    #[test]
    fn add_missing_name() {
        let backend = test_backend();
        let out = run_for_test(&["template", "add"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("usage:"));
    }

    // -- edit --

    #[test]
    fn edit_template() {
        let mut backend = test_backend();
        backend.existing_files.push(PathBuf::from(
            "/home/user/.config/forge/templates/my-template.yaml",
        ));
        let out = run_for_test(&["template", "edit", "my-template"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Template updated:"));

        let editors = backend.editor_opened.borrow();
        assert_eq!(editors.len(), 1);
    }

    #[test]
    fn edit_template_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["template", "edit", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found in user templates"));
    }

    #[test]
    fn edit_template_json() {
        let mut backend = test_backend();
        backend.existing_files.push(PathBuf::from(
            "/home/user/.config/forge/templates/my-template.yaml",
        ));
        let out = run_for_test(&["template", "--json", "edit", "my-template"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed["path"]
            .as_str()
            .unwrap()
            .contains("my-template.yaml"));
    }

    // -- run --

    #[test]
    fn run_template() {
        let mut backend = test_backend();
        backend.enqueue_result = Some(("agent-abc12345".to_string(), "item-def67890".to_string()));
        let out = run_for_test(&["template", "run", "commit"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Queued template"));
        assert!(out.stdout.contains("agent-ab"));
        assert!(out.stdout.contains("item-def"));
    }

    #[test]
    fn run_template_with_vars() {
        let mut backend = test_backend();
        backend.enqueue_result = Some(("agent-1".to_string(), "item-2".to_string()));
        let out = run_for_test(
            &["template", "run", "deploy", "--var", "target=production"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Queued template"));
    }

    #[test]
    fn run_template_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["template", "run", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn run_template_json() {
        let mut backend = test_backend();
        backend.enqueue_result = Some(("agent-abc".to_string(), "item-def".to_string()));
        let out = run_for_test(&["template", "--json", "run", "commit"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["template"], "commit");
        assert_eq!(parsed["agent_id"], "agent-abc");
        assert_eq!(parsed["item_id"], "item-def");
    }

    // -- delete --

    #[test]
    fn delete_template() {
        let mut backend = test_backend();
        backend
            .existing_files
            .push(PathBuf::from("/home/user/.config/forge/templates/old.yaml"));
        let out = run_for_test(&["template", "delete", "old"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Deleted template"));

        let removed = backend.removed_files.borrow();
        assert_eq!(removed.len(), 1);
    }

    #[test]
    fn delete_template_rm_alias() {
        let mut backend = test_backend();
        backend
            .existing_files
            .push(PathBuf::from("/home/user/.config/forge/templates/old.yaml"));
        let out = run_for_test(&["template", "rm", "old"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Deleted template"));
    }

    #[test]
    fn delete_template_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["template", "delete", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found in user templates"));
    }

    #[test]
    fn delete_template_json() {
        let mut backend = test_backend();
        backend
            .existing_files
            .push(PathBuf::from("/home/user/.config/forge/templates/old.yaml"));
        let out = run_for_test(&["template", "--json", "delete", "old"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["deleted"], "old");
    }

    // -- error cases --

    #[test]
    fn unknown_subcommand() {
        let backend = test_backend();
        let out = run_for_test(&["template", "foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown template subcommand: foobar"));
    }

    #[test]
    fn json_and_jsonl_conflict() {
        let backend = test_backend();
        let out = run_for_test(&["template", "--json", "--jsonl", "ls"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    // -- helper unit tests (Go parity) --

    #[test]
    fn filter_templates_no_filter() {
        let items = sample_templates();
        let result = filter_templates(&items, &[]);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn filter_templates_by_tag() {
        let items = sample_templates();
        let result = filter_templates(&items, &["git".to_string()]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_templates_by_review() {
        let items = sample_templates();
        let result = filter_templates(&items, &["review".to_string()]);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_templates_multiple_tags() {
        let items = sample_templates();
        let result = filter_templates(&items, &["git".to_string(), "review".to_string()]);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn filter_templates_nonexistent_tag() {
        let items = sample_templates();
        let result = filter_templates(&items, &["nonexistent".to_string()]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn find_template_exact() {
        let items = sample_templates();
        let result = find_template_by_name(&items, "commit");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "commit");
    }

    #[test]
    fn find_template_case_insensitive() {
        let items = sample_templates();
        let result = find_template_by_name(&items, "COMMIT");
        assert!(result.is_some());
    }

    #[test]
    fn find_template_not_found() {
        let items = sample_templates();
        let result = find_template_by_name(&items, "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn find_template_partial_fails() {
        let items = sample_templates();
        let result = find_template_by_name(&items, "comm");
        assert!(result.is_none());
    }

    #[test]
    fn parse_vars_single() {
        let result = parse_template_vars(&["key=value".to_string()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn parse_vars_multiple() {
        let result = parse_template_vars(&["k1=v1".to_string(), "k2=v2".to_string()]).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_vars_comma_separated() {
        let result = parse_template_vars(&["k1=v1,k2=v2".to_string()]).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_vars_empty_value() {
        let result = parse_template_vars(&["key=".to_string()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result["key"], "");
    }

    #[test]
    fn parse_vars_missing_equals() {
        let result = parse_template_vars(&["invalid".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_vars_empty_key() {
        let result = parse_template_vars(&["=value".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_vars_empty_input() {
        let result = parse_template_vars(&[]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn normalize_name_simple() {
        assert_eq!(normalize_template_name("mytemplate").unwrap(), "mytemplate");
    }

    #[test]
    fn normalize_name_dashes() {
        assert_eq!(
            normalize_template_name("my-template").unwrap(),
            "my-template"
        );
    }

    #[test]
    fn normalize_name_underscores() {
        assert_eq!(
            normalize_template_name("my_template").unwrap(),
            "my_template"
        );
    }

    #[test]
    fn normalize_name_empty() {
        assert!(normalize_template_name("").is_err());
    }

    #[test]
    fn normalize_name_whitespace() {
        assert!(normalize_template_name("   ").is_err());
    }

    #[test]
    fn normalize_name_with_slash() {
        assert!(normalize_template_name("foo/bar").is_err());
    }

    #[test]
    fn normalize_name_with_dots() {
        assert!(normalize_template_name("foo..bar").is_err());
    }

    #[test]
    fn source_label_builtin() {
        assert_eq!(
            template_source_label(
                "builtin",
                "/home/user/.config/forge/templates",
                "/project/.forge/templates"
            ),
            "builtin"
        );
    }

    #[test]
    fn source_label_user() {
        assert_eq!(
            template_source_label(
                "/home/user/.config/forge/templates/foo.yaml",
                "/home/user/.config/forge/templates",
                ""
            ),
            "user"
        );
    }

    #[test]
    fn source_label_project() {
        assert_eq!(
            template_source_label(
                "/project/.forge/templates/bar.yaml",
                "",
                "/project/.forge/templates"
            ),
            "project"
        );
    }

    #[test]
    fn source_label_file() {
        assert_eq!(
            template_source_label(
                "/some/other/path.yaml",
                "/home/user/.config/forge/templates",
                "/project/.forge/templates"
            ),
            "file"
        );
    }

    #[test]
    fn indent_single_line() {
        assert_eq!(indent_block("hello", "  "), "  hello");
    }

    #[test]
    fn indent_multi_line() {
        assert_eq!(
            indent_block("line1\nline2\nline3", ">> "),
            ">> line1\n>> line2\n>> line3"
        );
    }

    #[test]
    fn indent_trailing_newline_stripped() {
        assert_eq!(indent_block("line1\nline2\n", "  "), "  line1\n  line2");
    }

    #[test]
    fn indent_empty() {
        assert_eq!(indent_block("", "  "), "  ");
    }

    #[test]
    fn render_template_simple() {
        let tmpl = Template {
            name: "test".to_string(),
            description: "".to_string(),
            message: "Hello {{.name}}!".to_string(),
            variables: vec![],
            tags: vec![],
            source: "".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        let result = render_template(&tmpl, &vars).unwrap();
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn render_template_with_default() {
        let tmpl = Template {
            name: "test".to_string(),
            description: "".to_string(),
            message: "Deploy {{.target}}.".to_string(),
            variables: vec![TemplateVar {
                name: "target".to_string(),
                description: "".to_string(),
                default: "main".to_string(),
                required: false,
            }],
            tags: vec![],
            source: "".to_string(),
        };
        let result = render_template(&tmpl, &HashMap::new()).unwrap();
        assert_eq!(result, "Deploy main.");
    }

    #[test]
    fn render_template_required_missing() {
        let tmpl = Template {
            name: "test".to_string(),
            description: "".to_string(),
            message: "Deploy {{.target}}.".to_string(),
            variables: vec![TemplateVar {
                name: "target".to_string(),
                description: "".to_string(),
                default: "".to_string(),
                required: true,
            }],
            tags: vec![],
            source: "".to_string(),
        };
        let result = render_template(&tmpl, &HashMap::new());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing required variable"));
    }

    #[test]
    fn short_id_truncates() {
        assert_eq!(short_id("abcdefghijklmnop"), "abcdefgh");
    }

    #[test]
    fn short_id_short_passthrough() {
        assert_eq!(short_id("abc"), "abc");
    }
}

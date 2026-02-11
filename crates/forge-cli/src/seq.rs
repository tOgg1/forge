use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod filesystem_backend;
pub use filesystem_backend::FilesystemSeqBackend;

// ---------------------------------------------------------------------------
// Data models (mirror Go sequences.Sequence for JSON output parity)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Sequence {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub steps: Vec<SequenceStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<SequenceVar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SequenceStep {
    #[serde(rename = "Type")]
    pub step_type: String,
    #[serde(rename = "Content", default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(rename = "Message", default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(rename = "Duration", default, skip_serializing_if = "String::is_empty")]
    pub duration: String,
    #[serde(rename = "When", default, skip_serializing_if = "String::is_empty")]
    pub when: String,
    #[serde(rename = "Reason", default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(
        rename = "Expression",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct SequenceVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "Default", default, skip_serializing_if = "String::is_empty")]
    pub default_value: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Serialize)]
struct SequenceRunResult {
    sequence: String,
    agent_id: String,
    item_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SequencePathResult {
    path: String,
}

#[derive(Debug, Serialize)]
struct SequenceDeleteResult {
    deleted: String,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait SeqBackend {
    fn load_sequences(&self) -> Result<Vec<Sequence>, String>;

    fn user_sequence_dir(&self) -> Result<PathBuf, String>;
    fn project_sequence_dir(&self) -> Result<Option<PathBuf>, String>;

    fn file_exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String>;
    fn remove_file(&self, path: &Path) -> Result<(), String>;
    fn open_editor(&self, path: &Path) -> Result<(), String>;

    fn resolve_agent_id(&self, agent_flag: &str) -> Result<String, String>;
    fn enqueue_sequence_items(
        &mut self,
        agent_id: &str,
        items: &[RenderedQueueItem],
    ) -> Result<Vec<String>, String>;
}

// ---------------------------------------------------------------------------
// In-memory backend (testing)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct InMemorySeqBackend {
    pub sequences: Vec<Sequence>,
    pub user_dir: Option<PathBuf>,
    pub project_dir: Option<PathBuf>,
    pub existing_files: Vec<PathBuf>,
    pub created_dirs: std::cell::RefCell<Vec<PathBuf>>,
    pub written_files: std::cell::RefCell<Vec<(PathBuf, String)>>,
    pub removed_files: std::cell::RefCell<Vec<PathBuf>>,
    pub editor_opened: std::cell::RefCell<Vec<PathBuf>>,
    pub agent_id: Option<String>,
    pub enqueued: std::cell::RefCell<Vec<(String, usize)>>,
}

impl SeqBackend for InMemorySeqBackend {
    fn load_sequences(&self) -> Result<Vec<Sequence>, String> {
        Ok(self.sequences.clone())
    }

    fn user_sequence_dir(&self) -> Result<PathBuf, String> {
        self.user_dir
            .clone()
            .ok_or_else(|| "failed to get user sequence directory".to_string())
    }

    fn project_sequence_dir(&self) -> Result<Option<PathBuf>, String> {
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

    fn resolve_agent_id(&self, _agent_flag: &str) -> Result<String, String> {
        self.agent_id
            .clone()
            .ok_or_else(|| "agent resolution not configured in test backend".to_string())
    }

    fn enqueue_sequence_items(
        &mut self,
        agent_id: &str,
        items: &[RenderedQueueItem],
    ) -> Result<Vec<String>, String> {
        self.enqueued
            .borrow_mut()
            .push((agent_id.to_string(), items.len()));
        Ok((0..items.len())
            .map(|idx| format!("item-{idx:03}"))
            .collect())
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

pub fn run_for_test(args: &[&str], backend: &mut dyn SeqBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    CommandOutput {
        stdout,
        stderr,
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &mut dyn SeqBackend,
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
    backend: &mut dyn SeqBackend,
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
    backend: &mut dyn SeqBackend,
    tags: &[String],
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_sequences()?;
    let filtered: Vec<Sequence> = filter_sequences(&items, tags)
        .into_iter()
        .cloned()
        .collect();

    if json || jsonl {
        return write_json_or_jsonl(stdout, &filtered, jsonl);
    }

    if filtered.is_empty() {
        writeln!(stdout, "No sequences found").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let user_dir = backend.user_sequence_dir().unwrap_or_default();
    let project_dir = backend.project_sequence_dir().unwrap_or(None);

    let user_dir_str = user_dir.to_string_lossy().to_string();
    let project_dir_str = project_dir
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(filtered.len());
    for seq in &filtered {
        let source_label = sequence_source_label(&seq.source, &user_dir_str, &project_dir_str);
        rows.push(vec![
            seq.name.clone(),
            format!("{}", seq.steps.len()),
            seq.description.clone(),
            source_label.to_string(),
        ]);
    }
    write_table(stdout, &["NAME", "STEPS", "DESCRIPTION", "SOURCE"], &rows)?;
    Ok(())
}

fn execute_show(
    backend: &mut dyn SeqBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_sequences()?;
    let seq = find_sequence_by_name(&items, name)
        .ok_or_else(|| format!("sequence {name:?} not found"))?;

    if json || jsonl {
        return write_json_or_jsonl(stdout, seq, jsonl);
    }

    writeln!(stdout, "Sequence: {}", seq.name).map_err(|e| e.to_string())?;
    writeln!(stdout, "Source: {}", seq.source).map_err(|e| e.to_string())?;
    if !seq.description.is_empty() {
        writeln!(stdout, "Description: {}", seq.description).map_err(|e| e.to_string())?;
    }
    if !seq.tags.is_empty() {
        writeln!(stdout, "Tags: {}", seq.tags.join(",")).map_err(|e| e.to_string())?;
    }

    writeln!(stdout, "\nSteps:").map_err(|e| e.to_string())?;
    for (idx, step) in seq.steps.iter().enumerate() {
        writeln!(stdout, "  {}. {}", idx + 1, format_sequence_step(step))
            .map_err(|e| e.to_string())?;
    }

    if seq.variables.is_empty() {
        writeln!(stdout, "\nVariables: (none)").map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "\nVariables:").map_err(|e| e.to_string())?;
    for variable in &seq.variables {
        let mut line = format!("- {}", variable.name);
        if !variable.description.is_empty() {
            line.push_str(": ");
            line.push_str(&variable.description);
        }
        if variable.required {
            line.push_str(" (required)");
        }
        if !variable.default_value.is_empty() {
            line.push_str(&format!(" [default: {}]", variable.default_value));
        }
        writeln!(stdout, "{line}").map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn execute_add(
    backend: &mut dyn SeqBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_sequence_name(name)?;
    let user_dir = backend.user_sequence_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if backend.file_exists(&path) {
        return Err(format!(
            "sequence {:?} already exists at {}",
            normalized,
            path.to_string_lossy()
        ));
    }

    let parent = path
        .parent()
        .ok_or_else(|| "failed to resolve sequences directory".to_string())?;
    backend
        .create_dir_all(parent)
        .map_err(|err| format!("failed to create sequences directory: {err}"))?;

    backend
        .write_file(&path, &sequence_skeleton(&normalized))
        .map_err(|err| format!("failed to write sequence file: {err}"))?;

    backend.open_editor(&path)?;

    if json || jsonl {
        return write_json_or_jsonl(
            stdout,
            &SequencePathResult {
                path: path.to_string_lossy().to_string(),
            },
            jsonl,
        );
    }

    writeln!(stdout, "Sequence created: {}", path.to_string_lossy()).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_edit(
    backend: &mut dyn SeqBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_sequence_name(name)?;
    let user_dir = backend.user_sequence_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if !backend.file_exists(&path) {
        return Err(format!(
            "sequence {:?} not found in user sequences",
            normalized
        ));
    }

    backend.open_editor(&path)?;

    if json || jsonl {
        return write_json_or_jsonl(
            stdout,
            &SequencePathResult {
                path: path.to_string_lossy().to_string(),
            },
            jsonl,
        );
    }

    writeln!(stdout, "Sequence updated: {}", path.to_string_lossy()).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_delete(
    backend: &mut dyn SeqBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let normalized = normalize_sequence_name(name)?;
    let user_dir = backend.user_sequence_dir()?;
    let path = user_dir.join(format!("{normalized}.yaml"));

    if !backend.file_exists(&path) {
        return Err(format!(
            "sequence {:?} not found in user sequences",
            normalized
        ));
    }

    backend
        .remove_file(&path)
        .map_err(|err| format!("failed to delete sequence: {err}"))?;

    if json || jsonl {
        return write_json_or_jsonl(
            stdout,
            &SequenceDeleteResult {
                deleted: normalized,
            },
            jsonl,
        );
    }

    writeln!(stdout, "Deleted sequence {:?}", normalized).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_run(
    backend: &mut dyn SeqBackend,
    name: &str,
    agent: &str,
    vars: &[String],
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_sequences()?;
    let seq = find_sequence_by_name(&items, name)
        .ok_or_else(|| format!("sequence {name:?} not found"))?;

    let vars = parse_sequence_vars(vars)?;
    let rendered = render_sequence(seq, &vars)?;

    let agent_id = backend.resolve_agent_id(agent)?;
    let item_ids = backend.enqueue_sequence_items(&agent_id, &rendered)?;

    let result = SequenceRunResult {
        sequence: seq.name.clone(),
        agent_id: agent_id.clone(),
        item_ids: item_ids.clone(),
    };

    if json || jsonl {
        return write_json_or_jsonl(stdout, &result, jsonl);
    }

    writeln!(
        stdout,
        "Queued sequence {:?} ({} steps) for agent {}",
        seq.name,
        rendered.len(),
        short_id(&agent_id)
    )
    .map_err(|e| e.to_string())?;
    for (idx, step) in seq.steps.iter().enumerate() {
        writeln!(
            stdout,
            "  Step {}: {} -> queued",
            idx + 1,
            format_sequence_step_short(step)
        )
        .map_err(|e| e.to_string())?;
    }

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

    let start = if args.first().is_some_and(|a| a == "seq" || a == "sequence") {
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
            "-h" | "--help" => {
                positionals.push(token.clone());
                idx += 1;
            }
            "--json" => {
                json = true;
                idx += 1;
            }
            "--jsonl" => {
                jsonl = true;
                idx += 1;
            }
            "--quiet" => {
                // accepted but ignored
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
            flag if flag.starts_with("-a=") => {
                agent = flag.trim_start_matches("-a=").to_string();
                idx += 1;
            }
            flag if flag.starts_with("--agent=") => {
                agent = flag.trim_start_matches("--agent=").to_string();
                idx += 1;
            }
            flag if flag.starts_with("--tags=") => {
                tags.push(flag.trim_start_matches("--tags=").to_string());
                idx += 1;
            }
            flag if flag.starts_with("--var=") => {
                var_args.push(flag.trim_start_matches("--var=").to_string());
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
                .ok_or_else(|| "usage: forge seq show <name>".to_string())?
                .clone();
            SubCommand::Show { name }
        }
        Some("add") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge seq add <name>".to_string())?
                .clone();
            SubCommand::Add { name }
        }
        Some("edit") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge seq edit <name>".to_string())?
                .clone();
            SubCommand::Edit { name }
        }
        Some("run") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge seq run <name>".to_string())?
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
                .ok_or_else(|| "usage: forge seq delete <name>".to_string())?
                .clone();
            SubCommand::Delete { name }
        }
        Some(other) => return Err(format!("unknown seq subcommand: {other}")),
    };

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
}

// ---------------------------------------------------------------------------
// Sequence helpers (ported from Go)
// ---------------------------------------------------------------------------

fn filter_sequences<'a>(items: &'a [Sequence], tags: &[String]) -> Vec<&'a Sequence> {
    if tags.is_empty() {
        return items.iter().collect();
    }

    let mut wanted = HashSet::new();
    for entry in tags {
        for tag in split_comma_list(entry) {
            wanted.insert(tag.to_lowercase());
        }
    }

    items
        .iter()
        .filter(|seq| {
            if seq.tags.is_empty() {
                return false;
            }
            seq.tags
                .iter()
                .any(|tag| wanted.contains(&tag.to_lowercase()))
        })
        .collect()
}

fn find_sequence_by_name<'a>(items: &'a [Sequence], name: &str) -> Option<&'a Sequence> {
    items.iter().find(|seq| seq.name.eq_ignore_ascii_case(name))
}

fn normalize_sequence_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("sequence name is required".to_string());
    }
    if trimmed.contains(std::path::MAIN_SEPARATOR) || trimmed.contains("..") {
        return Err(format!("invalid sequence name {trimmed:?}"));
    }
    if trimmed.contains('/') {
        return Err(format!("invalid sequence name {trimmed:?}"));
    }
    Ok(trimmed.to_string())
}

fn sequence_skeleton(name: &str) -> String {
    format!("name: {name}\ndescription: Describe this sequence\nsteps:\n  - type: message\n    content: Describe the workflow here.\n")
}

fn sequence_source_label(source: &str, user_dir: &str, project_dir: &str) -> &'static str {
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
    let normalized_dir = if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{dir}/")
    };
    path.starts_with(&normalized_dir)
}

fn split_comma_list(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_sequence_vars(values: &[String]) -> Result<HashMap<String, String>, String> {
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

fn format_sequence_step(step: &SequenceStep) -> String {
    let step_type = step.step_type.as_str();
    match step_type {
        "message" => format!("[message] {}", normalized_step_content(step)),
        "pause" => {
            if !step.reason.is_empty() {
                format!("[pause {}] {}", step.duration, step.reason)
            } else {
                format!("[pause {}]", step.duration)
            }
        }
        "conditional" => {
            let mut when = step.when.trim().to_string();
            if when.is_empty() && !step.expression.trim().is_empty() {
                when = "expr".to_string();
            }
            if when.is_empty() {
                when = "custom".to_string();
            }
            if !step.expression.is_empty() {
                format!(
                    "[conditional:{}] {} (expr: {})",
                    when,
                    normalized_step_content(step),
                    step.expression
                )
            } else {
                format!("[conditional:{}] {}", when, normalized_step_content(step))
            }
        }
        other => format!("[{other}]"),
    }
}

fn format_sequence_step_short(step: &SequenceStep) -> String {
    let step_type = step.step_type.as_str();
    match step_type {
        "message" => "message".to_string(),
        "pause" => {
            if !step.duration.is_empty() {
                format!("pause {}", step.duration)
            } else {
                "pause".to_string()
            }
        }
        "conditional" => {
            let mut when = step.when.trim().to_string();
            if when.is_empty() && !step.expression.trim().is_empty() {
                when = "expr".to_string();
            }
            if when.is_empty() {
                when = "custom".to_string();
            }
            format!("conditional:{}", when)
        }
        other => other.to_string(),
    }
}

fn normalized_step_content(step: &SequenceStep) -> &str {
    if !step.content.is_empty() {
        &step.content
    } else {
        &step.message
    }
}

// ---------------------------------------------------------------------------
// Rendering (validation parity for `seq run`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum RenderedQueueItem {
    Message {
        text: String,
    },
    Pause {
        duration_seconds: i64,
        reason: String,
    },
    Conditional {
        condition_type: String,
        expression: String,
        message: String,
    },
}

fn render_sequence(
    seq: &Sequence,
    vars: &HashMap<String, String>,
) -> Result<Vec<RenderedQueueItem>, String> {
    // Apply defaults + required checks (Go: sequences.RenderSequence).
    let mut data: HashMap<String, String> = vars.clone();
    for variable in &seq.variables {
        let value = data
            .get(&variable.name)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if value.is_empty() {
            if !variable.default_value.is_empty() {
                data.insert(variable.name.clone(), variable.default_value.clone());
                continue;
            }
            if variable.required {
                return Err(format!("missing required variable {:?}", variable.name));
            }
        }
    }

    let mut out = Vec::with_capacity(seq.steps.len());
    for (idx, step) in seq.steps.iter().enumerate() {
        let step_no = idx + 1;
        match step.step_type.as_str() {
            "message" => {
                let text = render_text(&seq.name, normalized_step_content(step), &data).map_err(
                    |err| format!("render sequence {:?} step {}: {err}", seq.name, step_no),
                )?;
                out.push(RenderedQueueItem::Message { text });
            }
            "pause" => {
                let nanos = parse_go_duration_to_nanos(&step.duration).map_err(|err| {
                    format!(
                        "render sequence {:?} step {}: invalid pause duration: {err}",
                        seq.name, step_no
                    )
                })?;
                if nanos <= 0 {
                    return Err(format!(
                        "render sequence {:?} step {}: pause duration must be greater than 0",
                        seq.name, step_no
                    ));
                }
                let seconds = round_nanos_to_seconds(nanos);
                if seconds <= 0 {
                    return Err(format!(
                        "render sequence {:?} step {}: pause duration must be at least 1s",
                        seq.name, step_no
                    ));
                }
                out.push(RenderedQueueItem::Pause {
                    duration_seconds: seconds,
                    reason: step.reason.clone(),
                });
            }
            "conditional" => {
                let message = render_text(&seq.name, normalized_step_content(step), &data)
                    .map_err(|err| {
                        format!("render sequence {:?} step {}: {err}", seq.name, step_no)
                    })?;
                let (cond_type, expr) = condition_type_from_step(step).map_err(|err| {
                    format!("render sequence {:?} step {}: {err}", seq.name, step_no)
                })?;
                out.push(RenderedQueueItem::Conditional {
                    condition_type: cond_type,
                    expression: expr,
                    message,
                });
            }
            other => {
                return Err(format!(
                    "render sequence {:?} step {}: unknown step type {:?}",
                    seq.name, step_no, other
                ));
            }
        }
    }
    Ok(out)
}

fn render_text(
    _name: &str,
    content: &str,
    data: &HashMap<String, String>,
) -> Result<String, String> {
    // Minimal Go text/template parity: {{.Key}} and {{ .Key }}.
    // Builtin sequences only rely on this subset.
    let mut result = content.to_string();
    for (key, value) in data {
        let pattern_tight = format!("{{{{.{key}}}}}");
        let pattern_spaced = format!("{{{{ .{key} }}}}");
        result = result.replace(&pattern_tight, value);
        result = result.replace(&pattern_spaced, value);
    }
    Ok(result)
}

fn condition_type_from_step(step: &SequenceStep) -> Result<(String, String), String> {
    let when_raw = step.when.trim();
    let when_lower = when_raw.to_lowercase();

    if when_lower.starts_with("expr:") {
        let expr = when_raw.get(5..).unwrap_or("").trim();
        if expr.is_empty() {
            return Err("conditional expression is required".to_string());
        }
        return Ok(("custom_expression".to_string(), expr.to_string()));
    }
    if when_lower.starts_with("expression:") {
        let expr = when_raw.get(11..).unwrap_or("").trim();
        if expr.is_empty() {
            return Err("conditional expression is required".to_string());
        }
        return Ok(("custom_expression".to_string(), expr.to_string()));
    }

    let normalized = when_raw.to_lowercase().replace('_', "-");
    match normalized.as_str() {
        "idle" | "when-idle" | "whenidle" => Ok(("when_idle".to_string(), String::new())),
        "after-cooldown" | "cooldown" | "cooldown-over" | "aftercooldown" => Ok((
            "after_cooldown".to_string(),
            step.expression.trim().to_string(),
        )),
        "after-previous" | "afterprevious" => Ok(("after_previous".to_string(), String::new())),
        "queue-empty" | "queueempty" => Ok((
            "custom_expression".to_string(),
            "queue_length == 0".to_string(),
        )),
        "custom" | "expression" | "expr" => {
            let expr = step.expression.trim();
            if expr.is_empty() {
                return Err("conditional expression is required".to_string());
            }
            Ok(("custom_expression".to_string(), expr.to_string()))
        }
        _ => Err(format!("unknown conditional when {:?}", step.when)),
    }
}

fn round_nanos_to_seconds(nanos: i64) -> i64 {
    // Go: duration.Round(time.Second).Seconds() then int()
    // We only accept positive durations for pause steps.
    (nanos + 500_000_000) / 1_000_000_000
}

fn parse_go_duration_to_nanos(raw: &str) -> Result<i64, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("time: invalid duration {:?}", raw));
    }

    // Go supports sequences like "1h30m". Parse as repeated <number><unit>.
    let mut i = 0usize;
    let bytes = s.as_bytes();
    let mut total: f64 = 0.0;
    let mut parsed_any = false;

    let mut sign = 1.0;
    if s.starts_with('-') {
        sign = -1.0;
        i = 1;
    } else if s.starts_with('+') {
        i = 1;
    }

    while i < bytes.len() {
        parsed_any = true;
        // Parse number (integer or decimal).
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i == start {
            return Err(format!("time: invalid duration {:?}", raw));
        }
        let num_str = &s[start..i];
        let value: f64 = num_str
            .parse()
            .map_err(|_| format!("time: invalid duration {:?}", raw))?;

        let rest = &s[i..];
        let (mult, adv) = if rest.starts_with("ns") {
            (1.0, 2usize)
        } else if rest.starts_with("us") {
            (1_000.0, 2usize)
        } else if rest.starts_with("µs") {
            (1_000.0, "µs".len())
        } else if rest.starts_with("ms") {
            (1_000_000.0, 2usize)
        } else if rest.starts_with('s') {
            (1_000_000_000.0, 1usize)
        } else if rest.starts_with('m') {
            (60.0 * 1_000_000_000.0, 1usize)
        } else if rest.starts_with('h') {
            (3600.0 * 1_000_000_000.0, 1usize)
        } else {
            return Err(format!("time: invalid duration {:?}", raw));
        };
        i += adv;
        total += value * mult;
    }

    if !parsed_any {
        return Err(format!("time: invalid duration {:?}", raw));
    }

    if !total.is_finite() {
        return Err(format!("time: invalid duration {:?}", raw));
    }

    let nanos = (total * sign).round();
    if nanos > (i64::MAX as f64) {
        return Err(format!("time: invalid duration {:?}", raw));
    }
    Ok(nanos as i64)
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

// ---------------------------------------------------------------------------
// JSON / JSONL output (match Go: slice => many lines for JSONL)
// ---------------------------------------------------------------------------

fn write_json_or_jsonl(
    output: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        // Best-effort: if serializing a slice, write one item per line.
        let as_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
        if let serde_json::Value::Array(items) = as_value {
            for item in items {
                serde_json::to_writer(&mut *output, &item).map_err(|e| e.to_string())?;
                writeln!(output).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        serde_json::to_writer(&mut *output, &as_value).map_err(|e| e.to_string())?;
        writeln!(output).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        serde_json::to_writer_pretty(&mut *output, value).map_err(|e| e.to_string())?;
        writeln!(output).map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Human table output
// ---------------------------------------------------------------------------

fn write_table(out: &mut dyn Write, headers: &[&str], rows: &[Vec<String>]) -> Result<(), String> {
    let col_count = headers
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if col_count == 0 {
        return Ok(());
    }

    let mut widths = vec![0usize; col_count];
    for (idx, header) in headers.iter().enumerate() {
        widths[idx] = widths[idx].max(header.len());
    }
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx < widths.len() {
                widths[idx] = widths[idx].max(cell.len());
            }
        }
    }

    let mut write_row = |row: Vec<String>| -> Result<(), String> {
        for (idx, width) in widths.iter().enumerate().take(col_count) {
            let cell = row.get(idx).cloned().unwrap_or_default();
            write!(out, "{cell}").map_err(|e| e.to_string())?;
            if idx + 1 < col_count {
                let padding = (*width).saturating_sub(cell.len()) + 2;
                write!(out, "{}", " ".repeat(padding)).map_err(|e| e.to_string())?;
            }
        }
        writeln!(out).map_err(|e| e.to_string())?;
        Ok(())
    };

    write_row(headers.iter().map(|h| h.to_string()).collect())?;
    for row in rows {
        write_row(row.clone())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        stdout,
        "Create, edit, and run reusable multi-step sequences."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge seq <command> [options]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Aliases:")?;
    writeln!(stdout, "  seq, sequence")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  ls          List sequences")?;
    writeln!(stdout, "  show        Show sequence details")?;
    writeln!(stdout, "  add         Create a new sequence")?;
    writeln!(stdout, "  edit        Edit an existing sequence")?;
    writeln!(stdout, "  run         Queue a sequence")?;
    writeln!(stdout, "  delete      Delete a user sequence")?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "      --tags string   filter by tags (comma-separated or repeatable) (ls only)"
    )?;
    writeln!(
        stdout,
        "  -a, --agent string  agent ID or prefix (run only)"
    )?;
    writeln!(
        stdout,
        "      --var string    sequence variable key=value (run only)"
    )?;
    Ok(())
}

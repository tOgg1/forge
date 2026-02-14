use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use forge_loop::ledger_writer::{
    append_workflow_ledger_entry, ensure_workflow_ledger_file, WorkflowLedgerRecord,
    WorkflowRunLedgerRecord, WorkflowStepLedgerRecord,
};
use serde::{Deserialize, Serialize};

#[path = "workflow_bash_executor.rs"]
pub mod bash_executor;
#[path = "workflow_run_persistence.rs"]
pub mod run_persistence;

// ---------------------------------------------------------------------------
// Step types
// ---------------------------------------------------------------------------

const STEP_TYPE_AGENT: &str = "agent";
const STEP_TYPE_LOOP: &str = "loop";
const STEP_TYPE_BASH: &str = "bash";
const STEP_TYPE_LOGIC: &str = "logic";
const STEP_TYPE_JOB: &str = "job";
const STEP_TYPE_WORKFLOW: &str = "workflow";
const STEP_TYPE_HUMAN: &str = "human";
const WORKFLOW_MAX_PARALLEL_ENV: &str = "FORGE_WORKFLOW_MAX_PARALLEL";
const DEFAULT_WORKFLOW_MAX_PARALLEL: usize = 4;
const DEFAULT_HUMAN_TIMEOUT_LABEL: &str = "24h";

fn valid_step_types() -> HashSet<&'static str> {
    [
        STEP_TYPE_AGENT,
        STEP_TYPE_LOOP,
        STEP_TYPE_BASH,
        STEP_TYPE_LOGIC,
        STEP_TYPE_JOB,
        STEP_TYPE_WORKFLOW,
        STEP_TYPE_HUMAN,
    ]
    .into_iter()
    .collect()
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

const ERR_PARSE: &str = "ERR_PARSE";
const ERR_MISSING_FIELD: &str = "ERR_MISSING_FIELD";
const ERR_INVALID_FIELD: &str = "ERR_INVALID_FIELD";
const ERR_UNKNOWN_TYPE: &str = "ERR_UNKNOWN_TYPE";
const ERR_DUPLICATE_STEP: &str = "ERR_DUPLICATE_STEP";
const ERR_MISSING_STEP: &str = "ERR_MISSING_STEP";
const ERR_CYCLE: &str = "ERR_CYCLE";

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max_parallel: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<WorkflowHooks>,
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, rename = "type")]
    pub step_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub when: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<WorkflowHooks>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alive_with: Vec<String>,

    // agent/loop/human fields
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pool: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub max_runtime: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interval: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max_iterations: i64,
    // bash fields
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cmd: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workdir: String,
    // logic fields
    #[serde(default, rename = "if", skip_serializing_if = "String::is_empty")]
    pub if_cond: String,
    #[serde(default, rename = "then", skip_serializing_if = "Vec::is_empty")]
    pub then_targets: Vec<String>,
    #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
    pub else_targets: Vec<String>,
    // job fields
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub job_name: String,
    // workflow fields
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workflow_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timeout: String,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowHooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub post: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopCondition {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<StopTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<StopLLM>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopTool {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopLLM {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rubric: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pass_if: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub step_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub line: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub column: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub index: usize,
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

impl WorkflowError {
    fn human_string(&self) -> String {
        let mut parts = Vec::new();
        if !self.path.is_empty() {
            parts.push(self.path.clone());
        }
        if !self.step_id.is_empty() {
            parts.push(format!("step {}", self.step_id));
        } else if self.index > 0 {
            parts.push(format!("step #{}", self.index));
        }
        if !self.field.is_empty() {
            parts.push(self.field.clone());
        }

        let prefix = if parts.is_empty() {
            "workflow".to_string()
        } else {
            parts.join(": ")
        };

        let message = if self.message.is_empty() {
            self.code.clone()
        } else {
            self.message.clone()
        };

        let message = if self.line > 0 {
            if self.column > 0 {
                format!(
                    "{message} (line {line}:{col})",
                    line = self.line,
                    col = self.column
                )
            } else {
                format!("{message} (line {line})", line = self.line)
            }
        } else {
            message
        };

        format!("{prefix}: {message}")
    }
}

#[derive(Debug, Clone, Serialize)]
struct ValidationResult {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    path: String,
    valid: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<WorkflowError>,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowLogsResult {
    run_id: String,
    workflow_name: String,
    workflow_source: String,
    status: String,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
    steps: Vec<WorkflowStepLogsResult>,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowStepLogsResult {
    step_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
    fan_out_running: usize,
    fan_out_queued: usize,
    log: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct StepFanOutCounts {
    running: usize,
    queued: usize,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowApprovalResult {
    run_id: String,
    step_id: String,
    decision: String,
    run_status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remaining_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowBlockedStepResult {
    step_id: String,
    status: String,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowBlockedResult {
    run_id: String,
    workflow_name: String,
    blocked_steps: Vec<WorkflowBlockedStepResult>,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait WorkflowBackend {
    /// Load all workflows from search paths, sorted alphabetically by name.
    fn load_workflows(&self) -> Result<Vec<Workflow>, String>;

    /// Load a single workflow by name (case-insensitive, .toml suffix optional).
    fn load_workflow_by_name(&self, name: &str) -> Result<Workflow, String>;

    /// Return the project directory (for relative path display).
    fn project_dir(&self) -> Option<&Path>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct InMemoryWorkflowBackend {
    pub workflows: Vec<Workflow>,
    pub project_dir: Option<PathBuf>,
}

impl WorkflowBackend for InMemoryWorkflowBackend {
    fn load_workflows(&self) -> Result<Vec<Workflow>, String> {
        let mut items = self.workflows.clone();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    fn load_workflow_by_name(&self, name: &str) -> Result<Workflow, String> {
        let clean = normalize_workflow_name(name)?;
        self.workflows
            .iter()
            .find(|wf| wf.name.eq_ignore_ascii_case(&clean))
            .cloned()
            .ok_or_else(|| format!("workflow {clean:?} not found"))
    }

    fn project_dir(&self) -> Option<&Path> {
        self.project_dir.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct FilesystemWorkflowBackend {
    project_dir: PathBuf,
}

impl FilesystemWorkflowBackend {
    pub fn open_from_env() -> Self {
        let project_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { project_dir }
    }

    #[cfg(test)]
    pub fn for_project_dir(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    fn load_workflow_from_path(path: &Path) -> Result<Workflow, String> {
        let source = path.to_string_lossy().to_string();
        let data = fs::read_to_string(path)
            .map_err(|err| format!("read workflow {}: {err}", path.display()))?;
        parse_workflow_toml(&data, &source).map_err(|errors| {
            errors
                .iter()
                .map(WorkflowError::human_string)
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    fn workflow_dir(&self) -> PathBuf {
        self.project_dir.join(".forge").join("workflows")
    }
}

impl WorkflowBackend for FilesystemWorkflowBackend {
    fn load_workflows(&self) -> Result<Vec<Workflow>, String> {
        let dir = self.workflow_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("read workflows dir {}: {err}", dir.display())),
        };

        let mut items = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("read workflows dir {}: {err}", dir.display()))?;
            let file_type = entry
                .file_type()
                .map_err(|err| format!("read workflows dir {}: {err}", dir.display()))?;
            if file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let is_toml = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"));
            if !is_toml {
                continue;
            }
            items.push(Self::load_workflow_from_path(&path)?);
        }

        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    fn load_workflow_by_name(&self, name: &str) -> Result<Workflow, String> {
        let clean = normalize_workflow_name(name)?;
        let candidate = self.workflow_dir().join(format!("{clean}.toml"));
        if candidate.is_file() {
            return Self::load_workflow_from_path(&candidate);
        }

        self.load_workflows()?
            .into_iter()
            .find(|wf| wf.name.eq_ignore_ascii_case(&clean))
            .ok_or_else(|| format!("workflow {clean:?} not found"))
    }

    fn project_dir(&self) -> Option<&Path> {
        Some(&self.project_dir)
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum SubCommand {
    Help,
    List,
    Show {
        name: String,
    },
    Validate {
        name: String,
    },
    Run {
        name: String,
        node: Option<String>,
    },
    Logs {
        run_id: String,
    },
    Approve {
        run_id: String,
        step_id: String,
    },
    Deny {
        run_id: String,
        step_id: String,
        reason: String,
    },
    Blocked {
        run_id: String,
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

pub fn run_for_test(args: &[&str], backend: &dyn WorkflowBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run_with_backend(&owned_args, backend, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &dyn WorkflowBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout, stderr) {
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
    backend: &dyn WorkflowBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    match parsed.command {
        SubCommand::Help => {
            write_help(stdout).map_err(|e| e.to_string())?;
            Ok(())
        }
        SubCommand::List => execute_list(backend, parsed.json, parsed.jsonl, stdout),
        SubCommand::Show { ref name } => {
            execute_show(backend, name, parsed.json, parsed.jsonl, stdout)
        }
        SubCommand::Validate { ref name } => {
            execute_validate(backend, name, parsed.json, parsed.jsonl, stdout, stderr)
        }
        SubCommand::Run { ref name, ref node } => execute_run(
            backend,
            name,
            node.as_deref(),
            parsed.json,
            parsed.jsonl,
            stdout,
            stderr,
        ),
        SubCommand::Logs { ref run_id } => {
            execute_logs(backend, run_id, parsed.json, parsed.jsonl, stdout)
        }
        SubCommand::Approve {
            ref run_id,
            ref step_id,
        } => execute_approve(backend, run_id, step_id, parsed.json, parsed.jsonl, stdout),
        SubCommand::Deny {
            ref run_id,
            ref step_id,
            ref reason,
        } => execute_deny(run_id, step_id, reason, parsed.json, parsed.jsonl, stdout),
        SubCommand::Blocked { ref run_id } => {
            execute_blocked(backend, run_id, parsed.json, parsed.jsonl, stdout)
        }
    }
}

fn execute_list(
    backend: &dyn WorkflowBackend,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let items = backend.load_workflows()?;

    if json || jsonl {
        return write_json_output(stdout, &items, jsonl);
    }

    if items.is_empty() {
        writeln!(stdout, "No workflows found").map_err(|e| e.to_string())?;
        return Ok(());
    }

    let project_dir = backend.project_dir().unwrap_or_else(|| Path::new(""));

    let mut tw = tabwriter::TabWriter::new(Vec::new());
    let _ = writeln!(tw, "NAME\tSTEPS\tDESCRIPTION\tPATH");
    for wf in &items {
        let _ = writeln!(
            tw,
            "{}\t{}\t{}\t{}",
            wf.name,
            wf.steps.len(),
            wf.description,
            workflow_source_path(&wf.source, project_dir),
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
    backend: &dyn WorkflowBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let wf = backend.load_workflow_by_name(name)?;
    let (validated, _errors) = validate_workflow(&wf);

    if json || jsonl {
        return write_json_output(stdout, &validated, jsonl);
    }

    let project_dir = backend.project_dir().unwrap_or_else(|| Path::new(""));
    print_workflow(&validated, project_dir, stdout)?;
    Ok(())
}

fn execute_validate(
    backend: &dyn WorkflowBackend,
    name: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    let wf = backend.load_workflow_by_name(name)?;
    let (validated, errors) = validate_workflow(&wf);

    let result = ValidationResult {
        name: validated.name.clone(),
        path: validated.source.clone(),
        valid: errors.is_empty(),
        errors: errors.clone(),
    };

    if json || jsonl {
        write_json_output(stdout, &result, jsonl)?;
        if !errors.is_empty() {
            return Err(String::new()); // exit code 1, but message already written
        }
        return Ok(());
    }

    if errors.is_empty() {
        writeln!(stdout, "Workflow valid: {}", validated.name).map_err(|e| e.to_string())?;
    } else {
        writeln!(stdout, "Workflow invalid: {}", validated.name).map_err(|e| e.to_string())?;
        for err in &errors {
            let _ = writeln!(stderr, "- {}", err.human_string());
        }
        return Err(String::new()); // exit code 1, but message already written
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowRunCommandResult {
    run_id: String,
    workflow_name: String,
    status: String,
}

fn execute_run(
    backend: &dyn WorkflowBackend,
    name: &str,
    node: Option<&str>,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
    if let Some(node_id) = node {
        let command = crate::node::build_remote_command(
            "forge workflow run",
            &[name.to_string()],
            json,
            jsonl,
        );
        return crate::node::run_remote_passthrough(node_id, &command, stdout, stderr);
    }

    let wf = backend.load_workflow_by_name(name)?;
    let (validated, errors) = validate_workflow(&wf);
    if !errors.is_empty() {
        writeln!(stderr, "Workflow invalid: {}", validated.name).map_err(|e| e.to_string())?;
        for err in &errors {
            let _ = writeln!(stderr, "- {}", err.human_string());
        }
        return Err(String::new());
    }

    let result = run_workflow(&validated)?;
    if json || jsonl {
        write_json_output(stdout, &result, jsonl)?;
        if result.status == "failed" {
            return Err(String::new());
        }
        return Ok(());
    }

    writeln!(stdout, "Workflow run: {}", result.run_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Workflow: {}", result.workflow_name).map_err(|e| e.to_string())?;
    writeln!(stdout, "Status: {}", result.status).map_err(|e| e.to_string())?;
    if result.status == "failed" {
        return Err(format!("workflow run {} failed", result.run_id));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkflowParallelSource {
    Workflow,
    Environment,
    GlobalConfig,
    Default,
}

fn workflow_parallel_source_label(source: WorkflowParallelSource) -> &'static str {
    match source {
        WorkflowParallelSource::Workflow => "workflow",
        WorkflowParallelSource::Environment => "env",
        WorkflowParallelSource::GlobalConfig => "global config",
        WorkflowParallelSource::Default => "default",
    }
}

#[derive(Debug, Deserialize, Default)]
struct GlobalWorkflowConfig {
    #[serde(default)]
    scheduler: GlobalWorkflowSchedulerConfig,
}

#[derive(Debug, Deserialize, Default)]
struct GlobalWorkflowSchedulerConfig {
    #[serde(default)]
    workflow_max_parallel: i64,
}

fn parse_max_parallel_i64(value: i64, source: &str) -> Result<usize, String> {
    if value <= 0 {
        return Err(format!("{source} must be greater than 0"));
    }
    usize::try_from(value).map_err(|_| format!("{source} is too large"))
}

fn global_config_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FORGE_CONFIG_PATH") {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("forge")
            .join("config.yaml")
    })
}

fn resolve_workflow_max_parallel(wf: &Workflow) -> Result<(usize, WorkflowParallelSource), String> {
    if wf.max_parallel != 0 {
        let parsed = parse_max_parallel_i64(wf.max_parallel, "workflow.max_parallel")?;
        return Ok((parsed, WorkflowParallelSource::Workflow));
    }

    if let Ok(raw) = std::env::var(WORKFLOW_MAX_PARALLEL_ENV) {
        let parsed = raw
            .trim()
            .parse::<i64>()
            .map_err(|err| format!("{WORKFLOW_MAX_PARALLEL_ENV} parse error: {err}"))?;
        let parsed = parse_max_parallel_i64(parsed, WORKFLOW_MAX_PARALLEL_ENV)?;
        return Ok((parsed, WorkflowParallelSource::Environment));
    }

    if let Some(path) = global_config_path() {
        let raw = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(format!("read global config {}: {err}", path.display())),
        };
        if !raw.is_empty() {
            let parsed: GlobalWorkflowConfig = serde_yaml::from_str(&raw)
                .map_err(|err| format!("parse global config {}: {err}", path.display()))?;
            if parsed.scheduler.workflow_max_parallel != 0 {
                let value = parse_max_parallel_i64(
                    parsed.scheduler.workflow_max_parallel,
                    "scheduler.workflow_max_parallel",
                )?;
                return Ok((value, WorkflowParallelSource::GlobalConfig));
            }
        }
    }

    Ok((
        DEFAULT_WORKFLOW_MAX_PARALLEL,
        WorkflowParallelSource::Default,
    ))
}

fn run_workflow(wf: &Workflow) -> Result<WorkflowRunCommandResult, String> {
    let (max_parallel, _) = resolve_workflow_max_parallel(wf)?;
    let store = Arc::new(run_persistence::WorkflowRunStore::open_from_env());
    let step_ids: Vec<String> = wf.steps.iter().map(|step| step.id.clone()).collect();
    let mut run = store.create_run(&wf.name, &wf.source, &step_ids)?;

    let engine_steps: Vec<run_persistence::WorkflowEngineStep> = wf
        .steps
        .iter()
        .map(|step| run_persistence::WorkflowEngineStep {
            id: step.id.clone(),
            depends_on: step.depends_on.clone(),
        })
        .collect();
    let step_lookup: Arc<HashMap<String, WorkflowStep>> = Arc::new(
        wf.steps
            .iter()
            .cloned()
            .map(|step| (step.id.clone(), step))
            .collect(),
    );
    let repo_workdir = resolved_workflow_repo_dir(wf)?;
    let workflow_hook_log_step = step_ids.first().map(|value| value.as_str());
    let run_id = run.id.clone();
    let step_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let status_lock = Arc::new(Mutex::new(()));

    if let Some(hooks) = &wf.hooks {
        if let Err(err) = run_hook_list(
            store.as_ref(),
            &run.id,
            workflow_hook_log_step,
            "workflow.pre",
            &hooks.pre,
            &repo_workdir,
        ) {
            if let Some(step_id) = workflow_hook_log_step {
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                let _ = store.update_step_status(
                    &run.id,
                    step_id,
                    run_persistence::WorkflowStepStatus::Failed,
                );
                let _ = store.append_step_log(&run.id, step_id, format!("error: {err}").as_str());
            }
            run = store.update_run_status(&run.id, run_persistence::WorkflowRunStatus::Failed)?;
            return Ok(WorkflowRunCommandResult {
                run_id: run.id,
                workflow_name: run.workflow_name,
                status: run_status_label(&run.status).to_string(),
            });
        }
    }

    let engine_result =
        run_persistence::execute_parallel_workflow(&engine_steps, max_parallel, |step_id| {
            let Some(step) = step_lookup.get(step_id) else {
                return Err(format!("step {:?} missing from lookup", step_id));
            };

            {
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run_id,
                    step_id,
                    run_persistence::WorkflowStepStatus::Running,
                )?;
            }
            let fail_step =
                |message: String| -> Result<run_persistence::WorkflowEngineStepResult, String> {
                    if let Ok(_guard) = status_lock.lock() {
                        let _ = store.update_step_status(
                            &run_id,
                            step_id,
                            run_persistence::WorkflowStepStatus::Failed,
                        );
                    }
                    let _ = store.append_step_log(
                        &run_id,
                        step_id,
                        format!("error: {message}").as_str(),
                    );
                    Err(message)
                };
            let step_outputs_snapshot = step_outputs
                .lock()
                .map_err(|_| "workflow step output lock poisoned".to_string())?
                .clone();
            let resolved_inputs = match resolve_step_inputs(step, &step_outputs_snapshot) {
                Ok(value) => value,
                Err(err) => return fail_step(format!("resolve inputs for step {step_id}: {err}")),
            };

            match step.step_type.as_str() {
                STEP_TYPE_BASH => {
                    if let Some(hooks) = &step.hooks {
                        if let Err(err) = run_hook_list(
                            store.as_ref(),
                            &run_id,
                            Some(step_id),
                            "step.pre",
                            &hooks.pre,
                            &repo_workdir,
                        ) {
                            return fail_step(err);
                        }
                    }

                    let body_result: Result<(), String> = (|| {
                        let resolved_cmd = render_binding_template(
                            &step.cmd,
                            &step_outputs_snapshot,
                            Some(&resolved_inputs),
                        )
                        .map_err(|err| format!("resolve cmd for step {step_id}: {err}"))?;
                        let request = run_persistence::BashStepRequest::new(
                            step_id,
                            resolved_cmd,
                            repo_workdir.clone(),
                            &step.workdir,
                        )
                        .with_extra_env(bash_input_env(&resolved_inputs));
                        let result = run_persistence::execute_bash_step(&request)?;
                        run_persistence::append_bash_step_logs(store.as_ref(), &run_id, &result)?;
                        if !result.success {
                            return Err(format!("exit status {}", result.exit_code));
                        }

                        let outputs = build_step_outputs(
                            step_id,
                            step,
                            &result,
                            &resolved_inputs,
                            &step_outputs_snapshot,
                        )
                        .map_err(|err| format!("resolve outputs for step {step_id}: {err}"))?;
                        let persisted_outputs = outputs.clone();
                        step_outputs
                            .lock()
                            .map_err(|_| "workflow step output lock poisoned".to_string())?
                            .insert(step_id.to_string(), outputs);
                        {
                            let _guard = status_lock
                                .lock()
                                .map_err(|_| "workflow status lock poisoned".to_string())?;
                            store.update_step_outputs(&run_id, step_id, persisted_outputs)?;
                            store.update_step_status(
                                &run_id,
                                step_id,
                                run_persistence::WorkflowStepStatus::Success,
                            )?;
                        }
                        Ok(())
                    })();

                    let post_hook_result = if let Some(hooks) = &step.hooks {
                        run_hook_list(
                            store.as_ref(),
                            &run_id,
                            Some(step_id),
                            "step.post",
                            &hooks.post,
                            &repo_workdir,
                        )
                    } else {
                        Ok(())
                    };

                    match (body_result, post_hook_result) {
                        (Ok(()), Ok(())) => Ok(run_persistence::WorkflowEngineStepResult::Success),
                        (Err(body_err), Ok(())) => fail_step(body_err),
                        (Ok(()), Err(post_err)) => fail_step(post_err),
                        (Err(body_err), Err(post_err)) => {
                            fail_step(format!("{body_err}; post hook error: {post_err}"))
                        }
                    }
                }
                STEP_TYPE_HUMAN => {
                    if let Some(hooks) = &step.hooks {
                        if let Err(err) = run_hook_list(
                            store.as_ref(),
                            &run_id,
                            Some(step_id),
                            "step.pre",
                            &hooks.pre,
                            &repo_workdir,
                        ) {
                            return fail_step(err);
                        }
                    }

                    let timeout = match resolve_human_timeout(&step.timeout) {
                        Ok(value) => value,
                        Err(err) => return fail_step(format!("resolve human timeout: {err}")),
                    };
                    let timeout_at = human_timeout_deadline(timeout);
                    {
                        let _guard = status_lock
                            .lock()
                            .map_err(|_| "workflow status lock poisoned".to_string())?;
                        store.mark_step_waiting_approval(&run_id, step_id, timeout_at.clone())?;
                    }
                    let timeout_hint = if step.timeout.trim().is_empty() {
                        format!("default({DEFAULT_HUMAN_TIMEOUT_LABEL})")
                    } else {
                        step.timeout.trim().to_string()
                    };
                    let timeout_line = timeout_at
                        .as_deref()
                        .map(|value| format!(" timeout_at={value}"))
                        .unwrap_or_default();
                    store.append_step_log(
                        &run_id,
                        step_id,
                        format!("awaiting human approval timeout={timeout_hint}{timeout_line}")
                            .as_str(),
                    )?;
                    Ok(run_persistence::WorkflowEngineStepResult::WaitingApproval)
                }
                other => {
                    let err = format!(
                        "workflow run currently supports bash/human steps only; got step type {:?}",
                        other
                    );
                    fail_step(err)
                }
            }
        })?;

    let mut failed = false;
    let mut waiting_approval = false;
    for record in &engine_result.steps {
        match record.status {
            run_persistence::WorkflowEngineStepStatus::Skipped => {
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run.id,
                    &record.step_id,
                    run_persistence::WorkflowStepStatus::Skipped,
                )?;
            }
            run_persistence::WorkflowEngineStepStatus::Failed => {
                failed = true;
            }
            run_persistence::WorkflowEngineStepStatus::WaitingApproval => {
                waiting_approval = true;
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run.id,
                    &record.step_id,
                    run_persistence::WorkflowStepStatus::WaitingApproval,
                )?;
            }
            run_persistence::WorkflowEngineStepStatus::Pending
            | run_persistence::WorkflowEngineStepStatus::Running
            | run_persistence::WorkflowEngineStepStatus::Success => {}
        }
        if !record.error.is_empty() {
            store.append_step_log(
                &run.id,
                &record.step_id,
                format!("error: {}", record.error).as_str(),
            )?;
        }
    }

    if !waiting_approval {
        if let Some(hooks) = &wf.hooks {
            if let Err(err) = run_hook_list(
                store.as_ref(),
                &run.id,
                workflow_hook_log_step,
                "workflow.post",
                &hooks.post,
                &repo_workdir,
            ) {
                failed = true;
                if let Some(step_id) = workflow_hook_log_step {
                    let _guard = status_lock
                        .lock()
                        .map_err(|_| "workflow status lock poisoned".to_string())?;
                    let _ = store.update_step_status(
                        &run.id,
                        step_id,
                        run_persistence::WorkflowStepStatus::Failed,
                    );
                    let _ =
                        store.append_step_log(&run.id, step_id, format!("error: {err}").as_str());
                }
            }
        }
    }

    {
        let _guard = status_lock
            .lock()
            .map_err(|_| "workflow status lock poisoned".to_string())?;
        run = store.update_run_status(
            &run.id,
            if failed {
                run_persistence::WorkflowRunStatus::Failed
            } else if waiting_approval {
                run_persistence::WorkflowRunStatus::Running
            } else {
                run_persistence::WorkflowRunStatus::Success
            },
        )?;
    }
    append_workflow_run_ledger(
        wf,
        &run,
        &engine_result,
        step_lookup.as_ref(),
        &repo_workdir,
    )?;

    Ok(WorkflowRunCommandResult {
        run_id: run.id,
        workflow_name: run.workflow_name,
        status: run_status_label(&run.status).to_string(),
    })
}

fn resolve_step_inputs(
    step: &WorkflowStep,
    step_outputs: &HashMap<String, HashMap<String, String>>,
) -> Result<HashMap<String, String>, String> {
    let mut resolved = HashMap::new();
    for (key, value) in &step.inputs {
        let raw = toml_value_to_binding_text(value);
        let rendered = render_binding_template(&raw, step_outputs, None)?;
        resolved.insert(key.clone(), rendered);
    }
    Ok(resolved)
}

fn build_step_outputs(
    step_id: &str,
    step: &WorkflowStep,
    result: &run_persistence::BashStepExecutionResult,
    step_inputs: &HashMap<String, String>,
    step_outputs: &HashMap<String, HashMap<String, String>>,
) -> Result<HashMap<String, String>, String> {
    let mut outputs = HashMap::new();
    outputs.insert("output".to_string(), result.output.trim().to_string());
    outputs.insert("stdout".to_string(), result.stdout.trim().to_string());
    outputs.insert("stderr".to_string(), result.stderr.trim().to_string());
    outputs.insert("exit_code".to_string(), result.exit_code.to_string());

    let mut render_scope = step_outputs.clone();
    render_scope.insert(step_id.to_string(), outputs.clone());
    for (key, value) in &step.outputs {
        let raw = toml_value_to_binding_text(value);
        let rendered = render_binding_template(&raw, &render_scope, Some(step_inputs))?;
        outputs.insert(key.clone(), rendered);
        render_scope.insert(step_id.to_string(), outputs.clone());
    }
    Ok(outputs)
}

fn render_binding_template(
    text: &str,
    step_outputs: &HashMap<String, HashMap<String, String>>,
    step_inputs: Option<&HashMap<String, String>>,
) -> Result<String, String> {
    if !text.contains("{{") {
        return Ok(text.to_string());
    }

    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(start) = rest.find("{{") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err("unclosed template expression".to_string());
        };
        let token = after_start[..end].trim();
        if token.is_empty() {
            return Err("empty template expression".to_string());
        }
        let rendered = resolve_binding_token(token, step_outputs, step_inputs)?;
        out.push_str(&rendered);
        rest = &after_start[end + 2..];
    }
    Ok(out)
}

fn resolve_binding_token(
    token: &str,
    step_outputs: &HashMap<String, HashMap<String, String>>,
    step_inputs: Option<&HashMap<String, String>>,
) -> Result<String, String> {
    let mut parts = token.splitn(3, '.');
    let root = parts.next().unwrap_or_default();
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();

    match root {
        "steps" => {
            if first.is_empty() || second.is_empty() {
                return Err(format!(
                    "template token {:?} must use steps.<id>.<output>",
                    token
                ));
            }
            let Some(step_map) = step_outputs.get(first) else {
                return Err(format!(
                    "missing template step output: steps.{first}.{second}"
                ));
            };
            let Some(value) = step_map.get(second) else {
                return Err(format!(
                    "missing template step output: steps.{first}.{second}"
                ));
            };
            Ok(value.clone())
        }
        "inputs" => {
            if first.is_empty() || !second.is_empty() {
                return Err(format!("template token {:?} must use inputs.<name>", token));
            }
            let Some(inputs) = step_inputs else {
                return Err(format!(
                    "template token {:?} not available outside step context",
                    token
                ));
            };
            let Some(value) = inputs.get(first) else {
                return Err(format!("missing template input: inputs.{first}"));
            };
            Ok(value.clone())
        }
        _ => Err(format!("unsupported template token: {:?}", token)),
    }
}

fn toml_value_to_binding_text(value: &toml::Value) -> String {
    match value {
        toml::Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

fn bash_input_env(resolved_inputs: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut env_pairs: Vec<(String, String)> = resolved_inputs
        .iter()
        .map(|(key, value)| {
            (
                format!("FORGE_INPUT_{}", sanitize_binding_key(key)),
                value.clone(),
            )
        })
        .collect();
    env_pairs.sort_by(|a, b| a.0.cmp(&b.0));
    env_pairs
}

fn sanitize_binding_key(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "VALUE".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookFailureMode {
    Fail,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHook {
    mode: HookFailureMode,
    command: String,
}

fn parse_hook_spec(raw: &str) -> Result<ParsedHook, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("hook entry is empty".to_string());
    }

    let (mode, rest) = if let Some(value) = trimmed.strip_prefix("warn:") {
        (HookFailureMode::Warn, value)
    } else if let Some(value) = trimmed.strip_prefix("fail:") {
        (HookFailureMode::Fail, value)
    } else {
        (HookFailureMode::Fail, trimmed)
    };

    let Some(command) = rest.strip_prefix("bash:") else {
        return Err(format!("unsupported hook type: {:?}", raw));
    };
    let command = command.trim();
    if command.is_empty() {
        return Err(format!("hook command is required: {:?}", raw));
    }

    Ok(ParsedHook {
        mode,
        command: command.to_string(),
    })
}

fn run_hook_list(
    store: &run_persistence::WorkflowRunStore,
    run_id: &str,
    log_step_id: Option<&str>,
    phase: &str,
    hooks: &[String],
    workdir: &Path,
) -> Result<(), String> {
    for (index, raw_hook) in hooks.iter().enumerate() {
        let hook = parse_hook_spec(raw_hook)?;
        let mode = match hook.mode {
            HookFailureMode::Fail => "fail",
            HookFailureMode::Warn => "warn",
        };
        append_hook_log(
            store,
            run_id,
            log_step_id,
            format!(
                "hook {}#{} mode={} cmd={}",
                phase,
                index + 1,
                mode,
                hook.command
            )
            .as_str(),
        )?;

        let output = Command::new("bash")
            .arg("-c")
            .arg(&hook.command)
            .current_dir(workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|child| child.wait_with_output())
            .map_err(|err| format!("execute hook {}#{}: {err}", phase, index + 1))?;
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        append_hook_log(
            store,
            run_id,
            log_step_id,
            format!("hook {}#{} exit_code={exit_code}", phase, index + 1).as_str(),
        )?;
        if !stdout.trim().is_empty() {
            append_hook_log(
                store,
                run_id,
                log_step_id,
                format!("hook {}#{} stdout:", phase, index + 1).as_str(),
            )?;
            for line in stdout.lines() {
                append_hook_log(store, run_id, log_step_id, format!("  {line}").as_str())?;
            }
        }
        if !stderr.trim().is_empty() {
            append_hook_log(
                store,
                run_id,
                log_step_id,
                format!("hook {}#{} stderr:", phase, index + 1).as_str(),
            )?;
            for line in stderr.lines() {
                append_hook_log(store, run_id, log_step_id, format!("  {line}").as_str())?;
            }
        }

        if output.status.success() {
            continue;
        }

        let failure = format!(
            "hook {}#{} failed with exit status {exit_code}",
            phase,
            index + 1
        );
        match hook.mode {
            HookFailureMode::Warn => {
                append_hook_log(
                    store,
                    run_id,
                    log_step_id,
                    format!("warning: {failure}").as_str(),
                )?;
            }
            HookFailureMode::Fail => return Err(failure),
        }
    }
    Ok(())
}

fn append_hook_log(
    store: &run_persistence::WorkflowRunStore,
    run_id: &str,
    log_step_id: Option<&str>,
    line: &str,
) -> Result<(), String> {
    let Some(step_id) = log_step_id else {
        return Ok(());
    };
    store.append_step_log(run_id, step_id, line)
}

fn resolved_workflow_repo_dir(wf: &Workflow) -> Result<PathBuf, String> {
    let repo_root = repo_root_from_workflow(wf);
    if !repo_root.is_empty() {
        return Ok(PathBuf::from(repo_root));
    }
    std::env::current_dir().map_err(|err| format!("resolve current directory: {err}"))
}

fn append_workflow_run_ledger(
    wf: &Workflow,
    run: &run_persistence::WorkflowRunRecord,
    engine_run: &run_persistence::WorkflowEngineRun,
    step_lookup: &HashMap<String, WorkflowStep>,
    repo_workdir: &Path,
) -> Result<(), String> {
    let repo_path = repo_workdir.to_string_lossy().into_owned();
    let workflow_ledger = WorkflowLedgerRecord {
        workflow_name: run.workflow_name.clone(),
        workflow_source: run.workflow_source.clone(),
        repo_path: repo_path.clone(),
        ledger_path: default_workflow_ledger_path(&repo_path, &run.workflow_name, &wf.source),
    };
    ensure_workflow_ledger_file(&workflow_ledger)?;

    let started_at = parse_rfc3339_utc(&run.started_at).ok_or_else(|| {
        format!(
            "parse workflow run started_at {:?}: invalid rfc3339",
            run.started_at
        )
    })?;
    let finished_at = run.finished_at.as_deref().and_then(parse_rfc3339_utc);
    let workflow_run = WorkflowRunLedgerRecord {
        run_id: run.id.clone(),
        status: run_status_label(&run.status).to_string(),
        started_at,
        finished_at,
    };

    let mut engine_errors: HashMap<&str, &str> = HashMap::new();
    for step in &engine_run.steps {
        if !step.error.trim().is_empty() {
            engine_errors.insert(step.step_id.as_str(), step.error.as_str());
        }
    }

    let steps: Vec<WorkflowStepLedgerRecord> = run
        .steps
        .iter()
        .map(|step| {
            let started = step.started_at.as_deref().and_then(parse_rfc3339_utc);
            let finished = step.finished_at.as_deref().and_then(parse_rfc3339_utc);
            WorkflowStepLedgerRecord {
                step_id: step.step_id.clone(),
                step_type: step_lookup
                    .get(&step.step_id)
                    .map(|workflow_step| workflow_step.step_type.clone())
                    .unwrap_or_default(),
                status: step_status_label(&step.status).to_string(),
                duration_ms: step_duration_ms(started, finished),
                error: engine_errors
                    .get(step.step_id.as_str())
                    .copied()
                    .unwrap_or("")
                    .to_string(),
            }
        })
        .collect();

    append_workflow_ledger_entry(&workflow_ledger, &workflow_run, &steps)
}

fn parse_rfc3339_utc(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn step_duration_ms(
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
) -> Option<i64> {
    match (started_at, finished_at) {
        (Some(started_at), Some(finished_at)) => Some(
            finished_at
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0),
        ),
        _ => None,
    }
}

fn default_workflow_ledger_path(repo_path: &str, workflow_name: &str, source: &str) -> String {
    let slug = workflow_slug(workflow_name);
    let file_stem = if slug.is_empty() {
        let source_stem = Path::new(source)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        let source_slug = workflow_slug(&source_stem);
        if source_slug.is_empty() {
            "workflow-run".to_string()
        } else {
            format!("workflow-{source_slug}")
        }
    } else {
        format!("workflow-{slug}")
    };
    format!("{repo_path}/.forge/ledgers/{file_stem}.md")
}

fn workflow_slug(name: &str) -> String {
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if (ch == ' ' || ch == '-' || ch == '_') && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn resolve_human_timeout(raw: &str) -> Result<Option<Duration>, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return parse_human_timeout(DEFAULT_HUMAN_TIMEOUT_LABEL).map(Some);
    }
    if matches!(normalized.as_str(), "none" | "off" | "0") {
        return Ok(None);
    }
    parse_human_timeout(&normalized).map(Some)
}

fn parse_human_timeout(value: &str) -> Result<Duration, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err("timeout value is empty".to_string());
    }

    let digits_len = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    if digits_len == 0 {
        return Err(format!("invalid timeout {:?}", value));
    }

    let amount = value[..digits_len]
        .parse::<u64>()
        .map_err(|err| format!("invalid timeout {:?}: {err}", value))?;
    let unit = value[digits_len..].trim();
    let seconds = match unit {
        "" | "s" => amount,
        "m" => amount
            .checked_mul(60)
            .ok_or_else(|| format!("timeout overflow {:?}", value))?,
        "h" => amount
            .checked_mul(60 * 60)
            .ok_or_else(|| format!("timeout overflow {:?}", value))?,
        "d" => amount
            .checked_mul(24 * 60 * 60)
            .ok_or_else(|| format!("timeout overflow {:?}", value))?,
        _ => {
            return Err(format!("unsupported timeout unit {:?} (use s/m/h/d)", unit));
        }
    };
    Ok(Duration::from_secs(seconds))
}

fn human_timeout_deadline(timeout: Option<Duration>) -> Option<String> {
    let timeout = timeout?;
    let delta = chrono::Duration::from_std(timeout).ok()?;
    Some((Utc::now() + delta).to_rfc3339())
}

fn load_workflow_for_run_execution(
    backend: &dyn WorkflowBackend,
    run: &run_persistence::WorkflowRunRecord,
) -> Result<Workflow, String> {
    if !run.workflow_source.trim().is_empty() {
        if let Ok(raw) = fs::read_to_string(&run.workflow_source) {
            if let Ok(workflow) = parse_workflow_toml(&raw, &run.workflow_source) {
                return Ok(workflow);
            }
        }
    }
    backend.load_workflow_by_name(&run.workflow_name)
}

fn resume_workflow_after_approval(
    backend: &dyn WorkflowBackend,
    store: &run_persistence::WorkflowRunStore,
    run_id: &str,
) -> Result<run_persistence::WorkflowRunRecord, String> {
    let mut run = store.get_run(run_id)?;
    if run.status != run_persistence::WorkflowRunStatus::Running {
        return Ok(run);
    }

    if run.steps.iter().any(|step| {
        matches!(
            step.status,
            run_persistence::WorkflowStepStatus::WaitingApproval
        )
    }) {
        return Ok(run);
    }

    let pending_step_ids: HashSet<String> = run
        .steps
        .iter()
        .filter(|step| matches!(step.status, run_persistence::WorkflowStepStatus::Pending))
        .map(|step| step.step_id.clone())
        .collect();

    if pending_step_ids.is_empty() {
        run = store.update_run_status(run_id, run_persistence::WorkflowRunStatus::Success)?;
        return Ok(run);
    }

    let wf = load_workflow_for_run_execution(backend, &run)?;
    let (max_parallel, _) = resolve_workflow_max_parallel(&wf)?;
    let repo_workdir = resolved_workflow_repo_dir(&wf)?;
    let step_lookup: Arc<HashMap<String, WorkflowStep>> = Arc::new(
        wf.steps
            .iter()
            .cloned()
            .map(|step| (step.id.clone(), step))
            .collect(),
    );
    let engine_steps: Vec<run_persistence::WorkflowEngineStep> = wf
        .steps
        .iter()
        .filter(|step| pending_step_ids.contains(step.id.as_str()))
        .map(|step| run_persistence::WorkflowEngineStep {
            id: step.id.clone(),
            depends_on: step
                .depends_on
                .iter()
                .filter(|dep| pending_step_ids.contains(dep.as_str()))
                .cloned()
                .collect(),
        })
        .collect();
    let initial_outputs: HashMap<String, HashMap<String, String>> = run
        .steps
        .iter()
        .filter(|step| !step.outputs.is_empty())
        .map(|step| (step.step_id.clone(), step.outputs.clone()))
        .collect();
    let step_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>> =
        Arc::new(Mutex::new(initial_outputs));
    let status_lock = Arc::new(Mutex::new(()));
    let run_id = run.id.clone();

    let engine_result =
        run_persistence::execute_parallel_workflow(&engine_steps, max_parallel, |step_id| {
            let Some(step) = step_lookup.get(step_id) else {
                return Err(format!("step {:?} missing from lookup", step_id));
            };
            {
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run_id,
                    step_id,
                    run_persistence::WorkflowStepStatus::Running,
                )?;
            }
            let fail_step =
                |message: String| -> Result<run_persistence::WorkflowEngineStepResult, String> {
                    if let Ok(_guard) = status_lock.lock() {
                        let _ = store.update_step_status(
                            &run_id,
                            step_id,
                            run_persistence::WorkflowStepStatus::Failed,
                        );
                    }
                    let _ = store.append_step_log(
                        &run_id,
                        step_id,
                        format!("error: {message}").as_str(),
                    );
                    Err(message)
                };

            let step_outputs_snapshot = step_outputs
                .lock()
                .map_err(|_| "workflow step output lock poisoned".to_string())?
                .clone();
            let resolved_inputs = match resolve_step_inputs(step, &step_outputs_snapshot) {
                Ok(value) => value,
                Err(err) => return fail_step(format!("resolve inputs for step {step_id}: {err}")),
            };

            match step.step_type.as_str() {
                STEP_TYPE_BASH => {
                    let resolved_cmd = match render_binding_template(
                        &step.cmd,
                        &step_outputs_snapshot,
                        Some(&resolved_inputs),
                    ) {
                        Ok(value) => value,
                        Err(err) => {
                            return fail_step(format!("resolve cmd for step {step_id}: {err}"));
                        }
                    };
                    let request = run_persistence::BashStepRequest::new(
                        step_id,
                        resolved_cmd,
                        repo_workdir.clone(),
                        &step.workdir,
                    )
                    .with_extra_env(bash_input_env(&resolved_inputs));
                    let result = run_persistence::execute_bash_step(&request)?;
                    run_persistence::append_bash_step_logs(store, &run_id, &result)?;
                    if !result.success {
                        return fail_step(format!("exit status {}", result.exit_code));
                    }

                    let outputs = build_step_outputs(
                        step_id,
                        step,
                        &result,
                        &resolved_inputs,
                        &step_outputs_snapshot,
                    )
                    .map_err(|err| format!("resolve outputs for step {step_id}: {err}"))?;
                    let persisted_outputs = outputs.clone();
                    step_outputs
                        .lock()
                        .map_err(|_| "workflow step output lock poisoned".to_string())?
                        .insert(step_id.to_string(), outputs);
                    {
                        let _guard = status_lock
                            .lock()
                            .map_err(|_| "workflow status lock poisoned".to_string())?;
                        store.update_step_outputs(&run_id, step_id, persisted_outputs)?;
                        store.update_step_status(
                            &run_id,
                            step_id,
                            run_persistence::WorkflowStepStatus::Success,
                        )?;
                    }
                    Ok(run_persistence::WorkflowEngineStepResult::Success)
                }
                STEP_TYPE_HUMAN => {
                    let timeout = resolve_human_timeout(&step.timeout)
                        .map_err(|err| format!("resolve human timeout: {err}"))?;
                    let timeout_at = human_timeout_deadline(timeout);
                    {
                        let _guard = status_lock
                            .lock()
                            .map_err(|_| "workflow status lock poisoned".to_string())?;
                        store.mark_step_waiting_approval(&run_id, step_id, timeout_at.clone())?;
                    }
                    let timeout_hint = if step.timeout.trim().is_empty() {
                        format!("default({DEFAULT_HUMAN_TIMEOUT_LABEL})")
                    } else {
                        step.timeout.trim().to_string()
                    };
                    let timeout_line = timeout_at
                        .as_deref()
                        .map(|value| format!(" timeout_at={value}"))
                        .unwrap_or_default();
                    store.append_step_log(
                        &run_id,
                        step_id,
                        format!("awaiting human approval timeout={timeout_hint}{timeout_line}")
                            .as_str(),
                    )?;
                    Ok(run_persistence::WorkflowEngineStepResult::WaitingApproval)
                }
                other => fail_step(format!(
                    "workflow resume currently supports bash/human steps only; got step type {:?}",
                    other
                )),
            }
        })?;

    let mut failed = false;
    let mut waiting_approval = false;
    for record in &engine_result.steps {
        match record.status {
            run_persistence::WorkflowEngineStepStatus::Skipped => {
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run.id,
                    &record.step_id,
                    run_persistence::WorkflowStepStatus::Skipped,
                )?;
            }
            run_persistence::WorkflowEngineStepStatus::Failed => {
                failed = true;
            }
            run_persistence::WorkflowEngineStepStatus::WaitingApproval => {
                waiting_approval = true;
                let _guard = status_lock
                    .lock()
                    .map_err(|_| "workflow status lock poisoned".to_string())?;
                store.update_step_status(
                    &run.id,
                    &record.step_id,
                    run_persistence::WorkflowStepStatus::WaitingApproval,
                )?;
            }
            run_persistence::WorkflowEngineStepStatus::Pending
            | run_persistence::WorkflowEngineStepStatus::Running
            | run_persistence::WorkflowEngineStepStatus::Success => {}
        }
        if !record.error.is_empty() {
            store.append_step_log(
                &run.id,
                &record.step_id,
                format!("error: {}", record.error).as_str(),
            )?;
        }
    }

    run = store.update_run_status(
        &run.id,
        if failed {
            run_persistence::WorkflowRunStatus::Failed
        } else if waiting_approval {
            run_persistence::WorkflowRunStatus::Running
        } else {
            run_persistence::WorkflowRunStatus::Success
        },
    )?;
    append_workflow_run_ledger(
        &wf,
        &run,
        &engine_result,
        step_lookup.as_ref(),
        &repo_workdir,
    )?;

    Ok(run)
}

fn execute_approve(
    backend: &dyn WorkflowBackend,
    run_id: &str,
    step_id: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
    let _ = store.decide_step_approval(run_id, step_id, true, None)?;
    let run = resume_workflow_after_approval(backend, &store, run_id)?;
    let remaining_steps = run
        .steps
        .iter()
        .filter(|step| !is_terminal_step_status(&step.status))
        .map(|step| step.step_id.clone())
        .collect::<Vec<String>>();
    let result = WorkflowApprovalResult {
        run_id: run.id,
        step_id: step_id.trim().to_string(),
        decision: "approved".to_string(),
        run_status: run_status_label(&run.status).to_string(),
        remaining_steps,
    };

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    writeln!(stdout, "Workflow run: {}", result.run_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Step: {}", result.step_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Decision: {}", result.decision).map_err(|e| e.to_string())?;
    writeln!(stdout, "Run status: {}", result.run_status).map_err(|e| e.to_string())?;
    if !result.remaining_steps.is_empty() {
        writeln!(
            stdout,
            "Remaining steps: {}",
            result.remaining_steps.join(", ")
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn execute_deny(
    run_id: &str,
    step_id: &str,
    reason: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
    let run = store.decide_step_approval(run_id, step_id, false, Some(reason))?;
    let result = WorkflowApprovalResult {
        run_id: run.id,
        step_id: step_id.trim().to_string(),
        decision: "denied".to_string(),
        run_status: run_status_label(&run.status).to_string(),
        remaining_steps: run
            .steps
            .iter()
            .filter(|step| !is_terminal_step_status(&step.status))
            .map(|step| step.step_id.clone())
            .collect(),
    };

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    writeln!(stdout, "Workflow run: {}", result.run_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Step: {}", result.step_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Decision: {}", result.decision).map_err(|e| e.to_string())?;
    writeln!(stdout, "Reason: {}", reason.trim()).map_err(|e| e.to_string())?;
    writeln!(stdout, "Run status: {}", result.run_status).map_err(|e| e.to_string())?;
    Ok(())
}

fn execute_blocked(
    backend: &dyn WorkflowBackend,
    run_id: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
    let run = store.get_run(run_id)?;
    let workflow = backend.load_workflow_by_name(&run.workflow_name)?;
    let step_statuses = run
        .steps
        .iter()
        .map(|step| (step.step_id.as_str(), &step.status))
        .collect::<HashMap<&str, &run_persistence::WorkflowStepStatus>>();

    let blocked_steps = workflow
        .steps
        .iter()
        .filter_map(|step| {
            let status = step_statuses.get(step.id.as_str())?;
            let mut reasons = Vec::new();

            if matches!(
                **status,
                run_persistence::WorkflowStepStatus::WaitingApproval
            ) {
                reasons.push("awaiting human approval".to_string());
            }
            if matches!(**status, run_persistence::WorkflowStepStatus::Pending) {
                for dep in &step.depends_on {
                    let Some(dep_status) = step_statuses.get(dep.as_str()) else {
                        reasons.push(format!("missing dependency status: {dep}"));
                        continue;
                    };
                    if !matches!(**dep_status, run_persistence::WorkflowStepStatus::Success) {
                        reasons.push(format!(
                            "blocked by {} ({})",
                            dep,
                            step_status_label(dep_status)
                        ));
                    }
                }
            }
            if reasons.is_empty() {
                return None;
            }
            Some(WorkflowBlockedStepResult {
                step_id: step.id.clone(),
                status: step_status_label(status).to_string(),
                reasons,
            })
        })
        .collect::<Vec<WorkflowBlockedStepResult>>();
    let result = WorkflowBlockedResult {
        run_id: run.id,
        workflow_name: run.workflow_name,
        blocked_steps,
    };

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }
    if result.blocked_steps.is_empty() {
        writeln!(stdout, "No blocked steps for run {}", result.run_id)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "Workflow run: {}", result.run_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Workflow: {}", result.workflow_name).map_err(|e| e.to_string())?;
    writeln!(stdout, "Blocked steps:").map_err(|e| e.to_string())?;
    for step in &result.blocked_steps {
        writeln!(stdout, "- {} ({})", step.step_id, step.status).map_err(|e| e.to_string())?;
        for reason in &step.reasons {
            writeln!(stdout, "  - {}", reason).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn execute_logs(
    backend: &dyn WorkflowBackend,
    run_id: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
    let result = load_workflow_logs_result(backend, &store, run_id)?;

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    write_workflow_logs_human(stdout, &result)
}

fn load_workflow_logs_result(
    backend: &dyn WorkflowBackend,
    store: &run_persistence::WorkflowRunStore,
    run_id: &str,
) -> Result<WorkflowLogsResult, String> {
    let run = store.get_run(run_id)?;
    let run_workflow = load_run_workflow_for_fan_out(backend, &run);
    let fan_out_counts = fan_out_counts_for_run(&run, run_workflow.as_ref());
    let mut steps = Vec::with_capacity(run.steps.len());
    for step in &run.steps {
        let fan_out = fan_out_counts
            .get(step.step_id.as_str())
            .copied()
            .unwrap_or_default();
        let log = store.read_step_log(&run.id, &step.step_id)?;
        steps.push(WorkflowStepLogsResult {
            step_id: step.step_id.clone(),
            status: step_status_label(&step.status).to_string(),
            started_at: step.started_at.clone(),
            finished_at: step.finished_at.clone(),
            fan_out_running: fan_out.running,
            fan_out_queued: fan_out.queued,
            log,
        });
    }

    Ok(WorkflowLogsResult {
        run_id: run.id,
        workflow_name: run.workflow_name,
        workflow_source: run.workflow_source,
        status: run_status_label(&run.status).to_string(),
        started_at: run.started_at,
        finished_at: run.finished_at,
        steps,
    })
}

fn load_run_workflow_for_fan_out(
    backend: &dyn WorkflowBackend,
    run: &run_persistence::WorkflowRunRecord,
) -> Option<Workflow> {
    if !run.workflow_source.trim().is_empty() {
        if let Ok(raw) = fs::read_to_string(&run.workflow_source) {
            if let Ok(workflow) = parse_workflow_toml(&raw, &run.workflow_source) {
                return Some(workflow);
            }
        }
    }

    backend.load_workflow_by_name(&run.workflow_name).ok()
}

fn step_direct_dependents(steps: &[WorkflowStep]) -> HashMap<String, Vec<String>> {
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for step in steps {
        if !step.id.is_empty() {
            dependents.entry(step.id.clone()).or_default();
        }
    }

    for step in steps {
        if step.id.is_empty() {
            continue;
        }
        for dep in &step.depends_on {
            if dep.is_empty() {
                continue;
            }
            dependents
                .entry(dep.clone())
                .or_default()
                .push(step.id.clone());
        }
    }

    dependents
}

fn fan_out_counts_for_run(
    run: &run_persistence::WorkflowRunRecord,
    workflow: Option<&Workflow>,
) -> HashMap<String, StepFanOutCounts> {
    let mut by_step: HashMap<String, StepFanOutCounts> = HashMap::new();
    let status_by_id: HashMap<&str, &run_persistence::WorkflowStepStatus> = run
        .steps
        .iter()
        .map(|step| (step.step_id.as_str(), &step.status))
        .collect();
    let dependents = workflow
        .map(|wf| step_direct_dependents(&wf.steps))
        .unwrap_or_default();

    for step in &run.steps {
        let mut counts = StepFanOutCounts::default();
        if let Some(children) = dependents.get(&step.step_id) {
            for child in children {
                if let Some(status) = status_by_id.get(child.as_str()) {
                    match **status {
                        run_persistence::WorkflowStepStatus::Running => {
                            counts.running += 1;
                        }
                        run_persistence::WorkflowStepStatus::Pending => {
                            counts.queued += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        by_step.insert(step.step_id.clone(), counts);
    }

    by_step
}

fn static_fan_out_counts(steps: &[WorkflowStep]) -> HashMap<String, StepFanOutCounts> {
    let mut by_step: HashMap<String, StepFanOutCounts> = HashMap::new();
    let dependents = step_direct_dependents(steps);

    for step in steps {
        if step.id.is_empty() {
            continue;
        }
        let queued = dependents.get(&step.id).map_or(0, Vec::len);
        by_step.insert(step.id.clone(), StepFanOutCounts { running: 0, queued });
    }

    by_step
}

fn write_workflow_logs_human(
    stdout: &mut dyn Write,
    result: &WorkflowLogsResult,
) -> Result<(), String> {
    writeln!(stdout, "Workflow run: {}", result.run_id).map_err(|e| e.to_string())?;
    writeln!(stdout, "Workflow: {}", result.workflow_name).map_err(|e| e.to_string())?;
    if !result.workflow_source.is_empty() {
        writeln!(stdout, "Source: {}", result.workflow_source).map_err(|e| e.to_string())?;
    }
    writeln!(stdout, "Status: {}", result.status).map_err(|e| e.to_string())?;
    writeln!(stdout, "Started: {}", result.started_at).map_err(|e| e.to_string())?;
    if let Some(finished_at) = &result.finished_at {
        writeln!(stdout, "Finished: {}", finished_at).map_err(|e| e.to_string())?;
    }

    writeln!(stdout, "\nStep logs:").map_err(|e| e.to_string())?;
    for (index, step) in result.steps.iter().enumerate() {
        writeln!(
            stdout,
            "  {}. {} [{}] fan_out: running={} queued={}",
            index + 1,
            step.step_id,
            step.status,
            step.fan_out_running,
            step.fan_out_queued
        )
        .map_err(|e| e.to_string())?;

        if step.log.trim().is_empty() {
            writeln!(stdout, "     (no log output)").map_err(|e| e.to_string())?;
            continue;
        }

        for line in step.log.lines() {
            writeln!(stdout, "     {line}").map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn run_status_label(status: &run_persistence::WorkflowRunStatus) -> &'static str {
    match status {
        run_persistence::WorkflowRunStatus::Running => "running",
        run_persistence::WorkflowRunStatus::Success => "success",
        run_persistence::WorkflowRunStatus::Failed => "failed",
        run_persistence::WorkflowRunStatus::Canceled => "canceled",
    }
}

fn step_status_label(status: &run_persistence::WorkflowStepStatus) -> &'static str {
    match status {
        run_persistence::WorkflowStepStatus::Pending => "pending",
        run_persistence::WorkflowStepStatus::Running => "running",
        run_persistence::WorkflowStepStatus::WaitingApproval => "waiting_approval",
        run_persistence::WorkflowStepStatus::Success => "success",
        run_persistence::WorkflowStepStatus::Failed => "failed",
        run_persistence::WorkflowStepStatus::Skipped => "skipped",
        run_persistence::WorkflowStepStatus::Canceled => "canceled",
    }
}

fn is_terminal_step_status(status: &run_persistence::WorkflowStepStatus) -> bool {
    matches!(
        status,
        run_persistence::WorkflowStepStatus::Success
            | run_persistence::WorkflowStepStatus::Failed
            | run_persistence::WorkflowStepStatus::Skipped
            | run_persistence::WorkflowStepStatus::Canceled
    )
}

// ---------------------------------------------------------------------------
// Normalization (Go parity: normalize.go)
// ---------------------------------------------------------------------------

fn normalize_workflow(wf: &mut Workflow) {
    wf.name = wf.name.trim().to_string();
    wf.version = wf.version.trim().to_string();
    wf.description = wf.description.trim().to_string();

    for step in &mut wf.steps {
        step.id = step.id.trim().to_string();
        step.name = step.name.trim().to_string();
        step.step_type = step.step_type.trim().to_ascii_lowercase();
        step.depends_on = normalize_string_slice(&step.depends_on);
        step.alive_with = normalize_string_slice(&step.alive_with);
        step.then_targets = normalize_string_slice(&step.then_targets);
        step.else_targets = normalize_string_slice(&step.else_targets);
        step.when = step.when.trim().to_string();
        step.prompt = step.prompt.trim().to_string();
        step.prompt_path = step.prompt_path.trim().to_string();
        step.prompt_name = step.prompt_name.trim().to_string();
        step.prompt_id = step.prompt_id.trim().to_string();
        step.profile = step.profile.trim().to_string();
        step.agent_id = step.agent_id.trim().to_string();
        step.pool = step.pool.trim().to_string();
        step.max_runtime = step.max_runtime.trim().to_string();
        step.interval = step.interval.trim().to_string();
        step.cmd = step.cmd.trim().to_string();
        step.workdir = step.workdir.trim().to_string();
        step.if_cond = step.if_cond.trim().to_string();
        step.job_name = step.job_name.trim().to_string();
        step.workflow_name = step.workflow_name.trim().to_string();
        step.timeout = step.timeout.trim().to_string();

        if let Some(stop) = &mut step.stop {
            stop.expr = stop.expr.trim().to_string();
            if let Some(tool) = &mut stop.tool {
                tool.name = tool.name.trim().to_string();
            }
            if let Some(llm) = &mut stop.llm {
                llm.rubric = llm.rubric.trim().to_string();
                llm.pass_if = llm.pass_if.trim().to_string();
            }
        }

        if let Some(hooks) = &mut step.hooks {
            hooks.pre = normalize_string_slice(&hooks.pre);
            hooks.post = normalize_string_slice(&hooks.post);
        }
    }

    if let Some(hooks) = &mut wf.hooks {
        hooks.pre = normalize_string_slice(&hooks.pre);
        hooks.post = normalize_string_slice(&hooks.post);
    }
}

fn normalize_string_slice(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Validation (Go parity: validate.go)
// ---------------------------------------------------------------------------

fn validate_workflow(wf: &Workflow) -> (Workflow, Vec<WorkflowError>) {
    let mut wf = wf.clone();
    normalize_workflow(&mut wf);
    let path = wf.source.clone();
    let mut errors = Vec::new();
    let repo_root = repo_root_from_workflow(&wf);
    let default_prompt_path = detect_default_prompt_path(&repo_root);

    if wf.name.is_empty() {
        errors.push(WorkflowError {
            code: ERR_MISSING_FIELD.to_string(),
            message: "name is required".to_string(),
            path: path.clone(),
            field: "name".to_string(),
            ..default_error()
        });
    }

    if wf.steps.is_empty() {
        errors.push(WorkflowError {
            code: ERR_MISSING_FIELD.to_string(),
            message: "steps are required".to_string(),
            path: path.clone(),
            field: "steps".to_string(),
            ..default_error()
        });
    }

    if wf.max_parallel < 0 {
        errors.push(WorkflowError {
            code: ERR_INVALID_FIELD.to_string(),
            message: "max_parallel must be >= 0".to_string(),
            path: path.clone(),
            field: "max_parallel".to_string(),
            ..default_error()
        });
    }

    let types = valid_step_types();
    let mut step_index: HashMap<String, usize> = HashMap::new();

    for (i, step) in wf.steps.iter_mut().enumerate() {
        let index = i + 1;
        if step.prompt_name.is_empty() && !step.prompt_id.is_empty() {
            step.prompt_name = normalize_prompt_registry_name(&step.prompt_id);
        }
        if step.profile.is_empty() && !step.agent_id.is_empty() {
            step.profile = step.agent_id.trim().to_string();
        }
        if step.prompt.is_empty()
            && step.prompt_path.is_empty()
            && step.prompt_name.is_empty()
            && step.prompt_id.is_empty()
            && matches!(
                step.step_type.as_str(),
                STEP_TYPE_AGENT | STEP_TYPE_LOOP | STEP_TYPE_HUMAN
            )
        {
            if let Some(prompt_path) = default_prompt_path.as_deref() {
                step.prompt_path = prompt_path.to_string();
            }
        }

        if step.id.is_empty() {
            errors.push(WorkflowError {
                code: ERR_MISSING_FIELD.to_string(),
                message: "step id is required".to_string(),
                path: path.clone(),
                field: "steps.id".to_string(),
                index,
                ..default_error()
            });
        } else if let Some(&prev) = step_index.get(&step.id) {
            errors.push(WorkflowError {
                code: ERR_DUPLICATE_STEP.to_string(),
                message: format!("duplicate step id {:?} (also in step {})", step.id, prev),
                path: path.clone(),
                step_id: step.id.clone(),
                field: "steps.id".to_string(),
                index,
                ..default_error()
            });
        } else {
            step_index.insert(step.id.clone(), index);
        }

        if step.step_type.is_empty() {
            errors.push(WorkflowError {
                code: ERR_MISSING_FIELD.to_string(),
                message: "step type is required".to_string(),
                path: path.clone(),
                step_id: step.id.clone(),
                field: "steps.type".to_string(),
                index,
                ..default_error()
            });
        } else if !types.contains(step.step_type.as_str()) {
            errors.push(WorkflowError {
                code: ERR_UNKNOWN_TYPE.to_string(),
                message: format!("unknown step type {:?}", step.step_type),
                path: path.clone(),
                step_id: step.id.clone(),
                field: "steps.type".to_string(),
                index,
                ..default_error()
            });
        }

        validate_step_specific_fields(step, index, &path, &repo_root, &mut errors);
        validate_stop_condition(step, index, &path, &mut errors);
        validate_dependencies(step, index, &path, &mut errors);
    }

    validate_dependency_targets(&wf, &step_index, &mut errors);
    validate_logic_targets(&wf, &step_index, &mut errors);
    validate_cycles(&wf, &step_index, &mut errors);

    (wf, errors)
}

fn profile_exists_by_id(profile_id: &str) -> Result<bool, String> {
    let trimmed = profile_id.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    let cfg = forge_db::Config::new(crate::runtime_paths::resolve_database_path());
    let db = forge_db::Db::open(cfg).map_err(|err| format!("open database: {err}"))?;
    let repo = forge_db::profile_repository::ProfileRepository::new(&db);
    match repo.get(trimmed) {
        Ok(_) => Ok(true),
        Err(forge_db::DbError::ProfileNotFound) => Ok(false),
        Err(err) => {
            let message = err.to_string();
            if message.contains("no such table: profiles") {
                Ok(false)
            } else {
                Err(message)
            }
        }
    }
}

fn validate_step_specific_fields(
    step: &WorkflowStep,
    index: usize,
    path: &str,
    repo_root: &str,
    errors: &mut Vec<WorkflowError>,
) {
    match step.step_type.as_str() {
        STEP_TYPE_AGENT | STEP_TYPE_LOOP | STEP_TYPE_HUMAN => {
            if !step.prompt_id.is_empty() && !step.prompt_name.is_empty() {
                let prompt_id = normalize_prompt_registry_name(&step.prompt_id);
                let prompt_name = normalize_prompt_registry_name(&step.prompt_name);
                if !prompt_id.is_empty() && !prompt_name.is_empty() && prompt_id != prompt_name {
                    errors.push(WorkflowError {
                        code: ERR_INVALID_FIELD.to_string(),
                        message: "prompt_id and prompt_name must reference the same prompt"
                            .to_string(),
                        path: path.to_string(),
                        step_id: step.id.clone(),
                        field: "prompt_id".to_string(),
                        index,
                        ..default_error()
                    });
                }
            }
            if step.prompt.is_empty()
                && step.prompt_path.is_empty()
                && step.prompt_name.is_empty()
                && step.prompt_id.is_empty()
            {
                errors.push(WorkflowError {
                    code: ERR_MISSING_FIELD.to_string(),
                    message: "prompt, prompt_path, prompt_name, or prompt_id is required"
                        .to_string(),
                    path: path.to_string(),
                    step_id: step.id.clone(),
                    field: "prompt".to_string(),
                    index,
                    ..default_error()
                });
            }
            if !step.prompt_id.is_empty() && !repo_root.trim().is_empty() {
                let prompt_path = prompt_registry_path(repo_root, &step.prompt_id);
                if !prompt_path.is_file() {
                    errors.push(WorkflowError {
                        code: ERR_INVALID_FIELD.to_string(),
                        message: format!("prompt id {:?} not found", step.prompt_id),
                        path: path.to_string(),
                        step_id: step.id.clone(),
                        field: "prompt_id".to_string(),
                        index,
                        ..default_error()
                    });
                }
            }
            if !step.agent_id.is_empty()
                && !step.profile.is_empty()
                && step.agent_id != step.profile
            {
                errors.push(WorkflowError {
                    code: ERR_INVALID_FIELD.to_string(),
                    message: "agent_id and profile must reference the same profile".to_string(),
                    path: path.to_string(),
                    step_id: step.id.clone(),
                    field: "agent_id".to_string(),
                    index,
                    ..default_error()
                });
            }
            if !step.agent_id.is_empty() {
                match profile_exists_by_id(&step.agent_id) {
                    Ok(true) => {}
                    Ok(false) => errors.push(WorkflowError {
                        code: ERR_INVALID_FIELD.to_string(),
                        message: format!("agent id {:?} not found", step.agent_id),
                        path: path.to_string(),
                        step_id: step.id.clone(),
                        field: "agent_id".to_string(),
                        index,
                        ..default_error()
                    }),
                    Err(err) => errors.push(WorkflowError {
                        code: ERR_INVALID_FIELD.to_string(),
                        message: format!("agent id lookup failed: {err}"),
                        path: path.to_string(),
                        step_id: step.id.clone(),
                        field: "agent_id".to_string(),
                        index,
                        ..default_error()
                    }),
                }
            }
            if step.step_type == STEP_TYPE_HUMAN && !step.timeout.is_empty() {
                if let Err(err) = resolve_human_timeout(&step.timeout) {
                    errors.push(WorkflowError {
                        code: ERR_INVALID_FIELD.to_string(),
                        message: format!("invalid timeout: {err}"),
                        path: path.to_string(),
                        step_id: step.id.clone(),
                        field: "timeout".to_string(),
                        index,
                        ..default_error()
                    });
                }
            }
        }
        STEP_TYPE_BASH => {
            if step.cmd.is_empty() {
                errors.push(missing_field_error(path, &step.id, index, "cmd"));
            }
        }
        STEP_TYPE_LOGIC => {
            if step.if_cond.is_empty() {
                errors.push(missing_field_error(path, &step.id, index, "if"));
            }
            if step.then_targets.is_empty() && step.else_targets.is_empty() {
                errors.push(WorkflowError {
                    code: ERR_MISSING_FIELD.to_string(),
                    message: "logic step must define then or else targets".to_string(),
                    path: path.to_string(),
                    step_id: step.id.clone(),
                    field: "then".to_string(),
                    index,
                    ..default_error()
                });
            }
        }
        STEP_TYPE_JOB => {
            if step.job_name.is_empty() {
                errors.push(missing_field_error(path, &step.id, index, "job_name"));
            }
        }
        STEP_TYPE_WORKFLOW => {
            if step.workflow_name.is_empty() {
                errors.push(missing_field_error(path, &step.id, index, "workflow_name"));
            }
        }
        _ => {}
    }
}

fn validate_stop_condition(
    step: &WorkflowStep,
    index: usize,
    path: &str,
    errors: &mut Vec<WorkflowError>,
) {
    let stop = match &step.stop {
        Some(s) => s,
        None => return,
    };

    if stop.expr.is_empty() && stop.tool.is_none() && stop.llm.is_none() {
        errors.push(WorkflowError {
            code: ERR_MISSING_FIELD.to_string(),
            message: "stop condition requires expr, tool, or llm".to_string(),
            path: path.to_string(),
            step_id: step.id.clone(),
            field: "stop".to_string(),
            index,
            ..default_error()
        });
    }

    if let Some(tool) = &stop.tool {
        if tool.name.is_empty() {
            errors.push(missing_field_error(path, &step.id, index, "stop.tool.name"));
        }
    }

    if let Some(llm) = &stop.llm {
        if llm.rubric.is_empty() && llm.pass_if.is_empty() {
            errors.push(WorkflowError {
                code: ERR_MISSING_FIELD.to_string(),
                message: "stop.llm requires rubric or pass_if".to_string(),
                path: path.to_string(),
                step_id: step.id.clone(),
                field: "stop.llm".to_string(),
                index,
                ..default_error()
            });
        }
    }
}

fn validate_dependencies(
    step: &WorkflowStep,
    index: usize,
    path: &str,
    errors: &mut Vec<WorkflowError>,
) {
    let mut seen: HashSet<String> = HashSet::new();
    for dep in &step.depends_on {
        if dep.is_empty() {
            errors.push(WorkflowError {
                code: ERR_INVALID_FIELD.to_string(),
                message: "depends_on entries must be non-empty".to_string(),
                path: path.to_string(),
                step_id: step.id.clone(),
                field: "depends_on".to_string(),
                index,
                ..default_error()
            });
            continue;
        }
        if dep == &step.id && !step.id.is_empty() {
            errors.push(WorkflowError {
                code: ERR_INVALID_FIELD.to_string(),
                message: "step cannot depend on itself".to_string(),
                path: path.to_string(),
                step_id: step.id.clone(),
                field: "depends_on".to_string(),
                index,
                ..default_error()
            });
        }
        if seen.contains(dep) {
            errors.push(WorkflowError {
                code: ERR_INVALID_FIELD.to_string(),
                message: format!("duplicate dependency {:?}", dep),
                path: path.to_string(),
                step_id: step.id.clone(),
                field: "depends_on".to_string(),
                index,
                ..default_error()
            });
            continue;
        }
        seen.insert(dep.clone());
    }
}

fn validate_dependency_targets(
    wf: &Workflow,
    step_index: &HashMap<String, usize>,
    errors: &mut Vec<WorkflowError>,
) {
    for (i, step) in wf.steps.iter().enumerate() {
        let index = i + 1;
        for dep in &step.depends_on {
            if dep.is_empty() {
                continue;
            }
            if !step_index.contains_key(dep) {
                errors.push(WorkflowError {
                    code: ERR_MISSING_STEP.to_string(),
                    message: format!("unknown dependency {:?}", dep),
                    path: wf.source.clone(),
                    step_id: step.id.clone(),
                    field: "depends_on".to_string(),
                    index,
                    ..default_error()
                });
            }
        }
    }
}

fn validate_logic_targets(
    wf: &Workflow,
    step_index: &HashMap<String, usize>,
    errors: &mut Vec<WorkflowError>,
) {
    for (i, step) in wf.steps.iter().enumerate() {
        if step.step_type != STEP_TYPE_LOGIC {
            continue;
        }
        let index = i + 1;
        let targets: Vec<&String> = step
            .then_targets
            .iter()
            .chain(step.else_targets.iter())
            .collect();
        for target in targets {
            if target.is_empty() {
                continue;
            }
            if !step_index.contains_key(target.as_str()) {
                errors.push(WorkflowError {
                    code: ERR_MISSING_STEP.to_string(),
                    message: format!("unknown logic target {:?}", target),
                    path: wf.source.clone(),
                    step_id: step.id.clone(),
                    field: "then".to_string(),
                    index,
                    ..default_error()
                });
            }
        }
    }
}

fn validate_cycles(
    wf: &Workflow,
    step_index: &HashMap<String, usize>,
    errors: &mut Vec<WorkflowError>,
) {
    if step_index.is_empty() {
        return;
    }

    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();

    for id in step_index.keys() {
        in_degree.insert(id.clone(), 0);
    }

    for step in &wf.steps {
        if step.id.is_empty() {
            continue;
        }
        for dep in &step.depends_on {
            if !in_degree.contains_key(dep) {
                continue;
            }
            adj.entry(dep.clone()).or_default().push(step.id.clone());
            *in_degree.entry(step.id.clone()).or_insert(0) += 1;
        }
    }

    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();
    queue.sort(); // deterministic

    let mut processed = 0;
    while !queue.is_empty() {
        let id = queue.remove(0);
        processed += 1;
        if let Some(neighbors) = adj.get(&id) {
            for next in neighbors {
                if let Some(deg) = in_degree.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(next.clone());
                    }
                }
            }
        }
    }

    if processed == in_degree.len() {
        return;
    }

    let mut cycle: Vec<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg > 0)
        .map(|(id, _)| id.clone())
        .collect();
    if cycle.is_empty() {
        return;
    }
    cycle.sort();

    errors.push(WorkflowError {
        code: ERR_CYCLE.to_string(),
        message: format!("cycle detected among steps: {}", cycle.join(", ")),
        path: wf.source.clone(),
        ..default_error()
    });
}

fn missing_field_error(path: &str, step_id: &str, index: usize, field: &str) -> WorkflowError {
    WorkflowError {
        code: ERR_MISSING_FIELD.to_string(),
        message: format!("{field} is required"),
        path: path.to_string(),
        step_id: step_id.to_string(),
        field: field.to_string(),
        index,
        ..default_error()
    }
}

fn default_error() -> WorkflowError {
    WorkflowError {
        code: String::new(),
        message: String::new(),
        path: String::new(),
        step_id: String::new(),
        field: String::new(),
        line: 0,
        column: 0,
        index: 0,
    }
}

// ---------------------------------------------------------------------------
// Prompt resolution (Go parity: resolve.go)
// ---------------------------------------------------------------------------

struct PromptResolution {
    inline: String,
    path: String,
}

fn normalize_prompt_registry_name(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .strip_suffix(".md")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

fn prompt_registry_path(repo_root: &str, prompt_ref: &str) -> PathBuf {
    let mut name = normalize_prompt_registry_name(prompt_ref);
    if name.is_empty() {
        name = prompt_ref.trim().to_string();
    }
    Path::new(repo_root)
        .join(".forge")
        .join("prompts")
        .join(format!("{name}.md"))
}

fn detect_default_prompt_path(repo_root: &str) -> Option<String> {
    if repo_root.trim().is_empty() {
        return None;
    }
    let default_path = Path::new(repo_root)
        .join(".forge")
        .join("prompts")
        .join("default.md");
    if default_path.is_file() {
        Some(default_path.to_string_lossy().to_string())
    } else {
        None
    }
}

fn resolve_step_prompt(wf: &Workflow, step: &WorkflowStep) -> PromptResolution {
    if !step.prompt.is_empty() {
        return PromptResolution {
            inline: step.prompt.clone(),
            path: String::new(),
        };
    }

    let root = repo_root_from_workflow(wf);

    if !step.prompt_path.is_empty() {
        let path = if !root.is_empty() && !Path::new(&step.prompt_path).is_absolute() {
            Path::new(&root)
                .join(&step.prompt_path)
                .to_string_lossy()
                .to_string()
        } else {
            step.prompt_path.clone()
        };
        return PromptResolution {
            inline: String::new(),
            path,
        };
    }

    let prompt_ref = if !step.prompt_id.is_empty() {
        step.prompt_id.trim()
    } else {
        step.prompt_name.trim()
    };
    if !prompt_ref.is_empty() {
        let mut name = normalize_prompt_registry_name(prompt_ref);
        if !name.ends_with(".md") {
            name.push_str(".md");
        }
        let path = if !root.is_empty() {
            prompt_registry_path(&root, &name)
                .to_string_lossy()
                .to_string()
        } else {
            name
        };
        return PromptResolution {
            inline: String::new(),
            path,
        };
    }

    if let Some(path) = detect_default_prompt_path(&root) {
        return PromptResolution {
            inline: String::new(),
            path,
        };
    }

    PromptResolution {
        inline: String::new(),
        path: String::new(),
    }
}

fn repo_root_from_workflow(wf: &Workflow) -> String {
    if wf.source.trim().is_empty() {
        return String::new();
    }
    repo_root_from_path(&wf.source)
}

fn repo_root_from_path(path: &str) -> String {
    let mut dir = Path::new(path).parent();
    while let Some(d) = dir {
        if d.file_name().is_some_and(|n| n == ".forge") {
            return d
                .parent()
                .map_or(String::new(), |p| p.to_string_lossy().to_string());
        }
        let next = d.parent();
        if next == Some(d) {
            break;
        }
        dir = next;
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Display helpers (Go parity: workflow.go print functions)
// ---------------------------------------------------------------------------

fn workflow_source_path(source: &str, project_dir: &Path) -> String {
    if source.is_empty() {
        return String::new();
    }
    let project_str = project_dir.to_string_lossy();
    if !project_str.is_empty() {
        if let Ok(rel) = Path::new(source).strip_prefix(project_dir) {
            let rel_str = rel.to_string_lossy().to_string();
            if !rel_str.starts_with("..") {
                return rel_str;
            }
        }
    }
    source.to_string()
}

fn print_workflow(wf: &Workflow, project_dir: &Path, stdout: &mut dyn Write) -> Result<(), String> {
    let (max_parallel, max_parallel_source) = resolve_workflow_max_parallel(wf)?;
    let fan_out_counts = static_fan_out_counts(&wf.steps);
    let blocked_reasons = workflow_blocked_reasons(wf);
    writeln!(stdout, "Workflow: {}", wf.name).map_err(|e| e.to_string())?;
    writeln!(
        stdout,
        "Source: {}",
        workflow_source_path(&wf.source, project_dir)
    )
    .map_err(|e| e.to_string())?;
    if !wf.version.is_empty() {
        writeln!(stdout, "Version: {}", wf.version).map_err(|e| e.to_string())?;
    }
    if !wf.description.is_empty() {
        writeln!(stdout, "Description: {}", wf.description).map_err(|e| e.to_string())?;
    }
    writeln!(
        stdout,
        "Max Parallel: {} ({})",
        max_parallel,
        workflow_parallel_source_label(max_parallel_source)
    )
    .map_err(|e| e.to_string())?;

    if !wf.inputs.is_empty() {
        writeln!(stdout, "Inputs: {}", format_workflow_map(&wf.inputs))
            .map_err(|e| e.to_string())?;
    }
    if !wf.outputs.is_empty() {
        writeln!(stdout, "Outputs: {}", format_workflow_map(&wf.outputs))
            .map_err(|e| e.to_string())?;
    }

    writeln!(stdout, "\nSteps:").map_err(|e| e.to_string())?;
    for (i, step) in wf.steps.iter().enumerate() {
        writeln!(stdout, "  {}. {}", i + 1, format_workflow_step(step))
            .map_err(|e| e.to_string())?;
        let fan_out = fan_out_counts.get(&step.id).copied().unwrap_or_default();
        let details =
            format_workflow_step_details(wf, step, fan_out, blocked_reasons.get(step.id.as_str()));
        for line in &details {
            writeln!(stdout, "     {line}").map_err(|e| e.to_string())?;
        }
    }

    let lines = workflow_flowchart_lines(wf);
    if !lines.is_empty() {
        writeln!(stdout, "\nFlow:").map_err(|e| e.to_string())?;
        for line in &lines {
            writeln!(stdout, "  {line}").map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn format_workflow_map(values: &BTreeMap<String, toml::Value>) -> String {
    if values.is_empty() {
        return "(none)".to_string();
    }
    // BTreeMap is already sorted by key
    let parts: Vec<String> = values
        .iter()
        .map(|(k, v)| format!("{k}={}", toml_value_display(v)))
        .collect();
    parts.join(", ")
}

fn toml_value_display(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn format_workflow_step(step: &WorkflowStep) -> String {
    let mut label = if step.id.is_empty() {
        "(unnamed)".to_string()
    } else {
        step.id.clone()
    };
    label = format!("{label} [{}]", step.step_type);
    if !step.name.is_empty() {
        label = format!("{label} - {}", step.name);
    }
    if !step.depends_on.is_empty() {
        label = format!("{label} (depends_on: {})", step.depends_on.join(", "));
    }
    label
}

fn format_workflow_step_details(
    wf: &Workflow,
    step: &WorkflowStep,
    fan_out: StepFanOutCounts,
    blocked_reasons: Option<&Vec<String>>,
) -> Vec<String> {
    let mut lines = Vec::new();
    match step.step_type.as_str() {
        STEP_TYPE_AGENT | STEP_TYPE_LOOP | STEP_TYPE_HUMAN => {
            let resolution = resolve_step_prompt(wf, step);
            if !resolution.inline.is_empty() {
                lines.push("prompt: [inline]".to_string());
            } else if !resolution.path.is_empty() {
                lines.push(format!("prompt: {}", resolution.path));
            }
            if step.step_type == STEP_TYPE_HUMAN {
                if step.timeout.is_empty() {
                    lines.push(format!("timeout: default({DEFAULT_HUMAN_TIMEOUT_LABEL})"));
                } else {
                    lines.push(format!("timeout: {}", step.timeout));
                }
            }
        }
        STEP_TYPE_BASH => {
            if !step.cmd.is_empty() {
                lines.push(format!("cmd: {}", step.cmd));
            }
        }
        _ => {}
    }
    if let Some(reasons) = blocked_reasons {
        lines.push("blocked: yes".to_string());
        for reason in reasons {
            lines.push(format!("reason: {}", reason));
        }
    }
    lines.push(format!(
        "fan_out: running={} queued={}",
        fan_out.running, fan_out.queued
    ));
    lines
}

fn workflow_blocked_reasons(wf: &Workflow) -> HashMap<String, Vec<String>> {
    let mut reasons = HashMap::<String, Vec<String>>::new();
    let by_id = wf
        .steps
        .iter()
        .filter(|step| !step.id.is_empty())
        .map(|step| (step.id.as_str(), step))
        .collect::<HashMap<_, _>>();

    for step in &wf.steps {
        if step.id.is_empty() {
            continue;
        }

        let mut step_reasons = Vec::new();
        if step.step_type == STEP_TYPE_HUMAN {
            step_reasons.push("awaiting human approval".to_string());
            if !step.timeout.is_empty() {
                step_reasons.push(format!("approval timeout: {}", step.timeout));
            }
            step_reasons.push(format!(
                "approve via: forge workflow approve <run-id> --step {}",
                step.id
            ));
        }

        for dependency in &step.depends_on {
            let Some(dep_step) = by_id.get(dependency.as_str()) else {
                continue;
            };
            if dep_step.step_type != STEP_TYPE_HUMAN {
                continue;
            }

            let mut reason = format!("blocked until {} is approved", dep_step.id);
            if !dep_step.timeout.is_empty() {
                reason.push_str(&format!(" (timeout {})", dep_step.timeout));
            }
            step_reasons.push(reason);
            step_reasons.push(format!(
                "approve via: forge workflow approve <run-id> --step {}",
                dep_step.id
            ));
        }

        if !step_reasons.is_empty() {
            reasons.insert(step.id.clone(), step_reasons);
        }
    }

    reasons
}

fn workflow_flowchart_lines(wf: &Workflow) -> Vec<String> {
    if wf.steps.is_empty() {
        return Vec::new();
    }

    let mut order: HashMap<String, usize> = HashMap::new();
    for (i, step) in wf.steps.iter().enumerate() {
        if !step.id.is_empty() {
            order.insert(step.id.clone(), i);
        }
    }

    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    let mut incoming: HashMap<String, usize> = HashMap::new();

    for step in &wf.steps {
        if step.id.is_empty() {
            continue;
        }
        incoming.entry(step.id.clone()).or_insert(0);
        for dep in &step.depends_on {
            if dep.is_empty() {
                continue;
            }
            outgoing
                .entry(dep.clone())
                .or_default()
                .push(step.id.clone());
            *incoming.entry(step.id.clone()).or_insert(0) += 1;
        }
    }

    let mut lines = Vec::new();
    for step in &wf.steps {
        let id = &step.id;
        if id.is_empty() {
            continue;
        }
        if let Some(targets) = outgoing.get(id) {
            if targets.is_empty() {
                continue;
            }
            let mut sorted_targets = targets.clone();
            sorted_targets.sort_by_key(|t| order.get(t).copied().unwrap_or(usize::MAX));
            lines.push(format!("{id} -> {}", sorted_targets.join(", ")));
        }
    }

    for step in &wf.steps {
        let id = &step.id;
        if id.is_empty() {
            continue;
        }
        let in_count = incoming.get(id).copied().unwrap_or(0);
        let out_count = outgoing.get(id).map_or(0, |v| v.len());
        if in_count == 0 && out_count == 0 {
            lines.push(id.clone());
        }
    }

    lines
}

// ---------------------------------------------------------------------------
// Workflow name normalization
// ---------------------------------------------------------------------------

fn normalize_workflow_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("workflow name is required".to_string());
    }
    let trimmed = trimmed.strip_suffix(".toml").unwrap_or(trimmed);
    if trimmed.contains(std::path::MAIN_SEPARATOR) || trimmed.contains("..") {
        return Err(format!("invalid workflow name {:?}", trimmed));
    }
    // Also reject forward slash on all platforms for safety
    if trimmed.contains('/') {
        return Err(format!("invalid workflow name {:?}", trimmed));
    }
    Ok(trimmed.to_string())
}

// ---------------------------------------------------------------------------
// TOML parsing
// ---------------------------------------------------------------------------

pub fn parse_workflow_toml(data: &str, source: &str) -> Result<Workflow, Vec<WorkflowError>> {
    match toml::from_str::<Workflow>(data) {
        Ok(mut wf) => {
            wf.source = source.to_string();
            Ok(wf)
        }
        Err(err) => {
            let (line, column) = err
                .span()
                .map(|span| {
                    // Count line/column from byte offset
                    let prefix = &data[..span.start.min(data.len())];
                    let line = prefix.chars().filter(|&c| c == '\n').count() + 1;
                    let last_newline = prefix.rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let col = span.start - last_newline + 1;
                    (line, col)
                })
                .unwrap_or((0, 0));
            Err(vec![WorkflowError {
                code: ERR_PARSE.to_string(),
                message: err.message().to_string(),
                path: source.to_string(),
                line,
                column,
                ..default_error()
            }])
        }
    }
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

    let start = if args.first().is_some_and(|a| a == "workflow" || a == "wf") {
        1
    } else {
        0
    };

    let mut json = false;
    let mut jsonl = false;
    let mut node: Option<String> = None;
    let mut step: Option<String> = None;
    let mut reason: Option<String> = None;
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
                // accepted but ignored for workflow
                idx += 1;
            }
            "--step" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "usage: --step <step-id>".to_string())?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("usage: --step <step-id>".to_string());
                }
                step = Some(value);
                idx += 2;
            }
            "--reason" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "usage: --reason <text>".to_string())?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("usage: --reason <text>".to_string());
                }
                reason = Some(value);
                idx += 2;
            }
            "--node" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "usage: --node <node-id>".to_string())?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("usage: --node <node-id>".to_string());
                }
                node = Some(value);
                idx += 2;
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
        Some("ls") | Some("list") => SubCommand::List,
        Some("show") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow show <name>".to_string())?
                .clone();
            SubCommand::Show { name }
        }
        Some("validate") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow validate <name>".to_string())?
                .clone();
            SubCommand::Validate { name }
        }
        Some("run") => {
            let name = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow run <name>".to_string())?
                .clone();
            SubCommand::Run {
                name,
                node: node.clone(),
            }
        }
        Some("logs") => {
            let run_id = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow logs <run-id>".to_string())?
                .clone();
            SubCommand::Logs { run_id }
        }
        Some("approve") => {
            let run_id = positionals
                .get(1)
                .ok_or_else(|| {
                    "usage: forge workflow approve <run-id> --step <step-id>".to_string()
                })?
                .clone();
            let step_id = step
                .clone()
                .or_else(|| positionals.get(2).cloned())
                .ok_or_else(|| {
                    "usage: forge workflow approve <run-id> --step <step-id>".to_string()
                })?;
            SubCommand::Approve { run_id, step_id }
        }
        Some("deny") => {
            let run_id = positionals
                .get(1)
                .ok_or_else(|| {
                    "usage: forge workflow deny <run-id> --step <step-id> --reason <text>"
                        .to_string()
                })?
                .clone();
            let step_id = step
                .clone()
                .or_else(|| positionals.get(2).cloned())
                .ok_or_else(|| {
                    "usage: forge workflow deny <run-id> --step <step-id> --reason <text>"
                        .to_string()
                })?;
            let reason = reason
                .clone()
                .or_else(|| positionals.get(3).cloned())
                .ok_or_else(|| {
                    "usage: forge workflow deny <run-id> --step <step-id> --reason <text>"
                        .to_string()
                })?;
            SubCommand::Deny {
                run_id,
                step_id,
                reason,
            }
        }
        Some("blocked") => {
            let run_id = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow blocked <run-id>".to_string())?
                .clone();
            SubCommand::Blocked { run_id }
        }
        Some(other) => return Err(format!("unknown workflow subcommand: {other}")),
    };

    if node.is_some() && !matches!(command, SubCommand::Run { .. }) {
        return Err("--node is only supported for workflow run".to_string());
    }

    Ok(ParsedArgs {
        command,
        json,
        jsonl,
    })
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
    writeln!(stdout, "List, inspect, and validate workflow definitions.")?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge workflow <command> [options]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Aliases:")?;
    writeln!(stdout, "  workflow, wf")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  ls          List workflows")?;
    writeln!(stdout, "  show        Show workflow details")?;
    writeln!(stdout, "  validate    Validate a workflow")?;
    writeln!(stdout, "  run         Execute a workflow")?;
    writeln!(
        stdout,
        "  logs        Show persisted logs for a workflow run"
    )?;
    writeln!(
        stdout,
        "  approve     Approve a waiting human step for a workflow run"
    )?;
    writeln!(
        stdout,
        "  deny        Deny a waiting human step for a workflow run"
    )?;
    writeln!(
        stdout,
        "  blocked     List blocked steps for a workflow run"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(stdout, "  -h, --help  help for workflow")?;
    writeln!(
        stdout,
        "  --node      route workflow run through a mesh node"
    )?;
    writeln!(stdout, "  --step      workflow step id (approve/deny)")?;
    writeln!(stdout, "  --reason    deny reason text (deny)")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;

    fn parse_json_or_panic(raw: &str, context: &str) -> serde_json::Value {
        match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(err) => panic!("{context}: {err}"),
        }
    }

    fn ok_or_panic<T, E>(result: Result<T, E>, context: &str) -> T
    where
        E: std::fmt::Debug,
    {
        match result {
            Ok(value) => value,
            Err(err) => panic!("{context}: {err:?}"),
        }
    }

    fn err_or_panic<T, E>(result: Result<T, E>, context: &str) -> E {
        match result {
            Ok(_) => panic!("{context}"),
            Err(err) => err,
        }
    }

    fn array_or_panic<'a>(
        value: &'a serde_json::Value,
        context: &str,
    ) -> &'a Vec<serde_json::Value> {
        match value.as_array() {
            Some(array) => array,
            None => panic!("{context}"),
        }
    }

    fn basic_workflow() -> Workflow {
        Workflow {
            name: "basic".to_string(),
            version: "0.1".to_string(),
            description: "Basic workflow".to_string(),
            inputs: {
                let mut m = BTreeMap::new();
                m.insert("repo".to_string(), toml::Value::String(".".to_string()));
                m
            },
            outputs: BTreeMap::new(),
            max_parallel: 0,
            steps: vec![
                WorkflowStep {
                    id: "plan".to_string(),
                    step_type: "agent".to_string(),
                    prompt_name: "plan".to_string(),
                    ..default_step()
                },
                WorkflowStep {
                    id: "build".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo build".to_string(),
                    depends_on: vec!["plan".to_string()],
                    ..default_step()
                },
            ],
            hooks: None,
            source: "/project/.forge/workflows/basic.toml".to_string(),
        }
    }

    fn multi_workflow() -> Workflow {
        Workflow {
            name: "multi".to_string(),
            version: "1.0".to_string(),
            description: "Multi-step workflow".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            max_parallel: 0,
            steps: vec![
                WorkflowStep {
                    id: "setup".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo setup".to_string(),
                    ..default_step()
                },
                WorkflowStep {
                    id: "agent-work".to_string(),
                    name: "Do work".to_string(),
                    step_type: "agent".to_string(),
                    prompt: "Write tests".to_string(),
                    depends_on: vec!["setup".to_string()],
                    ..default_step()
                },
                WorkflowStep {
                    id: "teardown".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo done".to_string(),
                    depends_on: vec!["agent-work".to_string()],
                    ..default_step()
                },
            ],
            hooks: None,
            source: "/project/.forge/workflows/multi.toml".to_string(),
        }
    }

    fn invalid_workflow() -> Workflow {
        Workflow {
            name: "bad-dep".to_string(),
            version: String::new(),
            description: String::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            max_parallel: 0,
            steps: vec![WorkflowStep {
                id: "build".to_string(),
                step_type: "bash".to_string(),
                cmd: "make".to_string(),
                depends_on: vec!["missing".to_string()],
                ..default_step()
            }],
            hooks: None,
            source: ".forge/workflows/bad-dep.toml".to_string(),
        }
    }

    fn default_step() -> WorkflowStep {
        WorkflowStep {
            id: String::new(),
            name: String::new(),
            step_type: String::new(),
            depends_on: Vec::new(),
            when: String::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            stop: None,
            hooks: None,
            alive_with: Vec::new(),
            prompt: String::new(),
            prompt_id: String::new(),
            prompt_path: String::new(),
            prompt_name: String::new(),
            agent_id: String::new(),
            profile: String::new(),
            pool: String::new(),
            max_runtime: String::new(),
            interval: String::new(),
            max_iterations: 0,
            cmd: String::new(),
            workdir: String::new(),
            if_cond: String::new(),
            then_targets: Vec::new(),
            else_targets: Vec::new(),
            job_name: String::new(),
            workflow_name: String::new(),
            params: BTreeMap::new(),
            timeout: String::new(),
        }
    }

    fn test_backend() -> InMemoryWorkflowBackend {
        InMemoryWorkflowBackend {
            workflows: vec![basic_workflow(), multi_workflow(), invalid_workflow()],
            project_dir: Some(PathBuf::from("/project")),
        }
    }

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK.get_or_init(|| Mutex::new(()));
        match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn temp_data_dir(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: OnceLock<Mutex<u64>> = OnceLock::new();
        let lock = UNIQUE_SUFFIX.get_or_init(|| Mutex::new(0));
        let mut guard = match lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard += 1;
        let suffix = *guard;
        std::env::temp_dir().join(format!(
            "forge-workflow-logs-test-{tag}-{}-{suffix}",
            std::process::id()
        ))
    }

    struct EnvGuard {
        key: String,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }

        fn set_text(key: &str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key: key.to_string(),
                previous,
            }
        }

        fn unset(key: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::remove_var(key);
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(&self.key, value);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        if let Err(err) = std::fs::write(path, body) {
            panic!("write executable: {err}");
        }
        let mut perms = match std::fs::metadata(path) {
            Ok(metadata) => metadata.permissions(),
            Err(err) => panic!("stat executable: {err}"),
        };
        perms.set_mode(0o755);
        if let Err(err) = std::fs::set_permissions(path, perms) {
            panic!("chmod executable: {err}");
        }
    }

    fn seed_mesh_registry(data_dir: &Path, master_endpoint: &str, worker_endpoint: Option<&str>) {
        let store = crate::mesh::MeshStore::with_path(data_dir.join("mesh").join("registry.json"));
        let out = crate::mesh::run_for_test(
            &[
                "mesh",
                "promote",
                "node-master",
                "--endpoint",
                master_endpoint,
            ],
            &store,
        );
        assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);

        if let Some(worker_endpoint) = worker_endpoint {
            let out = crate::mesh::run_for_test(
                &[
                    "mesh",
                    "promote",
                    "node-worker",
                    "--endpoint",
                    worker_endpoint,
                ],
                &store,
            );
            assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);

            // Keep node-master as active master for deterministic route behavior in tests.
            let out = crate::mesh::run_for_test(
                &[
                    "mesh",
                    "promote",
                    "node-master",
                    "--endpoint",
                    master_endpoint,
                ],
                &store,
            );
            assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
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
        assert!(out.stdout.contains("validate"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn help_explicit() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn help_dash_h() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "-h"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    #[test]
    fn wf_alias_accepted() {
        let backend = test_backend();
        let out = run_for_test(&["wf", "help"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Commands:"));
    }

    // -- ls / list --

    #[test]
    fn list_all_workflows() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("basic"));
        assert!(out.stdout.contains("multi"));
        assert!(out.stdout.contains("bad-dep"));
        assert!(out.stdout.contains("NAME"));
        assert!(out.stdout.contains("STEPS"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn list_alias_works() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "list"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("basic"));
    }

    #[test]
    fn list_empty_workflows() {
        let backend = InMemoryWorkflowBackend::default();
        let out = run_for_test(&["workflow", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("No workflows found"));
    }

    #[test]
    fn list_json_output() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json_or_panic(&out.stdout, "parse list json");
        assert!(parsed.is_array());
        assert_eq!(
            array_or_panic(&parsed, "list output should be array").len(),
            3
        );
    }

    #[test]
    fn list_jsonl_output() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--jsonl", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json_or_panic(out.stdout.trim(), "parse list jsonl");
        assert!(parsed.is_array());
    }

    #[test]
    fn list_table_shows_step_count() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        // basic has 2 steps, multi has 3
        assert!(out.stdout.contains("2"));
        assert!(out.stdout.contains("3"));
    }

    // -- show --

    #[test]
    fn show_workflow() {
        let _lock = env_lock();
        let config_path = temp_data_dir("show-default-parallel").join("missing-config.yaml");
        let _cfg = EnvGuard::set("FORGE_CONFIG_PATH", &config_path);
        let _env = EnvGuard::unset(WORKFLOW_MAX_PARALLEL_ENV);
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workflow: basic"));
        assert!(out.stdout.contains("Version: 0.1"));
        assert!(out.stdout.contains("Description: Basic workflow"));
        assert!(out.stdout.contains("Steps:"));
        assert!(out.stdout.contains("plan [agent]"));
        assert!(out.stdout.contains("build [bash]"));
        assert!(out.stdout.contains("(depends_on: plan)"));
        assert!(out.stdout.contains("Max Parallel: "));
        assert!(out.stdout.contains("Flow:"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn show_workflow_includes_fan_out_counts() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("fan_out: running=0 queued=1"));
        assert!(out.stdout.contains("fan_out: running=0 queued=0"));
    }

    #[test]
    fn show_workflow_case_insensitive() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "BASIC"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workflow: basic"));
    }

    #[test]
    fn show_workflow_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn show_workflow_json() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "show", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json_or_panic(&out.stdout, "parse show json");
        assert_eq!(parsed["name"], "basic");
        assert_eq!(parsed["version"], "0.1");
    }

    #[test]
    fn show_missing_name() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("usage:"));
    }

    #[test]
    fn show_workflow_with_inputs() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Inputs: repo=."));
    }

    #[test]
    fn show_workflow_prompt_details() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "multi"], &backend);
        assert_eq!(out.exit_code, 0);
        // agent-work has an inline prompt
        assert!(out.stdout.contains("prompt: [inline]"));
        // setup has a cmd
        assert!(out.stdout.contains("cmd: echo setup"));
    }

    #[test]
    fn show_workflow_flow_section() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "show", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("plan -> build"));
    }

    #[test]
    fn show_workflow_surfaces_blocked_human_step_reason_and_timeout() {
        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "approval".to_string(),
                version: String::new(),
                description: "Approval-gated workflow".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "approve".to_string(),
                        step_type: STEP_TYPE_HUMAN.to_string(),
                        timeout: "15m".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "deploy".to_string(),
                        step_type: STEP_TYPE_BASH.to_string(),
                        cmd: "echo deploy".to_string(),
                        depends_on: vec!["approve".to_string()],
                        ..default_step()
                    },
                ],
                hooks: None,
                source: "/project/.forge/workflows/approval.toml".to_string(),
            }],
            project_dir: Some(PathBuf::from("/project")),
        };

        let out = run_for_test(&["workflow", "show", "approval"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("blocked: yes"));
        assert!(out.stdout.contains("awaiting human approval"));
        assert!(out.stdout.contains("approval timeout: 15m"));
        assert!(out.stdout.contains("timeout: 15m"));
        assert!(out.stdout.contains("blocked until approve is approved"));
        assert!(out
            .stdout
            .contains("approve via: forge workflow approve <run-id> --step approve"));
    }

    #[test]
    fn resolve_workflow_max_parallel_prefers_workflow_field() {
        let _lock = env_lock();
        let _env = EnvGuard::set_text(WORKFLOW_MAX_PARALLEL_ENV, "9");
        let mut wf = basic_workflow();
        wf.max_parallel = 4;
        let (value, source) = match resolve_workflow_max_parallel(&wf) {
            Ok(value) => value,
            Err(err) => panic!("resolve max parallel from workflow: {err}"),
        };
        assert_eq!(value, 4);
        assert_eq!(source, WorkflowParallelSource::Workflow);
    }

    #[test]
    fn resolve_workflow_max_parallel_uses_env_when_workflow_unset() {
        let _lock = env_lock();
        let _env = EnvGuard::set_text(WORKFLOW_MAX_PARALLEL_ENV, "3");
        let wf = basic_workflow();
        let (value, source) = match resolve_workflow_max_parallel(&wf) {
            Ok(value) => value,
            Err(err) => panic!("resolve max parallel from env: {err}"),
        };
        assert_eq!(value, 3);
        assert_eq!(source, WorkflowParallelSource::Environment);
    }

    #[test]
    fn resolve_workflow_max_parallel_uses_global_config() {
        let _lock = env_lock();
        let config_dir = temp_data_dir("workflow-max-parallel-global");
        let _ = fs::create_dir_all(&config_dir);
        let config_path = config_dir.join("config.yaml");
        let _ = fs::write(&config_path, "scheduler:\n  workflow_max_parallel: 5\n");
        let _cfg = EnvGuard::set("FORGE_CONFIG_PATH", &config_path);
        let _env = EnvGuard::unset(WORKFLOW_MAX_PARALLEL_ENV);

        let wf = basic_workflow();
        let (value, source) = match resolve_workflow_max_parallel(&wf) {
            Ok(value) => value,
            Err(err) => panic!("resolve max parallel from global config: {err}"),
        };
        assert_eq!(value, 5);
        assert_eq!(source, WorkflowParallelSource::GlobalConfig);

        let _ = std::fs::remove_dir_all(config_dir);
    }

    #[test]
    fn resolve_workflow_max_parallel_rejects_invalid_env() {
        let _lock = env_lock();
        let _env = EnvGuard::set_text(WORKFLOW_MAX_PARALLEL_ENV, "0");
        let err = match resolve_workflow_max_parallel(&basic_workflow()) {
            Ok(_) => panic!("invalid env should fail"),
            Err(err) => err,
        };
        assert!(err.contains("greater than 0"));
    }

    // -- validate --

    #[test]
    fn validate_valid_workflow() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "validate", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Workflow valid: basic"));
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn validate_invalid_workflow() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "validate", "bad-dep"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.contains("Workflow invalid: bad-dep"));
        assert!(out.stderr.contains("unknown dependency"));
    }

    #[test]
    fn validate_json_valid() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "validate", "basic"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["name"], "basic");
        assert_eq!(parsed["valid"], true);
    }

    #[test]
    fn validate_json_invalid() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "validate", "bad-dep"], &backend);
        assert_eq!(out.exit_code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["name"], "bad-dep");
        assert_eq!(parsed["valid"], false);
        assert!(!parsed["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn validate_missing_name() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "validate"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("usage:"));
    }

    #[test]
    fn validate_not_found() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "validate", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    // -- run --

    #[test]
    fn run_executes_simple_bash_workflow_and_returns_success() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-success");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-success");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("bash-only.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![workflow_for_run_test(
                "bash-only",
                &workflow_source,
                vec![
                    ("setup", "printf setup", Vec::new()),
                    ("build", "printf build", vec!["setup".to_string()]),
                ],
            )],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "bash-only"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let run_id = parsed["run_id"].as_str().unwrap_or_default().to_string();
        assert!(!run_id.is_empty());
        assert_eq!(parsed["status"], "success");

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(&run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Success);
        assert_eq!(run.steps.len(), 2);
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Success
        );
        let ledger = repo_root
            .join(".forge")
            .join("ledgers")
            .join("workflow-bash-only.md");
        let ledger_text = std::fs::read_to_string(ledger).unwrap();
        assert!(ledger_text.contains(format!("- run_id: {run_id}").as_str()));
        assert!(ledger_text.contains("- step_count: 2"));
        assert!(ledger_text.contains("- setup [bash] status=success duration_ms="));
        assert!(ledger_text.contains("- build [bash] status=success duration_ms="));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_failure_marks_downstream_step_as_skipped() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-failure");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-failure");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("fail-chain.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![workflow_for_run_test(
                "fail-chain",
                &workflow_source,
                vec![
                    ("setup", "printf setup", Vec::new()),
                    ("build", "printf err >&2; exit 3", vec!["setup".to_string()]),
                    ("ship", "printf ship", vec!["build".to_string()]),
                ],
            )],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "fail-chain"], &backend);
        assert_eq!(out.exit_code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let run_id = parsed["run_id"].as_str().unwrap_or_default().to_string();
        assert!(!run_id.is_empty());
        assert_eq!(parsed["status"], "failed");

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(&run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Failed);
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Failed
        );
        assert_eq!(
            run.steps[2].status,
            run_persistence::WorkflowStepStatus::Skipped
        );
        let ledger = repo_root
            .join(".forge")
            .join("ledgers")
            .join("workflow-fail-chain.md");
        let ledger_text = std::fs::read_to_string(ledger).unwrap();
        assert!(ledger_text.contains(format!("- run_id: {run_id}").as_str()));
        assert!(ledger_text.contains("- step_count: 3"));
        assert!(ledger_text.contains("- setup [bash] status=success duration_ms="));
        assert!(ledger_text.contains("- build [bash] status=failed duration_ms="));
        assert!(ledger_text.contains("error=exit status 3"));
        assert!(ledger_text.contains("- ship [bash] status=skipped duration_ms=n/a"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_pauses_at_human_step_and_persists_waiting_state() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-human-pause");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-human-pause");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("human-pause.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "human-pause".to_string(),
                version: "0.1".to_string(),
                description: "human pause fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf ready".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "approve".to_string(),
                        step_type: "human".to_string(),
                        depends_on: vec!["build".to_string()],
                        prompt: "Please approve deployment".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "ship".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["approve".to_string()],
                        cmd: "printf shipped".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "human-pause"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "running");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        assert!(!run_id.is_empty());

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Running);
        assert!(run.finished_at.is_none());
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::WaitingApproval
        );
        assert_eq!(
            run.steps[2].status,
            run_persistence::WorkflowStepStatus::Pending
        );

        let approval = run.steps[1].approval.as_ref().unwrap();
        assert_eq!(
            approval.state,
            run_persistence::WorkflowStepApprovalState::Pending
        );
        assert!(approval.decided_at.is_none());
        assert!(approval.timeout_at.is_some());
        assert!(!approval.requested_at.is_empty());

        let log = store.read_step_log(run_id, "approve").unwrap();
        assert!(log.contains("awaiting human approval"));
        assert!(log.contains("timeout=default(24h)"));

        let resume = store.load_resume_state(run_id).unwrap();
        assert_eq!(resume.remaining_step_ids, vec!["approve", "ship"]);

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_human_step_accepts_prompt_id_reference() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-human-prompt-id");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-human-prompt-id");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("human-prompt-id.toml");
        let prompts_dir = repo_root.join(".forge").join("prompts");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );
        let _ = std::fs::create_dir_all(&prompts_dir);
        let _ = std::fs::write(prompts_dir.join("approve.md"), "# approve");

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "human-prompt-id".to_string(),
                version: "0.1".to_string(),
                description: "human prompt id fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![WorkflowStep {
                    id: "approve".to_string(),
                    step_type: "human".to_string(),
                    prompt_id: "approve".to_string(),
                    ..default_step()
                }],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "human-prompt-id"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "running");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_binds_upstream_outputs_into_downstream_inputs() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-bindings-success");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-bindings-success");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("bindings.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let mut consume_inputs = BTreeMap::new();
        consume_inputs.insert(
            "payload".to_string(),
            toml::Value::String("{{steps.build.output}}".to_string()),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "bindings".to_string(),
                version: "0.1".to_string(),
                description: "binding fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf alpha".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "consume".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["build".to_string()],
                        inputs: consume_inputs,
                        cmd: "test \"$FORGE_INPUT_PAYLOAD\" = \"alpha\"".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "bindings"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "success");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        assert!(!run_id.is_empty());

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Success);
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Success
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_fails_with_clear_error_when_template_output_missing() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-bindings-missing");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-bindings-missing");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("bindings-missing.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let mut consume_inputs = BTreeMap::new();
        consume_inputs.insert(
            "payload".to_string(),
            toml::Value::String("{{steps.build.missing}}".to_string()),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "bindings-missing".to_string(),
                version: "0.1".to_string(),
                description: "binding missing fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf alpha".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "consume".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["build".to_string()],
                        inputs: consume_inputs,
                        cmd: "printf should-not-run".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "bindings-missing"], &backend);
        assert_eq!(out.exit_code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "failed");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        assert!(!run_id.is_empty());

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Failed);
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Failed
        );
        let log = store.read_step_log(run_id, "consume").unwrap();
        assert!(log.contains("missing template step output"));
        assert!(log.contains("steps.build.missing"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_executes_step_and_workflow_hooks_in_order_and_logs_output() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-hooks-order");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-hooks-order");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("hooks.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "hooks".to_string(),
                version: "0.1".to_string(),
                description: "hook order fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![WorkflowStep {
                    id: "build".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "printf body".to_string(),
                    hooks: Some(WorkflowHooks {
                        pre: vec![
                            "bash:printf step-pre-1".to_string(),
                            "bash:printf step-pre-2".to_string(),
                        ],
                        post: vec!["bash:printf step-post".to_string()],
                    }),
                    ..default_step()
                }],
                hooks: Some(WorkflowHooks {
                    pre: vec!["bash:printf workflow-pre".to_string()],
                    post: vec!["bash:printf workflow-post".to_string()],
                }),
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "hooks"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "success");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        assert!(!run_id.is_empty());

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let log = store.read_step_log(run_id, "build").unwrap();
        let workflow_pre_pos = log.find("workflow.pre#1").unwrap();
        let step_pre_1_pos = log.find("step.pre#1").unwrap();
        let step_pre_2_pos = log.find("step.pre#2").unwrap();
        let step_post_pos = log.find("step.post#1").unwrap();
        let workflow_post_pos = log.find("workflow.post#1").unwrap();
        assert!(workflow_pre_pos < step_pre_1_pos);
        assert!(step_pre_1_pos < step_pre_2_pos);
        assert!(step_pre_2_pos < step_post_pos);
        assert!(step_post_pos < workflow_post_pos);
        assert!(log.contains("workflow-pre"));
        assert!(log.contains("step-pre-1"));
        assert!(log.contains("step-post"));
        assert!(log.contains("workflow-post"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_hook_fail_mode_stops_workflow() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-hooks-fail");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-hooks-fail");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("hooks-fail.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "hooks-fail".to_string(),
                version: "0.1".to_string(),
                description: "hook fail fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![WorkflowStep {
                    id: "build".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "printf should-not-run".to_string(),
                    hooks: Some(WorkflowHooks {
                        pre: vec!["bash:exit 7".to_string()],
                        post: Vec::new(),
                    }),
                    ..default_step()
                }],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "hooks-fail"], &backend);
        assert_eq!(out.exit_code, 1);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "failed");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(run_id).unwrap();
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Failed);
        assert_eq!(
            run.steps[0].status,
            run_persistence::WorkflowStepStatus::Failed
        );
        let log = store.read_step_log(run_id, "build").unwrap();
        assert!(log.contains("hook step.pre#1 failed with exit status 7"));
        assert!(!log.contains("should-not-run"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn run_hook_warn_mode_logs_warning_and_continues() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("run-hooks-warn");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-hooks-warn");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("hooks-warn.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "hooks-warn".to_string(),
                version: "0.1".to_string(),
                description: "hook warn fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![WorkflowStep {
                    id: "build".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "printf body".to_string(),
                    hooks: Some(WorkflowHooks {
                        pre: vec!["warn:bash:echo warn-msg >&2; exit 9".to_string()],
                        post: Vec::new(),
                    }),
                    ..default_step()
                }],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let out = run_for_test(&["workflow", "--json", "run", "hooks-warn"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["status"], "success");
        let run_id = parsed["run_id"].as_str().unwrap_or_default();
        let store = run_persistence::WorkflowRunStore::open_from_env();
        let log = store.read_step_log(run_id, "build").unwrap();
        assert!(log.contains("warning: hook step.pre#1 failed with exit status 9"));
        assert!(log.contains("warn-msg"));
        assert!(log.contains("body"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    // -- logs --

    #[test]
    fn logs_prints_steps_in_workflow_order() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("logs-human");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);
        let store = run_persistence::WorkflowRunStore::open_from_env();

        let run = store
            .create_run(
                "deploy",
                "/repo/.forge/workflows/deploy.toml",
                &["plan".to_string(), "build".to_string(), "ship".to_string()],
            )
            .unwrap();
        store
            .update_step_status(
                &run.id,
                "plan",
                run_persistence::WorkflowStepStatus::Success,
            )
            .unwrap();
        store.append_step_log(&run.id, "plan", "plan: ok").unwrap();
        store
            .update_step_status(
                &run.id,
                "build",
                run_persistence::WorkflowStepStatus::Failed,
            )
            .unwrap();
        store
            .append_step_log(&run.id, "build", "build: fail")
            .unwrap();

        let backend = test_backend();
        let out = run_for_test(&["workflow", "logs", &run.id], &backend);
        assert_eq!(out.exit_code, 0);
        let plan_pos = out.stdout.find("1. plan [success]").unwrap();
        let build_pos = out.stdout.find("2. build [failed]").unwrap();
        let ship_pos = out.stdout.find("3. ship [pending]").unwrap();
        assert!(plan_pos < build_pos && build_pos < ship_pos);
        assert!(out.stdout.contains("plan: ok"));
        assert!(out.stdout.contains("build: fail"));
        assert!(out.stdout.contains("(no log output)"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn logs_include_fan_out_running_and_queued_counts() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("logs-fan-out");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);
        let store = run_persistence::WorkflowRunStore::open_from_env();

        let run = store
            .create_run(
                "deploy",
                "/repo/.forge/workflows/deploy.toml",
                &["plan".to_string(), "build".to_string(), "ship".to_string()],
            )
            .unwrap();
        store
            .update_step_status(
                &run.id,
                "plan",
                run_persistence::WorkflowStepStatus::Success,
            )
            .unwrap();
        store
            .update_step_status(
                &run.id,
                "build",
                run_persistence::WorkflowStepStatus::Running,
            )
            .unwrap();

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "deploy".to_string(),
                version: "0.1".to_string(),
                description: "fan-out fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "plan".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf plan".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["plan".to_string()],
                        cmd: "printf build".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "ship".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["build".to_string()],
                        cmd: "printf ship".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: "/repo/.forge/workflows/deploy.toml".to_string(),
            }],
            project_dir: Some(data_dir.clone()),
        };

        let out = run_for_test(&["workflow", "logs", &run.id], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out
            .stdout
            .contains("1. plan [success] fan_out: running=1 queued=0"));
        assert!(out
            .stdout
            .contains("2. build [running] fan_out: running=0 queued=1"));
        assert!(out
            .stdout
            .contains("3. ship [pending] fan_out: running=0 queued=0"));

        let out = run_for_test(&["workflow", "--json", "logs", &run.id], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["steps"][0]["fan_out_running"], 1);
        assert_eq!(parsed["steps"][0]["fan_out_queued"], 0);
        assert_eq!(parsed["steps"][1]["fan_out_running"], 0);
        assert_eq!(parsed["steps"][1]["fan_out_queued"], 1);
        assert_eq!(parsed["steps"][2]["fan_out_running"], 0);
        assert_eq!(parsed["steps"][2]["fan_out_queued"], 0);

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn logs_json_includes_step_logs() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("logs-json");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);
        let store = run_persistence::WorkflowRunStore::open_from_env();

        let run = store
            .create_run(
                "deploy",
                "/repo/.forge/workflows/deploy.toml",
                &["plan".to_string()],
            )
            .unwrap();
        store.append_step_log(&run.id, "plan", "line one").unwrap();
        store.append_step_log(&run.id, "plan", "line two").unwrap();

        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "logs", &run.id], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["run_id"], run.id);
        assert_eq!(parsed["workflow_name"], "deploy");
        assert_eq!(parsed["steps"][0]["step_id"], "plan");
        assert_eq!(parsed["steps"][0]["status"], "pending");
        assert_eq!(parsed["steps"][0]["log"], "line one\nline two\n");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn logs_missing_run_is_error() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("logs-missing");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);
        let backend = test_backend();
        let out = run_for_test(&["workflow", "logs", "wfr_missing"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("workflow run \"wfr_missing\" not found"));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn logs_missing_run_id_usage_error() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "logs"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("usage: forge workflow logs <run-id>"));
    }

    #[test]
    fn approve_command_marks_waiting_step_approved() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("approve-command");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-approve-command");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("human-pause.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "human-pause".to_string(),
                version: "0.1".to_string(),
                description: "human pause fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf ready".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "approve".to_string(),
                        step_type: "human".to_string(),
                        depends_on: vec!["build".to_string()],
                        prompt: "Please approve deployment".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "ship".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["approve".to_string()],
                        cmd: "printf shipped".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let run_out = run_for_test(&["workflow", "--json", "run", "human-pause"], &backend);
        assert_eq!(run_out.exit_code, 0, "stderr: {}", run_out.stderr);
        let run_json: serde_json::Value = serde_json::from_str(&run_out.stdout).unwrap();
        let run_id = run_json["run_id"].as_str().unwrap_or_default().to_string();
        assert!(!run_id.is_empty());

        let out = run_for_test(
            &[
                "workflow", "--json", "approve", &run_id, "--step", "approve",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["decision"], "approved");
        assert_eq!(parsed["run_status"], "success");
        assert!(
            parsed["remaining_steps"].is_null()
                || parsed["remaining_steps"] == serde_json::json!([])
        );

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(&run_id).unwrap();
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[2].status,
            run_persistence::WorkflowStepStatus::Success
        );
        assert_eq!(
            run.steps[1].approval.as_ref().unwrap().state,
            run_persistence::WorkflowStepApprovalState::Approved
        );
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Success);
        let ship_log = store.read_step_log(&run_id, "ship").unwrap();
        assert!(ship_log.contains("shipped"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn deny_command_fails_run_and_records_reason() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("deny-command");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-deny-command");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("human-pause.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "human-pause".to_string(),
                version: "0.1".to_string(),
                description: "human pause fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf ready".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "approve".to_string(),
                        step_type: "human".to_string(),
                        depends_on: vec!["build".to_string()],
                        prompt: "Please approve deployment".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "ship".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["approve".to_string()],
                        cmd: "printf shipped".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let run_out = run_for_test(&["workflow", "--json", "run", "human-pause"], &backend);
        assert_eq!(run_out.exit_code, 0, "stderr: {}", run_out.stderr);
        let run_json: serde_json::Value = serde_json::from_str(&run_out.stdout).unwrap();
        let run_id = run_json["run_id"].as_str().unwrap_or_default().to_string();
        assert!(!run_id.is_empty());

        let out = run_for_test(
            &[
                "workflow",
                "--json",
                "deny",
                &run_id,
                "--step",
                "approve",
                "--reason",
                "operator rejected rollout",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["decision"], "denied");
        assert_eq!(parsed["run_status"], "failed");

        let store = run_persistence::WorkflowRunStore::open_from_env();
        let run = store.get_run(&run_id).unwrap();
        assert_eq!(
            run.steps[1].status,
            run_persistence::WorkflowStepStatus::Failed
        );
        assert_eq!(
            run.steps[2].status,
            run_persistence::WorkflowStepStatus::Skipped
        );
        assert_eq!(run.status, run_persistence::WorkflowRunStatus::Failed);
        let log = store.read_step_log(&run_id, "approve").unwrap();
        assert!(log.contains("reason=operator rejected rollout"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn blocked_command_lists_waiting_and_dependency_blocked_steps() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("blocked-command");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let repo_root = data_dir.join("repo-blocked-command");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("human-pause.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );

        let backend = InMemoryWorkflowBackend {
            workflows: vec![Workflow {
                name: "human-pause".to_string(),
                version: "0.1".to_string(),
                description: "human pause fixture".to_string(),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                max_parallel: 0,
                steps: vec![
                    WorkflowStep {
                        id: "build".to_string(),
                        step_type: "bash".to_string(),
                        cmd: "printf ready".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "approve".to_string(),
                        step_type: "human".to_string(),
                        depends_on: vec!["build".to_string()],
                        prompt: "Please approve deployment".to_string(),
                        ..default_step()
                    },
                    WorkflowStep {
                        id: "ship".to_string(),
                        step_type: "bash".to_string(),
                        depends_on: vec!["approve".to_string()],
                        cmd: "printf shipped".to_string(),
                        ..default_step()
                    },
                ],
                hooks: None,
                source: workflow_source.to_string_lossy().to_string(),
            }],
            project_dir: Some(repo_root.clone()),
        };

        let run_out = run_for_test(&["workflow", "--json", "run", "human-pause"], &backend);
        assert_eq!(run_out.exit_code, 0, "stderr: {}", run_out.stderr);
        let run_json: serde_json::Value = serde_json::from_str(&run_out.stdout).unwrap();
        let run_id = run_json["run_id"].as_str().unwrap_or_default().to_string();
        assert!(!run_id.is_empty());

        let out = run_for_test(&["workflow", "--json", "blocked", &run_id], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        let blocked = parsed["blocked_steps"].as_array().unwrap();
        assert_eq!(blocked.len(), 2);
        assert_eq!(blocked[0]["step_id"], "approve");
        assert_eq!(blocked[0]["status"], "waiting_approval");
        assert!(blocked[0]["reasons"][0]
            .as_str()
            .unwrap_or_default()
            .contains("awaiting human approval"));
        assert_eq!(blocked[1]["step_id"], "ship");
        assert_eq!(blocked[1]["status"], "pending");
        assert!(blocked[1]["reasons"][0]
            .as_str()
            .unwrap_or_default()
            .contains("blocked by approve"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    // -- error cases --

    #[test]
    fn unknown_subcommand() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown workflow subcommand: foobar"));
    }

    #[test]
    fn approve_missing_step_usage_error() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "approve", "wfr_demo"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("usage: forge workflow approve <run-id> --step <step-id>"));
    }

    #[test]
    fn deny_missing_reason_usage_error() {
        let backend = test_backend();
        let out = run_for_test(
            &["workflow", "deny", "wfr_demo", "--step", "approve"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("usage: forge workflow deny <run-id> --step <step-id> --reason <text>"));
    }

    #[test]
    fn json_and_jsonl_conflict() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--json", "--jsonl", "ls"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_run_accepts_node_flag() {
        let parsed = parse_args(&[
            "workflow".to_string(),
            "run".to_string(),
            "deploy".to_string(),
            "--node".to_string(),
            "node-a".to_string(),
        ]);
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(err) => panic!("parse workflow run --node: {err}"),
        };

        assert_eq!(
            parsed.command,
            SubCommand::Run {
                name: "deploy".to_string(),
                node: Some("node-a".to_string())
            }
        );
    }

    #[test]
    fn parse_node_flag_rejected_for_non_run_subcommands() {
        let err = parse_args(&[
            "workflow".to_string(),
            "ls".to_string(),
            "--node".to_string(),
            "node-a".to_string(),
        ]);
        let err = match err {
            Ok(_) => panic!("--node should be rejected for non-run subcommands"),
            Err(err) => err,
        };

        assert_eq!(err, "--node is only supported for workflow run");
    }

    #[test]
    #[cfg(unix)]
    fn workflow_run_routes_through_master_node_locally() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("remote-run-master");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let bin_dir = data_dir.join("bin");
        if let Err(err) = std::fs::create_dir_all(&bin_dir) {
            panic!("create bin dir: {err}");
        }
        write_executable(
            &bin_dir.join("forge"),
            "#!/bin/sh\nif [ \"$1\" = \"workflow\" ] && [ \"$2\" = \"run\" ]; then\n  echo \"REMOTE_WORKFLOW:$3\"\n  exit 0\nfi\necho \"unexpected args: $*\" >&2\nexit 64\n",
        );

        let inherited_path = std::env::var("PATH").unwrap_or_default();
        let merged_path = format!("{}:{}", bin_dir.display(), inherited_path);
        let _path_guard = EnvGuard::set_text("PATH", &merged_path);

        seed_mesh_registry(&data_dir, "local", None);

        let backend = test_backend();
        let out = run_for_test(
            &["workflow", "run", "deploy", "--node", "node-master"],
            &backend,
        );

        assert_eq!(out.exit_code, 0, "stderr={}", out.stderr);
        assert!(out.stdout.contains("REMOTE_WORKFLOW:deploy"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    #[cfg(unix)]
    fn workflow_run_reports_master_offline_when_ssh_probe_fails() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("remote-run-offline");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let bin_dir = data_dir.join("bin");
        if let Err(err) = std::fs::create_dir_all(&bin_dir) {
            panic!("create bin dir: {err}");
        }
        write_executable(
            &bin_dir.join("ssh"),
            "#!/bin/sh\necho \"Connection refused\" >&2\nexit 255\n",
        );

        let inherited_path = std::env::var("PATH").unwrap_or_default();
        let merged_path = format!("{}:{}", bin_dir.display(), inherited_path);
        let _path_guard = EnvGuard::set_text("PATH", &merged_path);

        seed_mesh_registry(
            &data_dir,
            "ssh://master.example",
            Some("ssh://worker.example"),
        );

        let backend = test_backend();
        let out = run_for_test(
            &["workflow", "run", "deploy", "--node", "node-worker"],
            &backend,
        );

        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("master node node-master offline: Connection refused"));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    fn workflow_for_run_test(
        name: &str,
        source: &Path,
        steps: Vec<(&str, &str, Vec<String>)>,
    ) -> Workflow {
        Workflow {
            name: name.to_string(),
            version: "0.1".to_string(),
            description: "run workflow fixture".to_string(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            max_parallel: 0,
            steps: steps
                .into_iter()
                .map(|(id, cmd, depends_on)| WorkflowStep {
                    id: id.to_string(),
                    step_type: "bash".to_string(),
                    cmd: cmd.to_string(),
                    depends_on,
                    ..default_step()
                })
                .collect(),
            hooks: None,
            source: source.to_string_lossy().to_string(),
        }
    }

    // -- validation logic unit tests --

    #[test]
    fn validate_empty_name_error() {
        let wf = Workflow {
            name: String::new(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "bash".to_string(),
                cmd: "echo hi".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "name"));
    }

    #[test]
    fn validate_no_steps_error() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: Vec::new(),
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "steps"));
    }

    #[test]
    fn validate_missing_step_id() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: String::new(),
                step_type: "bash".to_string(),
                cmd: "echo".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "steps.id"));
    }

    #[test]
    fn validate_duplicate_step_id() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![
                WorkflowStep {
                    id: "dup".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "a".to_string(),
                    ..default_step()
                },
                WorkflowStep {
                    id: "dup".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "b".to_string(),
                    ..default_step()
                },
            ],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors.iter().any(|e| e.code == ERR_DUPLICATE_STEP));
    }

    #[test]
    fn validate_unknown_step_type() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "bogus".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors.iter().any(|e| e.code == ERR_UNKNOWN_TYPE));
    }

    #[test]
    fn validate_agent_needs_prompt() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "agent".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "prompt"));
    }

    #[test]
    fn validate_bash_needs_cmd() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "bash".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "cmd"));
    }

    #[test]
    fn validate_logic_needs_if_and_targets() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "logic".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "if"));
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "then"));
    }

    #[test]
    fn validate_job_needs_name() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "job".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "job_name"));
    }

    #[test]
    fn validate_nested_workflow_needs_name() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "workflow".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "workflow_name"));
    }

    #[test]
    fn validate_self_dependency() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "bash".to_string(),
                cmd: "echo".to_string(),
                depends_on: vec!["s1".to_string()],
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_INVALID_FIELD && e.message.contains("depend on itself")));
    }

    #[test]
    fn validate_duplicate_dependency() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![
                WorkflowStep {
                    id: "a".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo".to_string(),
                    ..default_step()
                },
                WorkflowStep {
                    id: "b".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo".to_string(),
                    depends_on: vec!["a".to_string(), "a".to_string()],
                    ..default_step()
                },
            ],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_INVALID_FIELD && e.message.contains("duplicate dependency")));
    }

    #[test]
    fn validate_missing_dependency_target() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "bash".to_string(),
                cmd: "echo".to_string(),
                depends_on: vec!["nonexistent".to_string()],
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_STEP && e.message.contains("unknown dependency")));
    }

    #[test]
    fn validate_cycle_detection() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![
                WorkflowStep {
                    id: "a".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo".to_string(),
                    depends_on: vec!["b".to_string()],
                    ..default_step()
                },
                WorkflowStep {
                    id: "b".to_string(),
                    step_type: "bash".to_string(),
                    cmd: "echo".to_string(),
                    depends_on: vec!["a".to_string()],
                    ..default_step()
                },
            ],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors.iter().any(|e| e.code == ERR_CYCLE));
    }

    #[test]
    fn validate_stop_condition_empty() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "agent".to_string(),
                prompt: "do things".to_string(),
                stop: Some(StopCondition {
                    expr: String::new(),
                    tool: None,
                    llm: None,
                }),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "stop"));
    }

    #[test]
    fn validate_stop_tool_missing_name() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "agent".to_string(),
                prompt: "do things".to_string(),
                stop: Some(StopCondition {
                    expr: String::new(),
                    tool: Some(StopTool {
                        name: String::new(),
                        args: Vec::new(),
                    }),
                    llm: None,
                }),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "stop.tool.name"));
    }

    #[test]
    fn validate_stop_llm_missing_fields() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "s1".to_string(),
                step_type: "agent".to_string(),
                prompt: "do things".to_string(),
                stop: Some(StopCondition {
                    expr: String::new(),
                    tool: None,
                    llm: Some(StopLLM {
                        rubric: String::new(),
                        pass_if: String::new(),
                    }),
                }),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "stop.llm"));
    }

    #[test]
    fn validate_logic_unknown_target() {
        let wf = Workflow {
            name: "test".to_string(),
            steps: vec![WorkflowStep {
                id: "gate".to_string(),
                step_type: "logic".to_string(),
                if_cond: "x > 0".to_string(),
                then_targets: vec!["nonexistent".to_string()],
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.code == ERR_MISSING_STEP && e.message.contains("unknown logic target")));
    }

    #[test]
    fn validate_negative_max_parallel() {
        let wf = Workflow {
            max_parallel: -1,
            steps: vec![WorkflowStep {
                id: "build".to_string(),
                step_type: "bash".to_string(),
                cmd: "echo ok".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.field == "max_parallel" && e.message.contains(">= 0")));
    }

    #[test]
    fn validate_human_timeout_literal() {
        let wf = Workflow {
            steps: vec![WorkflowStep {
                id: "approve".to_string(),
                step_type: "human".to_string(),
                prompt: "approve".to_string(),
                timeout: "15x".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.field == "timeout" && e.message.contains("invalid timeout")));
    }

    #[test]
    fn validate_prompt_id_not_found() {
        let repo_root = temp_data_dir("validate-prompt-id-missing");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("missing-prompt.toml");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );
        let wf = Workflow {
            name: "validate-prompt-id".to_string(),
            source: workflow_source.to_string_lossy().to_string(),
            steps: vec![WorkflowStep {
                id: "plan".to_string(),
                step_type: "human".to_string(),
                prompt_id: "does-not-exist".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.field == "prompt_id" && e.message.contains("not found")));
        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn validate_uses_default_prompt_file_when_missing_prompt_fields() {
        let repo_root = temp_data_dir("validate-default-prompt");
        let workflow_source = repo_root
            .join(".forge")
            .join("workflows")
            .join("default-prompt.toml");
        let prompts_dir = repo_root.join(".forge").join("prompts");
        let _ = std::fs::create_dir_all(
            workflow_source
                .parent()
                .unwrap_or_else(|| Path::new("/tmp/forge-workflow-tests")),
        );
        let _ = std::fs::create_dir_all(&prompts_dir);
        let _ = std::fs::write(prompts_dir.join("default.md"), "# default");

        let wf = Workflow {
            name: "validate-default-prompt".to_string(),
            source: workflow_source.to_string_lossy().to_string(),
            steps: vec![WorkflowStep {
                id: "plan".to_string(),
                step_type: "human".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (validated, errors) = validate_workflow(&wf);
        assert!(!errors
            .iter()
            .any(|e| e.code == ERR_MISSING_FIELD && e.field == "prompt"));
        assert_eq!(
            validated.steps[0].prompt_path,
            prompts_dir.join("default.md").to_string_lossy().to_string()
        );
        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn validate_agent_id_not_found() {
        let _lock = env_lock();
        let data_dir = temp_data_dir("validate-agent-id-missing");
        let _guard = EnvGuard::set("FORGE_DATA_DIR", &data_dir);

        let wf = Workflow {
            name: "validate-agent-id".to_string(),
            steps: vec![WorkflowStep {
                id: "plan".to_string(),
                step_type: "agent".to_string(),
                prompt: "do work".to_string(),
                agent_id: "profile-does-not-exist".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let (_, errors) = validate_workflow(&wf);
        assert!(errors
            .iter()
            .any(|e| e.field == "agent_id" && e.message.contains("not found")));
    }

    // -- normalization tests --

    #[test]
    fn normalize_trims_whitespace() {
        let mut wf = Workflow {
            name: "  test  ".to_string(),
            version: "  1.0  ".to_string(),
            description: "  desc  ".to_string(),
            steps: vec![WorkflowStep {
                id: "  s1  ".to_string(),
                step_type: "  BASH  ".to_string(),
                cmd: "  echo  ".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        normalize_workflow(&mut wf);
        assert_eq!(wf.name, "test");
        assert_eq!(wf.version, "1.0");
        assert_eq!(wf.description, "desc");
        assert_eq!(wf.steps[0].id, "s1");
        assert_eq!(wf.steps[0].step_type, "bash");
        assert_eq!(wf.steps[0].cmd, "echo");
    }

    // -- workflow name normalization --

    #[test]
    fn normalize_name_simple() {
        assert_eq!(
            ok_or_panic(normalize_workflow_name("basic"), "normalize simple name"),
            "basic"
        );
    }

    #[test]
    fn normalize_name_strips_toml_suffix() {
        assert_eq!(
            ok_or_panic(
                normalize_workflow_name("basic.toml"),
                "normalize name with .toml suffix",
            ),
            "basic"
        );
    }

    #[test]
    fn normalize_name_empty() {
        assert!(normalize_workflow_name("").is_err());
    }

    #[test]
    fn normalize_name_whitespace() {
        assert!(normalize_workflow_name("   ").is_err());
    }

    #[test]
    fn normalize_name_with_slash() {
        assert!(normalize_workflow_name("foo/bar").is_err());
    }

    #[test]
    fn normalize_name_with_dots() {
        assert!(normalize_workflow_name("foo..bar").is_err());
    }

    // -- TOML parsing --

    #[test]
    fn parse_toml_basic() {
        let toml_str = r#"
name = "basic"
version = "0.1"
description = "Basic workflow"

[inputs]
repo = "."

[[steps]]
id = "plan"
type = "agent"
prompt_name = "plan"

[[steps]]
id = "build"
type = "bash"
cmd = "echo build"
depends_on = ["plan"]
"#;
        let wf = ok_or_panic(
            parse_workflow_toml(toml_str, "test.toml"),
            "parse basic toml",
        );
        assert_eq!(wf.name, "basic");
        assert_eq!(wf.version, "0.1");
        assert_eq!(wf.steps.len(), 2);
        assert_eq!(wf.steps[0].id, "plan");
        assert_eq!(wf.steps[0].step_type, "agent");
        assert_eq!(wf.steps[1].id, "build");
        assert_eq!(wf.steps[1].depends_on, vec!["plan"]);
        assert_eq!(wf.source, "test.toml");
    }

    #[test]
    fn parse_toml_max_parallel() {
        let toml_str = r#"
name = "parallel"
max_parallel = 6

[[steps]]
id = "build"
type = "bash"
cmd = "echo ok"
"#;
        let wf = ok_or_panic(
            parse_workflow_toml(toml_str, "parallel.toml"),
            "parse parallel toml",
        );
        assert_eq!(wf.max_parallel, 6);
    }

    #[test]
    fn parse_toml_invalid() {
        let toml_str = "this is not valid toml {{{";
        let result = parse_workflow_toml(toml_str, "bad.toml");
        assert!(result.is_err());
        let errors = err_or_panic(result, "invalid toml should fail");
        assert!(!errors.is_empty());
        assert_eq!(errors[0].code, ERR_PARSE);
        assert_eq!(errors[0].path, "bad.toml");
    }

    // -- human string formatting --

    #[test]
    fn workflow_error_human_string_basic() {
        let err = WorkflowError {
            code: ERR_MISSING_FIELD.to_string(),
            message: "name is required".to_string(),
            path: "test.toml".to_string(),
            field: "name".to_string(),
            ..default_error()
        };
        assert_eq!(err.human_string(), "test.toml: name: name is required");
    }

    #[test]
    fn workflow_error_human_string_with_step() {
        let err = WorkflowError {
            code: ERR_MISSING_FIELD.to_string(),
            message: "cmd is required".to_string(),
            path: "test.toml".to_string(),
            step_id: "build".to_string(),
            field: "cmd".to_string(),
            ..default_error()
        };
        assert_eq!(
            err.human_string(),
            "test.toml: step build: cmd: cmd is required"
        );
    }

    #[test]
    fn workflow_error_human_string_with_line() {
        let err = WorkflowError {
            code: ERR_PARSE.to_string(),
            message: "syntax error".to_string(),
            path: "test.toml".to_string(),
            line: 5,
            column: 10,
            ..default_error()
        };
        assert_eq!(err.human_string(), "test.toml: syntax error (line 5:10)");
    }

    // -- flowchart --

    #[test]
    fn flowchart_basic() {
        let wf = basic_workflow();
        let lines = workflow_flowchart_lines(&wf);
        assert_eq!(lines, vec!["plan -> build"]);
    }

    #[test]
    fn flowchart_multi() {
        let wf = multi_workflow();
        let lines = workflow_flowchart_lines(&wf);
        assert_eq!(lines, vec!["setup -> agent-work", "agent-work -> teardown"]);
    }

    #[test]
    fn flowchart_isolated_step() {
        let wf = Workflow {
            name: "isolated".to_string(),
            steps: vec![WorkflowStep {
                id: "alone".to_string(),
                step_type: "bash".to_string(),
                cmd: "echo".to_string(),
                ..default_step()
            }],
            ..default_workflow()
        };
        let lines = workflow_flowchart_lines(&wf);
        assert_eq!(lines, vec!["alone"]);
    }

    #[test]
    fn flowchart_empty() {
        let wf = Workflow {
            name: "empty".to_string(),
            steps: Vec::new(),
            ..default_workflow()
        };
        let lines = workflow_flowchart_lines(&wf);
        assert!(lines.is_empty());
    }

    // -- source path display --

    #[test]
    fn source_path_relative() {
        let result = workflow_source_path(
            "/project/.forge/workflows/basic.toml",
            Path::new("/project"),
        );
        assert_eq!(result, ".forge/workflows/basic.toml");
    }

    #[test]
    fn source_path_outside_project() {
        let result = workflow_source_path("/other/path/basic.toml", Path::new("/project"));
        assert_eq!(result, "/other/path/basic.toml");
    }

    #[test]
    fn source_path_empty() {
        let result = workflow_source_path("", Path::new("/project"));
        assert_eq!(result, "");
    }

    // -- prompt resolution --

    #[test]
    fn resolve_inline_prompt() {
        let wf = Workflow {
            source: "/project/.forge/workflows/test.toml".to_string(),
            ..default_workflow()
        };
        let step = WorkflowStep {
            prompt: "do work".to_string(),
            ..default_step()
        };
        let res = resolve_step_prompt(&wf, &step);
        assert_eq!(res.inline, "do work");
        assert!(res.path.is_empty());
    }

    #[test]
    fn resolve_prompt_path() {
        let wf = Workflow {
            source: "/project/.forge/workflows/test.toml".to_string(),
            ..default_workflow()
        };
        let step = WorkflowStep {
            prompt_path: "docs/plan.md".to_string(),
            ..default_step()
        };
        let res = resolve_step_prompt(&wf, &step);
        assert!(res.inline.is_empty());
        assert_eq!(res.path, "/project/docs/plan.md");
    }

    #[test]
    fn resolve_prompt_name() {
        let wf = Workflow {
            source: "/project/.forge/workflows/test.toml".to_string(),
            ..default_workflow()
        };
        let step = WorkflowStep {
            prompt_name: "plan".to_string(),
            ..default_step()
        };
        let res = resolve_step_prompt(&wf, &step);
        assert!(res.inline.is_empty());
        assert_eq!(res.path, "/project/.forge/prompts/plan.md");
    }

    #[test]
    fn resolve_prompt_id() {
        let wf = Workflow {
            source: "/project/.forge/workflows/test.toml".to_string(),
            ..default_workflow()
        };
        let step = WorkflowStep {
            prompt_id: "plan".to_string(),
            ..default_step()
        };
        let res = resolve_step_prompt(&wf, &step);
        assert!(res.inline.is_empty());
        assert_eq!(res.path, "/project/.forge/prompts/plan.md");
    }

    fn default_workflow() -> Workflow {
        Workflow {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            max_parallel: 0,
            steps: Vec::new(),
            hooks: None,
            source: String::new(),
        }
    }
}

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    pub profile: String,
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
    log: String,
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
    Show { name: String },
    Validate { name: String },
    Run { name: String },
    Logs { run_id: String },
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
        SubCommand::Run { ref name } => {
            execute_run(backend, name, parsed.json, parsed.jsonl, stdout, stderr)
        }
        SubCommand::Logs { ref run_id } => execute_logs(run_id, parsed.json, parsed.jsonl, stdout),
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
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), String> {
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

fn run_workflow(wf: &Workflow) -> Result<WorkflowRunCommandResult, String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
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
    let step_lookup: HashMap<String, WorkflowStep> = wf
        .steps
        .iter()
        .cloned()
        .map(|step| (step.id.clone(), step))
        .collect();
    let repo_workdir = resolved_workflow_repo_dir(wf)?;

    let engine_result = run_persistence::execute_sequential_workflow(&engine_steps, |step_id| {
        let Some(step) = step_lookup.get(step_id) else {
            return Err(format!("step {:?} missing from lookup", step_id));
        };
        store.update_step_status(
            &run.id,
            step_id,
            run_persistence::WorkflowStepStatus::Running,
        )?;

        match step.step_type.as_str() {
            STEP_TYPE_BASH => {
                let request = run_persistence::BashStepRequest::new(
                    step_id,
                    &step.cmd,
                    repo_workdir.clone(),
                    &step.workdir,
                );
                let result = run_persistence::execute_bash_step(&request)?;
                run_persistence::append_bash_step_logs(&store, &run.id, &result)?;
                if result.success {
                    store.update_step_status(
                        &run.id,
                        step_id,
                        run_persistence::WorkflowStepStatus::Success,
                    )?;
                    Ok(())
                } else {
                    let err = format!("exit status {}", result.exit_code);
                    store.update_step_status(
                        &run.id,
                        step_id,
                        run_persistence::WorkflowStepStatus::Failed,
                    )?;
                    store.append_step_log(&run.id, step_id, format!("error: {err}").as_str())?;
                    Err(err)
                }
            }
            other => {
                let err = format!(
                    "workflow run currently supports bash steps only; got step type {:?}",
                    other
                );
                store.update_step_status(
                    &run.id,
                    step_id,
                    run_persistence::WorkflowStepStatus::Failed,
                )?;
                store.append_step_log(&run.id, step_id, format!("error: {err}").as_str())?;
                Err(err)
            }
        }
    })?;

    let mut failed = false;
    for record in &engine_result.steps {
        match record.status {
            run_persistence::WorkflowEngineStepStatus::Skipped => {
                store.update_step_status(
                    &run.id,
                    &record.step_id,
                    run_persistence::WorkflowStepStatus::Skipped,
                )?;
            }
            run_persistence::WorkflowEngineStepStatus::Failed => {
                failed = true;
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
        } else {
            run_persistence::WorkflowRunStatus::Success
        },
    )?;

    Ok(WorkflowRunCommandResult {
        run_id: run.id,
        workflow_name: run.workflow_name,
        status: run_status_label(&run.status).to_string(),
    })
}

fn resolved_workflow_repo_dir(wf: &Workflow) -> Result<PathBuf, String> {
    let repo_root = repo_root_from_workflow(wf);
    if !repo_root.is_empty() {
        return Ok(PathBuf::from(repo_root));
    }
    std::env::current_dir().map_err(|err| format!("resolve current directory: {err}"))
}

fn execute_logs(
    run_id: &str,
    json: bool,
    jsonl: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let store = run_persistence::WorkflowRunStore::open_from_env();
    let result = load_workflow_logs_result(&store, run_id)?;

    if json || jsonl {
        return write_json_output(stdout, &result, jsonl);
    }

    write_workflow_logs_human(stdout, &result)
}

fn load_workflow_logs_result(
    store: &run_persistence::WorkflowRunStore,
    run_id: &str,
) -> Result<WorkflowLogsResult, String> {
    let run = store.get_run(run_id)?;
    let mut steps = Vec::with_capacity(run.steps.len());
    for step in &run.steps {
        let log = store.read_step_log(&run.id, &step.step_id)?;
        steps.push(WorkflowStepLogsResult {
            step_id: step.step_id.clone(),
            status: step_status_label(&step.status).to_string(),
            started_at: step.started_at.clone(),
            finished_at: step.finished_at.clone(),
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
            "  {}. {} [{}]",
            index + 1,
            step.step_id,
            step.status
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
        run_persistence::WorkflowStepStatus::Success => "success",
        run_persistence::WorkflowStepStatus::Failed => "failed",
        run_persistence::WorkflowStepStatus::Skipped => "skipped",
        run_persistence::WorkflowStepStatus::Canceled => "canceled",
    }
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
        step.profile = step.profile.trim().to_string();
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

    let types = valid_step_types();
    let mut step_index: HashMap<String, usize> = HashMap::new();

    for (i, step) in wf.steps.iter().enumerate() {
        let index = i + 1;

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

        validate_step_specific_fields(step, index, &path, &mut errors);
        validate_stop_condition(step, index, &path, &mut errors);
        validate_dependencies(step, index, &path, &mut errors);
    }

    validate_dependency_targets(&wf, &step_index, &mut errors);
    validate_logic_targets(&wf, &step_index, &mut errors);
    validate_cycles(&wf, &step_index, &mut errors);

    (wf, errors)
}

fn validate_step_specific_fields(
    step: &WorkflowStep,
    index: usize,
    path: &str,
    errors: &mut Vec<WorkflowError>,
) {
    match step.step_type.as_str() {
        STEP_TYPE_AGENT | STEP_TYPE_LOOP | STEP_TYPE_HUMAN => {
            if step.prompt.is_empty() && step.prompt_path.is_empty() && step.prompt_name.is_empty()
            {
                errors.push(WorkflowError {
                    code: ERR_MISSING_FIELD.to_string(),
                    message: "prompt, prompt_path, or prompt_name is required".to_string(),
                    path: path.to_string(),
                    step_id: step.id.clone(),
                    field: "prompt".to_string(),
                    index,
                    ..default_error()
                });
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

    if !step.prompt_name.is_empty() {
        let mut name = step.prompt_name.clone();
        if !name.ends_with(".md") {
            name.push_str(".md");
        }
        let path = if !root.is_empty() {
            Path::new(&root)
                .join(".forge")
                .join("prompts")
                .join(&name)
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
        let details = format_workflow_step_details(wf, step);
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

fn format_workflow_step_details(wf: &Workflow, step: &WorkflowStep) -> Vec<String> {
    let mut lines = Vec::new();
    match step.step_type.as_str() {
        STEP_TYPE_AGENT | STEP_TYPE_LOOP | STEP_TYPE_HUMAN => {
            let resolution = resolve_step_prompt(wf, step);
            if !resolution.inline.is_empty() {
                lines.push("prompt: [inline]".to_string());
            } else if !resolution.path.is_empty() {
                lines.push(format!("prompt: {}", resolution.path));
            }
        }
        STEP_TYPE_BASH => {
            if !step.cmd.is_empty() {
                lines.push(format!("cmd: {}", step.cmd));
            }
        }
        _ => {}
    }
    lines
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
            SubCommand::Run { name }
        }
        Some("logs") => {
            let run_id = positionals
                .get(1)
                .ok_or_else(|| "usage: forge workflow logs <run-id>".to_string())?
                .clone();
            SubCommand::Logs { run_id }
        }
        Some(other) => return Err(format!("unknown workflow subcommand: {other}")),
    };

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
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(stdout, "  -h, --help  help for workflow")?;
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
            prompt_path: String::new(),
            prompt_name: String::new(),
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
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 3);
    }

    #[test]
    fn list_jsonl_output() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "--jsonl", "ls"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
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
        assert!(out.stdout.contains("Flow:"));
        assert!(out.stderr.is_empty());
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
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
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

    // -- error cases --

    #[test]
    fn unknown_subcommand() {
        let backend = test_backend();
        let out = run_for_test(&["workflow", "foobar"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown workflow subcommand: foobar"));
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
        assert_eq!(normalize_workflow_name("basic").unwrap(), "basic");
    }

    #[test]
    fn normalize_name_strips_toml_suffix() {
        assert_eq!(normalize_workflow_name("basic.toml").unwrap(), "basic");
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
        let wf = parse_workflow_toml(toml_str, "test.toml").unwrap();
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
    fn parse_toml_invalid() {
        let toml_str = "this is not valid toml {{{";
        let result = parse_workflow_toml(toml_str, "bad.toml");
        assert!(result.is_err());
        let errors = result.unwrap_err();
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

    fn default_workflow() -> Workflow {
        Workflow {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            steps: Vec::new(),
            hooks: None,
            source: String::new(),
        }
    }
}

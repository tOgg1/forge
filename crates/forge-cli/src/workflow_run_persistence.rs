use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};
use forge_loop::harness_wrapper::{build_execution_plan, ProfileSpec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Running,
    Success,
    Failed,
    Canceled,
}

impl WorkflowRunStatus {
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
    Canceled,
}

impl WorkflowStepStatus {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Skipped | Self::Canceled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowStepRun {
    pub step_id: String,
    pub status: WorkflowStepStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub log_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunRecord {
    pub id: String,
    pub workflow_name: String,
    pub workflow_source: String,
    pub status: WorkflowRunStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub steps: Vec<WorkflowStepRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowResumeState {
    pub run: WorkflowRunRecord,
    pub remaining_step_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowRunStore {
    root_dir: PathBuf,
}

impl WorkflowRunStore {
    pub fn open_from_env() -> Self {
        Self::new(crate::runtime_paths::resolve_data_dir().join("workflow-runs"))
    }

    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn create_run(
        &self,
        workflow_name: &str,
        workflow_source: &str,
        step_ids: &[String],
    ) -> Result<WorkflowRunRecord, String> {
        let workflow_name = workflow_name.trim();
        if workflow_name.is_empty() {
            return Err("workflow name is required".to_string());
        }

        ensure_dir(&self.root_dir)?;

        let run_id = format!("wfr_{}", Uuid::new_v4().simple());
        let run_dir = self.run_dir(&run_id)?;
        ensure_dir(&run_dir)?;
        ensure_dir(&run_dir.join("logs"))?;

        let now = now_rfc3339();
        let mut steps = Vec::with_capacity(step_ids.len());
        for (index, step_id) in step_ids.iter().enumerate() {
            let trimmed = step_id.trim();
            if trimmed.is_empty() {
                return Err(format!("step id at index {index} is empty"));
            }
            steps.push(WorkflowStepRun {
                step_id: trimmed.to_string(),
                status: WorkflowStepStatus::Pending,
                started_at: None,
                finished_at: None,
                log_file: format!("{:03}_{}.log", index + 1, sanitize_segment(trimmed)),
            });
        }

        let record = WorkflowRunRecord {
            id: run_id,
            workflow_name: workflow_name.to_string(),
            workflow_source: workflow_source.to_string(),
            status: WorkflowRunStatus::Running,
            started_at: now.clone(),
            finished_at: None,
            updated_at: now,
            steps,
        };
        self.write_run(&record)?;
        Ok(record)
    }

    pub fn get_run(&self, run_id: &str) -> Result<WorkflowRunRecord, String> {
        let path = self.run_json_path(run_id)?;
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!("workflow run {:?} not found", run_id));
            }
            Err(err) => return Err(format!("read workflow run {}: {err}", path.display())),
        };
        serde_json::from_str(&raw).map_err(|err| {
            format!(
                "decode workflow run {}: {err}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        })
    }

    pub fn update_run_status(
        &self,
        run_id: &str,
        status: WorkflowRunStatus,
    ) -> Result<WorkflowRunRecord, String> {
        let mut run = self.get_run(run_id)?;
        run.status = status;
        if run.status.is_terminal() {
            run.finished_at = Some(now_rfc3339());
        }
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn update_step_status(
        &self,
        run_id: &str,
        step_id: &str,
        status: WorkflowStepStatus,
    ) -> Result<WorkflowRunRecord, String> {
        let mut run = self.get_run(run_id)?;
        let trimmed_step = step_id.trim();
        if trimmed_step.is_empty() {
            return Err("step id is required".to_string());
        }

        let Some(step) = run
            .steps
            .iter_mut()
            .find(|entry| entry.step_id == trimmed_step)
        else {
            return Err(format!(
                "workflow run {} does not contain step {:?}",
                run_id, trimmed_step
            ));
        };

        if matches!(status, WorkflowStepStatus::Running) && step.started_at.is_none() {
            step.started_at = Some(now_rfc3339());
        }
        if status.is_terminal() {
            step.finished_at = Some(now_rfc3339());
        }
        step.status = status;
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn append_step_log(&self, run_id: &str, step_id: &str, line: &str) -> Result<(), String> {
        let run = self.get_run(run_id)?;
        let trimmed_step = step_id.trim();
        if trimmed_step.is_empty() {
            return Err("step id is required".to_string());
        }
        let Some(step) = run.steps.iter().find(|entry| entry.step_id == trimmed_step) else {
            return Err(format!(
                "workflow run {} does not contain step {:?}",
                run_id, trimmed_step
            ));
        };

        let path = self.step_log_path(run_id, &step.log_file)?;
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| format!("open workflow run log {}: {err}", path.display()))?;

        let has_trailing_newline = line.ends_with('\n');
        file.write_all(line.as_bytes())
            .map_err(|err| format!("write workflow run log {}: {err}", path.display()))?;
        if !has_trailing_newline {
            file.write_all(b"\n")
                .map_err(|err| format!("write workflow run log {}: {err}", path.display()))?;
        }
        Ok(())
    }

    pub fn read_step_log(&self, run_id: &str, step_id: &str) -> Result<String, String> {
        let run = self.get_run(run_id)?;
        let trimmed_step = step_id.trim();
        if trimmed_step.is_empty() {
            return Err("step id is required".to_string());
        }
        let Some(step) = run.steps.iter().find(|entry| entry.step_id == trimmed_step) else {
            return Err(format!(
                "workflow run {} does not contain step {:?}",
                run_id, trimmed_step
            ));
        };
        let path = self.step_log_path(run_id, &step.log_file)?;
        match fs::read_to_string(&path) {
            Ok(content) => Ok(content),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(err) => Err(format!("read workflow run log {}: {err}", path.display())),
        }
    }

    pub fn load_resume_state(&self, run_id: &str) -> Result<WorkflowResumeState, String> {
        let run = self.get_run(run_id)?;
        let remaining_step_ids = run
            .steps
            .iter()
            .filter(|step| !step.status.is_terminal())
            .map(|step| step.step_id.clone())
            .collect();
        Ok(WorkflowResumeState {
            run,
            remaining_step_ids,
        })
    }

    fn write_run(&self, run: &WorkflowRunRecord) -> Result<(), String> {
        let run_dir = self.run_dir(&run.id)?;
        ensure_dir(&run_dir)?;
        let path = run_dir.join("run.json");
        let body = serde_json::to_string_pretty(run)
            .map_err(|err| format!("encode workflow run {}: {err}", run.id))?;
        fs::write(&path, body)
            .map_err(|err| format!("write workflow run {}: {err}", path.display()))
    }

    fn run_json_path(&self, run_id: &str) -> Result<PathBuf, String> {
        Ok(self.run_dir(run_id)?.join("run.json"))
    }

    fn run_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        validate_segment(run_id, "run id")?;
        Ok(self.root_dir.join(run_id))
    }

    fn step_log_path(&self, run_id: &str, log_file: &str) -> Result<PathBuf, String> {
        validate_segment(log_file, "log file")?;
        Ok(self.run_dir(run_id)?.join("logs").join(log_file))
    }
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|err| format!("create directory {}: {err}", path.display()))
}

fn validate_segment(value: &str, field: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(format!("invalid {field}: {:?}", value));
    }
    Ok(())
}

fn sanitize_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "step".to_string()
    } else {
        trimmed.to_string()
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        append_workflow_ledger_entry, WorkflowRunStatus, WorkflowRunStore, WorkflowStepStatus,
    };

    fn temp_root(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-workflow-run-store-{tag}-{nanos}-{}-{suffix}",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn create_run_persists_and_is_retrievable_by_id() {
        let root = temp_root("create");
        let store = WorkflowRunStore::new(root.clone());
        let created = store
            .create_run(
                "build-and-test",
                "/repo/.forge/workflows/build-and-test.toml",
                &["plan".to_string(), "test".to_string()],
            )
            .expect("create run");

        assert!(created.id.starts_with("wfr_"));
        assert_eq!(created.status, WorkflowRunStatus::Running);
        assert_eq!(created.steps.len(), 2);
        assert_eq!(created.steps[0].status, WorkflowStepStatus::Pending);
        assert_eq!(created.steps[0].step_id, "plan");

        let reloaded = store.get_run(&created.id).expect("reload");
        assert_eq!(reloaded.id, created.id);
        assert_eq!(reloaded.workflow_name, "build-and-test");
        assert!(root.join(&created.id).join("run.json").is_file());

        cleanup(&root);
    }

    #[test]
    fn step_status_updates_are_persisted_and_resume_state_is_derived() {
        let root = temp_root("resume");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "resume-demo",
                "/repo/.forge/workflows/resume-demo.toml",
                &["plan".to_string(), "test".to_string(), "ship".to_string()],
            )
            .expect("create run");

        let run = store
            .update_step_status(&run.id, "plan", WorkflowStepStatus::Running)
            .expect("step running");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Running);
        assert!(run.steps[0].started_at.is_some());
        assert!(run.steps[0].finished_at.is_none());

        let run = store
            .update_step_status(&run.id, "plan", WorkflowStepStatus::Success)
            .expect("step success");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Success);
        assert!(run.steps[0].finished_at.is_some());

        let resume = store.load_resume_state(&run.id).expect("resume");
        assert_eq!(resume.remaining_step_ids, vec!["test", "ship"]);

        cleanup(&root);
    }

    #[test]
    fn append_and_read_step_log_by_run_id() {
        let root = temp_root("logs");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "log-demo",
                "/repo/.forge/workflows/log-demo.toml",
                &["plan".to_string()],
            )
            .expect("create run");

        store
            .append_step_log(&run.id, "plan", "first line")
            .expect("append one");
        store
            .append_step_log(&run.id, "plan", "second line\n")
            .expect("append two");

        let log = store.read_step_log(&run.id, "plan").expect("read");
        assert_eq!(log, "first line\nsecond line\n");

        cleanup(&root);
    }

    #[test]
    fn missing_run_returns_error() {
        let root = temp_root("missing");
        let store = WorkflowRunStore::new(root.clone());
        let err = store.get_run("wfr_missing").expect_err("should fail");
        assert!(
            err.contains("workflow run \"wfr_missing\" not found"),
            "unexpected error message: {err}"
        );
        cleanup(&root);
    }

    #[test]
    fn workflow_ledger_entry_contains_run_id_step_summaries_and_durations() {
        let root = temp_root("ledger");
        let repo_root = root.join("repo");
        if let Err(err) = std::fs::create_dir_all(repo_root.join(".forge").join("workflows")) {
            panic!("create repo root: {err}");
        }
        let store = WorkflowRunStore::new(root.join("store"));
        let run = match store.create_run(
            "deploy",
            repo_root.join(".forge/workflows/deploy.toml").to_string_lossy().as_ref(),
            &["plan".to_string(), "ship".to_string()],
        ) {
            Ok(run) => run,
            Err(err) => panic!("create run: {err}"),
        };
        if let Err(err) = store.update_step_status(&run.id, "plan", WorkflowStepStatus::Running) {
            panic!("set plan running: {err}");
        }
        if let Err(err) = store.update_step_status(&run.id, "plan", WorkflowStepStatus::Success) {
            panic!("set plan success: {err}");
        }
        if let Err(err) = store.update_step_status(&run.id, "ship", WorkflowStepStatus::Running) {
            panic!("set ship running: {err}");
        }
        if let Err(err) = store.update_step_status(&run.id, "ship", WorkflowStepStatus::Failed) {
            panic!("set ship failed: {err}");
        }
        if let Err(err) = store.update_run_status(&run.id, WorkflowRunStatus::Failed) {
            panic!("set run failed: {err}");
        }

        let ledger_path = match append_workflow_ledger_entry(&store, &run.id, &repo_root) {
            Ok(path) => path,
            Err(err) => panic!("append ledger entry: {err}"),
        };
        let text = match std::fs::read_to_string(&ledger_path) {
            Ok(text) => text,
            Err(err) => panic!("read workflow ledger: {err}"),
        };
        assert!(text.contains("# Workflow Run Ledger"));
        assert!(text.contains(format!("- run_id: {}", run.id).as_str()));
        assert!(text.contains("- plan [success] duration_ms:"));
        assert!(text.contains("- ship [failed] duration_ms:"));

        // Header should be written once.
        if let Err(err) = append_workflow_ledger_entry(&store, &run.id, &repo_root) {
            panic!("append ledger entry second pass: {err}");
        }
        let text_again = match std::fs::read_to_string(&ledger_path) {
            Ok(text) => text,
            Err(err) => panic!("read workflow ledger second pass: {err}"),
        };
        let header_count = text_again.matches("# Workflow Run Ledger").count();
        assert_eq!(header_count, 1);

        cleanup(&root);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStepRequest {
    pub step_id: String,
    pub prompt_content: String,
    pub prompt_path: String,
    pub profile: ProfileSpec,
    pub workdir: PathBuf,
    pub base_env: Vec<String>,
}

impl AgentStepRequest {
    pub fn new(
        step_id: impl Into<String>,
        prompt_content: impl Into<String>,
        profile: ProfileSpec,
        workdir: PathBuf,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            prompt_content: prompt_content.into(),
            prompt_path: String::new(),
            profile,
            workdir,
            base_env: capture_base_env(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStepExecutionResult {
    pub step_id: String,
    pub command: String,
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub output: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub logs: Vec<String>,
}

pub fn execute_agent_step(request: &AgentStepRequest) -> Result<AgentStepExecutionResult, String> {
    let mut logs = Vec::new();
    let started_at = Utc::now();
    logs.push(format!(
        "{} step={} started",
        started_at.to_rfc3339(),
        request.step_id
    ));

    let plan = build_execution_plan(
        &request.profile,
        &request.prompt_path,
        &request.prompt_content,
        &request.base_env,
    )
    .map_err(|err| {
        format!(
            "build agent execution plan for step {}: {err}",
            request.step_id
        )
    })?;
    logs.push(format!(
        "{} step={} command={}",
        Utc::now().to_rfc3339(),
        request.step_id,
        plan.command
    ));

    let mut command = Command::new("bash");
    command.arg("-c").arg(&plan.command);
    command.current_dir(&request.workdir);
    command.env_clear();
    for pair in &plan.env {
        if let Some((key, value)) = pair.split_once('=') {
            command.env(key, value);
        }
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if plan.stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("spawn agent step {}: {err}", request.step_id))?;
    if let Some(stdin_payload) = &plan.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_payload.as_bytes())
                .map_err(|err| format!("write stdin for step {}: {err}", request.step_id))?;
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("wait for agent step {}: {err}", request.step_id))?;
    let finished_at = Utc::now();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let success = output.status.success();
    logs.push(format!(
        "{} step={} completed exit_code={}",
        finished_at.to_rfc3339(),
        request.step_id,
        exit_code
    ));

    Ok(AgentStepExecutionResult {
        step_id: request.step_id.clone(),
        command: plan.command,
        exit_code,
        success,
        output: combine_output(&stdout, &stderr),
        stdout,
        stderr,
        started_at,
        finished_at,
        duration_ms: finished_at
            .signed_duration_since(started_at)
            .num_milliseconds(),
        logs,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashStepRequest {
    pub step_id: String,
    pub cmd: String,
    pub repo_workdir: PathBuf,
    pub workdir: String,
}

impl BashStepRequest {
    pub fn new(
        step_id: impl Into<String>,
        cmd: impl Into<String>,
        repo_workdir: PathBuf,
        workdir: impl Into<String>,
    ) -> Self {
        Self {
            step_id: step_id.into(),
            cmd: cmd.into(),
            repo_workdir,
            workdir: workdir.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashStepExecutionResult {
    pub step_id: String,
    pub cmd: String,
    pub resolved_workdir: PathBuf,
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub output: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub logs: Vec<String>,
}

pub fn execute_bash_step(request: &BashStepRequest) -> Result<BashStepExecutionResult, String> {
    let step_id = request.step_id.trim();
    if step_id.is_empty() {
        return Err("bash step id is required".to_string());
    }

    let cmd = request.cmd.trim();
    if cmd.is_empty() {
        return Err(format!("bash step {step_id} command is required"));
    }

    let resolved_workdir = resolve_bash_workdir(&request.repo_workdir, &request.workdir)?;
    ensure_bash_workdir(step_id, &resolved_workdir)?;

    let started_at = Utc::now();
    let mut logs = Vec::new();
    logs.push(format!(
        "{} step={} type=bash started",
        started_at.to_rfc3339(),
        step_id
    ));
    logs.push(format!("step={} cmd={}", step_id, cmd));
    logs.push(format!(
        "step={} workdir={}",
        step_id,
        resolved_workdir.display()
    ));

    let mut command = Command::new("bash");
    command.arg("-c").arg(cmd);
    command.current_dir(&resolved_workdir);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command
        .spawn()
        .and_then(|child| child.wait_with_output())
        .map_err(|err| {
            format!(
                "execute bash step {step_id} in {}: {err}",
                resolved_workdir.display()
            )
        })?;

    let finished_at = Utc::now();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let success = output.status.success();

    logs.push(format!(
        "{} step={} completed exit_code={}",
        finished_at.to_rfc3339(),
        step_id,
        exit_code
    ));

    Ok(BashStepExecutionResult {
        step_id: step_id.to_string(),
        cmd: cmd.to_string(),
        resolved_workdir,
        exit_code,
        success,
        output: combine_output(&stdout, &stderr),
        stdout,
        stderr,
        started_at,
        finished_at,
        duration_ms: finished_at
            .signed_duration_since(started_at)
            .num_milliseconds(),
        logs,
    })
}

pub fn append_bash_step_logs(
    store: &WorkflowRunStore,
    run_id: &str,
    result: &BashStepExecutionResult,
) -> Result<(), String> {
    for line in format_bash_step_log_lines(result) {
        store.append_step_log(run_id, &result.step_id, &line)?;
    }
    Ok(())
}

pub fn format_bash_step_log_lines(result: &BashStepExecutionResult) -> Vec<String> {
    let mut out = Vec::new();
    out.extend(result.logs.iter().cloned());
    if !result.stdout.is_empty() {
        out.push("stdout:".to_string());
        for line in result.stdout.lines() {
            out.push(format!("  {line}"));
        }
    }
    if !result.stderr.is_empty() {
        out.push("stderr:".to_string());
        for line in result.stderr.lines() {
            out.push(format!("  {line}"));
        }
    }
    out
}

fn resolve_bash_workdir(repo_workdir: &Path, raw_workdir: &str) -> Result<PathBuf, String> {
    if repo_workdir.as_os_str().is_empty() {
        return Err("repo workdir is required".to_string());
    }

    let trimmed = raw_workdir.trim();
    if trimmed.is_empty() {
        return Ok(repo_workdir.to_path_buf());
    }

    let workdir = Path::new(trimmed);
    if workdir.is_absolute() {
        return Ok(workdir.to_path_buf());
    }
    Ok(repo_workdir.join(workdir))
}

fn ensure_bash_workdir(step_id: &str, workdir: &Path) -> Result<(), String> {
    if !workdir.exists() {
        return Err(format!(
            "bash step {step_id} workdir {} does not exist",
            workdir.display()
        ));
    }
    if !workdir.is_dir() {
        return Err(format!(
            "bash step {step_id} workdir {} is not a directory",
            workdir.display()
        ));
    }
    Ok(())
}

pub fn capture_base_env() -> Vec<String> {
    std::env::vars()
        .map(|(key, value)| format!("{key}={value}"))
        .collect()
}

fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        return stderr.to_string();
    }
    if stderr.is_empty() {
        return stdout.to_string();
    }
    format!("{stdout}\n{stderr}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStepRequest {
    pub step_id: String,
    pub iteration_request: AgentStepRequest,
    pub max_iterations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopStepStopStatus {
    StopConditionMet,
    MaxIterationsReached,
    IterationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStepExecutionResult {
    pub step_id: String,
    pub iterations: u32,
    pub stop_status: LoopStepStopStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub last_exit_code: i32,
    pub logs: Vec<String>,
    pub iteration_results: Vec<AgentStepExecutionResult>,
}

pub fn execute_loop_step<F>(
    request: &LoopStepRequest,
    mut stop_condition: F,
) -> Result<LoopStepExecutionResult, String>
where
    F: FnMut(u32, &AgentStepExecutionResult) -> bool,
{
    if request.max_iterations == 0 {
        return Err("max_iterations must be greater than 0".to_string());
    }

    let started_at = Utc::now();
    let mut logs = vec![format!(
        "{} step={} loop_started max_iterations={}",
        started_at.to_rfc3339(),
        request.step_id,
        request.max_iterations
    )];
    let mut iteration_results = Vec::new();
    let mut last_exit_code = 0;

    for iteration in 1..=request.max_iterations {
        logs.push(format!(
            "{} step={} iteration={} started",
            Utc::now().to_rfc3339(),
            request.step_id,
            iteration
        ));

        let mut iteration_request = request.iteration_request.clone();
        iteration_request.step_id = format!("{}#{}", request.step_id, iteration);

        let result = execute_agent_step(&iteration_request).map_err(|err| {
            format!(
                "execute loop step {} iteration {}: {err}",
                request.step_id, iteration
            )
        })?;
        last_exit_code = result.exit_code;
        logs.push(format!(
            "{} step={} iteration={} completed exit_code={}",
            Utc::now().to_rfc3339(),
            request.step_id,
            iteration,
            result.exit_code
        ));

        let should_stop = stop_condition(iteration, &result);
        let success = result.success;
        iteration_results.push(result);
        if !success {
            let finished_at = Utc::now();
            logs.push(format!(
                "{} step={} stop_status=iteration_failed iterations={}",
                finished_at.to_rfc3339(),
                request.step_id,
                iteration
            ));
            return Ok(LoopStepExecutionResult {
                step_id: request.step_id.clone(),
                iterations: iteration,
                stop_status: LoopStepStopStatus::IterationFailed,
                started_at,
                finished_at,
                last_exit_code,
                logs,
                iteration_results,
            });
        }

        if should_stop {
            let finished_at = Utc::now();
            logs.push(format!(
                "{} step={} stop_status=stop_condition_met iterations={}",
                finished_at.to_rfc3339(),
                request.step_id,
                iteration
            ));
            return Ok(LoopStepExecutionResult {
                step_id: request.step_id.clone(),
                iterations: iteration,
                stop_status: LoopStepStopStatus::StopConditionMet,
                started_at,
                finished_at,
                last_exit_code,
                logs,
                iteration_results,
            });
        }
    }

    let finished_at = Utc::now();
    logs.push(format!(
        "{} step={} stop_status=max_iterations_reached iterations={}",
        finished_at.to_rfc3339(),
        request.step_id,
        request.max_iterations
    ));
    Ok(LoopStepExecutionResult {
        step_id: request.step_id.clone(),
        iterations: request.max_iterations,
        stop_status: LoopStepStopStatus::MaxIterationsReached,
        started_at,
        finished_at,
        last_exit_code,
        logs,
        iteration_results,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEngineStep {
    pub id: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowEngineStepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEngineStepRecord {
    pub step_id: String,
    pub status: WorkflowEngineStepStatus,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEngineRun {
    pub started_at: String,
    pub finished_at: String,
    pub ordered_step_ids: Vec<String>,
    pub steps: Vec<WorkflowEngineStepRecord>,
}

pub fn workflow_step_order(steps: &[WorkflowEngineStep]) -> Result<Vec<String>, String> {
    let mut index_by_id = std::collections::HashMap::new();
    for (index, step) in steps.iter().enumerate() {
        let id = step.id.trim();
        if id.is_empty() {
            return Err(format!("step id at index {} is empty", index + 1));
        }
        if index_by_id.insert(id.to_string(), index).is_some() {
            return Err(format!("duplicate step id {:?}", id));
        }
    }

    for step in steps {
        for dep in &step.depends_on {
            if !index_by_id.contains_key(dep) {
                return Err(format!(
                    "step {:?} has unknown dependency {:?}",
                    step.id, dep
                ));
            }
        }
    }

    let mut in_degree: std::collections::HashMap<&str, usize> = steps
        .iter()
        .map(|step| (step.id.as_str(), 0usize))
        .collect();
    let mut adjacency: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for step in steps {
        for dep in &step.depends_on {
            adjacency
                .entry(dep.as_str())
                .or_default()
                .push(step.id.as_str());
            if let Some(value) = in_degree.get_mut(step.id.as_str()) {
                *value += 1;
            }
        }
    }

    let mut ready: Vec<&str> = steps
        .iter()
        .filter_map(|step| {
            let id = step.id.as_str();
            if in_degree.get(id).copied().unwrap_or(0) == 0 {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    ready.sort_by_key(|id| index_by_id.get(*id).copied().unwrap_or(usize::MAX));

    let mut ordered = Vec::with_capacity(steps.len());
    while let Some(id) = ready.first().copied() {
        ready.remove(0);
        ordered.push(id.to_string());
        if let Some(neighbors) = adjacency.get(id) {
            for next in neighbors {
                if let Some(value) = in_degree.get_mut(next) {
                    *value = value.saturating_sub(1);
                    if *value == 0 {
                        ready.push(next);
                    }
                }
            }
            ready.sort_by_key(|candidate| {
                index_by_id.get(*candidate).copied().unwrap_or(usize::MAX)
            });
        }
    }

    if ordered.len() != steps.len() {
        return Err("cycle detected in workflow steps".to_string());
    }
    Ok(ordered)
}

pub fn execute_sequential_workflow<F>(
    steps: &[WorkflowEngineStep],
    mut execute_step: F,
) -> Result<WorkflowEngineRun, String>
where
    F: FnMut(&str) -> Result<(), String>,
{
    let ordered_step_ids = workflow_step_order(steps)?;
    let started_at = now_rfc3339();

    let mut by_id: std::collections::HashMap<&str, &WorkflowEngineStep> =
        std::collections::HashMap::new();
    let mut records: std::collections::HashMap<String, WorkflowEngineStepRecord> =
        std::collections::HashMap::new();
    for step in steps {
        by_id.insert(step.id.as_str(), step);
        records.insert(
            step.id.clone(),
            WorkflowEngineStepRecord {
                step_id: step.id.clone(),
                status: WorkflowEngineStepStatus::Pending,
                error: String::new(),
            },
        );
    }

    let mut stop_remaining = false;
    for step_id in &ordered_step_ids {
        let Some(step) = by_id.get(step_id.as_str()).copied() else {
            return Err(format!("step {:?} missing from execution index", step_id));
        };

        if stop_remaining {
            if let Some(record) = records.get_mut(step_id) {
                record.status = WorkflowEngineStepStatus::Skipped;
            }
            continue;
        }

        let dependencies_satisfied = step.depends_on.iter().all(|dep| {
            records
                .get(dep)
                .is_some_and(|record| matches!(record.status, WorkflowEngineStepStatus::Success))
        });
        if !dependencies_satisfied {
            if let Some(record) = records.get_mut(step_id) {
                record.status = WorkflowEngineStepStatus::Skipped;
            }
            continue;
        }

        if let Some(record) = records.get_mut(step_id) {
            record.status = WorkflowEngineStepStatus::Running;
        }
        match execute_step(step_id) {
            Ok(()) => {
                if let Some(record) = records.get_mut(step_id) {
                    record.status = WorkflowEngineStepStatus::Success;
                }
            }
            Err(err) => {
                if let Some(record) = records.get_mut(step_id) {
                    record.status = WorkflowEngineStepStatus::Failed;
                    record.error = err;
                }
                stop_remaining = true;
            }
        }
    }

    let steps = ordered_step_ids
        .iter()
        .filter_map(|step_id| records.get(step_id).cloned())
        .collect();
    Ok(WorkflowEngineRun {
        started_at,
        finished_at: now_rfc3339(),
        ordered_step_ids,
        steps,
    })
}

#[cfg(test)]
mod engine_tests {
    use super::{
        execute_sequential_workflow, workflow_step_order, WorkflowEngineStep,
        WorkflowEngineStepStatus,
    };

    #[test]
    fn workflow_step_order_is_deterministic_for_simple_dag() {
        let steps = vec![
            WorkflowEngineStep {
                id: "a".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "c".to_string(),
                depends_on: vec!["a".to_string()],
            },
            WorkflowEngineStep {
                id: "b".to_string(),
                depends_on: vec!["a".to_string()],
            },
            WorkflowEngineStep {
                id: "d".to_string(),
                depends_on: vec!["b".to_string(), "c".to_string()],
            },
        ];

        let ordered = match workflow_step_order(&steps) {
            Ok(value) => value,
            Err(err) => panic!("order steps: {err}"),
        };
        assert_eq!(ordered, vec!["a", "c", "b", "d"]);
    }

    #[test]
    fn workflow_failure_stops_remaining_steps() {
        let steps = vec![
            WorkflowEngineStep {
                id: "a".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "b".to_string(),
                depends_on: vec!["a".to_string()],
            },
            WorkflowEngineStep {
                id: "c".to_string(),
                depends_on: vec!["b".to_string()],
            },
        ];

        let run = match execute_sequential_workflow(&steps, |step_id| {
            if step_id == "b" {
                Err("step failed".to_string())
            } else {
                Ok(())
            }
        }) {
            Ok(value) => value,
            Err(err) => panic!("execute workflow: {err}"),
        };

        assert_eq!(run.ordered_step_ids, vec!["a", "b", "c"]);
        assert_eq!(run.steps[0].status, WorkflowEngineStepStatus::Success);
        assert_eq!(run.steps[1].status, WorkflowEngineStepStatus::Failed);
        assert_eq!(run.steps[2].status, WorkflowEngineStepStatus::Skipped);
    }

    #[test]
    fn workflow_step_order_rejects_unknown_dependency() {
        let steps = vec![WorkflowEngineStep {
            id: "a".to_string(),
            depends_on: vec!["missing".to_string()],
        }];

        let err = match workflow_step_order(&steps) {
            Ok(_) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("unknown dependency"));
    }
}

#[cfg(test)]
mod bash_step_tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        append_bash_step_logs, execute_bash_step, BashStepRequest, WorkflowRunStatus,
        WorkflowRunStore, WorkflowStepStatus,
    };

    fn temp_root(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-workflow-bash-step-{tag}-{nanos}-{}-{suffix}",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn bash_step_captures_stdout_stderr_and_exit_code() {
        let workdir = temp_root("capture");
        let _ = std::fs::create_dir_all(&workdir);

        let request = BashStepRequest::new(
            "build",
            "printf 'out'; printf 'err' >&2; exit 7",
            workdir.clone(),
            "",
        );
        let result = match execute_bash_step(&request) {
            Ok(value) => value,
            Err(err) => panic!("execute bash step: {err}"),
        };

        assert_eq!(result.step_id, "build");
        assert_eq!(result.exit_code, 7);
        assert!(!result.success);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert!(result.output.contains("out"));
        assert!(result.output.contains("err"));

        cleanup(&workdir);
    }

    #[test]
    fn bash_step_resolves_relative_workdir() {
        let root = temp_root("workdir");
        let nested = root.join("scripts");
        let _ = std::fs::create_dir_all(&nested);

        let request = BashStepRequest::new("build", "pwd", root.clone(), "scripts");
        let result = match execute_bash_step(&request) {
            Ok(value) => value,
            Err(err) => panic!("execute bash step: {err}"),
        };
        let expected = match std::fs::canonicalize(&nested) {
            Ok(value) => value,
            Err(err) => panic!("canonicalize nested: {err}"),
        };
        let reported = match std::fs::canonicalize(Path::new(result.stdout.trim())) {
            Ok(value) => value,
            Err(err) => panic!("canonicalize pwd output: {err}"),
        };

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.resolved_workdir, nested);
        assert_eq!(reported, expected);

        cleanup(&root);
    }

    #[test]
    fn bash_step_logs_are_stored_and_viewable() {
        let root = temp_root("persist");
        let repo_workdir = root.join("repo");
        let _ = std::fs::create_dir_all(&repo_workdir);

        let store = WorkflowRunStore::new(root.join("store"));
        let run = match store.create_run(
            "deploy",
            "/repo/.forge/workflows/deploy.toml",
            &["build".to_string()],
        ) {
            Ok(value) => value,
            Err(err) => panic!("create run: {err}"),
        };

        let request = BashStepRequest::new(
            "build",
            "printf 'hello'; printf 'warn' >&2; exit 3",
            repo_workdir,
            "",
        );
        let result = match execute_bash_step(&request) {
            Ok(value) => value,
            Err(err) => panic!("execute bash step: {err}"),
        };
        if let Err(err) = append_bash_step_logs(&store, &run.id, &result) {
            panic!("append bash logs: {err}");
        }
        if let Err(err) = store.update_step_status(&run.id, "build", WorkflowStepStatus::Failed) {
            panic!("set step status: {err}");
        }
        if let Err(err) = store.update_run_status(&run.id, WorkflowRunStatus::Failed) {
            panic!("set run status: {err}");
        }

        let log = match store.read_step_log(&run.id, "build") {
            Ok(value) => value,
            Err(err) => panic!("read step log: {err}"),
        };
        assert!(log.contains("stdout:"));
        assert!(log.contains("hello"));
        assert!(log.contains("stderr:"));
        assert!(log.contains("warn"));
        assert!(log.contains("exit_code=3"));

        cleanup(&root);
    }
}

#[cfg(test)]
mod loop_step_tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use forge_loop::harness_wrapper::{HarnessKind, ProfileSpec, PromptMode};

    use super::{execute_loop_step, AgentStepRequest, LoopStepRequest, LoopStepStopStatus};

    #[test]
    fn loop_step_stops_on_stop_condition_before_max_iterations() {
        let request = loop_request("printf ok", 5);
        let result = match execute_loop_step(&request, |iteration, _| iteration >= 2) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 2);
        assert_eq!(result.stop_status, LoopStepStopStatus::StopConditionMet);
        assert_eq!(result.last_exit_code, 0);
        assert_eq!(result.iteration_results.len(), 2);
    }

    #[test]
    fn loop_step_stops_at_max_iterations_when_condition_never_matches() {
        let request = loop_request("printf ok", 3);
        let result = match execute_loop_step(&request, |_iteration, _| false) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 3);
        assert_eq!(result.stop_status, LoopStepStopStatus::MaxIterationsReached);
        assert_eq!(result.last_exit_code, 0);
        assert_eq!(result.iteration_results.len(), 3);
    }

    #[test]
    fn loop_step_stops_when_iteration_fails() {
        let request = loop_request("exit 9", 4);
        let result = match execute_loop_step(&request, |_iteration, _| false) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 1);
        assert_eq!(result.stop_status, LoopStepStopStatus::IterationFailed);
        assert_eq!(result.last_exit_code, 9);
        assert_eq!(result.iteration_results.len(), 1);
    }

    fn loop_request(command_template: &str, max_iterations: u32) -> LoopStepRequest {
        let iteration_request = AgentStepRequest {
            step_id: "loop-step-iteration".to_string(),
            prompt_content: "loop prompt".to_string(),
            prompt_path: String::new(),
            profile: test_profile(command_template, Some(PromptMode::Env)),
            workdir: current_workdir(),
            base_env: Vec::new(),
        };
        LoopStepRequest {
            step_id: "loop-step".to_string(),
            iteration_request,
            max_iterations,
        }
    }

    fn current_workdir() -> PathBuf {
        match std::env::current_dir() {
            Ok(path) => path,
            Err(err) => panic!("current dir should be available for test: {err}"),
        }
    }

    fn test_profile(command_template: &str, prompt_mode: Option<PromptMode>) -> ProfileSpec {
        ProfileSpec {
            harness: HarnessKind::Other("test".to_string()),
            prompt_mode,
            command_template: command_template.to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod agent_step_tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use forge_loop::harness_wrapper::{HarnessKind, ProfileSpec, PromptMode};

    use super::{execute_agent_step, AgentStepRequest};

    #[test]
    fn agent_step_captures_stdout_stderr_and_exit_code() {
        let request = AgentStepRequest {
            step_id: "agent-1".to_string(),
            prompt_content: "hello".to_string(),
            prompt_path: String::new(),
            profile: test_profile(
                "printf 'out'; printf 'err' >&2; exit 7",
                Some(PromptMode::Env),
            ),
            workdir: current_workdir(),
            base_env: Vec::new(),
        };

        let result = match execute_agent_step(&request) {
            Ok(value) => value,
            Err(err) => panic!("execute agent step: {err}"),
        };

        assert_eq!(result.exit_code, 7);
        assert!(!result.success);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert!(result.output.contains("out"));
        assert!(result.output.contains("err"));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("step=agent-1 started")));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("step=agent-1 completed exit_code=7")));
    }

    #[test]
    fn agent_step_writes_prompt_to_stdin_when_requested() {
        let request = AgentStepRequest {
            step_id: "agent-stdin".to_string(),
            prompt_content: "stdin payload".to_string(),
            prompt_path: String::new(),
            profile: test_profile("cat", Some(PromptMode::Stdin)),
            workdir: current_workdir(),
            base_env: Vec::new(),
        };

        let result = match execute_agent_step(&request) {
            Ok(value) => value,
            Err(err) => panic!("execute agent step: {err}"),
        };

        assert_eq!(result.exit_code, 0);
        assert!(result.success);
        assert_eq!(result.stdout, "stdin payload");
        assert!(result.stderr.is_empty());
        assert_eq!(result.output, "stdin payload");
    }

    #[test]
    fn agent_step_returns_error_for_invalid_path_prompt_mode() {
        let request = AgentStepRequest {
            step_id: "agent-path".to_string(),
            prompt_content: "ignored".to_string(),
            prompt_path: String::new(),
            profile: test_profile("echo noop", Some(PromptMode::Path)),
            workdir: current_workdir(),
            base_env: Vec::new(),
        };

        let err = match execute_agent_step(&request) {
            Ok(_) => panic!("expected error"),
            Err(err) => err,
        };
        assert!(err.contains("prompt path is required for path mode"));
    }

    fn current_workdir() -> PathBuf {
        match std::env::current_dir() {
            Ok(path) => path,
            Err(err) => panic!("current dir should be available for test: {err}"),
        }
    }

    fn test_profile(command_template: &str, prompt_mode: Option<PromptMode>) -> ProfileSpec {
        ProfileSpec {
            harness: HarnessKind::Other("test".to_string()),
            prompt_mode,
            command_template: command_template.to_string(),
            extra_args: Vec::new(),
            auth_home: String::new(),
            env: BTreeMap::new(),
        }
    }
}

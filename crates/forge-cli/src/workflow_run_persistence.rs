use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::{DateTime, Utc};
use forge_loop::harness_wrapper::{build_execution_plan, ProfileSpec};
use forge_loop::stop_rules::{self, StopToolSpec};
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
    WaitingApproval,
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
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepApprovalState {
    Pending,
    Approved,
    Rejected,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowStepApproval {
    pub state: WorkflowStepApprovalState,
    pub requested_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowStepRun {
    pub step_id: String,
    pub status: WorkflowStepStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub outputs: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<WorkflowStepApproval>,
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
                outputs: HashMap::new(),
                approval: None,
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

    pub fn mark_step_waiting_approval(
        &self,
        run_id: &str,
        step_id: &str,
        timeout_at: Option<String>,
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

        let now = now_rfc3339();
        if step.started_at.is_none() {
            step.started_at = Some(now.clone());
        }
        step.status = WorkflowStepStatus::WaitingApproval;
        step.approval = Some(WorkflowStepApproval {
            state: WorkflowStepApprovalState::Pending,
            requested_at: now,
            decided_at: None,
            timeout_at,
        });
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn update_step_outputs(
        &self,
        run_id: &str,
        step_id: &str,
        outputs: HashMap<String, String>,
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

        step.outputs = outputs;
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn approve_step_waiting_approval(
        &self,
        run_id: &str,
        step_id: &str,
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
        if !matches!(step.status, WorkflowStepStatus::WaitingApproval) {
            return Err(format!(
                "workflow step {:?} is not waiting approval",
                trimmed_step
            ));
        }

        let Some(approval) = step.approval.as_mut() else {
            return Err(format!(
                "workflow step {:?} has no approval metadata",
                trimmed_step
            ));
        };

        let now = now_rfc3339();
        approval.state = WorkflowStepApprovalState::Approved;
        approval.decided_at = Some(now.clone());
        step.status = WorkflowStepStatus::Success;
        if step.started_at.is_none() {
            step.started_at = Some(now.clone());
        }
        step.finished_at = Some(now);
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn deny_step_waiting_approval(
        &self,
        run_id: &str,
        step_id: &str,
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
        if !matches!(step.status, WorkflowStepStatus::WaitingApproval) {
            return Err(format!(
                "workflow step {:?} is not waiting approval",
                trimmed_step
            ));
        }

        let Some(approval) = step.approval.as_mut() else {
            return Err(format!(
                "workflow step {:?} has no approval metadata",
                trimmed_step
            ));
        };

        let now = now_rfc3339();
        approval.state = WorkflowStepApprovalState::Rejected;
        approval.decided_at = Some(now.clone());
        step.status = WorkflowStepStatus::Failed;
        if step.started_at.is_none() {
            step.started_at = Some(now.clone());
        }
        step.finished_at = Some(now);
        run.updated_at = now_rfc3339();
        self.write_run(&run)?;
        Ok(run)
    }

    pub fn decide_step_approval(
        &self,
        run_id: &str,
        step_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<WorkflowRunRecord, String> {
        let mut run = self.get_run(run_id)?;
        let trimmed_step = step_id.trim();
        if trimmed_step.is_empty() {
            return Err("step id is required".to_string());
        }

        let now = now_rfc3339();
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

        if step.status != WorkflowStepStatus::WaitingApproval {
            return Err(format!(
                "step {} is not waiting for approval (status={})",
                trimmed_step,
                step_status_label(&step.status)
            ));
        }

        let Some(approval) = step.approval.as_mut() else {
            return Err(format!(
                "step {} missing approval metadata while waiting_approval",
                trimmed_step
            ));
        };
        if approval.state != WorkflowStepApprovalState::Pending {
            return Err(format!(
                "step {} approval already decided (state={:?})",
                trimmed_step, approval.state
            ));
        }

        approval.state = if approved {
            WorkflowStepApprovalState::Approved
        } else {
            WorkflowStepApprovalState::Rejected
        };
        approval.decided_at = Some(now.clone());
        step.status = if approved {
            WorkflowStepStatus::Success
        } else {
            WorkflowStepStatus::Failed
        };
        step.finished_at = Some(now.clone());

        if approved {
            let has_failed = run.steps.iter().any(|entry| {
                matches!(
                    entry.status,
                    WorkflowStepStatus::Failed | WorkflowStepStatus::Canceled
                )
            });
            let has_remaining = run.steps.iter().any(|entry| !entry.status.is_terminal());
            run.status = if has_failed {
                WorkflowRunStatus::Failed
            } else if has_remaining {
                WorkflowRunStatus::Running
            } else {
                WorkflowRunStatus::Success
            };
            run.finished_at = if run.status.is_terminal() {
                Some(now.clone())
            } else {
                None
            };
        } else {
            for entry in run.steps.iter_mut().filter(|entry| {
                entry.step_id != trimmed_step
                    && matches!(
                        entry.status,
                        WorkflowStepStatus::Pending
                            | WorkflowStepStatus::Running
                            | WorkflowStepStatus::WaitingApproval
                    )
            }) {
                entry.status = WorkflowStepStatus::Skipped;
                if entry.started_at.is_none() {
                    entry.started_at = Some(now.clone());
                }
                entry.finished_at = Some(now.clone());
                if let Some(existing) = entry.approval.as_mut() {
                    if existing.state == WorkflowStepApprovalState::Pending {
                        existing.state = WorkflowStepApprovalState::Rejected;
                        existing.decided_at = Some(now.clone());
                    }
                }
            }
            run.status = WorkflowRunStatus::Failed;
            run.finished_at = Some(now.clone());
        }

        run.updated_at = now;
        self.write_run(&run)?;

        let audit_line = if approved {
            "approval: approved via forge workflow approve".to_string()
        } else {
            let suffix = reason
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!(" reason={value}"))
                .unwrap_or_default();
            format!("approval: denied via forge workflow deny{suffix}")
        };
        self.append_step_log(run_id, trimmed_step, &audit_line)?;

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

pub fn append_workflow_ledger_entry(
    store: &WorkflowRunStore,
    run_id: &str,
    repo_root: &Path,
) -> Result<PathBuf, String> {
    let run = store.get_run(run_id)?;
    if repo_root.as_os_str().is_empty() {
        return Err("repo root is required".to_string());
    }

    let ledgers_dir = repo_root.join(".forge").join("ledgers");
    ensure_dir(&ledgers_dir)?;
    let ledger_path = ledgers_dir.join(format!(
        "workflow-{}.md",
        sanitize_segment(&run.workflow_name)
    ));

    let should_write_header = match fs::metadata(&ledger_path) {
        Ok(meta) => meta.len() == 0,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(err) => {
            return Err(format!(
                "stat workflow ledger {}: {err}",
                ledger_path.display()
            ))
        }
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .map_err(|err| format!("open workflow ledger {}: {err}", ledger_path.display()))?;

    if should_write_header {
        file.write_all(b"# Workflow Run Ledger\n\n")
            .map_err(|err| {
                format!(
                    "write workflow ledger header {}: {err}",
                    ledger_path.display()
                )
            })?;
    }

    let finished_at = run.finished_at.as_deref().unwrap_or("-");
    let mut entry = String::new();
    entry.push_str("## Workflow Run\n");
    entry.push_str(format!("- recorded_at: {}\n", now_rfc3339()).as_str());
    entry.push_str(format!("- run_id: {}\n", run.id).as_str());
    entry.push_str(format!("- workflow_name: {}\n", run.workflow_name).as_str());
    if !run.workflow_source.trim().is_empty() {
        entry.push_str(format!("- workflow_source: {}\n", run.workflow_source).as_str());
    }
    entry.push_str(format!("- status: {}\n", run_status_label(&run.status)).as_str());
    entry.push_str(format!("- started_at: {}\n", run.started_at).as_str());
    entry.push_str(format!("- finished_at: {}\n", finished_at).as_str());
    entry.push_str(format!("- step_count: {}\n", run.steps.len()).as_str());
    entry.push_str("- steps:\n");
    for step in &run.steps {
        let duration = step_duration_ms(step);
        let error = step_error_for_ledger(store, &run.id, &step.step_id);
        let error_suffix = if error.is_empty() {
            String::new()
        } else {
            format!(" error={error}")
        };
        entry.push_str(
            format!(
                "  - {} [bash] status={} duration_ms={}{}\n",
                step.step_id,
                step_status_label(&step.status),
                duration,
                error_suffix
            )
            .as_str(),
        );
    }
    entry.push('\n');

    file.write_all(entry.as_bytes()).map_err(|err| {
        format!(
            "write workflow ledger entry {}: {err}",
            ledger_path.display()
        )
    })?;

    Ok(ledger_path)
}

fn run_status_label(status: &WorkflowRunStatus) -> &'static str {
    match status {
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Success => "success",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Canceled => "canceled",
    }
}

fn step_status_label(status: &WorkflowStepStatus) -> &'static str {
    match status {
        WorkflowStepStatus::Pending => "pending",
        WorkflowStepStatus::Running => "running",
        WorkflowStepStatus::WaitingApproval => "waiting_approval",
        WorkflowStepStatus::Success => "success",
        WorkflowStepStatus::Failed => "failed",
        WorkflowStepStatus::Skipped => "skipped",
        WorkflowStepStatus::Canceled => "canceled",
    }
}

fn step_duration_ms(step: &WorkflowStepRun) -> String {
    let Some(started_at) = step.started_at.as_deref() else {
        return "-".to_string();
    };
    let Some(finished_at) = step.finished_at.as_deref() else {
        return "-".to_string();
    };

    let started_at = DateTime::parse_from_rfc3339(started_at);
    let finished_at = DateTime::parse_from_rfc3339(finished_at);
    match (started_at, finished_at) {
        (Ok(started), Ok(finished)) => {
            let duration = finished.signed_duration_since(started).num_milliseconds();
            duration.max(0).to_string()
        }
        _ => "-".to_string(),
    }
}

fn step_error_for_ledger(store: &WorkflowRunStore, run_id: &str, step_id: &str) -> String {
    let log = match store.read_step_log(run_id, step_id) {
        Ok(content) => content,
        Err(_) => return String::new(),
    };
    for line in log.lines().rev() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("error:") {
            return value.trim().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        append_workflow_ledger_entry, WorkflowRunStatus, WorkflowRunStore,
        WorkflowStepApprovalState, WorkflowStepStatus,
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
    fn waiting_approval_state_persists_and_resume_state_keeps_pending_work() {
        let root = temp_root("waiting-approval");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "approval-demo",
                "/repo/.forge/workflows/approval-demo.toml",
                &["review".to_string(), "ship".to_string()],
            )
            .expect("create run");

        let timeout_at = "2099-01-01T00:00:00Z".to_string();
        let run = store
            .mark_step_waiting_approval(&run.id, "review", Some(timeout_at.clone()))
            .expect("mark waiting approval");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::WaitingApproval);
        let approval = run.steps[0]
            .approval
            .as_ref()
            .expect("approval metadata should be present");
        assert_eq!(approval.state, WorkflowStepApprovalState::Pending);
        assert!(approval.decided_at.is_none());
        assert_eq!(approval.timeout_at.as_deref(), Some(timeout_at.as_str()));
        assert!(!approval.requested_at.is_empty());

        let reloaded = store.get_run(&run.id).expect("reload");
        assert_eq!(
            reloaded.steps[0].status,
            WorkflowStepStatus::WaitingApproval
        );
        assert!(reloaded.steps[0].approval.is_some());

        let resume = store.load_resume_state(&run.id).expect("resume");
        assert_eq!(resume.remaining_step_ids, vec!["review", "ship"]);

        cleanup(&root);
    }

    #[test]
    fn approve_waiting_approval_marks_step_success_and_decision_timestamp() {
        let root = temp_root("approve-waiting");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "approval-approve",
                "/repo/.forge/workflows/approval-approve.toml",
                &["review".to_string()],
            )
            .expect("create run");
        let run = store
            .mark_step_waiting_approval(&run.id, "review", None)
            .expect("mark waiting");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::WaitingApproval);

        let run = store
            .approve_step_waiting_approval(&run.id, "review")
            .expect("approve waiting");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Success);
        let approval = run.steps[0].approval.as_ref().expect("approval metadata");
        assert_eq!(approval.state, WorkflowStepApprovalState::Approved);
        assert!(approval.decided_at.is_some());

        cleanup(&root);
    }

    #[test]
    fn deny_waiting_approval_marks_step_failed_and_decision_timestamp() {
        let root = temp_root("deny-waiting");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "approval-deny",
                "/repo/.forge/workflows/approval-deny.toml",
                &["review".to_string()],
            )
            .expect("create run");
        let run = store
            .mark_step_waiting_approval(&run.id, "review", None)
            .expect("mark waiting");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::WaitingApproval);

        let run = store
            .deny_step_waiting_approval(&run.id, "review")
            .expect("deny waiting");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Failed);
        let approval = run.steps[0].approval.as_ref().expect("approval metadata");
        assert_eq!(approval.state, WorkflowStepApprovalState::Rejected);
        assert!(approval.decided_at.is_some());

        cleanup(&root);
    }

    #[test]
    fn approve_waiting_step_marks_step_success_and_keeps_run_running() {
        let root = temp_root("approve-waiting");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "approval-demo",
                "/repo/.forge/workflows/approval-demo.toml",
                &["review".to_string(), "ship".to_string()],
            )
            .expect("create run");
        let run = store
            .mark_step_waiting_approval(&run.id, "review", Some("2099-01-01T00:00:00Z".to_owned()))
            .expect("mark waiting approval");

        let run = store
            .decide_step_approval(&run.id, "review", true, None)
            .expect("approve step");
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Success);
        let approval = run.steps[0].approval.as_ref().expect("approval metadata");
        assert_eq!(approval.state, WorkflowStepApprovalState::Approved);
        assert!(approval.decided_at.is_some());
        assert_eq!(run.status, WorkflowRunStatus::Running);
        assert_eq!(run.steps[1].status, WorkflowStepStatus::Pending);

        let log = store
            .read_step_log(&run.id, "review")
            .expect("read review log");
        assert!(log.contains("approval: approved via forge workflow approve"));
        cleanup(&root);
    }

    #[test]
    fn deny_waiting_step_marks_run_failed_and_skips_remaining_steps() {
        let root = temp_root("deny-waiting");
        let store = WorkflowRunStore::new(root.clone());
        let run = store
            .create_run(
                "approval-demo",
                "/repo/.forge/workflows/approval-demo.toml",
                &["review".to_string(), "ship".to_string()],
            )
            .expect("create run");
        let run = store
            .mark_step_waiting_approval(&run.id, "review", Some("2099-01-01T00:00:00Z".to_owned()))
            .expect("mark waiting approval");

        let run = store
            .decide_step_approval(&run.id, "review", false, Some("insufficient evidence"))
            .expect("deny step");
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert_eq!(run.steps[0].status, WorkflowStepStatus::Failed);
        assert_eq!(run.steps[1].status, WorkflowStepStatus::Skipped);
        let approval = run.steps[0].approval.as_ref().expect("approval metadata");
        assert_eq!(approval.state, WorkflowStepApprovalState::Rejected);
        assert!(approval.decided_at.is_some());

        let log = store
            .read_step_log(&run.id, "review")
            .expect("read review log");
        assert!(
            log.contains("approval: denied via forge workflow deny reason=insufficient evidence")
        );
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
            repo_root
                .join(".forge/workflows/deploy.toml")
                .to_string_lossy()
                .as_ref(),
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
        assert!(text.contains("- step_count: 2"));
        assert!(text.contains("- plan [bash] status=success duration_ms="));
        assert!(text.contains("- ship [bash] status=failed duration_ms="));

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
    pub extra_env: Vec<(String, String)>,
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
            extra_env: Vec::new(),
        }
    }

    pub fn with_extra_env(mut self, extra_env: Vec<(String, String)>) -> Self {
        self.extra_env = extra_env;
        self
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
    for (key, value) in &request.extra_env {
        command.env(key, value);
    }
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
pub struct LoopStepStopCondition {
    pub expr: String,
    pub tool: Option<StopToolSpec>,
    pub has_llm_condition: bool,
    pub tool_timeout: Duration,
}

impl Default for LoopStepStopCondition {
    fn default() -> Self {
        Self {
            expr: String::new(),
            tool: None,
            has_llm_condition: false,
            tool_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStepStopEvaluation {
    pub should_stop: bool,
    pub reason: String,
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
    pub stop_reason: String,
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
    F: FnMut(u32, &AgentStepExecutionResult) -> Result<LoopStepStopEvaluation, String>,
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

        let stop_evaluation = stop_condition(iteration, &result).map_err(|err| {
            format!(
                "evaluate loop step {} stop condition at iteration {}: {err}",
                request.step_id, iteration
            )
        })?;
        let success = result.success;
        iteration_results.push(result);
        if !success {
            let finished_at = Utc::now();
            let stop_reason = format!("iteration {iteration} failed");
            logs.push(format!(
                "{} step={} stop_status=iteration_failed iterations={} reason={}",
                finished_at.to_rfc3339(),
                request.step_id,
                iteration,
                stop_reason
            ));
            return Ok(LoopStepExecutionResult {
                step_id: request.step_id.clone(),
                iterations: iteration,
                stop_status: LoopStepStopStatus::IterationFailed,
                stop_reason,
                started_at,
                finished_at,
                last_exit_code,
                logs,
                iteration_results,
            });
        }

        if stop_evaluation.should_stop {
            let finished_at = Utc::now();
            let stop_reason = if stop_evaluation.reason.trim().is_empty() {
                "stop condition matched".to_string()
            } else {
                stop_evaluation.reason
            };
            logs.push(format!(
                "{} step={} stop_status=stop_condition_met iterations={} reason={}",
                finished_at.to_rfc3339(),
                request.step_id,
                iteration,
                stop_reason
            ));
            return Ok(LoopStepExecutionResult {
                step_id: request.step_id.clone(),
                iterations: iteration,
                stop_status: LoopStepStopStatus::StopConditionMet,
                stop_reason,
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
        stop_reason: format!("max iterations {} reached", request.max_iterations),
        started_at,
        finished_at,
        last_exit_code,
        logs,
        iteration_results,
    })
}

pub fn execute_loop_step_with_stop_condition<F>(
    request: &LoopStepRequest,
    stop_condition: &LoopStepStopCondition,
    mut tasks_open_provider: F,
) -> Result<LoopStepExecutionResult, String>
where
    F: FnMut() -> Result<i64, String>,
{
    let stop_condition = stop_condition.clone();
    let workdir = request.iteration_request.workdir.clone();
    execute_loop_step(request, move |_iteration, _result| {
        evaluate_loop_stop_condition(&workdir, &stop_condition, &mut tasks_open_provider)
    })
}

pub fn evaluate_loop_stop_condition<F>(
    workdir: &Path,
    stop_condition: &LoopStepStopCondition,
    tasks_open_provider: &mut F,
) -> Result<LoopStepStopEvaluation, String>
where
    F: FnMut() -> Result<i64, String>,
{
    if stop_condition.has_llm_condition {
        return Err("stop.llm is not supported yet".to_string());
    }

    let expr = stop_condition.expr.trim();
    if !expr.is_empty() {
        let tasks_open = tasks_open_provider()
            .map_err(|err| format!("resolve count(tasks.open) for stop.expr: {err}"))?;
        let expr_ctx = stop_rules::StopExprContext { tasks_open };
        let matched = stop_rules::eval_stop_expr_text(expr, &expr_ctx)
            .map_err(|err| format!("evaluate stop.expr {:?}: {err}", expr))?;
        if matched {
            return Ok(LoopStepStopEvaluation {
                should_stop: true,
                reason: format!("stop.expr matched: {expr} (tasks_open={tasks_open})"),
            });
        }
    }

    if let Some(tool) = &stop_condition.tool {
        let timeout = if stop_condition.tool_timeout > Duration::ZERO {
            stop_condition.tool_timeout
        } else {
            Duration::from_secs(30)
        };
        let result = stop_rules::run_stop_tool(workdir, tool, timeout).map_err(|err| {
            format!(
                "evaluate stop.tool {}: {err}",
                format_stop_tool_command(tool)
            )
        })?;
        if result.should_stop {
            let source = match result.decision_source {
                stop_rules::StopToolDecisionSource::Output => "output",
                stop_rules::StopToolDecisionSource::ExitStatus => "exit_status",
            };
            return Ok(LoopStepStopEvaluation {
                should_stop: true,
                reason: format!(
                    "stop.tool matched via {source}: {} (exit_code={})",
                    result.command, result.exit_code
                ),
            });
        }
    }

    Ok(LoopStepStopEvaluation {
        should_stop: false,
        reason: String::new(),
    })
}

fn format_stop_tool_command(tool: &StopToolSpec) -> String {
    if tool.args.is_empty() {
        return tool.name.clone();
    }
    format!("{} {}", tool.name, tool.args.join(" "))
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
    WaitingApproval,
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowEngineStepResult {
    Success,
    WaitingApproval,
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
    pub paused: bool,
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
    F: FnMut(&str) -> Result<WorkflowEngineStepResult, String>,
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
            Ok(WorkflowEngineStepResult::Success) => {
                if let Some(record) = records.get_mut(step_id) {
                    record.status = WorkflowEngineStepStatus::Success;
                }
            }
            Ok(WorkflowEngineStepResult::WaitingApproval) => {
                if let Some(record) = records.get_mut(step_id) {
                    record.status = WorkflowEngineStepStatus::WaitingApproval;
                }
                stop_remaining = true;
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

    let steps: Vec<WorkflowEngineStepRecord> = ordered_step_ids
        .iter()
        .filter_map(|step_id| records.get(step_id).cloned())
        .collect();
    Ok(WorkflowEngineRun {
        started_at,
        finished_at: now_rfc3339(),
        paused: steps
            .iter()
            .any(|step| matches!(step.status, WorkflowEngineStepStatus::WaitingApproval)),
        ordered_step_ids,
        steps,
    })
}

pub fn execute_parallel_workflow<F>(
    steps: &[WorkflowEngineStep],
    max_parallel: usize,
    execute_step: F,
) -> Result<WorkflowEngineRun, String>
where
    F: Fn(&str) -> Result<WorkflowEngineStepResult, String> + Sync + Send,
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

    let concurrency_limit = max_parallel.max(1);
    let execute_step = &execute_step;
    let mut pause_requested = false;
    std::thread::scope(|scope| -> Result<(), String> {
        let (tx, rx) =
            std::sync::mpsc::channel::<(String, Result<WorkflowEngineStepResult, String>)>();
        let mut running_count = 0usize;

        loop {
            // Any step with a failed/skipped dependency can never become ready.
            for step_id in &ordered_step_ids {
                let step = by_id
                    .get(step_id.as_str())
                    .copied()
                    .ok_or_else(|| format!("step {:?} missing from execution index", step_id))?;
                let current_status = records
                    .get(step_id)
                    .map(|record| record.status.clone())
                    .ok_or_else(|| format!("step {:?} missing from status index", step_id))?;
                if !matches!(current_status, WorkflowEngineStepStatus::Pending) {
                    continue;
                }

                let blocked_by_failure = step.depends_on.iter().any(|dep| {
                    records.get(dep).is_some_and(|dep_record| {
                        matches!(
                            dep_record.status,
                            WorkflowEngineStepStatus::Failed | WorkflowEngineStepStatus::Skipped
                        )
                    })
                });
                if blocked_by_failure {
                    if let Some(record) = records.get_mut(step_id) {
                        record.status = WorkflowEngineStepStatus::Skipped;
                    }
                }
            }

            // Launch currently ready steps up to concurrency limit.
            if !pause_requested {
                for step_id in &ordered_step_ids {
                    if running_count >= concurrency_limit {
                        break;
                    }

                    let step = by_id.get(step_id.as_str()).copied().ok_or_else(|| {
                        format!("step {:?} missing from execution index", step_id)
                    })?;
                    let current_status = records
                        .get(step_id)
                        .map(|record| record.status.clone())
                        .ok_or_else(|| {
                        format!("step {:?} missing from status index", step_id)
                    })?;
                    if !matches!(current_status, WorkflowEngineStepStatus::Pending) {
                        continue;
                    }

                    let dependencies_ready = step.depends_on.iter().all(|dep| {
                        records.get(dep).is_some_and(|record| {
                            matches!(record.status, WorkflowEngineStepStatus::Success)
                        })
                    });
                    if !dependencies_ready {
                        continue;
                    }

                    if let Some(record) = records.get_mut(step_id) {
                        record.status = WorkflowEngineStepStatus::Running;
                    }
                    running_count += 1;

                    let tx = tx.clone();
                    let step_id = step_id.clone();
                    scope.spawn(move || {
                        let result = execute_step(&step_id);
                        let _ = tx.send((step_id, result));
                    });
                }
            }

            let has_pending_or_running = ordered_step_ids.iter().any(|step_id| {
                records.get(step_id).is_some_and(|record| {
                    matches!(
                        record.status,
                        WorkflowEngineStepStatus::Pending | WorkflowEngineStepStatus::Running
                    )
                })
            });
            if !has_pending_or_running {
                break;
            }

            if running_count == 0 {
                if pause_requested {
                    break;
                }
                for step_id in &ordered_step_ids {
                    if let Some(record) = records.get_mut(step_id) {
                        if matches!(record.status, WorkflowEngineStepStatus::Pending) {
                            record.status = WorkflowEngineStepStatus::Skipped;
                        }
                    }
                }
                break;
            }

            let (step_id, result) = rx
                .recv()
                .map_err(|err| format!("receive step completion: {err}"))?;
            running_count = running_count.saturating_sub(1);
            match result {
                Ok(WorkflowEngineStepResult::Success) => {
                    if let Some(record) = records.get_mut(&step_id) {
                        record.status = WorkflowEngineStepStatus::Success;
                    }
                }
                Ok(WorkflowEngineStepResult::WaitingApproval) => {
                    if let Some(record) = records.get_mut(&step_id) {
                        record.status = WorkflowEngineStepStatus::WaitingApproval;
                    }
                    pause_requested = true;
                }
                Err(err) => {
                    if let Some(record) = records.get_mut(&step_id) {
                        record.status = WorkflowEngineStepStatus::Failed;
                        record.error = err;
                    }
                }
            }
        }
        Ok(())
    })?;

    let steps = ordered_step_ids
        .iter()
        .filter_map(|step_id| records.get(step_id).cloned())
        .collect();
    Ok(WorkflowEngineRun {
        started_at,
        finished_at: now_rfc3339(),
        paused: pause_requested,
        ordered_step_ids,
        steps,
    })
}

#[cfg(test)]
mod engine_tests {
    use super::{
        execute_parallel_workflow, execute_sequential_workflow, workflow_step_order,
        WorkflowEngineStep, WorkflowEngineStepRecord, WorkflowEngineStepResult,
        WorkflowEngineStepStatus,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn step_record<'a>(
        steps: &'a [WorkflowEngineStepRecord],
        step_id: &str,
    ) -> &'a WorkflowEngineStepRecord {
        steps
            .iter()
            .find(|step| step.step_id == step_id)
            .unwrap_or_else(|| panic!("missing step record: {step_id}"))
    }

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
    fn parallel_workflow_runs_independent_steps_concurrently() {
        let steps = vec![
            WorkflowEngineStep {
                id: "a".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "b".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "c".to_string(),
                depends_on: vec!["a".to_string(), "b".to_string()],
            },
        ];
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);

        let run = execute_parallel_workflow(&steps, 2, |_step_id| {
            let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
            loop {
                let prev = peak.load(Ordering::SeqCst);
                if now_active <= prev {
                    break;
                }
                if peak
                    .compare_exchange(prev, now_active, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(40));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(WorkflowEngineStepResult::Success)
        })
        .unwrap_or_else(|err| panic!("execute parallel workflow: {err}"));

        assert_eq!(
            step_record(&run.steps, "a").status,
            WorkflowEngineStepStatus::Success
        );
        assert_eq!(
            step_record(&run.steps, "b").status,
            WorkflowEngineStepStatus::Success
        );
        assert_eq!(
            step_record(&run.steps, "c").status,
            WorkflowEngineStepStatus::Success
        );
        assert!(
            peak.load(Ordering::SeqCst) >= 2,
            "expected at least 2 concurrent steps, peak={}",
            peak.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn parallel_workflow_respects_concurrency_limit() {
        let steps = vec![
            WorkflowEngineStep {
                id: "a".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "b".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "c".to_string(),
                depends_on: vec![],
            },
        ];
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);

        let run = execute_parallel_workflow(&steps, 2, |_step_id| {
            let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
            loop {
                let prev = peak.load(Ordering::SeqCst);
                if now_active <= prev {
                    break;
                }
                if peak
                    .compare_exchange(prev, now_active, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(40));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(WorkflowEngineStepResult::Success)
        })
        .unwrap_or_else(|err| panic!("execute parallel workflow: {err}"));

        assert_eq!(run.steps.len(), 3);
        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "expected max 2 concurrent steps, peak={}",
            peak.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn parallel_workflow_failure_stops_only_dependents() {
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
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "d".to_string(),
                depends_on: vec!["c".to_string()],
            },
            WorkflowEngineStep {
                id: "e".to_string(),
                depends_on: vec!["a".to_string(), "c".to_string()],
            },
        ];

        let run = execute_parallel_workflow(&steps, 2, |step_id| {
            if step_id == "a" {
                Err("step failed".to_string())
            } else {
                Ok(WorkflowEngineStepResult::Success)
            }
        })
        .unwrap_or_else(|err| panic!("execute parallel workflow: {err}"));

        assert_eq!(
            step_record(&run.steps, "a").status,
            WorkflowEngineStepStatus::Failed
        );
        assert_eq!(
            step_record(&run.steps, "b").status,
            WorkflowEngineStepStatus::Skipped
        );
        assert_eq!(
            step_record(&run.steps, "c").status,
            WorkflowEngineStepStatus::Success
        );
        assert_eq!(
            step_record(&run.steps, "d").status,
            WorkflowEngineStepStatus::Success
        );
        assert_eq!(
            step_record(&run.steps, "e").status,
            WorkflowEngineStepStatus::Skipped
        );
    }

    #[test]
    fn parallel_workflow_zero_limit_defaults_to_one() {
        let steps = vec![
            WorkflowEngineStep {
                id: "a".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "b".to_string(),
                depends_on: vec![],
            },
        ];
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);

        let run = execute_parallel_workflow(&steps, 0, |_step_id| {
            let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
            loop {
                let prev = peak.load(Ordering::SeqCst);
                if now_active <= prev {
                    break;
                }
                if peak
                    .compare_exchange(prev, now_active, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(WorkflowEngineStepResult::Success)
        })
        .unwrap_or_else(|err| panic!("execute parallel workflow: {err}"));

        assert_eq!(run.steps.len(), 2);
        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "expected serialized execution with zero limit default"
        );
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
                Ok(WorkflowEngineStepResult::Success)
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
    fn parallel_workflow_waiting_approval_pauses_and_preserves_pending_steps() {
        let steps = vec![
            WorkflowEngineStep {
                id: "plan".to_string(),
                depends_on: vec![],
            },
            WorkflowEngineStep {
                id: "approve".to_string(),
                depends_on: vec!["plan".to_string()],
            },
            WorkflowEngineStep {
                id: "ship".to_string(),
                depends_on: vec!["approve".to_string()],
            },
        ];

        let run = execute_parallel_workflow(&steps, 2, |step_id| {
            if step_id == "approve" {
                Ok(WorkflowEngineStepResult::WaitingApproval)
            } else {
                Ok(WorkflowEngineStepResult::Success)
            }
        })
        .unwrap_or_else(|err| panic!("execute parallel workflow: {err}"));

        assert!(run.paused);
        assert_eq!(
            step_record(&run.steps, "plan").status,
            WorkflowEngineStepStatus::Success
        );
        assert_eq!(
            step_record(&run.steps, "approve").status,
            WorkflowEngineStepStatus::WaitingApproval
        );
        assert_eq!(
            step_record(&run.steps, "ship").status,
            WorkflowEngineStepStatus::Pending
        );
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
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use forge_loop::harness_wrapper::{HarnessKind, ProfileSpec, PromptMode};
    use forge_loop::stop_rules::StopToolSpec;

    use super::{
        evaluate_loop_stop_condition, execute_loop_step, execute_loop_step_with_stop_condition,
        AgentStepRequest, LoopStepRequest, LoopStepStopCondition, LoopStepStopEvaluation,
        LoopStepStopStatus,
    };

    #[test]
    fn loop_step_stops_on_stop_condition_before_max_iterations() {
        let request = loop_request("printf ok", 5);
        let result = match execute_loop_step(&request, |iteration, _| {
            Ok(LoopStepStopEvaluation {
                should_stop: iteration >= 2,
                reason: if iteration >= 2 {
                    "iteration threshold reached".to_string()
                } else {
                    String::new()
                },
            })
        }) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 2);
        assert_eq!(result.stop_status, LoopStepStopStatus::StopConditionMet);
        assert!(result.stop_reason.contains("threshold"));
        assert_eq!(result.last_exit_code, 0);
        assert_eq!(result.iteration_results.len(), 2);
    }

    #[test]
    fn loop_step_stops_at_max_iterations_when_condition_never_matches() {
        let request = loop_request("printf ok", 3);
        let result = match execute_loop_step(&request, |_iteration, _| {
            Ok(LoopStepStopEvaluation {
                should_stop: false,
                reason: String::new(),
            })
        }) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 3);
        assert_eq!(result.stop_status, LoopStepStopStatus::MaxIterationsReached);
        assert!(result.stop_reason.contains("max iterations"));
        assert_eq!(result.last_exit_code, 0);
        assert_eq!(result.iteration_results.len(), 3);
    }

    #[test]
    fn loop_step_stops_when_iteration_fails() {
        let request = loop_request("exit 9", 4);
        let result = match execute_loop_step(&request, |_iteration, _| {
            Ok(LoopStepStopEvaluation {
                should_stop: false,
                reason: String::new(),
            })
        }) {
            Ok(value) => value,
            Err(err) => panic!("execute loop step: {err}"),
        };

        assert_eq!(result.iterations, 1);
        assert_eq!(result.stop_status, LoopStepStopStatus::IterationFailed);
        assert!(result.stop_reason.contains("failed"));
        assert_eq!(result.last_exit_code, 9);
        assert_eq!(result.iteration_results.len(), 1);
    }

    #[test]
    fn loop_step_expr_condition_stops_and_records_reason() {
        let request = loop_request("printf ok", 5);
        let stop_condition = LoopStepStopCondition {
            expr: "count(tasks.open) == 0".to_string(),
            ..LoopStepStopCondition::default()
        };
        let result =
            match execute_loop_step_with_stop_condition(&request, &stop_condition, || Ok(0)) {
                Ok(result) => result,
                Err(err) => panic!("loop step stop.expr should succeed: {err}"),
            };
        assert_eq!(result.iterations, 1);
        assert_eq!(result.stop_status, LoopStepStopStatus::StopConditionMet);
        assert!(result.stop_reason.contains("stop.expr matched"));
        assert!(result.stop_reason.contains("tasks_open=0"));
    }

    #[test]
    fn loop_step_tool_condition_stops_and_records_reason() {
        let request = loop_request("printf ok", 5);
        let stop_condition = LoopStepStopCondition {
            tool: Some(StopToolSpec {
                name: "sh".to_string(),
                args: vec!["-c".to_string(), "printf 'true\\n'".to_string()],
            }),
            ..LoopStepStopCondition::default()
        };
        let result =
            match execute_loop_step_with_stop_condition(&request, &stop_condition, || Ok(10)) {
                Ok(result) => result,
                Err(err) => panic!("loop step stop.tool should succeed: {err}"),
            };
        assert_eq!(result.iterations, 1);
        assert_eq!(result.stop_status, LoopStepStopStatus::StopConditionMet);
        assert!(result.stop_reason.contains("stop.tool matched"));
    }

    #[test]
    fn loop_step_surfaces_stop_tool_failures() {
        let request = loop_request("printf ok", 2);
        let stop_condition = LoopStepStopCondition {
            tool: Some(StopToolSpec {
                name: "__forge_missing_stop_tool_binary__".to_string(),
                args: Vec::new(),
            }),
            tool_timeout: Duration::from_secs(1),
            ..LoopStepStopCondition::default()
        };
        let err = match execute_loop_step_with_stop_condition(&request, &stop_condition, || Ok(1)) {
            Ok(_) => panic!("expected stop.tool evaluation failure"),
            Err(err) => err,
        };
        assert!(err.contains("evaluate stop.tool"));
        assert!(err.contains("spawn stop tool"));
    }

    #[test]
    fn loop_step_rejects_llm_stop_condition_for_now() {
        let mut open_count = || Ok(0);
        let stop_condition = LoopStepStopCondition {
            has_llm_condition: true,
            ..LoopStepStopCondition::default()
        };
        let err =
            match evaluate_loop_stop_condition(Path::new("."), &stop_condition, &mut open_count) {
                Ok(_) => panic!("expected stop.llm unsupported error"),
                Err(err) => err,
            };
        assert!(err.contains("stop.llm is not supported"));
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

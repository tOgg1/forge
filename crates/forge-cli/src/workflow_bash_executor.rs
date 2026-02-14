use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};

use super::run_persistence::WorkflowRunStore;

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

    let resolved_workdir = resolve_workdir(&request.repo_workdir, &request.workdir)?;
    ensure_workdir(step_id, &resolved_workdir)?;

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
    command.arg("-lc").arg(cmd);
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

fn resolve_workdir(repo_workdir: &Path, raw_workdir: &str) -> Result<PathBuf, String> {
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

fn ensure_workdir(step_id: &str, workdir: &Path) -> Result<(), String> {
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

fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        return stderr.to_string();
    }
    if stderr.is_empty() {
        return stdout.to_string();
    }
    format!("{stdout}\n{stderr}")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{append_bash_step_logs, execute_bash_step, BashStepRequest};
    use crate::workflow::run_persistence::{
        WorkflowRunStatus, WorkflowRunStore, WorkflowStepStatus,
    };
    use crate::workflow::InMemoryWorkflowBackend;

    fn temp_root(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
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

    fn ok_or_panic<T, E>(result: Result<T, E>, context: &str) -> T
    where
        E: std::fmt::Display,
    {
        match result {
            Ok(value) => value,
            Err(err) => panic!("{context}: {err}"),
        }
    }

    fn err_or_panic<T, E>(result: Result<T, E>, context: &str) -> String
    where
        E: std::fmt::Display,
    {
        match result {
            Ok(_) => panic!("{context}"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn bash_step_captures_stdout_stderr_and_exit_code() {
        let workdir = temp_root("capture");
        ok_or_panic(std::fs::create_dir_all(&workdir), "create temp workdir");

        let request = BashStepRequest::new(
            "build",
            "printf 'out'; printf 'err' >&2; exit 7",
            workdir.clone(),
            "",
        );
        let result = ok_or_panic(execute_bash_step(&request), "execute bash step");

        assert_eq!(result.step_id, "build");
        assert_eq!(result.exit_code, 7);
        assert!(!result.success);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert!(result.output.contains("out"));
        assert!(result.output.contains("err"));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("step=build type=bash started")));
        assert!(result
            .logs
            .iter()
            .any(|line| line.contains("step=build completed exit_code=7")));

        cleanup(&workdir);
    }

    #[test]
    fn bash_step_resolves_relative_workdir() {
        let root = temp_root("workdir");
        let nested = root.join("scripts");
        ok_or_panic(std::fs::create_dir_all(&nested), "create nested workdir");

        let request = BashStepRequest::new("build", "pwd", root.clone(), "scripts");
        let result = ok_or_panic(execute_bash_step(&request), "execute bash step");
        let expected = ok_or_panic(std::fs::canonicalize(&nested), "canonicalize nested");
        let reported = ok_or_panic(
            std::fs::canonicalize(Path::new(result.stdout.trim())),
            "canonicalize pwd",
        );

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.resolved_workdir, nested);
        assert_eq!(reported, expected);

        cleanup(&root);
    }

    #[test]
    fn bash_step_errors_when_workdir_missing() {
        let root = temp_root("missing-workdir");
        let request = BashStepRequest::new("build", "echo ok", root.clone(), "missing");

        let err = err_or_panic(
            execute_bash_step(&request),
            "expected missing workdir error",
        );
        assert!(err.contains("does not exist"));

        cleanup(&root);
    }

    #[test]
    fn bash_step_logs_persist_and_are_viewable() {
        let root = temp_root("persist");
        let repo_workdir = root.join("repo");
        ok_or_panic(
            std::fs::create_dir_all(&repo_workdir),
            "create repo workdir",
        );

        let store = WorkflowRunStore::new(root.join("store"));
        let run = store.create_run(
            "deploy",
            "/repo/.forge/workflows/deploy.toml",
            &["build".to_string()],
        );
        let run = ok_or_panic(run, "create run");

        let request = BashStepRequest::new(
            "build",
            "printf 'hello'; printf 'warn' >&2; exit 3",
            repo_workdir,
            "",
        );
        let result = ok_or_panic(execute_bash_step(&request), "execute bash step");
        ok_or_panic(
            append_bash_step_logs(&store, &run.id, &result),
            "append run logs",
        );
        ok_or_panic(
            store.update_step_status(&run.id, "build", WorkflowStepStatus::Failed),
            "set step status",
        );
        ok_or_panic(
            store.update_run_status(&run.id, WorkflowRunStatus::Failed),
            "set run status",
        );

        let backend = InMemoryWorkflowBackend::default();
        let logs_result = ok_or_panic(
            super::super::load_workflow_logs_result(&backend, &store, &run.id),
            "load workflow logs result",
        );
        assert_eq!(logs_result.steps.len(), 1);
        assert_eq!(logs_result.steps[0].step_id, "build");
        assert!(logs_result.steps[0].log.contains("stdout:"));
        assert!(logs_result.steps[0].log.contains("hello"));
        assert!(logs_result.steps[0].log.contains("stderr:"));
        assert!(logs_result.steps[0].log.contains("warn"));
        assert!(logs_result.steps[0].log.contains("exit_code=3"));

        cleanup(&root);
    }
}

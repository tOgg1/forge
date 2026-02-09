use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::Utc;
use uuid::Uuid;

use super::types::{
    LoopRunner, LoopRunnerError, LoopRunnerState, StartLoopRunnerRequest, StopLoopRunnerResult,
};

type LoopCommandBuilder = Arc<dyn Fn(&str, &[String]) -> Command + Send + Sync>;

struct LoopRunnerInfo {
    loop_id: String,
    instance_id: String,
    config_path: String,
    command_path: String,
    pid: i32,
    state: LoopRunnerState,
    last_error: String,
    started_at: chrono::DateTime<chrono::Utc>,
    stopped_at: Option<chrono::DateTime<chrono::Utc>>,
    child: Option<std::process::Child>,
}

struct Inner {
    loop_runners: BTreeMap<String, LoopRunnerInfo>,
    command_builder: LoopCommandBuilder,
}

/// Owns and tracks daemon-spawned loop runner processes.
///
/// Scope: Start/Stop/Get/List parity with Go daemon semantics.
#[derive(Clone)]
pub struct LoopRunnerManager {
    inner: Arc<Mutex<Inner>>,
}

impl Default for LoopRunnerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopRunnerManager {
    pub fn new() -> Self {
        Self::with_command_builder(Arc::new(default_command_builder))
    }

    pub fn with_command_builder(command_builder: LoopCommandBuilder) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                loop_runners: BTreeMap::new(),
                command_builder,
            })),
        }
    }

    pub fn start_loop_runner(
        &self,
        req: StartLoopRunnerRequest,
    ) -> Result<LoopRunner, LoopRunnerError> {
        let loop_id = req.loop_id.trim().to_string();
        if loop_id.is_empty() {
            return Err(LoopRunnerError::InvalidArgument);
        }

        let config_path = req.config_path.trim().to_string();
        let mut command_path = req.command_path.trim().to_string();
        if command_path.is_empty() {
            command_path = "forge".to_string();
        }

        let args = build_loop_runner_args(&loop_id, &config_path);

        {
            let mut guard = lock_inner(&self.inner);
            if let Some(existing) = guard.loop_runners.get_mut(&loop_id) {
                refresh_loop_runner_locked(existing);
                if existing.state == LoopRunnerState::Running {
                    return Err(LoopRunnerError::AlreadyExists(loop_id));
                }
            }
        }

        let mut command = {
            let guard = lock_inner(&self.inner);
            (guard.command_builder)(&command_path, &args)
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = match command.spawn() {
            Ok(child) => child,
            Err(err) => return Err(LoopRunnerError::StartFailed(err.to_string())),
        };

        let pid = child.id() as i32;

        let now = Utc::now();
        let instance_id = Uuid::new_v4().to_string();

        {
            let mut guard = lock_inner(&self.inner);
            guard.loop_runners.insert(
                loop_id.clone(),
                LoopRunnerInfo {
                    loop_id: loop_id.clone(),
                    instance_id: instance_id.clone(),
                    config_path: config_path.clone(),
                    command_path: command_path.clone(),
                    pid,
                    state: LoopRunnerState::Running,
                    last_error: String::new(),
                    started_at: now,
                    stopped_at: None,
                    child: Some(child),
                },
            );
        }

        self.spawn_waiter(loop_id.clone(), instance_id);

        self.get_loop_runner(&loop_id)
    }

    pub fn stop_loop_runner(
        &self,
        loop_id: &str,
        force: bool,
    ) -> Result<StopLoopRunnerResult, LoopRunnerError> {
        let loop_id = loop_id.trim().to_string();
        if loop_id.is_empty() {
            return Err(LoopRunnerError::InvalidArgument);
        }

        let pid = {
            let mut guard = lock_inner(&self.inner);
            let info = match guard.loop_runners.get_mut(&loop_id) {
                Some(info) => info,
                None => return Err(LoopRunnerError::NotFound(loop_id)),
            };

            refresh_loop_runner_locked(info);

            if info.state != LoopRunnerState::Running {
                return Ok(StopLoopRunnerResult {
                    success: true,
                    runner: info_to_runner(info),
                });
            }
            if info.child.is_none() {
                return Err(LoopRunnerError::NoProcessHandle(loop_id));
            }
            info.pid
        };

        if let Err(err) = stop_loop_runner_process(pid, force) {
            return Err(LoopRunnerError::StopFailed(loop_id, err));
        }

        {
            let mut guard = lock_inner(&self.inner);
            if let Some(info) = guard.loop_runners.get_mut(&loop_id) {
                info.state = LoopRunnerState::Stopped;
                info.last_error.clear();
                info.stopped_at = Some(Utc::now());
            }
        }

        Ok(StopLoopRunnerResult {
            success: true,
            runner: self.get_loop_runner(&loop_id)?,
        })
    }

    pub fn get_loop_runner(&self, loop_id: &str) -> Result<LoopRunner, LoopRunnerError> {
        let loop_id = loop_id.trim().to_string();
        if loop_id.is_empty() {
            return Err(LoopRunnerError::InvalidArgument);
        }

        let mut guard = lock_inner(&self.inner);
        let info = match guard.loop_runners.get_mut(&loop_id) {
            Some(info) => info,
            None => return Err(LoopRunnerError::NotFound(loop_id)),
        };
        refresh_loop_runner_locked(info);
        Ok(info_to_runner(info))
    }

    pub fn list_loop_runners(&self) -> Vec<LoopRunner> {
        let mut guard = lock_inner(&self.inner);
        let keys: Vec<String> = guard.loop_runners.keys().cloned().collect();
        let mut out = Vec::with_capacity(keys.len());
        for loop_id in keys {
            if let Some(info) = guard.loop_runners.get_mut(&loop_id) {
                refresh_loop_runner_locked(info);
                out.push(info_to_runner(info));
            }
        }
        out
    }

    pub fn stop_all_loop_runners(&self, force: bool) {
        let loop_ids: Vec<String> = {
            let guard = lock_inner(&self.inner);
            guard.loop_runners.keys().cloned().collect()
        };

        for loop_id in loop_ids {
            let _ = self.stop_loop_runner(&loop_id, force);
        }
    }

    fn spawn_waiter(&self, loop_id: String, instance_id: String) {
        let inner = Arc::clone(&self.inner);
        thread::spawn(move || loop_runner_wait_loop(inner, loop_id, instance_id));
    }
}

impl Drop for LoopRunnerManager {
    fn drop(&mut self) {
        self.stop_all_loop_runners(true);
    }
}

fn lock_inner(inner: &Arc<Mutex<Inner>>) -> std::sync::MutexGuard<'_, Inner> {
    match inner.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn default_command_builder(command_path: &str, args: &[String]) -> Command {
    let mut cmd = Command::new(command_path);
    cmd.args(args);
    cmd
}

fn build_loop_runner_args(loop_id: &str, config_path: &str) -> Vec<String> {
    if config_path.is_empty() {
        return vec!["loop".to_string(), "run".to_string(), loop_id.to_string()];
    }

    vec![
        "--config".to_string(),
        config_path.to_string(),
        "loop".to_string(),
        "run".to_string(),
        loop_id.to_string(),
    ]
}

fn stop_loop_runner_process(pid: i32, force: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let target = Pid::from_raw(pid);
        if force {
            return kill(target, Signal::SIGKILL).map_err(|err| err.to_string());
        }

        match kill(target, Signal::SIGINT) {
            Ok(()) => Ok(()),
            Err(_) => kill(target, Signal::SIGKILL).map_err(|err| err.to_string()),
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = force;
        Err("stop_loop_runner_process is only supported on unix".to_string())
    }
}

fn refresh_loop_runner_locked(info: &mut LoopRunnerInfo) {
    let child = match info.child.as_mut() {
        Some(child) => child,
        None => return,
    };

    let status = match child.try_wait() {
        Ok(status) => status,
        Err(_) => return,
    };

    let status = match status {
        Some(status) => status,
        None => return,
    };

    if info.stopped_at.is_none() {
        info.stopped_at = Some(Utc::now());
    }

    if status.success() {
        if info.state != LoopRunnerState::Stopped {
            info.state = LoopRunnerState::Stopped;
            info.last_error.clear();
        }
    } else if info.state == LoopRunnerState::Running {
        info.state = LoopRunnerState::Error;
        info.last_error = status.to_string();
    }

    info.child = None;
}

fn info_to_runner(info: &LoopRunnerInfo) -> LoopRunner {
    LoopRunner {
        loop_id: info.loop_id.clone(),
        instance_id: info.instance_id.clone(),
        config_path: info.config_path.clone(),
        command_path: info.command_path.clone(),
        pid: info.pid,
        state: info.state.clone(),
        last_error: info.last_error.clone(),
        started_at: info.started_at,
        stopped_at: info.stopped_at,
    }
}

fn loop_runner_wait_loop(inner: Arc<Mutex<Inner>>, loop_id: String, instance_id: String) {
    loop {
        thread::sleep(Duration::from_millis(250));

        let mut guard = match inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let info = match guard.loop_runners.get_mut(&loop_id) {
            Some(info) => info,
            None => return,
        };
        if info.instance_id != instance_id {
            return;
        }

        if info.child.is_none() {
            return;
        }

        refresh_loop_runner_locked(info);

        if info.child.is_none() {
            return;
        }
    }
}

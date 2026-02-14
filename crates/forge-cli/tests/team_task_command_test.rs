#![allow(clippy::unwrap_used)]

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn run_cli(args: &[&str]) -> (i32, String, String) {
    let owned = args
        .iter()
        .map(|item| (*item).to_string())
        .collect::<Vec<_>>();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = forge_cli::run_with_args(&owned, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

#[test]
fn team_and_task_flow_runs_via_root_dispatch() {
    let _lock = env_lock();
    let db_path = temp_db_path("root-dispatch");
    let db_value = db_path.as_os_str();
    let _db_guard = EnvGuard::set("FORGE_DATABASE_PATH", db_value);

    let (code, _stdout, stderr) = run_cli(&["team", "new", "ops"]);
    assert_eq!(code, 0, "stderr={stderr}");

    let (code, stdout, stderr) = run_cli(&[
        "task",
        "send",
        "--team",
        "ops",
        "--type",
        "incident",
        "--title",
        "database outage",
        "--json",
    ]);
    assert_eq!(code, 0, "stderr={stderr}");
    let task_id = serde_json::from_str::<serde_json::Value>(&stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (code, _stdout, stderr) = run_cli(&["task", "assign", &task_id, "--agent", "agent-a"]);
    assert_eq!(code, 0, "stderr={stderr}");

    let (code, stdout, stderr) = run_cli(&["task", "show", &task_id]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("status: assigned"));
    assert!(stdout.contains("assignee: agent-a"));

    cleanup_db(&db_path);
}

#[test]
fn team_member_lifecycle_via_root_dispatch() {
    let _lock = env_lock();
    let db_path = temp_db_path("member-lifecycle");
    let db_value = db_path.as_os_str();
    let _db_guard = EnvGuard::set("FORGE_DATABASE_PATH", db_value);

    let (code, _stdout, stderr) = run_cli(&["team", "new", "ops"]);
    assert_eq!(code, 0, "stderr={stderr}");

    let (code, _stdout, stderr) = run_cli(&[
        "team",
        "member",
        "add",
        "ops",
        "agent-lead",
        "--role",
        "leader",
    ]);
    assert_eq!(code, 0, "stderr={stderr}");

    let (code, stdout, stderr) = run_cli(&["team", "member", "ls", "ops"]);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("agent-lead"));

    let (code, _stdout, stderr) = run_cli(&["team", "member", "rm", "ops", "agent-lead"]);
    assert_eq!(code, 0, "stderr={stderr}");

    cleanup_db(&db_path);
}

fn temp_db_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!(
        "forge-cli-team-task-int-{tag}-{pid}-{nanos}-{seq}.sqlite"
    ))
}

fn cleanup_db(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

struct EnvGuard {
    key: String,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &str, value: &OsStr) -> Self {
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

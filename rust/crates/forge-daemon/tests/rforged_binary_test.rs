//! Integration test: spawn the rforged binary and verify Ping/GetStatus via gRPC.
//!
//! This tests the full binary lifecycle:
//!   1. Spawn rforged on a random port.
//!   2. Connect a gRPC client with retry.
//!   3. Call Ping and GetStatus RPCs.
//!   4. Send SIGTERM and verify clean exit.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use tonic::transport::Channel;
use uuid::Uuid;

/// Path to the compiled rforged binary (resolved by cargo at build time).
const RFORGED_BIN: &str = env!("CARGO_BIN_EXE_rforged");
const RUN_MARKER: &str = "run-marker-from-harness";
const PROMPT_TEXT: &str = "hello-rust-loop";

#[derive(Debug)]
struct CliOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Find a free TCP port by binding to port 0 and returning the assigned port.
fn find_free_port() -> u16 {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("bind to port 0 for free port discovery");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Spawn rforged binary with explicit storage paths for isolated e2e tests.
fn spawn_rforged_with_storage(port: u16, db_path: &Path, data_dir: &Path) -> Child {
    Command::new(RFORGED_BIN)
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .env("FORGE_DB_PATH", db_path)
        .env("FORGE_DATA_DIR", data_dir)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn rforged binary with isolated storage")
}

/// Wait for rforged to emit its "ready" log line on stderr, indicating the server is listening.
/// Returns true if the ready message was seen within the timeout.
fn wait_for_ready(child: &mut Child, timeout: Duration) -> bool {
    let stderr = child.stderr.take().expect("child stderr");
    let reader = BufReader::new(stderr);
    let deadline = std::time::Instant::now() + timeout;

    for line in reader.lines() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        match line {
            Ok(text) => {
                if text.contains("ready") || text.contains("gRPC serving") {
                    return true;
                }
            }
            Err(_) => return false,
        }
    }
    false
}

/// Connect a gRPC client with retry.
async fn connect_with_retry(port: u16) -> ForgedServiceClient<Channel> {
    let endpoint = format!("http://127.0.0.1:{port}");

    for attempt in 0..40 {
        let channel = Channel::from_shared(endpoint.clone()).expect("build gRPC channel endpoint");

        match channel.connect().await {
            Ok(connection) => return ForgedServiceClient::new(connection),
            Err(_) => {
                if attempt >= 39 {
                    panic!("timed out waiting for rforged gRPC on port {port}");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    unreachable!()
}

/// Send SIGTERM to a child process.
#[cfg(unix)]
fn send_sigterm(child: &Child) -> Result<(), String> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM)
        .map_err(|err| format!("send SIGTERM to rforged child process: {err}"))
}

#[cfg(not(unix))]
fn send_sigterm(child: &mut Child) -> Result<(), String> {
    child
        .kill()
        .map_err(|err| format!("kill rforged child process: {err}"))
}

#[cfg(unix)]
fn send_sigterm_or_panic(child: &mut Child) {
    send_sigterm(child).expect("send SIGTERM to rforged child process");
}

#[cfg(not(unix))]
fn send_sigterm_or_panic(child: &mut Child) {
    send_sigterm(child).expect("kill rforged child process");
}

fn wait_for_exit(child: &mut Child, timeout: Duration, label: &str) -> ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    panic!("{label} should exit within {}", timeout.as_secs());
                }
            }
            Err(err) => panic!("error waiting for {label} exit: {err}"),
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn assert_clean_exit(exit_status: ExitStatus, label: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let code = exit_status.code().unwrap_or(-1);
        let signal = exit_status.signal().unwrap_or(-1);
        assert!(
            code == 0 || signal == 15,
            "{label} should exit cleanly (code={code}, signal={signal})"
        );
    }

    #[cfg(not(unix))]
    {
        let _ = exit_status;
    }
}

fn wait_for_daemon_rpc_ready(port: u16) {
    run_async(async move {
        let mut client = connect_with_retry(port).await;
        client
            .ping(proto::PingRequest {})
            .await
            .expect("rforged ping should succeed");
    });
}

fn run_async<F>(future: F) -> F::Output
where
    F: Future,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime for async helper");
    runtime.block_on(future)
}

fn workspace_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("resolve workspace root from crate path")
        .to_path_buf()
}

fn resolve_target_dir(workspace_root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(raw) => {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        }
        None => workspace_root.join("target"),
    }
}

fn resolve_bin_path(name: &str) -> PathBuf {
    let root = workspace_root();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    #[cfg(windows)]
    let mut path = resolve_target_dir(&root).join(profile).join(name);
    #[cfg(not(windows))]
    let path = resolve_target_dir(&root).join(profile).join(name);
    #[cfg(windows)]
    {
        path.set_extension("exe");
    }
    path
}

fn ensure_rforge_binary() -> PathBuf {
    let rforge = resolve_bin_path("rforge");
    if rforge.exists() {
        return rforge;
    }

    let status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("forge-cli")
        .arg("--bin")
        .arg("rforge")
        .current_dir(workspace_root())
        .status()
        .expect("build rforge binary for e2e test");

    assert!(status.success(), "cargo build for rforge must succeed");
    assert!(rforge.exists(), "rforge binary should exist after build");
    rforge
}

fn run_rforge(
    rforge_bin: &Path,
    repo_path: &Path,
    db_path: &Path,
    data_dir: &Path,
    daemon_target: &str,
    args: &[&str],
) -> CliOutput {
    let output = Command::new(rforge_bin)
        .args(args)
        .current_dir(repo_path)
        .env("FORGE_DB_PATH", db_path)
        .env("FORGE_DATA_DIR", data_dir)
        .env("FORGE_DAEMON_TARGET", daemon_target)
        .output()
        .expect("run rforge command");

    CliOutput {
        stdout: String::from_utf8(output.stdout).expect("rforge stdout should be utf-8"),
        stderr: String::from_utf8(output.stderr).expect("rforge stderr should be utf-8"),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

fn assert_command_ok(output: &CliOutput, label: &str) {
    assert_eq!(
        output.exit_code, 0,
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        output.stdout, output.stderr
    );
}

fn json_string_field(input: &str, field: &str) -> Option<String> {
    let marker = format!("\"{field}\": \"");
    let start = input.find(&marker)? + marker.len();
    let rest = &input[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn json_i64_field(input: &str, field: &str) -> Option<i64> {
    let marker = format!("\"{field}\": ");
    let start = input.find(&marker)? + marker.len();
    let rest = &input[start..];
    let end = rest.find([',', '\n', '}']).unwrap_or(rest.len());
    rest[..end].trim().parse::<i64>().ok()
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn wait_for_runs_at_least(
    rforge_bin: &Path,
    repo_path: &Path,
    db_path: &Path,
    data_dir: &Path,
    daemon_target: &str,
    min_runs: i64,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;

    loop {
        let output = run_rforge(
            rforge_bin,
            repo_path,
            db_path,
            data_dir,
            daemon_target,
            &["ps", "--json"],
        );
        assert_command_ok(&output, "rforge ps --json");
        let ps_json = output.stdout;

        if json_i64_field(&ps_json, "runs").unwrap_or(0) >= min_runs {
            return ps_json;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for runs >= {min_runs}\nlast ps json:\n{ps_json}");
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_pending_queue_at_least(
    rforge_bin: &Path,
    repo_path: &Path,
    db_path: &Path,
    data_dir: &Path,
    daemon_target: &str,
    min_pending: i64,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;

    loop {
        let output = run_rforge(
            rforge_bin,
            repo_path,
            db_path,
            data_dir,
            daemon_target,
            &["ps", "--json"],
        );
        assert_command_ok(&output, "rforge ps --json");
        let ps_json = output.stdout;

        if json_i64_field(&ps_json, "pending_queue").unwrap_or(0) >= min_pending {
            return ps_json;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for pending_queue >= {min_pending}\nlast ps json:\n{ps_json}"
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}

#[test]
fn rforged_binary_serves_ping_and_get_status_then_exits_on_sigterm() {
    let temp = TempDir::new("rforged-binary-lifecycle");
    let db_path = temp.path().join("forge.db");
    let data_dir = temp.path().join("data");
    fs::create_dir_all(&data_dir).expect("create isolated data dir");

    let port = find_free_port();
    let mut child = spawn_rforged_with_storage(port, &db_path, &data_dir);

    // Wait for ready or fall through to retry-based connection.
    // The ready check uses stderr which is consumed; connection retry handles the rest.
    let _ready = wait_for_ready(&mut child, Duration::from_secs(10));

    // --- Ping ---
    let ping_resp = run_async(async {
        let mut client = connect_with_retry(port).await;
        client
            .ping(proto::PingRequest {})
            .await
            .expect("Ping RPC should succeed")
            .into_inner()
    });

    assert!(
        !ping_resp.version.is_empty(),
        "Ping response must include version"
    );
    assert!(
        ping_resp.timestamp.is_some(),
        "Ping response must include timestamp"
    );

    // --- GetStatus ---
    let status_resp = run_async(async {
        let mut client = connect_with_retry(port).await;
        client
            .get_status(proto::GetStatusRequest {})
            .await
            .expect("GetStatus RPC should succeed")
            .into_inner()
    });

    let status = status_resp
        .status
        .expect("GetStatus response must include status");
    assert!(
        !status.version.is_empty(),
        "status.version must be non-empty"
    );
    assert!(
        !status.hostname.is_empty(),
        "status.hostname must be non-empty"
    );
    assert!(
        status.started_at.is_some(),
        "status.started_at must be present"
    );
    assert!(status.uptime.is_some(), "status.uptime must be present");
    assert_eq!(status.agent_count, 0, "no agents should be running");
    assert!(status.health.is_some(), "status.health must be present");

    // --- Shutdown ---
    send_sigterm_or_panic(&mut child);
    let exit_status = wait_for_exit(&mut child, Duration::from_secs(10), "rforged");
    assert_clean_exit(exit_status, "rforged");
}

#[test]
fn rforged_and_rforge_up_spawn_owner_daemon_e2e_tmp_repo() {
    let temp = TempDir::new("rforged-rforge-daemon-e2e");
    let repo_path = temp.path().join("repo");
    let data_dir = temp.path().join("data");
    let db_path = temp.path().join("forge.db");
    fs::create_dir_all(&repo_path).expect("create isolated repo dir");
    fs::create_dir_all(&data_dir).expect("create isolated data dir");

    let port = find_free_port();
    let daemon_target = format!("http://127.0.0.1:{port}");
    let mut daemon = spawn_rforged_with_storage(port, &db_path, &data_dir);

    let _ready = wait_for_ready(&mut daemon, Duration::from_secs(10));
    wait_for_daemon_rpc_ready(port);

    let rforge_bin = ensure_rforge_binary();
    let migrate = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &["migrate", "up"],
    );
    assert_command_ok(&migrate, "rforge migrate up");

    let profile = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &[
            "profile",
            "add",
            "pi",
            "--name",
            "local-e2e",
            "--command",
            "cat {prompt} >> loop_prompt.txt; echo run-marker-from-harness",
        ],
    );
    assert_command_ok(&profile, "rforge profile add local-e2e");

    let up = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &[
            "up",
            "--name",
            "daemon-e2e",
            "--profile",
            "local-e2e",
            "--prompt-msg",
            PROMPT_TEXT,
            "--max-iterations",
            "20",
            "--interval",
            "1s",
            "--spawn-owner",
            "daemon",
            "--json",
        ],
    );
    assert_command_ok(&up, "rforge up --spawn-owner daemon");
    assert!(
        up.stdout.contains("\"short_id\""),
        "up json should return loop rows"
    );

    let ps_after_runs = wait_for_runs_at_least(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        2,
        Duration::from_secs(30),
    );
    assert_eq!(
        json_string_field(&ps_after_runs, "runner_owner").as_deref(),
        Some("daemon"),
        "ps should preserve daemon runner owner"
    );
    let runner_instance = json_string_field(&ps_after_runs, "runner_instance_id")
        .expect("ps should include runner_instance_id");
    assert!(
        !runner_instance.is_empty(),
        "runner_instance_id must be non-empty"
    );
    let short_id =
        json_string_field(&ps_after_runs, "short_id").expect("ps should include short_id");
    let short_prefix: String = short_id.chars().take(4).collect();
    assert_eq!(short_prefix.len(), 4, "short_id prefix should have 4 chars");

    let logs = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &["logs", short_prefix.as_str()],
    );
    assert_command_ok(&logs, "rforge logs <short-prefix>");
    assert!(
        count_occurrences(&logs.stdout, RUN_MARKER) >= 2,
        "logs should contain >=2 run markers\n{}",
        logs.stdout
    );

    let stop = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &["stop", short_prefix.as_str()],
    );
    assert_command_ok(&stop, "rforge stop <short-prefix>");
    assert!(
        stop.stdout.contains("Stopped 1 loop(s)"),
        "stop output should confirm one loop stopped\n{}",
        stop.stdout
    );

    let ps_after_stop = wait_for_pending_queue_at_least(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        1,
        Duration::from_secs(10),
    );
    let final_runs = json_i64_field(&ps_after_stop, "runs").unwrap_or(0);
    assert!(
        final_runs >= 2,
        "expected at least 2 runs; got {final_runs}"
    );

    let side_effect_path = repo_path.join("loop_prompt.txt");
    let side_effect = fs::read_to_string(&side_effect_path).unwrap_or_else(|err| {
        panic!(
            "read side-effect file {}: {err}",
            side_effect_path.display()
        )
    });
    assert!(
        count_occurrences(&side_effect, PROMPT_TEXT) >= 2,
        "side-effect file should include prompt text >=2 times\n{}",
        side_effect
    );

    send_sigterm_or_panic(&mut daemon);
    let daemon_exit = wait_for_exit(&mut daemon, Duration::from_secs(10), "rforged");
    assert_clean_exit(daemon_exit, "rforged");
}

/// Parse ps JSON array output into a Vec of serde_json::Value objects.
fn parse_ps_json_array(raw: &str) -> Vec<serde_json::Value> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw).unwrap_or_else(|err| panic!("parse ps json: {err}\n{raw}"));
    match parsed.as_array() {
        Some(arr) => arr.clone(),
        None => panic!("ps json should be an array\n{raw}"),
    }
}

/// Find a loop entry in the ps JSON array by name.
fn find_loop_by_name<'a>(
    entries: &'a [serde_json::Value],
    name: &str,
) -> Option<&'a serde_json::Value> {
    entries.iter().find(|entry| {
        entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|n| n == name)
    })
}

/// Wait for all loops (by name) to reach at least min_runs each.
fn wait_for_all_loops_runs(
    rforge_bin: &Path,
    repo_path: &Path,
    db_path: &Path,
    data_dir: &Path,
    daemon_target: &str,
    loop_names: &[&str],
    min_runs: i64,
    timeout: Duration,
) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + timeout;

    loop {
        let output = run_rforge(
            rforge_bin,
            repo_path,
            db_path,
            data_dir,
            daemon_target,
            &["ps", "--json"],
        );
        assert_command_ok(&output, "rforge ps --json (multi-loop poll)");
        let entries = parse_ps_json_array(&output.stdout);

        let all_ready = loop_names.iter().all(|name| {
            find_loop_by_name(&entries, name)
                .and_then(|e| e.get("runs").and_then(serde_json::Value::as_i64))
                .unwrap_or(0)
                >= min_runs
        });

        if all_ready {
            return entries;
        }

        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for all loops to reach {min_runs} runs\nlast ps json:\n{}",
                output.stdout
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}

/// Multi-loop daemon e2e: spawn 3 loops through daemon, verify concurrent execution,
/// short-id prefix resolution, and bulk stop.
#[test]
fn rforged_multi_loop_daemon_e2e_three_loops_prefix_targeting_bulk_stop() {
    let temp = TempDir::new("rforged-multi-loop-e2e");
    let repo_path = temp.path().join("repo");
    let data_dir = temp.path().join("data");
    let db_path = temp.path().join("forge.db");
    fs::create_dir_all(&repo_path).expect("create isolated repo dir");
    fs::create_dir_all(&data_dir).expect("create isolated data dir");

    let port = find_free_port();
    let daemon_target = format!("http://127.0.0.1:{port}");
    let mut daemon = spawn_rforged_with_storage(port, &db_path, &data_dir);

    let _ready = wait_for_ready(&mut daemon, Duration::from_secs(10));
    wait_for_daemon_rpc_ready(port);

    let rforge_bin = ensure_rforge_binary();
    let migrate = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &["migrate", "up"],
    );
    assert_command_ok(&migrate, "rforge migrate up");

    // Create one profile per loop so each writes to its own side-effect file,
    // avoiding concurrent-write contention on a single shared file.
    let loop_defs: Vec<(&str, String)> = vec![
        ("loop-alpha", "prof-alpha".to_string()),
        ("loop-beta", "prof-beta".to_string()),
        ("loop-gamma", "prof-gamma".to_string()),
    ];
    for (loop_name, prof_name) in &loop_defs {
        let cmd = format!("echo marker-{loop_name} >> {loop_name}.txt; echo marker-{loop_name}");
        let profile = run_rforge(
            &rforge_bin,
            &repo_path,
            &db_path,
            &data_dir,
            &daemon_target,
            &[
                "profile",
                "add",
                "pi",
                "--name",
                prof_name,
                "--command",
                &cmd,
            ],
        );
        assert_command_ok(&profile, &format!("rforge profile add {prof_name}"));
    }

    // --- Spawn 3 daemon-owned loops ---
    let loop_names: Vec<&str> = loop_defs.iter().map(|(n, _)| *n).collect();
    let mut short_ids: Vec<String> = Vec::new();

    for (name, prof_name) in &loop_defs {
        let up = run_rforge(
            &rforge_bin,
            &repo_path,
            &db_path,
            &data_dir,
            &daemon_target,
            &[
                "up",
                "--name",
                name,
                "--profile",
                prof_name,
                "--prompt-msg",
                "multi-test",
                "--max-iterations",
                "4",
                "--interval",
                "1s",
                "--spawn-owner",
                "daemon",
                "--json",
            ],
        );
        assert_command_ok(&up, &format!("rforge up --spawn-owner daemon ({name})"));
        assert!(
            up.stdout.contains("\"short_id\""),
            "up json should return loop rows for {name}\n{}",
            up.stdout
        );
    }

    // --- Wait for all 3 loops to complete at least 2 runs ---
    let entries = wait_for_all_loops_runs(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &loop_names,
        2,
        Duration::from_secs(60),
    );

    // --- Verify concurrent behavior: all 3 loops present and daemon-owned ---
    assert!(
        entries.len() >= 3,
        "ps should list at least 3 loops, got {}",
        entries.len()
    );

    for name in &loop_names {
        let entry = find_loop_by_name(&entries, name)
            .unwrap_or_else(|| panic!("loop {name} not found in ps output"));

        let runner_owner = entry
            .get("runner_owner")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert_eq!(
            runner_owner, "daemon",
            "loop {name} should have runner_owner=daemon"
        );

        let runner_instance_id = entry
            .get("runner_instance_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        assert!(
            !runner_instance_id.is_empty(),
            "loop {name} should have non-empty runner_instance_id"
        );

        let runs = entry
            .get("runs")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        assert!(runs >= 2, "loop {name} should have >=2 runs, got {runs}");

        let sid = entry
            .get("short_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        assert!(!sid.is_empty(), "loop {name} should have a short_id");
        short_ids.push(sid);
    }

    // --- Verify unique runner_instance_ids across loops ---
    let instance_ids: Vec<&str> = loop_names
        .iter()
        .map(|name| {
            find_loop_by_name(&entries, name)
                .unwrap()
                .get("runner_instance_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        })
        .collect();
    let unique_ids: std::collections::HashSet<&&str> = instance_ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        3,
        "each loop should have a distinct runner_instance_id: {:?}",
        instance_ids
    );

    // --- Short-id prefix targeting: use the first 4 chars of each short_id ---
    for (idx, name) in loop_names.iter().enumerate() {
        let sid = &short_ids[idx];
        let prefix: String = sid.chars().take(4).collect();

        let logs = run_rforge(
            &rforge_bin,
            &repo_path,
            &db_path,
            &data_dir,
            &daemon_target,
            &["logs", &prefix],
        );
        assert_command_ok(&logs, &format!("rforge logs {prefix} ({name})"));
        let marker = format!("marker-{name}");
        assert!(
            logs.stdout.contains(&marker),
            "logs for {name} (prefix={prefix}) should contain run marker\n{}",
            logs.stdout
        );
    }

    // --- Bulk stop: stop all daemon-owned loops at once ---
    let stop_all = run_rforge(
        &rforge_bin,
        &repo_path,
        &db_path,
        &data_dir,
        &daemon_target,
        &["stop", "--all"],
    );
    assert_command_ok(&stop_all, "rforge stop --all");
    assert!(
        stop_all.stdout.contains("Stopped 3 loop(s)"),
        "stop --all should confirm 3 loops stopped\n{}",
        stop_all.stdout
    );

    // --- Verify per-loop side-effect files received writes ---
    for name in &loop_names {
        let side_effect_path = repo_path.join(format!("{name}.txt"));
        let side_effect = fs::read_to_string(&side_effect_path).unwrap_or_else(|err| {
            panic!(
                "read side-effect file {}: {err}",
                side_effect_path.display()
            )
        });
        let marker = format!("marker-{name}");
        let marker_count = count_occurrences(&side_effect, &marker);
        assert!(
            marker_count >= 2,
            "side-effect file for {name} should contain at least 2 markers, got {marker_count}\n{}",
            side_effect
        );
    }

    // --- ListLoopRunners via gRPC should reflect the runners ---
    let runners = run_async(async {
        let mut client = connect_with_retry(port).await;
        client
            .list_loop_runners(proto::ListLoopRunnersRequest {})
            .await
            .expect("ListLoopRunners should succeed")
            .into_inner()
    });
    // After stop, runners may be stopped or cleaned up. Verify the response is valid.
    assert!(
        runners.runners.len() >= 3 || runners.runners.is_empty(),
        "ListLoopRunners should return all tracked runners or be empty after stop; got {}",
        runners.runners.len()
    );

    // --- Clean shutdown ---
    send_sigterm_or_panic(&mut daemon);
    let daemon_exit = wait_for_exit(&mut daemon, Duration::from_secs(10), "rforged");
    assert_clean_exit(daemon_exit, "rforged");
}

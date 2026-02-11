use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use serde::Serialize;
use serde_json::Value;
use tonic::{transport::Endpoint, Code};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopState {
    Pending,
    Running,
    Stopped,
    Error,
}

impl LoopState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub state: LoopState,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSelector {
    pub all: bool,
    pub loop_ref: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub state: String,
    pub tag: String,
}

pub trait StopBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn runner_owner(&self, loop_id: &str) -> Result<String, String>;
    fn stop_daemon_runner(&mut self, loop_id: &str) -> Result<(), String>;
    fn enqueue_stop(&mut self, loop_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryStopBackend {
    loops: Vec<LoopRecord>,
    runner_owners: HashMap<String, String>,
    pub daemon_stop_error: Option<String>,
    pub daemon_stopped: Vec<String>,
    pub enqueued: Vec<String>,
}

impl InMemoryStopBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        let mut runner_owners = HashMap::new();
        for loop_entry in &loops {
            runner_owners.insert(loop_entry.id.clone(), String::new());
        }
        Self {
            loops,
            runner_owners,
            daemon_stop_error: None,
            daemon_stopped: Vec::new(),
            enqueued: Vec::new(),
        }
    }

    pub fn with_runner_owner(mut self, loop_id: &str, owner: &str) -> Self {
        self.runner_owners
            .insert(loop_id.to_string(), owner.to_string());
        self
    }
}

impl StopBackend for InMemoryStopBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn runner_owner(&self, loop_id: &str) -> Result<String, String> {
        Ok(self.runner_owners.get(loop_id).cloned().unwrap_or_default())
    }

    fn stop_daemon_runner(&mut self, loop_id: &str) -> Result<(), String> {
        self.daemon_stopped.push(loop_id.to_string());
        if let Some(message) = self.daemon_stop_error.clone() {
            return Err(message);
        }
        Ok(())
    }

    fn enqueue_stop(&mut self, loop_id: &str) -> Result<(), String> {
        if !self.loops.iter().any(|entry| entry.id == loop_id) {
            return Err(format!("loop {loop_id} not found"));
        }
        self.enqueued.push(loop_id.to_string());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteStopBackend {
    db_path: PathBuf,
}

impl SqliteStopBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }
}

impl StopBackend for SqliteStopBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let loops = match loop_repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        let mut out = Vec::new();
        for entry in loops {
            let state = map_loop_state(&entry.state);
            out.push(LoopRecord {
                id: entry.id.clone(),
                short_id: if entry.short_id.is_empty() {
                    entry.id
                } else {
                    entry.short_id
                },
                name: entry.name,
                repo: entry.repo_path,
                pool: entry.pool_id,
                profile: entry.profile_id,
                state,
                tags: entry.tags,
            });
        }
        Ok(out)
    }

    fn runner_owner(&self, loop_id: &str) -> Result<String, String> {
        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let loop_entry = loop_repo.get(loop_id).map_err(|err| err.to_string())?;
        Ok(metadata_string(
            loop_entry.metadata.as_ref(),
            "runner_owner",
        ))
    }

    fn stop_daemon_runner(&mut self, loop_id: &str) -> Result<(), String> {
        stop_daemon_loop_runner(loop_id)
    }

    fn enqueue_stop(&mut self, loop_id: &str) -> Result<(), String> {
        let db = self.open_db()?;

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

        let mut items = vec![forge_db::loop_queue_repository::LoopQueueItem {
            item_type: "stop_graceful".to_string(),
            payload: r#"{"reason":"operator"}"#.to_string(),
            ..Default::default()
        }];

        queue_repo
            .enqueue(loop_id, &mut items)
            .map_err(|err| format!("enqueue stop for {loop_id}: {err}"))?;

        Ok(())
    }
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}

fn map_loop_state(state: &forge_db::loop_repository::LoopState) -> LoopState {
    match state {
        forge_db::loop_repository::LoopState::Running => LoopState::Running,
        forge_db::loop_repository::LoopState::Sleeping
        | forge_db::loop_repository::LoopState::Waiting => LoopState::Running,
        forge_db::loop_repository::LoopState::Stopped => LoopState::Stopped,
        forge_db::loop_repository::LoopState::Error => LoopState::Error,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    selector: LoopSelector,
}

#[derive(Debug, Serialize)]
struct StopResult {
    action: &'static str,
    loops: usize,
}

pub fn run_for_test(args: &[&str], backend: &mut dyn StopBackend) -> CommandOutput {
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
    backend: &mut dyn StopBackend,
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

fn execute(
    args: &[String],
    backend: &mut dyn StopBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    let loops = backend.list_loops()?;
    let mut matched = filter_loops(loops, &parsed.selector);
    if !parsed.selector.loop_ref.is_empty() {
        matched = match_loop_ref(&matched, &parsed.selector.loop_ref)?;
    }

    if matched.is_empty() {
        return Err("no loops matched".to_string());
    }

    for entry in &matched {
        let runner_owner = backend.runner_owner(&entry.id)?;
        if should_stop_daemon_runner(entry, &runner_owner) {
            backend.stop_daemon_runner(&entry.id)?;
        }
        backend.enqueue_stop(&entry.id)?;
    }

    if parsed.json || parsed.jsonl {
        let payload = StopResult {
            action: "stop_graceful",
            loops: matched.len(),
        };
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    writeln!(stdout, "Stopped {} loop(s)", matched.len()).map_err(|err| err.to_string())?;
    Ok(())
}

fn should_stop_daemon_runner(entry: &LoopRecord, runner_owner: &str) -> bool {
    if !runner_owner.eq_ignore_ascii_case("daemon") {
        return false;
    }
    matches!(entry.state, LoopState::Running | LoopState::Pending)
}

fn filter_loops(loops: Vec<LoopRecord>, selector: &LoopSelector) -> Vec<LoopRecord> {
    loops
        .into_iter()
        .filter(|entry| {
            (selector.repo.is_empty() || entry.repo == selector.repo)
                && (selector.pool.is_empty() || entry.pool == selector.pool)
                && (selector.profile.is_empty() || entry.profile == selector.profile)
                && (selector.state.is_empty() || entry.state.as_str() == selector.state)
                && (selector.tag.is_empty() || entry.tags.iter().any(|tag| tag == &selector.tag))
        })
        .collect()
}

fn match_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<Vec<LoopRecord>, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{}' not found", trimmed));
    }

    let found_exact_short = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed));
    if let Some(entry) = found_exact_short {
        return Ok(vec![entry.clone()]);
    }

    let found_exact_id = loops.iter().find(|entry| entry.id == trimmed);
    if let Some(entry) = found_exact_id {
        return Ok(vec![entry.clone()]);
    }

    let found_exact_name = loops.iter().find(|entry| entry.name == trimmed);
    if let Some(entry) = found_exact_name {
        return Ok(vec![entry.clone()]);
    }

    let normalized = trimmed.to_ascii_lowercase();
    let mut prefix_matches: Vec<LoopRecord> = loops
        .iter()
        .filter(|entry| {
            short_id(entry)
                .to_ascii_lowercase()
                .starts_with(&normalized)
                || entry.id.starts_with(trimmed)
        })
        .cloned()
        .collect();

    if prefix_matches.len() == 1 {
        return Ok(vec![prefix_matches.remove(0)]);
    }

    if !prefix_matches.is_empty() {
        prefix_matches.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| short_id(left).cmp(short_id(right)))
        });
        let labels = prefix_matches
            .iter()
            .map(format_loop_match)
            .collect::<Vec<String>>()
            .join(", ");
        return Err(format!(
            "loop '{}' is ambiguous; matches: {} (use a longer prefix or full ID)",
            trimmed, labels
        ));
    }

    let example = &loops[0];
    Err(format!(
        "loop '{}' not found. Example input: '{}' or '{}'",
        trimmed,
        example.name,
        short_id(example)
    ))
}

fn short_id(entry: &LoopRecord) -> &str {
    if entry.short_id.is_empty() {
        return &entry.id;
    }
    &entry.short_id
}

fn metadata_string(
    metadata: Option<&std::collections::HashMap<String, Value>>,
    key: &str,
) -> String {
    metadata
        .and_then(|map| map.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn stop_daemon_loop_runner(loop_id: &str) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("initialize daemon RPC runtime: {err}"))?;
    runtime.block_on(stop_daemon_loop_runner_async(loop_id))
}

async fn stop_daemon_loop_runner_async(loop_id: &str) -> Result<(), String> {
    let target = resolved_daemon_target();
    let endpoint = Endpoint::from_shared(target.clone())
        .map_err(|err| format!("forged daemon unavailable: {err}"))?
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2));

    let channel = endpoint
        .connect()
        .await
        .map_err(|err| format!("forged daemon unavailable: {err}"))?;

    let mut client = ForgedServiceClient::new(channel);
    let request = proto::StopLoopRunnerRequest {
        loop_id: loop_id.to_string(),
        force: false,
    };
    match client.stop_loop_runner(request).await {
        Ok(_) => Ok(()),
        Err(status) if status.code() == Code::NotFound => Ok(()),
        Err(status) => Err(format!("failed to stop loop via daemon: {status}")),
    }
}

fn resolved_daemon_target() -> String {
    let env_target = std::env::var("FORGE_DAEMON_TARGET").unwrap_or_default();
    if !env_target.trim().is_empty() {
        return normalize_daemon_target(&env_target);
    }
    "http://127.0.0.1:50051".to_string()
}

fn normalize_daemon_target(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    format!("http://{trimmed}")
}

fn format_loop_match(entry: &LoopRecord) -> String {
    format!("{} ({})", entry.name, short_id(entry))
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "stop") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut selector = LoopSelector::default();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err(HELP_TEXT.to_string());
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--jsonl" => {
                jsonl = true;
                index += 1;
            }
            "--quiet" => {
                quiet = true;
                index += 1;
            }
            "--all" => {
                selector.all = true;
                index += 1;
            }
            "--repo" => {
                selector.repo = take_value(args, index, "--repo")?;
                index += 2;
            }
            "--pool" => {
                selector.pool = take_value(args, index, "--pool")?;
                index += 2;
            }
            "--profile" => {
                selector.profile = take_value(args, index, "--profile")?;
                index += 2;
            }
            "--state" => {
                selector.state = take_value(args, index, "--state")?;
                index += 2;
            }
            "--tag" => {
                selector.tag = take_value(args, index, "--tag")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for stop: '{flag}'"));
            }
            value => {
                if selector.loop_ref.is_empty() {
                    selector.loop_ref = value.to_string();
                    index += 1;
                } else {
                    return Err(
                        "error: accepts at most 1 argument, received multiple loop references"
                            .to_string(),
                    );
                }
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    if selector.loop_ref.is_empty()
        && !selector.all
        && selector.repo.is_empty()
        && selector.pool.is_empty()
        && selector.profile.is_empty()
        && selector.state.is_empty()
        && selector.tag.is_empty()
    {
        return Err("specify a loop or selector".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        selector,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

const HELP_TEXT: &str = "\
Stop loops after current iteration

Usage:
  forge stop [loop] [flags]

Flags:
      --all              stop all loops
  -h, --help             help for stop
      --pool string      filter by pool
      --profile string   filter by profile
      --repo string      filter by repo path
      --state string     filter by state
      --tag string       filter by tag";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        parse_args, run_for_test, InMemoryStopBackend, LoopRecord, LoopState, SqliteStopBackend,
    };

    #[test]
    fn parse_requires_selector_or_loop() {
        let args = vec!["stop".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "specify a loop or selector");
    }

    #[test]
    fn parse_accepts_loop_ref() {
        let args = vec!["stop".to_string(), "my-loop".to_string()];
        let parsed = match parse_args(&args) {
            Ok(value) => value,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        assert_eq!(parsed.selector.loop_ref, "my-loop");
    }

    #[test]
    fn parse_accepts_all_flag() {
        let args = vec!["stop".to_string(), "--all".to_string()];
        let parsed = match parse_args(&args) {
            Ok(value) => value,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        assert!(parsed.selector.all);
    }

    #[test]
    fn stop_enqueues_for_matched_loop() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(backend.enqueued, vec!["loop-001"]);
    }

    #[test]
    fn stop_daemon_owned_loop_stops_daemon_runner_before_enqueue() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "daemon-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend =
            InMemoryStopBackend::with_loops(loops).with_runner_owner("loop-001", "daemon");
        let out = run_for_test(&["stop", "daemon-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(backend.daemon_stopped, vec!["loop-001"]);
        assert_eq!(backend.enqueued, vec!["loop-001"]);
    }

    #[test]
    fn stop_daemon_stop_failure_returns_error_without_enqueue() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "daemon-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend =
            InMemoryStopBackend::with_loops(loops).with_runner_owner("loop-001", "daemon");
        backend.daemon_stop_error = Some("failed to stop loop via daemon: unavailable".to_string());
        let out = run_for_test(&["stop", "daemon-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "failed to stop loop via daemon: unavailable\n");
        assert_eq!(backend.daemon_stopped, vec!["loop-001"]);
        assert!(backend.enqueued.is_empty());
    }

    #[test]
    fn stop_json_output_matches_oracle() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 1\n}\n"
        );
    }

    #[test]
    fn stop_human_output() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "oracle-loop"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Stopped 1 loop(s)\n");
    }

    #[test]
    fn stop_quiet_suppresses_output() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "oracle-loop", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn stop_no_match_returns_error() {
        let mut backend = InMemoryStopBackend::default();
        let out = run_for_test(&["stop", "--all"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "no loops matched\n");
    }

    #[test]
    fn stop_all_enqueues_for_every_loop() {
        let loops = vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc01".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "abc02".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Stopped,
                tags: vec![],
            },
        ];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "--all", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 2\n}\n"
        );
        assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
    }

    #[test]
    fn stop_filters_by_tag() {
        let loops = vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc01".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec!["team-a".to_string()],
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "abc02".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec!["team-b".to_string()],
            },
        ];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "--tag", "team-a", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.enqueued, vec!["loop-001"]);
    }

    #[test]
    fn stop_jsonl_output() {
        let loops = vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec![],
        }];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "oracle-loop", "--jsonl"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\"action\":\"stop_graceful\",\"loops\":1}\n");
    }

    #[test]
    fn stop_ambiguous_ref_returns_error() {
        let loops = vec![
            LoopRecord {
                id: "loop-abc001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
            },
            LoopRecord {
                id: "loop-abc002".to_string(),
                short_id: "abc002".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
            },
        ];
        let mut backend = InMemoryStopBackend::with_loops(loops);
        let out = run_for_test(&["stop", "abc"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002)"));
    }

    // -----------------------------------------------------------------------
    // SQLite integration tests
    // -----------------------------------------------------------------------

    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-stop-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    #[test]
    fn stop_sqlite_backend_enqueues_stop_graceful() {
        let db_path = temp_db_path("sqlite-stop");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "stop-test-loop".to_string(),
            repo_path: "/tmp/stop-test".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut backend = SqliteStopBackend::new(db_path.clone());
        let out = run_for_test(&["stop", "stop-test-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 1\n}\n"
        );

        // Verify stop_graceful was enqueued in the database.
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let items = queue_repo
            .list(&loop_entry.id)
            .unwrap_or_else(|err| panic!("list queue: {err}"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, "stop_graceful");
        assert_eq!(items[0].status, "pending");

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn stop_sqlite_backend_lists_loops() {
        let db_path = temp_db_path("sqlite-list");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_a = forge_db::loop_repository::Loop {
            name: "alpha-loop".to_string(),
            repo_path: "/tmp/alpha".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            tags: vec!["team-a".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_a)
            .unwrap_or_else(|err| panic!("create loop a: {err}"));
        let mut loop_b = forge_db::loop_repository::Loop {
            name: "beta-loop".to_string(),
            repo_path: "/tmp/beta".to_string(),
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_b)
            .unwrap_or_else(|err| panic!("create loop b: {err}"));

        let mut backend = SqliteStopBackend::new(db_path.clone());

        // Stop only team-a tagged loops.
        let out = run_for_test(&["stop", "--tag", "team-a", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 1\n}\n"
        );

        // Verify only alpha-loop got the stop item.
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let items_a = queue_repo
            .list(&loop_a.id)
            .unwrap_or_else(|err| panic!("list queue a: {err}"));
        assert_eq!(items_a.len(), 1);

        let items_b = queue_repo
            .list(&loop_b.id)
            .unwrap_or_else(|err| panic!("list queue b: {err}"));
        assert!(items_b.is_empty());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn stop_sqlite_backend_missing_db_returns_no_match() {
        let db_path = std::env::temp_dir().join("forge-cli-stop-nonexistent.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let mut backend = SqliteStopBackend::new(db_path);
        let out = run_for_test(&["stop", "--all"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "no loops matched\n");
    }

    #[test]
    fn stop_sqlite_all_enqueues_for_every_loop() {
        let db_path = temp_db_path("sqlite-all");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_a = forge_db::loop_repository::Loop {
            name: "loop-one".to_string(),
            repo_path: "/tmp/one".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_a)
            .unwrap_or_else(|err| panic!("create loop a: {err}"));
        let mut loop_b = forge_db::loop_repository::Loop {
            name: "loop-two".to_string(),
            repo_path: "/tmp/two".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_b)
            .unwrap_or_else(|err| panic!("create loop b: {err}"));

        let mut backend = SqliteStopBackend::new(db_path.clone());
        let out = run_for_test(&["stop", "--all", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"stop_graceful\",\n  \"loops\": 2\n}\n"
        );

        // Verify both loops got stop items.
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let items_a = queue_repo
            .list(&loop_a.id)
            .unwrap_or_else(|err| panic!("list queue a: {err}"));
        assert_eq!(items_a.len(), 1);
        assert_eq!(items_a[0].item_type, "stop_graceful");

        let items_b = queue_repo
            .list(&loop_b.id)
            .unwrap_or_else(|err| panic!("list queue b: {err}"));
        assert_eq!(items_b.len(), 1);
        assert_eq!(items_b[0].item_type, "stop_graceful");

        let _ = std::fs::remove_file(db_path);
    }
}

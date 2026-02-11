use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

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

pub trait KillBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn enqueue_kill(&mut self, loop_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryKillBackend {
    loops: Vec<LoopRecord>,
    pub enqueued: Vec<String>,
}

impl InMemoryKillBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            enqueued: Vec::new(),
        }
    }
}

impl KillBackend for InMemoryKillBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn enqueue_kill(&mut self, loop_id: &str) -> Result<(), String> {
        if !self.loops.iter().any(|entry| entry.id == loop_id) {
            return Err(format!("loop {loop_id} not found"));
        }
        self.enqueued.push(loop_id.to_string());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteKillBackend {
    db_path: PathBuf,
}

impl SqliteKillBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl KillBackend for SqliteKillBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;

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

    fn enqueue_kill(&mut self, loop_id: &str) -> Result<(), String> {
        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let mut items = vec![forge_db::loop_queue_repository::LoopQueueItem {
            item_type: "kill_now".to_string(),
            payload: r#"{"reason":"operator"}"#.to_string(),
            ..Default::default()
        }];

        queue_repo
            .enqueue(loop_id, &mut items)
            .map_err(|err| format!("enqueue kill for {loop_id}: {err}"))?;

        // Go parity: best-effort process signal, then persist stopped state.
        let mut loop_entry = loop_repo
            .get(loop_id)
            .map_err(|err| format!("load loop {loop_id}: {err}"))?;
        if let Some(pid) = loop_pid(loop_entry.metadata.as_ref()) {
            kill_process(pid);
        }
        loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("persist stop state for {loop_id}: {err}"))?;
        reconcile_running_runs(&db, loop_id)?;

        Ok(())
    }
}

fn reconcile_running_runs(db: &forge_db::Db, loop_id: &str) -> Result<(), String> {
    db.conn()
        .execute(
            "UPDATE loop_runs
             SET status = 'killed',
                 finished_at = COALESCE(NULLIF(finished_at, ''), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                 exit_code = COALESCE(exit_code, -9)
             WHERE loop_id = ?1
               AND status = 'running'",
            rusqlite::params![loop_id],
        )
        .map_err(|err| format!("reconcile running runs for {loop_id}: {err}"))?;
    Ok(())
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

fn loop_pid(metadata: Option<&HashMap<String, Value>>) -> Option<i32> {
    let value = metadata?.get("pid")?;
    match value {
        Value::Number(n) => n.as_i64().and_then(|pid| i32::try_from(pid).ok()),
        Value::String(s) => s.parse::<i32>().ok(),
        _ => None,
    }
}

fn kill_process(pid: i32) {
    if pid <= 0 {
        return;
    }

    #[cfg(unix)]
    {
        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
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
struct KillResult {
    action: &'static str,
    loops: usize,
}

pub fn run_for_test(args: &[&str], backend: &mut dyn KillBackend) -> CommandOutput {
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
    backend: &mut dyn KillBackend,
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
    backend: &mut dyn KillBackend,
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
        backend.enqueue_kill(&entry.id)?;
    }

    if parsed.json || parsed.jsonl {
        let payload = KillResult {
            action: "kill_now",
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

    writeln!(stdout, "Killed {} loop(s)", matched.len()).map_err(|err| err.to_string())?;
    Ok(())
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

fn format_loop_match(entry: &LoopRecord) -> String {
    format!("{} ({})", entry.name, short_id(entry))
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "kill") {
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
                return Err(format!("error: unknown argument for kill: '{flag}'"));
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
Kill loops immediately

Usage:
  forge kill [loop] [flags]

Flags:
      --all              kill all loops
  -h, --help             help for kill
      --pool string      filter by pool
      --profile string   filter by profile
      --repo string      filter by repo path
      --state string     filter by state
      --tag string       filter by tag";

#[cfg(test)]
mod tests {
    use super::{
        parse_args, run_for_test, InMemoryKillBackend, LoopRecord, LoopState, SqliteKillBackend,
    };

    #[test]
    fn parse_requires_selector_or_loop() {
        let args = vec!["kill".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "specify a loop or selector");
    }

    #[test]
    fn parse_accepts_loop_ref() {
        let args = vec!["kill".to_string(), "my-loop".to_string()];
        let parsed = match parse_args(&args) {
            Ok(value) => value,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        assert_eq!(parsed.selector.loop_ref, "my-loop");
    }

    #[test]
    fn parse_accepts_all_flag() {
        let args = vec!["kill".to_string(), "--all".to_string()];
        let parsed = match parse_args(&args) {
            Ok(value) => value,
            Err(err) => panic!("expected parse ok: {err}"),
        };
        assert!(parsed.selector.all);
    }

    #[test]
    fn kill_enqueues_for_matched_loop() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(backend.enqueued, vec!["loop-001"]);
    }

    #[test]
    fn kill_json_output_matches_oracle() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "oracle-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"kill_now\",\n  \"loops\": 1\n}\n"
        );
    }

    #[test]
    fn kill_human_output() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "oracle-loop"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "Killed 1 loop(s)\n");
    }

    #[test]
    fn kill_quiet_suppresses_output() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "oracle-loop", "--quiet"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn kill_no_match_returns_error() {
        let mut backend = InMemoryKillBackend::default();
        let out = run_for_test(&["kill", "--all"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "no loops matched\n");
    }

    #[test]
    fn kill_all_enqueues_for_every_loop() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "--all", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"kill_now\",\n  \"loops\": 2\n}\n"
        );
        assert_eq!(backend.enqueued, vec!["loop-001", "loop-002"]);
    }

    #[test]
    fn kill_filters_by_tag() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "--tag", "team-a", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(backend.enqueued, vec!["loop-001"]);
    }

    #[test]
    fn kill_jsonl_output() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "oracle-loop", "--jsonl"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "{\"action\":\"kill_now\",\"loops\":1}\n");
    }

    #[test]
    fn kill_ambiguous_ref_returns_error() {
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
        let mut backend = InMemoryKillBackend::with_loops(loops);
        let out = run_for_test(&["kill", "abc"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002)"));
    }

    // -----------------------------------------------------------------------
    // SQLite integration tests
    // -----------------------------------------------------------------------

    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-kill-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    #[test]
    fn kill_sqlite_backend_enqueues_kill_now_and_sets_state_stopped() {
        let db_path = temp_db_path("sqlite-kill");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "kill-test-loop".to_string(),
            repo_path: "/tmp/kill-test".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut backend = SqliteKillBackend::new(db_path.clone());
        let out = run_for_test(&["kill", "kill-test-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "{\n  \"action\": \"kill_now\",\n  \"loops\": 1\n}\n"
        );

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let items = queue_repo
            .list(&loop_entry.id)
            .unwrap_or_else(|err| panic!("list queue: {err}"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, "kill_now");
        assert_eq!(items[0].status, "pending");

        let updated = loop_repo
            .get(&loop_entry.id)
            .unwrap_or_else(|err| panic!("get loop: {err}"));
        assert_eq!(updated.state, forge_db::loop_repository::LoopState::Stopped);

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn kill_sqlite_backend_marks_running_loop_runs_as_killed() {
        let db_path = temp_db_path("sqlite-kill-runs");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "kill-run-reconcile-loop".to_string(),
            repo_path: "/tmp/kill-run-reconcile".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut run_entry = forge_db::loop_run_repository::LoopRun {
            loop_id: loop_entry.id.clone(),
            status: forge_db::loop_run_repository::LoopRunStatus::Running,
            ..Default::default()
        };
        run_repo
            .create(&mut run_entry)
            .unwrap_or_else(|err| panic!("create run: {err}"));

        let mut backend = SqliteKillBackend::new(db_path.clone());
        let out = run_for_test(&["kill", "kill-run-reconcile-loop"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let updated_run = run_repo
            .get(&run_entry.id)
            .unwrap_or_else(|err| panic!("get run: {err}"));
        assert_eq!(
            updated_run.status,
            forge_db::loop_run_repository::LoopRunStatus::Killed
        );
        assert!(updated_run.finished_at.is_some());
        assert_eq!(updated_run.exit_code, Some(-9));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn kill_sqlite_backend_signals_pid_from_metadata() {
        let db_path = temp_db_path("sqlite-signal");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap_or_else(|err| panic!("spawn sleep: {err}"));

        let pid = child.id();
        let pid_i64 = i64::from(pid);

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut metadata = HashMap::new();
        metadata.insert("pid".to_string(), json!(pid_i64));
        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "kill-signal-loop".to_string(),
            repo_path: "/tmp/kill-signal".to_string(),
            state: forge_db::loop_repository::LoopState::Running,
            metadata: Some(metadata),
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut backend = SqliteKillBackend::new(db_path.clone());
        let out = run_for_test(&["kill", "kill-signal-loop", "--json"], &mut backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);

        let mut exited = false;
        for _ in 0..40 {
            let polled = child
                .try_wait()
                .unwrap_or_else(|err| panic!("poll child: {err}"));
            if polled.is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !exited {
            let _ = child.kill();
            panic!("expected kill command to terminate child process");
        }

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn kill_sqlite_backend_missing_db_returns_no_match() {
        let db_path = std::env::temp_dir().join("forge-cli-kill-nonexistent.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let mut backend = SqliteKillBackend::new(db_path);
        let out = run_for_test(&["kill", "--all"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stderr, "no loops matched\n");
    }
}

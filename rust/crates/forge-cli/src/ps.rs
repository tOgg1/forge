use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tabwriter::TabWriter;

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
    Sleeping,
    Waiting,
    Stopped,
    Error,
}

impl LoopState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Waiting => "waiting",
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
    pub runs: u64,
    pub pending_queue: u64,
    pub last_run: String,
    pub wait_until: String,
    pub runner_owner: String,
    pub runner_instance_id: String,
    pub runner_pid_alive: Option<bool>,
    pub runner_daemon_alive: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSelector {
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub state: String,
    pub tag: String,
}

pub trait PsBackend {
    fn list_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String>;
}

#[derive(Debug, Clone)]
pub struct SqlitePsBackend {
    db_path: PathBuf,
}

impl SqlitePsBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl PsBackend for SqlitePsBackend {
    fn list_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);

        let loops = match loop_repo.list() {
            Ok(loops) => loops,
            Err(err) if err.to_string().contains("no such table: loops") => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };

        let repo_filter = if selector.repo.is_empty() {
            String::new()
        } else {
            normalize_repo_filter(&selector.repo)?
        };
        let pool_filter = if selector.pool.is_empty() {
            String::new()
        } else {
            resolve_pool_ref(&pool_repo, &selector.pool)?
        };
        let profile_filter = if selector.profile.is_empty() {
            String::new()
        } else {
            resolve_profile_ref(&profile_repo, &selector.profile)?
        };

        let mut out = Vec::new();
        for entry in loops {
            let state = map_loop_state(&entry.state);
            if !repo_filter.is_empty() && entry.repo_path != repo_filter {
                continue;
            }
            if !pool_filter.is_empty() && entry.pool_id != pool_filter {
                continue;
            }
            if !profile_filter.is_empty() && entry.profile_id != profile_filter {
                continue;
            }
            if !selector.state.is_empty() && state.as_str() != selector.state {
                continue;
            }
            if !selector.tag.is_empty() && !entry.tags.iter().any(|tag| tag == &selector.tag) {
                continue;
            }

            let run_count = run_repo
                .count_by_loop(&entry.id)
                .map_err(|err| format!("count loop runs: {err}"))?;
            let queue_items = queue_repo
                .list(&entry.id)
                .map_err(|err| format!("list queue items: {err}"))?;
            let pending_queue = queue_items
                .iter()
                .filter(|item| item.status == "pending")
                .count() as u64;

            let wait_until = if matches!(state, LoopState::Waiting) {
                metadata_scalar(entry.metadata.as_ref(), "wait_until")
            } else {
                String::new()
            };
            let runner_owner = metadata_string(entry.metadata.as_ref(), "runner_owner");
            let runner_instance_id = metadata_string(entry.metadata.as_ref(), "runner_instance_id");
            let runner_pid_alive =
                metadata_nested_bool(entry.metadata.as_ref(), "runner_liveness", "pid_alive");
            let runner_daemon_alive = metadata_nested_bool(
                entry.metadata.as_ref(),
                "runner_liveness",
                "daemon_runner_alive",
            )
            .or_else(|| {
                metadata_nested_bool(entry.metadata.as_ref(), "runner_liveness", "daemon_alive")
            });

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
                runs: run_count as u64,
                pending_queue,
                last_run: entry.last_run_at.unwrap_or_default(),
                wait_until,
                runner_owner,
                runner_instance_id,
                runner_pid_alive,
                runner_daemon_alive,
            });
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryPsBackend {
    loops: Vec<LoopRecord>,
}

impl InMemoryPsBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self { loops }
    }
}

impl PsBackend for InMemoryPsBackend {
    fn list_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String> {
        Ok(self
            .loops
            .iter()
            .filter(|entry| {
                (selector.repo.is_empty() || entry.repo == selector.repo)
                    && (selector.pool.is_empty() || entry.pool == selector.pool)
                    && (selector.profile.is_empty() || entry.profile == selector.profile)
                    && (selector.state.is_empty() || entry.state.as_str() == selector.state)
                    && (selector.tag.is_empty()
                        || entry.tags.iter().any(|tag| tag == &selector.tag))
            })
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    no_color: bool,
    selector: LoopSelector,
}

#[derive(Debug, Serialize)]
struct PsJsonEntry<'a> {
    id: &'a str,
    short_id: &'a str,
    name: &'a str,
    repo_path: &'a str,
    state: &'a str,
    profile_id: &'a str,
    pool_id: &'a str,
    runs: u64,
    pending_queue: u64,
    #[serde(skip_serializing_if = "str::is_empty")]
    last_run: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    wait_until: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    runner_owner: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    runner_instance_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    runner_pid_alive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runner_daemon_alive: Option<bool>,
}

pub fn run_for_test(args: &[&str], backend: &dyn PsBackend) -> CommandOutput {
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
    backend: &dyn PsBackend,
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

fn execute(args: &[String], backend: &dyn PsBackend, stdout: &mut dyn Write) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let loops = backend.list_loops(&parsed.selector)?;

    if parsed.json || parsed.jsonl {
        let entries: Vec<PsJsonEntry<'_>> = loops
            .iter()
            .map(|entry| PsJsonEntry {
                id: &entry.id,
                short_id: &entry.short_id,
                name: &entry.name,
                repo_path: &entry.repo,
                state: entry.state.as_str(),
                profile_id: &entry.profile,
                pool_id: &entry.pool,
                runs: entry.runs,
                pending_queue: entry.pending_queue,
                last_run: &entry.last_run,
                wait_until: &entry.wait_until,
                runner_owner: &entry.runner_owner,
                runner_instance_id: &entry.runner_instance_id,
                runner_pid_alive: entry.runner_pid_alive,
                runner_daemon_alive: entry.runner_daemon_alive,
            })
            .collect();
        if parsed.jsonl {
            for entry in &entries {
                serde_json::to_writer(&mut *stdout, entry).map_err(|err| err.to_string())?;
                writeln!(stdout).map_err(|err| err.to_string())?;
            }
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &entries).map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
        return Ok(());
    }

    if loops.is_empty() {
        writeln!(stdout, "No loops found").map_err(|err| err.to_string())?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    let use_color = color_enabled(parsed.no_color);
    let display_ids: Vec<&str> = loops.iter().map(display_short_id).collect();
    let unique_prefixes = loop_unique_prefix_lengths(&display_ids);

    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(
        tw,
        "ID\tNAME\tRUNS\tSTATE\tWAIT_UNTIL\tPROFILE\tPOOL\tQUEUE\tLAST_RUN\tREPO"
    )
    .map_err(|err| err.to_string())?;
    for entry in &loops {
        let display_id = display_short_id(entry);
        let unique_len = unique_prefixes
            .get(display_id)
            .copied()
            .unwrap_or(display_id.len());
        writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            format_loop_short_id(display_id, unique_len, use_color),
            entry.name,
            entry.runs,
            entry.state.as_str(),
            entry.wait_until,
            entry.profile,
            entry.pool,
            entry.pending_queue,
            entry.last_run,
            entry.repo,
        )
        .map_err(|err| err.to_string())?;
    }
    tw.flush().map_err(|err| err.to_string())?;
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

fn normalize_repo_filter(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("failed to resolve current directory: {err}"))?
            .join(path)
    };
    Ok(abs.to_string_lossy().into_owned())
}

fn resolve_pool_ref(
    repo: &forge_db::pool_repository::PoolRepository<'_>,
    value: &str,
) -> Result<String, String> {
    if let Ok(pool) = repo.get(value) {
        return Ok(pool.id);
    }
    if let Ok(pool) = repo.get_by_name(value) {
        return Ok(pool.id);
    }
    Err(format!("pool {value:?} not found"))
}

fn resolve_profile_ref(
    repo: &forge_db::profile_repository::ProfileRepository<'_>,
    value: &str,
) -> Result<String, String> {
    if let Ok(profile) = repo.get(value) {
        return Ok(profile.id);
    }
    if let Ok(profile) = repo.get_by_name(value) {
        return Ok(profile.id);
    }
    Err(format!("profile {value:?} not found"))
}

fn map_loop_state(state: &forge_db::loop_repository::LoopState) -> LoopState {
    match state {
        forge_db::loop_repository::LoopState::Running => LoopState::Running,
        forge_db::loop_repository::LoopState::Sleeping => LoopState::Sleeping,
        forge_db::loop_repository::LoopState::Waiting => LoopState::Waiting,
        forge_db::loop_repository::LoopState::Stopped => LoopState::Stopped,
        forge_db::loop_repository::LoopState::Error => LoopState::Error,
    }
}

fn metadata_string(metadata: Option<&HashMap<String, Value>>, key: &str) -> String {
    metadata
        .and_then(|meta| meta.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn metadata_scalar(metadata: Option<&HashMap<String, Value>>, key: &str) -> String {
    let Some(value) = metadata.and_then(|meta| meta.get(key)) else {
        return String::new();
    };
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(v) = value.as_i64() {
        return v.to_string();
    }
    if let Some(v) = value.as_u64() {
        return v.to_string();
    }
    if let Some(v) = value.as_f64() {
        return v.to_string();
    }
    if let Some(v) = value.as_bool() {
        return v.to_string();
    }
    value.to_string()
}

fn metadata_nested_bool(
    metadata: Option<&HashMap<String, Value>>,
    outer: &str,
    inner: &str,
) -> Option<bool> {
    metadata
        .and_then(|meta| meta.get(outer))
        .and_then(Value::as_object)
        .and_then(|nested| nested.get(inner))
        .and_then(Value::as_bool)
}

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_CYAN: &str = "\x1b[36m";

fn display_short_id(entry: &LoopRecord) -> &str {
    if entry.short_id.is_empty() {
        return &entry.id;
    }
    &entry.short_id
}

fn color_enabled(no_color: bool) -> bool {
    if no_color {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    true
}

fn colorize(value: &str, color: &str, enabled: bool) -> String {
    if !enabled || color.is_empty() || value.is_empty() {
        return value.to_string();
    }
    format!("{color}{value}{COLOR_RESET}")
}

fn loop_unique_prefix_lengths(ids: &[&str]) -> HashMap<String, usize> {
    let mut out = HashMap::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        if id.is_empty() {
            continue;
        }
        let max_len = id.len();
        for len in 1..=max_len {
            let prefix = &id[..len];
            let unique = ids
                .iter()
                .enumerate()
                .all(|(other_index, other)| other_index == index || !other.starts_with(prefix));
            if unique {
                out.insert((*id).to_string(), len);
                break;
            }
        }
        out.entry((*id).to_string()).or_insert(max_len);
    }
    out
}

fn format_loop_short_id(id: &str, unique_len: usize, use_color: bool) -> String {
    if id.is_empty() {
        return String::new();
    }
    let prefix_len = unique_len.clamp(1, id.len());
    let prefix = &id[..prefix_len];
    if !use_color {
        return id.to_string();
    }
    let mut out = colorize(prefix, COLOR_YELLOW, true);
    if prefix_len < id.len() {
        out.push_str(&colorize(&id[prefix_len..], COLOR_CYAN, true));
    }
    out
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args
        .get(index)
        .is_some_and(|token| token == "ps" || token == "ls")
    {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut no_color = false;
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
            "--no-color" => {
                no_color = true;
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
                return Err(format!("error: unknown argument for ps: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: ps takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        no_color,
        selector,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

const HELP_TEXT: &str = "\
List loops

Usage:
  forge ps [flags]

Aliases:
  ps, ls

Flags:
  -h, --help             help for ps
      --no-color         disable colored ID output
      --pool string      filter by pool
      --profile string   filter by profile
      --repo string      filter by repo path
      --state string     filter by state
      --tag string       filter by tag";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::{
        format_loop_short_id, loop_unique_prefix_lengths, parse_args, run_for_test,
        InMemoryPsBackend, LoopRecord, LoopState, ParsedArgs, SqlitePsBackend, COLOR_CYAN,
        COLOR_YELLOW,
    };

    fn parse_ok(args: &[String]) -> ParsedArgs {
        match parse_args(args) {
            Ok(parsed) => parsed,
            Err(err) => panic!("should parse: {err}"),
        }
    }

    fn parse_err(args: &[String]) -> String {
        match parse_args(args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        }
    }

    fn parse_json(text: &str) -> serde_json::Value {
        match serde_json::from_str(text) {
            Ok(value) => value,
            Err(err) => panic!("expected valid json: {err}"),
        }
    }

    #[test]
    fn parse_accepts_no_args() {
        let args = vec!["ps".to_string()];
        let parsed = parse_ok(&args);
        assert!(!parsed.json);
        assert!(!parsed.jsonl);
        assert!(!parsed.quiet);
        assert!(!parsed.no_color);
        assert!(parsed.selector.repo.is_empty());
    }

    #[test]
    fn parse_accepts_ls_alias() {
        let args = vec!["ls".to_string(), "--json".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.json);
    }

    #[test]
    fn parse_accepts_no_color_flag() {
        let args = vec!["ps".to_string(), "--no-color".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.no_color);
    }

    #[test]
    fn parse_rejects_positional_args() {
        let args = vec!["ps".to_string(), "some-loop".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("no positional arguments"));
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let args = vec!["ps".to_string(), "--bogus".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("unknown argument for ps"));
    }

    #[test]
    fn parse_rejects_json_and_jsonl_together() {
        let args = vec![
            "ps".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_accepts_all_filter_flags() {
        let args = vec![
            "ps".to_string(),
            "--repo".to_string(),
            "/repo".to_string(),
            "--pool".to_string(),
            "default".to_string(),
            "--profile".to_string(),
            "codex".to_string(),
            "--state".to_string(),
            "running".to_string(),
            "--tag".to_string(),
            "team-a".to_string(),
        ];
        let parsed = parse_ok(&args);
        assert_eq!(parsed.selector.repo, "/repo");
        assert_eq!(parsed.selector.pool, "default");
        assert_eq!(parsed.selector.profile, "codex");
        assert_eq!(parsed.selector.state, "running");
        assert_eq!(parsed.selector.tag, "team-a");
    }

    #[test]
    fn ps_empty_list_prints_no_loops_found() {
        let backend = InMemoryPsBackend::default();
        let out = run_for_test(&["ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, "No loops found\n");
    }

    #[test]
    fn ps_empty_list_json_returns_empty_array() {
        let backend = InMemoryPsBackend::default();
        let out = run_for_test(&["ps", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, "[]\n");
    }

    #[test]
    fn ps_single_loop_json() {
        let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
        let out = run_for_test(&["ps", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let parsed = parse_json(&out.stdout);
        let arr = match parsed.as_array() {
            Some(array) => array,
            None => panic!("json output must be an array"),
        };
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "oracle-loop");
        assert_eq!(arr[0]["state"], "stopped");
        assert_eq!(arr[0]["runs"], 5);
        assert_eq!(arr[0]["pending_queue"], 2);
        assert_eq!(arr[0]["runner_owner"], "local");
    }

    #[test]
    fn ps_single_loop_jsonl() {
        let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
        let out = run_for_test(&["ps", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 1);
        let parsed = parse_json(lines[0]);
        assert_eq!(parsed["name"], "oracle-loop");
    }

    #[test]
    fn ps_human_output_has_table_header() {
        let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
        let out = run_for_test(&["ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("ID"));
        assert!(out.stdout.contains("NAME"));
        assert!(out.stdout.contains("RUNS"));
        assert!(out.stdout.contains("STATE"));
        assert!(out.stdout.contains("PROFILE"));
        assert!(out.stdout.contains("POOL"));
        assert!(out.stdout.contains("QUEUE"));
        assert!(out.stdout.contains("LAST_RUN"));
        assert!(out.stdout.contains("REPO"));
        assert!(out.stdout.contains("oracle-loop"));
        assert!(out.stdout.contains("stopped"));
    }

    #[test]
    fn ps_quiet_suppresses_table() {
        let backend = InMemoryPsBackend::with_loops(vec![sample_loop()]);
        let out = run_for_test(&["ps", "--quiet"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn ps_filters_by_state() {
        let backend = InMemoryPsBackend::with_loops(vec![
            sample_loop(),
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "xyz02".to_string(),
                name: "running-loop".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec![],
                runs: 10,
                pending_queue: 0,
                last_run: "2025-01-02T00:00:00Z".to_string(),
                wait_until: String::new(),
                runner_owner: "local".to_string(),
                runner_instance_id: String::new(),
                runner_pid_alive: None,
                runner_daemon_alive: None,
            },
        ]);
        let out = run_for_test(&["ps", "--state", "running", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);
        let arr = match parsed.as_array() {
            Some(array) => array,
            None => panic!("json output must be an array"),
        };
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "running-loop");
    }

    #[test]
    fn ps_filters_by_repo() {
        let backend = InMemoryPsBackend::with_loops(vec![
            sample_loop(),
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "xyz02".to_string(),
                name: "other-loop".to_string(),
                repo: "/other-repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Stopped,
                tags: vec![],
                runs: 0,
                pending_queue: 0,
                last_run: String::new(),
                wait_until: String::new(),
                runner_owner: String::new(),
                runner_instance_id: String::new(),
                runner_pid_alive: None,
                runner_daemon_alive: None,
            },
        ]);
        let out = run_for_test(&["ps", "--repo", "/other-repo", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);
        let arr = match parsed.as_array() {
            Some(array) => array,
            None => panic!("json output must be an array"),
        };
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "other-loop");
    }

    #[test]
    fn ps_filters_by_tag() {
        let backend = InMemoryPsBackend::with_loops(vec![
            sample_loop(),
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "xyz02".to_string(),
                name: "tagged-loop".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Running,
                tags: vec!["special".to_string()],
                runs: 0,
                pending_queue: 0,
                last_run: String::new(),
                wait_until: String::new(),
                runner_owner: String::new(),
                runner_instance_id: String::new(),
                runner_pid_alive: None,
                runner_daemon_alive: None,
            },
        ]);
        let out = run_for_test(&["ps", "--tag", "special", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed = parse_json(&out.stdout);
        let arr = match parsed.as_array() {
            Some(array) => array,
            None => panic!("json output must be an array"),
        };
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "tagged-loop");
    }

    #[test]
    fn ps_help_returns_usage() {
        let backend = InMemoryPsBackend::default();
        let out = run_for_test(&["ps", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("List loops"));
        assert!(out.stderr.contains("forge ps"));
    }

    #[test]
    fn ps_multiple_loops_jsonl() {
        let backend = InMemoryPsBackend::with_loops(vec![
            sample_loop(),
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "xyz02".to_string(),
                name: "second-loop".to_string(),
                repo: "/repo".to_string(),
                pool: "burst".to_string(),
                profile: "claude".to_string(),
                state: LoopState::Running,
                tags: vec![],
                runs: 3,
                pending_queue: 1,
                last_run: "2025-01-02T00:00:00Z".to_string(),
                wait_until: String::new(),
                runner_owner: String::new(),
                runner_instance_id: String::new(),
                runner_pid_alive: None,
                runner_daemon_alive: None,
            },
        ]);
        let out = run_for_test(&["ps", "--jsonl"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        let first = parse_json(lines[0]);
        let second = parse_json(lines[1]);
        assert_eq!(first["name"], "oracle-loop");
        assert_eq!(second["name"], "second-loop");
    }

    #[test]
    fn unique_prefix_lengths_match_go_behavior() {
        let ids = vec!["ab123456", "ad123547", "zxy99999"];
        let prefixes = loop_unique_prefix_lengths(&ids);
        assert_eq!(prefixes.get("ab123456"), Some(&2));
        assert_eq!(prefixes.get("ad123547"), Some(&2));
        assert_eq!(prefixes.get("zxy99999"), Some(&1));
    }

    #[test]
    fn format_loop_short_id_colors_unique_prefix_and_suffix() {
        let formatted = format_loop_short_id("ab123456", 2, true);
        assert!(formatted.contains(COLOR_YELLOW));
        assert!(formatted.contains(COLOR_CYAN));
    }

    #[test]
    fn format_loop_short_id_respects_no_color() {
        let formatted = format_loop_short_id("ab123456", 2, false);
        assert_eq!(formatted, "ab123456");
    }

    #[test]
    fn ps_sqlite_backend_lists_real_loop_rows() {
        let db_path = temp_db_path("ps-sqlite");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);

        let mut profile = forge_db::profile_repository::Profile {
            name: "ops".to_string(),
            command_template: "codex exec".to_string(),
            harness: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let mut pool = forge_db::pool_repository::Pool {
            name: "default".to_string(),
            strategy: "round_robin".to_string(),
            ..Default::default()
        };
        pool_repo
            .create(&mut pool)
            .unwrap_or_else(|err| panic!("create pool: {err}"));

        let mut metadata = HashMap::new();
        metadata.insert("runner_owner".to_string(), json!("local"));
        metadata.insert("runner_instance_id".to_string(), json!("inst-42"));
        metadata.insert("wait_until".to_string(), json!("2026-02-10T12:00:00Z"));
        metadata.insert(
            "runner_liveness".to_string(),
            json!({
                "pid_alive": true,
                "daemon_runner_alive": false
            }),
        );

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "sqlite-loop".to_string(),
            repo_path: "/tmp/sqlite-loop".to_string(),
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Waiting,
            metadata: Some(metadata),
            tags: vec!["team-a".to_string()],
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut run_one = forge_db::loop_run_repository::LoopRun {
            loop_id: loop_entry.id.clone(),
            profile_id: profile.id.clone(),
            status: forge_db::loop_run_repository::LoopRunStatus::Success,
            ..Default::default()
        };
        run_repo
            .create(&mut run_one)
            .unwrap_or_else(|err| panic!("create run one: {err}"));

        let mut run_two = forge_db::loop_run_repository::LoopRun {
            loop_id: loop_entry.id.clone(),
            profile_id: profile.id.clone(),
            status: forge_db::loop_run_repository::LoopRunStatus::Success,
            ..Default::default()
        };
        run_repo
            .create(&mut run_two)
            .unwrap_or_else(|err| panic!("create run two: {err}"));

        let mut queued = vec![forge_db::loop_queue_repository::LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: r#"{"text":"hello"}"#.to_string(),
            ..Default::default()
        }];
        queue_repo
            .enqueue(&loop_entry.id, &mut queued)
            .unwrap_or_else(|err| panic!("queue add: {err}"));

        let backend = SqlitePsBackend::new(db_path.clone());
        let out = run_for_test(&["ps", "--json", "--profile", "ops"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        let parsed = parse_json(&out.stdout);
        let arr = match parsed.as_array() {
            Some(array) => array,
            None => panic!("json output must be an array"),
        };
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "sqlite-loop");
        assert_eq!(arr[0]["state"], "waiting");
        assert_eq!(arr[0]["runs"], 2);
        assert_eq!(arr[0]["pending_queue"], 1);
        assert_eq!(arr[0]["runner_owner"], "local");
        assert_eq!(arr[0]["runner_instance_id"], "inst-42");
        assert_eq!(arr[0]["runner_pid_alive"], true);
        assert_eq!(arr[0]["runner_daemon_alive"], false);
        assert_eq!(arr[0]["wait_until"], "2026-02-10T12:00:00Z");
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-ps-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    fn sample_loop() -> LoopRecord {
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "orc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Stopped,
            tags: vec!["team-a".to_string()],
            runs: 5,
            pending_queue: 2,
            last_run: "2025-01-01T00:00:00Z".to_string(),
            wait_until: String::new(),
            runner_owner: "local".to_string(),
            runner_instance_id: "inst-001".to_string(),
            runner_pid_alive: None,
            runner_daemon_alive: None,
        }
    }
}

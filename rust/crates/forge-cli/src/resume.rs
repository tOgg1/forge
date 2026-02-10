use std::env;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub state: LoopState,
    pub runner_owner: String,
    pub runner_instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeResult {
    pub owner: String,
    pub instance_id: String,
}

pub trait ResumeBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn resume_loop(&mut self, loop_id: &str, spawn_owner: &str) -> Result<ResumeResult, String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryResumeBackend {
    loops: Vec<LoopRecord>,
    tick: usize,
}

impl InMemoryResumeBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self { loops, tick: 0 }
    }

    fn next_instance_id(&mut self) -> String {
        self.tick += 1;
        format!("resume-{:03}", self.tick)
    }
}

impl ResumeBackend for InMemoryResumeBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn resume_loop(&mut self, loop_id: &str, spawn_owner: &str) -> Result<ResumeResult, String> {
        let Some(index) = self.loops.iter().position(|entry| entry.id == loop_id) else {
            return Err(format!("loop {loop_id} not found"));
        };

        let loop_state = self.loops[index].state.clone();
        match loop_state {
            LoopState::Stopped | LoopState::Error => {}
            other => {
                return Err(format!(
                    "loop \"{}\" is {}; only stopped or errored loops can be resumed",
                    self.loops[index].name,
                    other.as_str()
                ));
            }
        }

        let owner = resolve_spawn_owner(spawn_owner)?;
        let instance_id = self.next_instance_id();

        let loop_entry = &mut self.loops[index];
        loop_entry.state = LoopState::Running;
        loop_entry.runner_owner = owner.clone();
        loop_entry.runner_instance_id = instance_id.clone();

        Ok(ResumeResult { owner, instance_id })
    }
}

#[derive(Debug, Clone)]
pub struct SqliteResumeBackend {
    db_path: PathBuf,
}

impl SqliteResumeBackend {
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

    fn next_instance_id(&self) -> String {
        format!("resume-{}", uuid::Uuid::new_v4().simple())
    }
}

impl ResumeBackend for SqliteResumeBackend {
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
            out.push(LoopRecord {
                id: entry.id.clone(),
                short_id: if entry.short_id.is_empty() {
                    entry.id
                } else {
                    entry.short_id
                },
                name: entry.name,
                state: map_loop_state(&entry.state),
                runner_owner: metadata_string(entry.metadata.as_ref(), "runner_owner"),
                runner_instance_id: metadata_string(entry.metadata.as_ref(), "runner_instance_id"),
            });
        }
        Ok(out)
    }

    fn resume_loop(&mut self, loop_id: &str, spawn_owner: &str) -> Result<ResumeResult, String> {
        let owner = resolve_spawn_owner(spawn_owner)?;
        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let mut loop_entry = loop_repo.get(loop_id).map_err(|err| err.to_string())?;

        let state = map_loop_state(&loop_entry.state);
        match state {
            LoopState::Stopped | LoopState::Error => {}
            other => {
                return Err(format!(
                    "loop \"{}\" is {}; only stopped or errored loops can be resumed",
                    loop_entry.name,
                    other.as_str()
                ))
            }
        }

        loop_entry.state = forge_db::loop_repository::LoopState::Running;
        let mut metadata = loop_entry.metadata.unwrap_or_default();
        let instance_id = self.next_instance_id();
        metadata.insert("runner_owner".to_string(), Value::String(owner.clone()));
        metadata.insert(
            "runner_instance_id".to_string(),
            Value::String(instance_id.clone()),
        );
        loop_entry.metadata = Some(metadata);
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| err.to_string())?;

        Ok(ResumeResult { owner, instance_id })
    }
}

fn map_loop_state(state: &forge_db::loop_repository::LoopState) -> LoopState {
    match state {
        forge_db::loop_repository::LoopState::Running => LoopState::Running,
        forge_db::loop_repository::LoopState::Sleeping
        | forge_db::loop_repository::LoopState::Waiting => LoopState::Pending,
        forge_db::loop_repository::LoopState::Stopped => LoopState::Stopped,
        forge_db::loop_repository::LoopState::Error => LoopState::Error,
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    loop_ref: String,
    spawn_owner: String,
    json: bool,
    jsonl: bool,
    quiet: bool,
}

pub fn run_from_env_with_backend(backend: &mut dyn ResumeBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn ResumeBackend) -> CommandOutput {
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
    backend: &mut dyn ResumeBackend,
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
    backend: &mut dyn ResumeBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let loops = backend.list_loops()?;
    let loop_entry = match_loop_ref(&loops, &parsed.loop_ref)?;

    match loop_entry.state {
        LoopState::Stopped | LoopState::Error => {}
        _ => {
            return Err(format!(
                "loop \"{}\" is {}; only stopped or errored loops can be resumed",
                loop_entry.name,
                loop_entry.state.as_str()
            ));
        }
    }

    let _ = backend.resume_loop(&loop_entry.id, &parsed.spawn_owner)?;

    if parsed.json || parsed.jsonl {
        let payload = serde_json::json!({
            "resumed": true,
            "loop_id": loop_entry.id,
            "name": loop_entry.name,
        });
        write_serialized(stdout, &payload, parsed.jsonl)?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    writeln!(
        stdout,
        "Loop \"{}\" resumed ({})",
        loop_entry.name,
        short_id(&loop_entry)
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|arg| arg == "resume") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut spawn_owner = "auto".to_string();
    let mut loop_ref = String::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
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
            "--spawn-owner" => {
                spawn_owner = take_value(args, index, "--spawn-owner")?;
                index += 2;
            }
            "--help" | "-h" => {
                return Err("usage: resume <loop> [--spawn-owner local|daemon|auto]".to_string());
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for resume: '{flag}'"));
            }
            value => {
                if loop_ref.is_empty() {
                    loop_ref = value.to_string();
                    index += 1;
                } else {
                    return Err("resume accepts exactly 1 loop reference".to_string());
                }
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }
    if loop_ref.trim().is_empty() {
        return Err("loop name or ID required".to_string());
    }
    resolve_spawn_owner(&spawn_owner)?;

    Ok(ParsedArgs {
        loop_ref,
        spawn_owner,
        json,
        jsonl,
        quiet,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn resolve_spawn_owner(value: &str) -> Result<String, String> {
    match value {
        "auto" | "local" | "daemon" => Ok(value.to_string()),
        _ => Err(format!("invalid --spawn-owner value: {value}")),
    }
}

fn match_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<LoopRecord, String> {
    if loop_ref.trim().is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop \"{loop_ref}\" not found"));
    }

    if let Some(entry) = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(loop_ref))
    {
        return Ok(entry.clone());
    }
    if let Some(entry) = loops.iter().find(|entry| entry.id == loop_ref) {
        return Ok(entry.clone());
    }
    if let Some(entry) = loops.iter().find(|entry| entry.name == loop_ref) {
        return Ok(entry.clone());
    }

    let normalized = loop_ref.to_ascii_lowercase();
    let mut prefix_matches: Vec<LoopRecord> = loops
        .iter()
        .filter(|entry| {
            short_id(entry)
                .to_ascii_lowercase()
                .starts_with(&normalized)
                || entry.id.starts_with(loop_ref)
        })
        .cloned()
        .collect();

    if prefix_matches.len() == 1 {
        return Ok(prefix_matches.remove(0));
    }
    if !prefix_matches.is_empty() {
        prefix_matches.sort_by(|left, right| left.name.cmp(&right.name));
        let labels = prefix_matches
            .iter()
            .map(|entry| format!("{} ({})", entry.name, short_id(entry)))
            .collect::<Vec<String>>()
            .join(", ");
        return Err(format!(
            "loop '{loop_ref}' is ambiguous; matches: {labels} (use a longer prefix or full ID)"
        ));
    }

    Err(format!("loop \"{loop_ref}\" not found"))
}

fn short_id(entry: &LoopRecord) -> &str {
    if entry.short_id.is_empty() {
        &entry.id
    } else {
        &entry.short_id
    }
}

fn write_serialized(
    out: &mut dyn Write,
    value: &impl Serialize,
    jsonl: bool,
) -> Result<(), String> {
    if jsonl {
        serde_json::to_writer(&mut *out, value).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *out, value).map_err(|err| err.to_string())?;
    }
    writeln!(out).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use serde_json::json;

    use super::{
        parse_args, run_for_test, InMemoryResumeBackend, LoopRecord, LoopState, SqliteResumeBackend,
    };

    #[test]
    fn parse_requires_loop_ref() {
        let args = vec!["resume".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse failure"),
            Err(message) => message,
        };
        assert_eq!(err, "loop name or ID required");
    }

    #[test]
    fn parse_rejects_invalid_spawn_owner() {
        let args = vec![
            "resume".to_string(),
            "abc".to_string(),
            "--spawn-owner".to_string(),
            "invalid".to_string(),
        ];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse failure"),
            Err(message) => message,
        };
        assert_eq!(err, "invalid --spawn-owner value: invalid");
    }

    #[test]
    fn resume_running_loop_fails() {
        let loops = vec![LoopRecord {
            id: "loop-1".to_string(),
            short_id: "abc123".to_string(),
            name: "demo".to_string(),
            state: LoopState::Running,
            runner_owner: "local".to_string(),
            runner_instance_id: "inst-1".to_string(),
        }];
        let mut backend = InMemoryResumeBackend::with_loops(loops);
        let out = run_for_test(&["resume", "demo"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop \"demo\" is running; only stopped or errored loops can be resumed\n"
        );
    }

    #[test]
    fn sqlite_resume_updates_runner_metadata_and_preserves_runtime_keys() {
        let (db_path, _tmp, loop_id) = setup_sqlite_resume_fixture();
        let mut backend = SqliteResumeBackend::new(db_path.clone());

        let out = run_for_test(
            &["resume", "demo", "--spawn-owner", "daemon", "--json"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty());

        let parsed: serde_json::Value = match serde_json::from_str(&out.stdout) {
            Ok(value) => value,
            Err(err) => panic!("parse json output: {err}"),
        };
        assert_eq!(parsed["resumed"], true);
        assert_eq!(parsed["loop_id"], loop_id);

        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        let repo = forge_db::loop_repository::LoopRepository::new(&db);
        let entry = repo
            .get(&loop_id)
            .unwrap_or_else(|err| panic!("get loop {loop_id}: {err}"));
        assert_eq!(entry.state, forge_db::loop_repository::LoopState::Running);

        let metadata = entry.metadata.unwrap_or_default();
        assert_eq!(
            metadata.get("wait_until"),
            Some(&json!("2026-12-31T00:00:00Z"))
        );
        assert_eq!(
            metadata.get("loop_started_at"),
            Some(&json!("2026-02-10T00:00:00Z"))
        );
        assert_eq!(metadata.get("loop_iteration_count"), Some(&json!(42)));
        assert_eq!(metadata.get("runner_owner"), Some(&json!("daemon")));
        let instance_id = metadata
            .get("runner_instance_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        assert!(instance_id.starts_with("resume-"));
    }

    fn setup_sqlite_resume_fixture() -> (PathBuf, TempDir, String) {
        let tmp = TempDir::new("resume-sqlite");
        let db_path = tmp.path.join("forge.db");

        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db {}: {err}", db_path.display()));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);
        let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);

        let mut pool = forge_db::pool_repository::Pool {
            name: "default".to_string(),
            is_default: true,
            ..Default::default()
        };
        pool_repo
            .create(&mut pool)
            .unwrap_or_else(|err| panic!("create pool: {err}"));

        let mut profile = forge_db::profile_repository::Profile {
            name: "codex".to_string(),
            command_template: "codex".to_string(),
            ..Default::default()
        };
        profile_repo
            .create(&mut profile)
            .unwrap_or_else(|err| panic!("create profile: {err}"));

        let repo_path = std::env::current_dir()
            .unwrap_or_else(|err| panic!("cwd: {err}"))
            .to_string_lossy()
            .to_string();
        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("wait_until".to_string(), json!("2026-12-31T00:00:00Z"));
        metadata.insert("loop_started_at".to_string(), json!("2026-02-10T00:00:00Z"));
        metadata.insert("loop_iteration_count".to_string(), json!(42));

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "demo".to_string(),
            repo_path,
            pool_id: pool.id.clone(),
            profile_id: profile.id.clone(),
            state: forge_db::loop_repository::LoopState::Stopped,
            metadata: Some(metadata),
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        (db_path, tmp, loop_entry.id)
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            let uniq = format!(
                "{}-{}-{}",
                prefix,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            path.push(uniq);
            std::fs::create_dir_all(&path)
                .unwrap_or_else(|err| panic!("mkdir {}: {err}", path.display()));
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

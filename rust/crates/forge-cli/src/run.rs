use std::io::Write;
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub state: LoopState,
}

pub trait RunBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn run_once(&mut self, loop_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct SqliteRunBackend {
    db_path: PathBuf,
}

impl SqliteRunBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
        }
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRunBackend {
    loops: Vec<LoopRecord>,
    pub ran: Vec<String>,
}

impl InMemoryRunBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            ran: Vec::new(),
        }
    }
}

impl RunBackend for InMemoryRunBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn run_once(&mut self, loop_id: &str) -> Result<(), String> {
        self.ran.push(loop_id.to_string());
        Ok(())
    }
}

impl RunBackend for SqliteRunBackend {
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

        Ok(loops
            .into_iter()
            .map(|entry| LoopRecord {
                id: entry.id.clone(),
                short_id: if entry.short_id.is_empty() {
                    entry.id
                } else {
                    entry.short_id
                },
                name: entry.name,
                state: map_loop_state(&entry.state),
            })
            .collect())
    }

    fn run_once(&mut self, loop_id: &str) -> Result<(), String> {
        let db = forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);

        let mut loop_entry = loop_repo
            .get(loop_id)
            .map_err(|err| format!("load loop {loop_id}: {err}"))?;

        let mut run = forge_db::loop_run_repository::LoopRun {
            loop_id: loop_id.to_string(),
            profile_id: loop_entry.profile_id.clone(),
            status: forge_db::loop_run_repository::LoopRunStatus::Running,
            prompt_source: if !loop_entry.base_prompt_path.trim().is_empty() {
                "path".to_string()
            } else if !loop_entry.base_prompt_msg.trim().is_empty() {
                "inline".to_string()
            } else {
                String::new()
            },
            prompt_path: loop_entry.base_prompt_path.clone(),
            ..Default::default()
        };
        run_repo
            .create(&mut run)
            .map_err(|err| format!("create loop run: {err}"))?;

        run.status = forge_db::loop_run_repository::LoopRunStatus::Success;
        run.exit_code = Some(0);
        run_repo
            .finish(&mut run)
            .map_err(|err| format!("finish loop run: {err}"))?;

        loop_entry.last_run_at = run.finished_at.clone();
        loop_entry.last_exit_code = Some(0);
        loop_entry.last_error.clear();
        if matches!(
            loop_entry.state,
            forge_db::loop_repository::LoopState::Error
        ) {
            loop_entry.state = forge_db::loop_repository::LoopState::Stopped;
        }
        loop_repo
            .update(&mut loop_entry)
            .map_err(|err| format!("update loop after run: {err}"))?;
        Ok(())
    }
}

pub fn run_for_test(args: &[&str], backend: &mut dyn RunBackend) -> CommandOutput {
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
    backend: &mut dyn RunBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let _ = stdout;
    match execute(args, backend) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(args: &[String], backend: &mut dyn RunBackend) -> Result<(), String> {
    let loop_ref = parse_args(args)?;
    let loops = backend.list_loops()?;
    let entry = resolve_loop_ref(&loops, &loop_ref)?;
    backend
        .run_once(&entry.id)
        .map_err(|err| format!("loop run failed: {err}"))
}

fn parse_args(args: &[String]) -> Result<String, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "run") {
        index += 1;
    }

    let mut loop_ref: Option<String> = None;
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                return Err("Usage: forge run <loop>".to_string());
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for run: '{flag}'"));
            }
            value => {
                if loop_ref.is_some() {
                    return Err("error: accepts exactly 1 argument, received multiple".to_string());
                }
                loop_ref = Some(value.to_string());
                index += 1;
            }
        }
    }

    loop_ref.ok_or_else(|| "error: requires exactly 1 argument: <loop>".to_string())
}

pub(crate) fn resolve_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<LoopRecord, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{trimmed}' not found"));
    }

    // exact short_id match
    if let Some(entry) = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed))
    {
        return Ok(entry.clone());
    }

    // exact id match
    if let Some(entry) = loops.iter().find(|entry| entry.id == trimmed) {
        return Ok(entry.clone());
    }

    // exact name match
    if let Some(entry) = loops.iter().find(|entry| entry.name == trimmed) {
        return Ok(entry.clone());
    }

    // prefix match
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
        return Ok(prefix_matches.remove(0));
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
            "loop '{trimmed}' is ambiguous; matches: {labels} (use a longer prefix or full ID)"
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
        forge_db::loop_repository::LoopState::Running
        | forge_db::loop_repository::LoopState::Sleeping
        | forge_db::loop_repository::LoopState::Waiting => LoopState::Running,
        forge_db::loop_repository::LoopState::Stopped => LoopState::Stopped,
        forge_db::loop_repository::LoopState::Error => LoopState::Error,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{run_for_test, InMemoryRunBackend, LoopRecord, LoopState, SqliteRunBackend};
    use crate::run::run_with_backend;

    #[test]
    fn run_requires_loop_arg() {
        let mut backend = InMemoryRunBackend::default();
        let out = run_for_test(&["run"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: requires exactly 1 argument: <loop>\n");
    }

    #[test]
    fn run_rejects_extra_args() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha", "beta"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "error: accepts exactly 1 argument, received multiple\n"
        );
    }

    #[test]
    fn run_rejects_unknown_flags() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "--bogus"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unknown argument for run: '--bogus'\n");
    }

    #[test]
    fn run_resolves_by_name() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_resolves_by_short_id() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "abc001"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_resolves_by_full_id() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "loop-001"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
        assert_eq!(backend.ran, vec!["loop-001"]);
    }

    #[test]
    fn run_missing_loop_returns_error() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "does-not-exist"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'does-not-exist' not found. Example input: 'alpha' or 'abc001'\n"
        );
    }

    #[test]
    fn run_backend_failure_wraps_error() {
        let mut backend = FailingRunBackend;
        let out = run_for_test(&["run", "any-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "loop run failed: database error\n");
    }

    #[test]
    fn run_ambiguous_prefix_reports_matches() {
        let mut backend = InMemoryRunBackend::with_loops(vec![
            LoopRecord {
                id: "loop-abc001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                state: LoopState::Stopped,
            },
            LoopRecord {
                id: "loop-abc002".to_string(),
                short_id: "abc002".to_string(),
                name: "beta".to_string(),
                state: LoopState::Stopped,
            },
        ]);
        let out = run_for_test(&["run", "abc"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002) (use a longer prefix or full ID)\n"
        );
    }

    #[test]
    fn run_produces_no_stdout_on_success() {
        let mut backend = seeded();
        let out = run_for_test(&["run", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(
            out.stdout.is_empty(),
            "run should produce no stdout on success"
        );
        assert!(out.stderr.is_empty());
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "forge-cli-run-{tag}-{nanos}-{}-{suffix}.sqlite",
            std::process::id(),
        ))
    }

    #[test]
    fn run_sqlite_backend_creates_finished_run_record() {
        let db_path = temp_db_path("sqlite-run");
        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path))
            .unwrap_or_else(|err| panic!("open db: {err}"));
        db.migrate_up()
            .unwrap_or_else(|err| panic!("migrate db: {err}"));

        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);

        let mut loop_entry = forge_db::loop_repository::Loop {
            name: "alpha-loop".to_string(),
            repo_path: "/tmp/alpha".to_string(),
            state: forge_db::loop_repository::LoopState::Stopped,
            ..Default::default()
        };
        loop_repo
            .create(&mut loop_entry)
            .unwrap_or_else(|err| panic!("create loop: {err}"));

        let mut backend = SqliteRunBackend::new(db_path.clone());
        let args = vec!["run".to_string(), "alpha-loop".to_string()];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = run_with_backend(&args, &mut backend, &mut stdout, &mut stderr);
        assert_eq!(exit_code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
        assert!(
            String::from_utf8_lossy(&stdout).is_empty(),
            "stdout: {}",
            String::from_utf8_lossy(&stdout)
        );

        let runs = run_repo
            .list_by_loop(&loop_entry.id)
            .unwrap_or_else(|err| panic!("list loop runs: {err}"));
        assert_eq!(runs.len(), 1);
        assert_eq!(
            runs[0].status,
            forge_db::loop_run_repository::LoopRunStatus::Success
        );
        assert_eq!(runs[0].exit_code, Some(0));
        assert!(runs[0].finished_at.is_some());

        let stored_loop = loop_repo
            .get(&loop_entry.id)
            .unwrap_or_else(|err| panic!("get loop after run: {err}"));
        assert!(stored_loop.last_run_at.is_some());
        assert_eq!(stored_loop.last_exit_code, Some(0));

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn run_sqlite_backend_missing_db_reports_not_found() {
        let db_path = temp_db_path("sqlite-missing");
        let _ = std::fs::remove_file(&db_path);

        let mut backend = SqliteRunBackend::new(db_path);
        let out = run_for_test(&["run", "anything"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "loop 'anything' not found\n");
    }

    fn seeded() -> InMemoryRunBackend {
        InMemoryRunBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                state: LoopState::Running,
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "def002".to_string(),
                name: "beta".to_string(),
                state: LoopState::Stopped,
            },
        ])
    }

    struct FailingRunBackend;

    impl super::RunBackend for FailingRunBackend {
        fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
            Ok(vec![LoopRecord {
                id: "loop-fail".to_string(),
                short_id: "fail01".to_string(),
                name: "any-loop".to_string(),
                state: LoopState::Running,
            }])
        }

        fn run_once(&mut self, _loop_id: &str) -> Result<(), String> {
            Err("database error".to_string())
        }
    }
}

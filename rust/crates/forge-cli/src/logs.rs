use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo: String,
    pub log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    loop_ref: String,
    all: bool,
    follow: bool,
    lines: i32,
    since: String,
}

pub trait LogsBackend {
    fn data_dir(&self) -> &str;
    fn repo_path(&self) -> Result<String, String>;
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String>;
    fn follow_log(&mut self, path: &str, lines: i32, stdout: &mut dyn Write) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct InMemoryLogsBackend {
    loops: Vec<LoopRecord>,
    data_dir: String,
    repo_path: String,
    logs: BTreeMap<String, String>,
    follow_output: BTreeMap<String, String>,
    pub followed_paths: Vec<(String, i32)>,
}

impl Default for InMemoryLogsBackend {
    fn default() -> Self {
        Self {
            loops: Vec::new(),
            data_dir: "/tmp/forge".to_string(),
            repo_path: "/repo".to_string(),
            logs: BTreeMap::new(),
            follow_output: BTreeMap::new(),
            followed_paths: Vec::new(),
        }
    }
}

impl InMemoryLogsBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self {
            loops,
            ..Self::default()
        }
    }

    pub fn with_repo_path(mut self, repo_path: &str) -> Self {
        self.repo_path = repo_path.to_string();
        self
    }

    pub fn with_data_dir(mut self, data_dir: &str) -> Self {
        self.data_dir = data_dir.to_string();
        self
    }

    pub fn with_log(mut self, path: &str, content: &str) -> Self {
        self.logs.insert(path.to_string(), content.to_string());
        self
    }

    pub fn with_follow_output(mut self, path: &str, content: &str) -> Self {
        self.follow_output
            .insert(path.to_string(), content.to_string());
        self
    }
}

impl LogsBackend for InMemoryLogsBackend {
    fn data_dir(&self) -> &str {
        &self.data_dir
    }

    fn repo_path(&self) -> Result<String, String> {
        Ok(self.repo_path.clone())
    }

    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String> {
        let Some(content) = self.logs.get(path) else {
            return Err(format!("open {path}: no such file or directory"));
        };
        Ok(filter_log_content(content, lines, since))
    }

    fn follow_log(&mut self, path: &str, lines: i32, stdout: &mut dyn Write) -> Result<(), String> {
        self.followed_paths.push((path.to_string(), lines));
        if let Some(text) = self.follow_output.get(path) {
            write!(stdout, "{text}").map_err(|err| err.to_string())?;
            return Ok(());
        }
        let tail = self.read_log(path, lines, "")?;
        if !tail.is_empty() {
            writeln!(stdout, "{tail}").map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteLogsBackend {
    db_path: PathBuf,
    data_dir: String,
}

impl SqliteLogsBackend {
    pub fn open_from_env() -> Self {
        Self {
            db_path: resolve_database_path(),
            data_dir: resolve_data_dir(),
        }
    }

    pub fn new(db_path: PathBuf, data_dir: String) -> Self {
        Self { db_path, data_dir }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }
}

impl LogsBackend for SqliteLogsBackend {
    fn data_dir(&self) -> &str {
        &self.data_dir
    }

    fn repo_path(&self) -> Result<String, String> {
        std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|err| format!("resolve current directory: {err}"))
    }

    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }

        let db = self.open_db()?;
        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
        let loops = match loop_repo.list() {
            Ok(value) => value,
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
                repo: entry.repo_path,
                log_path: entry
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("log_path"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect())
    }

    fn read_log(&self, path: &str, lines: i32, since: &str) -> Result<String, String> {
        let content = std::fs::read_to_string(path).map_err(|err| format!("open {path}: {err}"))?;
        Ok(filter_log_content(&content, lines, since))
    }

    fn follow_log(&mut self, path: &str, lines: i32, stdout: &mut dyn Write) -> Result<(), String> {
        let tail = self.read_log(path, lines, "")?;
        if !tail.is_empty() {
            writeln!(stdout, "{tail}").map_err(|err| err.to_string())?;
        }
        Ok(())
    }
}

pub fn default_log_path(data_dir: &str, name: &str, id: &str) -> String {
    let slug = loop_slug(name);
    let file_stem = if slug.is_empty() { id } else { slug.as_str() };
    format!("{data_dir}/logs/loops/{file_stem}.log")
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    let mut path = PathBuf::from(resolve_data_dir());
    path.push("forge.db");
    path
}

fn resolve_data_dir() -> String {
    if let Some(path) = std::env::var_os("FORGE_DATA_DIR") {
        return PathBuf::from(path).to_string_lossy().into_owned();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        return path.to_string_lossy().into_owned();
    }
    ".forge-data".to_string()
}

fn filter_log_content(content: &str, lines: i32, since: &str) -> String {
    let limit = if lines <= 0 { 50 } else { lines as usize };
    let since_marker = parse_since_marker(since);
    let mut filtered = Vec::new();

    for line in content.lines() {
        if let Some(marker) = since_marker.as_deref() {
            if let Some(ts) = parse_log_timestamp(line) {
                if ts < marker {
                    continue;
                }
            }
        }
        filtered.push(line.to_string());
    }

    if filtered.len() > limit {
        filtered = filtered.split_off(filtered.len() - limit);
    }
    filtered.join("\n")
}

pub fn run_for_test(args: &[&str], backend: &mut dyn LogsBackend) -> CommandOutput {
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
    backend: &mut dyn LogsBackend,
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
    backend: &mut dyn LogsBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let mut loops = backend.list_loops()?;

    if parsed.all {
        let repo = backend.repo_path()?;
        loops.retain(|entry| entry.repo == repo);
    }

    if !parsed.loop_ref.is_empty() {
        loops = match_loop_ref(&loops, &parsed.loop_ref)?;
    }

    if loops.is_empty() {
        return Err("no loops matched".to_string());
    }

    for (index, entry) in loops.iter().enumerate() {
        let path = if entry.log_path.is_empty() {
            default_log_path(backend.data_dir(), &entry.name, &entry.id)
        } else {
            entry.log_path.clone()
        };

        if index > 0 {
            writeln!(stdout).map_err(|err| err.to_string())?;
        }
        writeln!(stdout, "==> {} <==", entry.name).map_err(|err| err.to_string())?;

        if parsed.follow {
            backend.follow_log(&path, parsed.lines, stdout)?;
            continue;
        }

        let content = backend.read_log(&path, parsed.lines, &parsed.since)?;
        if !content.is_empty() {
            writeln!(stdout, "{content}").map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args
        .get(index)
        .is_some_and(|token| token == "logs" || token == "log")
    {
        index += 1;
    }

    let mut all = false;
    let mut follow = false;
    let mut lines: i32 = 50;
    let mut since = String::new();
    let mut positionals = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => return Err(HELP_TEXT.to_string()),
            "-f" | "--follow" => {
                follow = true;
                index += 1;
            }
            "-n" | "--lines" => {
                let raw = take_value(args, index, "--lines")?;
                lines = raw
                    .parse::<i32>()
                    .map_err(|_| format!("error: invalid value '{}' for --lines", raw))?;
                index += 2;
            }
            "--since" => {
                since = take_value(args, index, "--since")?;
                index += 2;
            }
            "--all" => {
                all = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for logs: '{flag}'"));
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
            }
        }
    }

    if positionals.len() > 1 {
        return Err("error: accepts at most 1 argument, received multiple".to_string());
    }

    let loop_ref = positionals.into_iter().next().unwrap_or_default();
    if loop_ref.is_empty() && !all {
        return Err("loop name required (or use --all)".to_string());
    }

    Ok(ParsedArgs {
        loop_ref,
        all,
        follow,
        lines,
        since,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn match_loop_ref(loops: &[LoopRecord], loop_ref: &str) -> Result<Vec<LoopRecord>, String> {
    let trimmed = loop_ref.trim();
    if trimmed.is_empty() {
        return Err("loop name or ID required".to_string());
    }
    if loops.is_empty() {
        return Err(format!("loop '{trimmed}' not found"));
    }

    if let Some(entry) = loops
        .iter()
        .find(|entry| short_id(entry).eq_ignore_ascii_case(trimmed))
    {
        return Ok(vec![entry.clone()]);
    }

    if let Some(entry) = loops.iter().find(|entry| entry.id == trimmed) {
        return Ok(vec![entry.clone()]);
    }

    if let Some(entry) = loops.iter().find(|entry| entry.name == trimmed) {
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

fn parse_since_marker(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if is_rfc3339_utc(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

fn parse_log_timestamp(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let end = trimmed.find(']')?;
    let ts = &trimmed[1..end];
    if is_rfc3339_utc(ts) {
        return Some(ts);
    }
    None
}

fn is_rfc3339_utc(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    matches_format(bytes)
}

fn matches_format(bytes: &[u8]) -> bool {
    is_digit(bytes[0])
        && is_digit(bytes[1])
        && is_digit(bytes[2])
        && is_digit(bytes[3])
        && bytes[4] == b'-'
        && is_digit(bytes[5])
        && is_digit(bytes[6])
        && bytes[7] == b'-'
        && is_digit(bytes[8])
        && is_digit(bytes[9])
        && bytes[10] == b'T'
        && is_digit(bytes[11])
        && is_digit(bytes[12])
        && bytes[13] == b':'
        && is_digit(bytes[14])
        && is_digit(bytes[15])
        && bytes[16] == b':'
        && is_digit(bytes[17])
        && is_digit(bytes[18])
        && bytes[19] == b'Z'
}

fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn loop_slug(name: &str) -> String {
    let lowered = name.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut prev_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            prev_dash = false;
            continue;
        }
        if (ch == ' ' || ch == '-' || ch == '_') && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

const HELP_TEXT: &str = "\
Tail loop logs.

Usage:
  forge logs [loop]

Flags:
  -f, --follow      follow log output
  -n, --lines N     number of lines to show (default 50)
      --since VAL   show logs since duration or timestamp
      --all         show logs for all loops in repo
";

#[cfg(test)]
mod tests {
    use super::{default_log_path, run_for_test, InMemoryLogsBackend, LoopRecord};

    #[test]
    fn logs_requires_loop_or_all() {
        let mut backend = InMemoryLogsBackend::default();
        let out = run_for_test(&["logs"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "loop name required (or use --all)\n");
    }

    #[test]
    fn logs_tail_by_loop_name() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: alpha_path.to_string(),
        }])
        .with_log(
            alpha_path,
            "[2026-01-01T00:00:00Z] one\n[2026-01-01T00:00:01Z] two\n[2026-01-01T00:00:02Z] three\n",
        );

        let out = run_for_test(&["logs", "alpha", "--lines", "2"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:01Z] two\n[2026-01-01T00:00:02Z] three\n"
        );
    }

    #[test]
    fn logs_all_filters_by_repo() {
        let alpha_path = "/tmp/forge/logs/loops/alpha.log";
        let beta_path = "/tmp/forge/logs/loops/beta.log";
        let gamma_path = "/tmp/forge/logs/loops/gamma.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![
            LoopRecord {
                id: "loop-001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                repo: "/repo-a".to_string(),
                log_path: alpha_path.to_string(),
            },
            LoopRecord {
                id: "loop-002".to_string(),
                short_id: "def002".to_string(),
                name: "beta".to_string(),
                repo: "/repo-a".to_string(),
                log_path: beta_path.to_string(),
            },
            LoopRecord {
                id: "loop-003".to_string(),
                short_id: "ghi003".to_string(),
                name: "gamma".to_string(),
                repo: "/repo-b".to_string(),
                log_path: gamma_path.to_string(),
            },
        ])
        .with_repo_path("/repo-a")
        .with_log(alpha_path, "[2026-01-01T00:00:00Z] alpha\n")
        .with_log(beta_path, "[2026-01-01T00:00:00Z] beta\n")
        .with_log(gamma_path, "[2026-01-01T00:00:00Z] gamma\n");

        let out = run_for_test(&["logs", "--all"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("==> alpha <=="));
        assert!(out.stdout.contains("==> beta <=="));
        assert!(!out.stdout.contains("==> gamma <=="));
    }

    #[test]
    fn logs_since_rfc3339_filters_old_entries() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(
            path,
            "[2026-01-01T00:00:00Z] old\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] keep2\n",
        );
        let out = run_for_test(
            &["logs", "alpha", "--since", "2026-01-01T00:00:01Z"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:01Z] keep\n[2026-01-01T00:00:02Z] keep2\n"
        );
    }

    #[test]
    fn logs_alias_log_is_supported() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_log(path, "[2026-01-01T00:00:00Z] line\n");

        let out = run_for_test(&["log", "alpha"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("==> alpha <=="));
    }

    #[test]
    fn logs_follow_uses_backend_follow_path() {
        let path = "/tmp/forge/logs/loops/alpha.log";
        let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            log_path: path.to_string(),
        }])
        .with_follow_output(path, "[2026-01-01T00:00:03Z] streaming\n");
        let out = run_for_test(&["logs", "alpha", "--follow"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(
            backend.followed_paths,
            vec![(path.to_string(), 50)],
            "follow should use default --lines=50"
        );
        assert_eq!(
            out.stdout,
            "==> alpha <==\n[2026-01-01T00:00:03Z] streaming\n"
        );
    }

    #[test]
    fn logs_unknown_flag_is_error() {
        let mut backend = InMemoryLogsBackend::default();
        let out = run_for_test(&["logs", "--bogus"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unknown argument for logs: '--bogus'\n");
    }

    #[test]
    fn default_log_path_matches_go_shape() {
        assert_eq!(
            default_log_path("/tmp/forge", "My Loop_Name", "loop-1"),
            "/tmp/forge/logs/loops/my-loop-name.log"
        );
        assert_eq!(
            default_log_path("/tmp/forge", " ", "loop-1"),
            "/tmp/forge/logs/loops/loop-1.log"
        );
    }
}

use std::io::Write;

use serde::Serialize;
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

    let mut tw = TabWriter::new(&mut *stdout).padding(2);
    writeln!(
        tw,
        "ID\tNAME\tRUNS\tSTATE\tWAIT_UNTIL\tPROFILE\tPOOL\tQUEUE\tLAST_RUN\tREPO"
    )
    .map_err(|err| err.to_string())?;
    for entry in &loops {
        writeln!(
            tw,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            if entry.short_id.is_empty() {
                &entry.id
            } else {
                &entry.short_id
            },
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
      --pool string      filter by pool
      --profile string   filter by profile
      --repo string      filter by repo path
      --state string     filter by state
      --tag string       filter by tag";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{parse_args, run_for_test, InMemoryPsBackend, LoopRecord, LoopState, ParsedArgs};

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
        assert!(parsed.selector.repo.is_empty());
    }

    #[test]
    fn parse_accepts_ls_alias() {
        let args = vec!["ls".to_string(), "--json".to_string()];
        let parsed = parse_ok(&args);
        assert!(parsed.json);
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

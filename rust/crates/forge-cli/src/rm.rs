use std::io::Write;

use serde::Serialize;

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

pub trait LoopBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String>;
    fn delete_loop(&mut self, loop_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryLoopBackend {
    loops: Vec<LoopRecord>,
}

impl InMemoryLoopBackend {
    pub fn with_loops(loops: Vec<LoopRecord>) -> Self {
        Self { loops }
    }
}

impl LoopBackend for InMemoryLoopBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(self.loops.clone())
    }

    fn delete_loop(&mut self, loop_id: &str) -> Result<(), String> {
        let Some(index) = self.loops.iter().position(|entry| entry.id == loop_id) else {
            return Err(format!("loop {loop_id} not found"));
        };
        self.loops.remove(index);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    force: bool,
    selector: LoopSelector,
}

#[derive(Debug, Serialize)]
struct RemoveSingleResult<'a> {
    removed: usize,
    loop_id: &'a str,
    name: &'a str,
}

#[derive(Debug, Serialize)]
struct RemoveManyResult<'a> {
    removed: usize,
    loop_ids: Vec<&'a str>,
    names: Vec<&'a str>,
}

pub fn run_for_test(args: &[&str], backend: &mut dyn LoopBackend) -> CommandOutput {
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
    backend: &mut dyn LoopBackend,
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
    backend: &mut dyn LoopBackend,
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

    let active = matched
        .iter()
        .find(|entry| !matches!(entry.state, LoopState::Stopped));
    if let Some(entry) = active {
        if !parsed.force {
            return Err(format!(
                "loop '{}' is {}; use --force to remove anyway",
                entry.name,
                entry.state.as_str()
            ));
        }
    }

    for entry in &matched {
        backend.delete_loop(&entry.id)?;
    }

    if parsed.json || parsed.jsonl {
        let direct_loop_ref = is_direct_loop_ref_selector(&parsed.selector);
        write_json(stdout, &matched, parsed.jsonl, direct_loop_ref)?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    if matched.len() == 1 {
        writeln!(stdout, "Loop '{}' removed", matched[0].name).map_err(|err| err.to_string())?;
        return Ok(());
    }
    writeln!(stdout, "Removed {} loop(s)", matched.len()).map_err(|err| err.to_string())?;
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

fn write_json(
    stdout: &mut dyn Write,
    matched: &[LoopRecord],
    jsonl: bool,
    direct_loop_ref: bool,
) -> Result<(), String> {
    if direct_loop_ref {
        let first = &matched[0];
        let payload = RemoveSingleResult {
            removed: 1,
            loop_id: &first.id,
            name: &first.name,
        };
        if jsonl {
            serde_json::to_writer(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
        return Ok(());
    }

    let payload = RemoveManyResult {
        removed: matched.len(),
        loop_ids: matched.iter().map(|entry| entry.id.as_str()).collect(),
        names: matched.iter().map(|entry| entry.name.as_str()).collect(),
    };
    if jsonl {
        serde_json::to_writer(&mut *stdout, &payload).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

fn is_direct_loop_ref_selector(selector: &LoopSelector) -> bool {
    !selector.loop_ref.is_empty()
        && !selector.all
        && selector.repo.is_empty()
        && selector.pool.is_empty()
        && selector.profile.is_empty()
        && selector.state.is_empty()
        && selector.tag.is_empty()
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "rm") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut force = false;
    let mut selector = LoopSelector::default();

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
            "--force" => {
                force = true;
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
                return Err(format!("error: unknown argument for rm: '{flag}'"));
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

    let selector_based = selector.all
        || !selector.repo.is_empty()
        || !selector.pool.is_empty()
        || !selector.profile.is_empty()
        || !selector.state.is_empty()
        || !selector.tag.is_empty();
    if selector_based && !force {
        return Err("selector-based removal requires --force".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        force,
        selector,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

#[cfg(test)]
mod tests {
    use super::{parse_args, run_for_test, InMemoryLoopBackend, LoopRecord, LoopState};

    #[test]
    fn parse_requires_selector_or_loop() {
        let args = vec!["rm".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "specify a loop or selector");
    }

    #[test]
    fn parse_requires_force_for_selector_mode() {
        let args = vec!["rm".to_string(), "--all".to_string()];
        let err = match parse_args(&args) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert_eq!(err, "selector-based removal requires --force");
    }

    #[test]
    fn rm_reports_ambiguous_match() {
        let loops = vec![
            LoopRecord {
                id: "loop-abc001".to_string(),
                short_id: "abc001".to_string(),
                name: "alpha".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Stopped,
                tags: vec![],
            },
            LoopRecord {
                id: "loop-abc002".to_string(),
                short_id: "abc002".to_string(),
                name: "beta".to_string(),
                repo: "/repo".to_string(),
                pool: "default".to_string(),
                profile: "codex".to_string(),
                state: LoopState::Stopped,
                tags: vec![],
            },
        ];
        let mut backend = InMemoryLoopBackend::with_loops(loops);
        let out = run_for_test(&["rm", "abc", "--force"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002) (use a longer prefix or full ID)\n"
        );
    }

    #[test]
    fn rm_requires_force_for_running_loop() {
        let loops = vec![LoopRecord {
            id: "loop-1".to_string(),
            short_id: "loop1".to_string(),
            name: "hot-loop".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec!["prod".to_string()],
        }];
        let mut backend = InMemoryLoopBackend::with_loops(loops);
        let out = run_for_test(&["rm", "hot-loop"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "loop 'hot-loop' is running; use --force to remove anyway\n"
        );
    }
}

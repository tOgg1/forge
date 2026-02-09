use std::env;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRecord {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub tag: String,
    pub state: LoopState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSelector {
    pub repo: String,
    pub pool: String,
    pub profile: String,
    pub tag: String,
}

pub trait LoopBackend {
    fn select_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String>;
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
    fn select_loops(&self, selector: &LoopSelector) -> Result<Vec<LoopRecord>, String> {
        let matches_selector = |entry: &LoopRecord| {
            (selector.repo.is_empty() || entry.repo == selector.repo)
                && (selector.pool.is_empty() || entry.pool == selector.pool)
                && (selector.profile.is_empty() || entry.profile == selector.profile)
                && (selector.tag.is_empty() || entry.tag == selector.tag)
        };

        Ok(self
            .loops
            .iter()
            .filter(|entry| matches_selector(entry))
            .cloned()
            .collect())
    }

    fn delete_loop(&mut self, loop_id: &str) -> Result<(), String> {
        if let Some(index) = self.loops.iter().position(|entry| entry.id == loop_id) {
            self.loops.remove(index);
            return Ok(());
        }
        Err(format!("loop {loop_id} not found"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    quiet: bool,
    selector: LoopSelector,
}

#[derive(Debug, Serialize)]
struct CleanSingleResult<'a> {
    removed: usize,
    loop_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CleanManyResult<'a> {
    removed: usize,
    loop_ids: Vec<&'a str>,
    names: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<usize>,
}

pub fn run_from_env_with_backend(backend: &mut dyn LoopBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
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
    let loops = backend.select_loops(&parsed.selector)?;

    let mut cleanable = Vec::new();
    let mut skipped = 0usize;
    for entry in loops {
        match entry.state {
            LoopState::Stopped | LoopState::Error => cleanable.push(entry),
            LoopState::Pending | LoopState::Running => skipped += 1,
        }
    }

    if cleanable.is_empty() {
        return Err("no inactive loops matched".to_string());
    }

    for entry in &cleanable {
        backend.delete_loop(&entry.id)?;
    }

    if parsed.json {
        write_json(stdout, &cleanable, skipped)?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    if cleanable.len() == 1 {
        writeln!(stdout, "Loop '{}' removed", cleanable[0].name).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if skipped > 0 {
        writeln!(
            stdout,
            "Removed {} loop(s); skipped {} active loop(s)",
            cleanable.len(),
            skipped
        )
        .map_err(|err| err.to_string())?;
        return Ok(());
    }

    writeln!(stdout, "Removed {} loop(s)", cleanable.len()).map_err(|err| err.to_string())?;
    Ok(())
}

fn write_json(
    stdout: &mut dyn Write,
    cleanable: &[LoopRecord],
    skipped: usize,
) -> Result<(), String> {
    if cleanable.len() == 1 {
        let first = &cleanable[0];
        let payload = CleanSingleResult {
            removed: 1,
            loop_id: &first.id,
            name: &first.name,
            skipped: (skipped > 0).then_some(skipped),
        };
        serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
        writeln!(stdout).map_err(|err| err.to_string())?;
        return Ok(());
    }

    let payload = CleanManyResult {
        removed: cleanable.len(),
        loop_ids: cleanable.iter().map(|entry| entry.id.as_str()).collect(),
        names: cleanable.iter().map(|entry| entry.name.as_str()).collect(),
        skipped: (skipped > 0).then_some(skipped),
    };
    serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|err| err.to_string())?;
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|arg| arg == "clean") {
        index += 1;
    }

    let mut json = false;
    let mut quiet = false;
    let mut selector = LoopSelector::default();
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                json = true;
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
            "--tag" => {
                selector.tag = take_value(args, index, "--tag")?;
                index += 2;
            }
            unknown => return Err(format!("error: unknown argument for clean: '{unknown}'")),
        }
    }

    Ok(ParsedArgs {
        json,
        quiet,
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
    use super::{run_for_test, InMemoryLoopBackend, LoopRecord, LoopState};

    #[test]
    fn clean_single_loop_human_output() {
        let loops = vec![LoopRecord {
            id: "lp-1".to_string(),
            name: "demo".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            tag: "tag-a".to_string(),
            state: LoopState::Stopped,
        }];
        let mut backend = InMemoryLoopBackend::with_loops(loops);
        let out = run_for_test(&["clean"], &mut backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(out.stdout, "Loop 'demo' removed\n");
    }

    #[test]
    fn clean_requires_inactive_loop_match() {
        let loops = vec![LoopRecord {
            id: "lp-1".to_string(),
            name: "demo".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            tag: "tag-a".to_string(),
            state: LoopState::Running,
        }];
        let mut backend = InMemoryLoopBackend::with_loops(loops);
        let out = run_for_test(&["clean"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "no inactive loops matched\n");
    }
}

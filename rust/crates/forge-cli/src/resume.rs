use std::env;
use std::io::Write;

use serde::Serialize;

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
    use super::{parse_args, run_for_test, InMemoryResumeBackend, LoopRecord, LoopState};

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
}

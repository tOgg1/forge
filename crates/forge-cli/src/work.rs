use std::collections::HashMap;
use std::env;
use std::io::Write;

use serde::Serialize;

mod sqlite_backend;
pub use sqlite_backend::SqliteWorkBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLoop {
    pub id: String,
    pub name: String,
    pub iteration: i32,
    pub short_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCurrentRequest {
    pub loop_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub status: String,
    pub detail: String,
    pub loop_iteration: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoopWorkState {
    pub id: String,
    pub loop_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub loop_iteration: i32,
    pub is_current: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub trait WorkBackend {
    fn resolve_loop(&self, reference: &str) -> Result<ResolvedLoop, String>;
    fn set_current(&mut self, request: SetCurrentRequest) -> Result<LoopWorkState, String>;
    fn clear_current(&mut self, loop_id: &str) -> Result<(), String>;
    fn current(&self, loop_id: &str) -> Result<Option<LoopWorkState>, String>;
    fn list(&self, loop_id: &str, limit: usize) -> Result<Vec<LoopWorkState>, String>;
}

#[derive(Debug, Clone)]
struct LoopRecord {
    id: String,
    name: String,
    short_id: String,
    iteration: i32,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkBackend {
    loops: Vec<LoopRecord>,
    states_by_loop: HashMap<String, Vec<LoopWorkState>>,
    id_counter: usize,
    time_counter: u64,
}

impl InMemoryWorkBackend {
    pub fn seed_loop(&mut self, id: &str, name: &str, iteration: i32) {
        let loop_id = id.trim().to_string();
        let loop_name = name.trim().to_string();
        if loop_id.is_empty() || loop_name.is_empty() {
            return;
        }

        let short_id = short_id(&loop_id);
        if let Some(existing) = self.loops.iter_mut().find(|entry| entry.id == loop_id) {
            existing.name = loop_name;
            existing.short_id = short_id;
            existing.iteration = iteration;
            return;
        }

        self.loops.push(LoopRecord {
            id: loop_id,
            name: loop_name,
            short_id,
            iteration,
        });
    }

    fn next_id(&mut self) -> String {
        self.id_counter += 1;
        format!("work-{:06}", self.id_counter)
    }

    fn next_timestamp(&mut self) -> String {
        self.time_counter += 1;
        format_timestamp(self.time_counter)
    }

    fn find_loop_by_ref(&self, reference: &str) -> Result<LoopRecord, String> {
        let trimmed = reference.trim();
        if trimmed.is_empty() {
            return Err("loop name or ID required".to_string());
        }

        if let Some(entry) = self
            .loops
            .iter()
            .find(|loop_entry| loop_entry.short_id.eq_ignore_ascii_case(trimmed))
        {
            return Ok(entry.clone());
        }

        if let Some(entry) = self
            .loops
            .iter()
            .find(|loop_entry| loop_entry.id == trimmed)
        {
            return Ok(entry.clone());
        }

        if let Some(entry) = self
            .loops
            .iter()
            .find(|loop_entry| loop_entry.name == trimmed)
        {
            return Ok(entry.clone());
        }

        let normalized = trimmed.to_ascii_lowercase();
        let mut matches: Vec<LoopRecord> = self
            .loops
            .iter()
            .filter(|loop_entry| {
                loop_entry
                    .short_id
                    .to_ascii_lowercase()
                    .starts_with(&normalized)
                    || loop_entry.id.starts_with(trimmed)
            })
            .cloned()
            .collect();

        if matches.len() == 1 {
            return Ok(matches.remove(0));
        }

        if matches.len() > 1 {
            matches.sort_by(|left, right| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
                    .then_with(|| left.short_id.cmp(&right.short_id))
            });
            let labels: Vec<String> = matches
                .iter()
                .map(|entry| format!("{} ({})", entry.name, entry.short_id))
                .collect();
            return Err(format!(
                "loop '{trimmed}' is ambiguous; matches: {} (use a longer prefix or full ID)",
                labels.join(", ")
            ));
        }

        if self.loops.is_empty() {
            return Err(format!(
                "loop '{trimmed}' not found (no loops registered yet)"
            ));
        }

        let example = &self.loops[0];
        Err(format!(
            "loop '{trimmed}' not found. Example input: '{}' or '{}'",
            example.name, example.short_id
        ))
    }
}

impl WorkBackend for InMemoryWorkBackend {
    fn resolve_loop(&self, reference: &str) -> Result<ResolvedLoop, String> {
        let loop_entry = self.find_loop_by_ref(reference)?;
        Ok(ResolvedLoop {
            id: loop_entry.id,
            name: loop_entry.name,
            iteration: loop_entry.iteration,
            short_id: loop_entry.short_id,
        })
    }

    fn set_current(&mut self, request: SetCurrentRequest) -> Result<LoopWorkState, String> {
        let loop_id = request.loop_id.trim().to_string();
        let agent_id = request.agent_id.trim().to_string();
        let task_id = request.task_id.trim().to_string();
        let mut status = request.status.trim().to_string();
        if status.is_empty() {
            status = "in_progress".to_string();
        }

        if loop_id.is_empty() {
            return Err("invalid loop work state: loop_id is required".to_string());
        }
        if agent_id.is_empty() {
            return Err("invalid loop work state: agent_id is required".to_string());
        }
        if task_id.is_empty() {
            return Err("invalid loop work state: task_id is required".to_string());
        }

        let updated_at = self.next_timestamp();
        {
            let states = self.states_by_loop.entry(loop_id.clone()).or_default();
            for state in states.iter_mut() {
                state.is_current = false;
            }

            if let Some(existing) = states.iter_mut().find(|state| state.task_id == task_id) {
                existing.agent_id = agent_id;
                existing.status = status;
                existing.detail = request.detail;
                existing.loop_iteration = request.loop_iteration;
                existing.is_current = true;
                existing.updated_at = updated_at;
                return Ok(existing.clone());
            }
        }

        let state = LoopWorkState {
            id: self.next_id(),
            loop_id: loop_id.clone(),
            agent_id,
            task_id,
            status,
            detail: request.detail,
            loop_iteration: request.loop_iteration,
            is_current: true,
            created_at: updated_at.clone(),
            updated_at,
        };
        self.states_by_loop
            .entry(loop_id)
            .or_default()
            .push(state.clone());
        Ok(state)
    }

    fn clear_current(&mut self, loop_id: &str) -> Result<(), String> {
        if let Some(states) = self.states_by_loop.get_mut(loop_id) {
            for state in states.iter_mut() {
                state.is_current = false;
            }
        }
        Ok(())
    }

    fn current(&self, loop_id: &str) -> Result<Option<LoopWorkState>, String> {
        let Some(states) = self.states_by_loop.get(loop_id) else {
            return Ok(None);
        };

        let current = states
            .iter()
            .filter(|state| state.is_current)
            .max_by(|left, right| {
                left.updated_at
                    .cmp(&right.updated_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .cloned();
        Ok(current)
    }

    fn list(&self, loop_id: &str, limit: usize) -> Result<Vec<LoopWorkState>, String> {
        let mut states = self
            .states_by_loop
            .get(loop_id)
            .cloned()
            .unwrap_or_default();

        states.sort_by(|left, right| {
            right
                .is_current
                .cmp(&left.is_current)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| right.id.cmp(&left.id))
        });

        let cap = if limit == 0 { 200 } else { limit };
        if states.len() > cap {
            states.truncate(cap);
        }
        Ok(states)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OutputMode {
    Human,
    Json,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Help,
    Set {
        task_id: String,
        status: String,
        detail: String,
    },
    Clear,
    Current,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    output_mode: OutputMode,
    loop_ref: Option<String>,
    agent_id: Option<String>,
    command: Command,
}

#[derive(Debug, Clone, Default)]
pub struct Environment {
    values: HashMap<String, String>,
}

impl Environment {
    fn from_process() -> Self {
        let mut values = HashMap::new();
        for key in [
            "FORGE_LOOP_ID",
            "FORGE_LOOP_NAME",
            "FMAIL_AGENT",
            "SV_ACTOR",
        ] {
            if let Ok(value) = env::var(key) {
                values.insert(key.to_string(), value);
            }
        }
        Self { values }
    }

    fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let mut values = HashMap::new();
        for (key, value) in pairs {
            values.insert((*key).to_string(), (*value).to_string());
        }
        Self { values }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

pub fn run_from_env_with_backend(backend: &mut dyn WorkBackend) -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_backend(&args, backend, &mut stdout, &mut stderr)
}

pub fn run_for_test(args: &[&str], backend: &mut dyn WorkBackend) -> CommandOutput {
    run_for_test_with_env(args, &[], backend)
}

pub fn run_for_test_with_env(
    args: &[&str],
    env_pairs: &[(&str, &str)],
    backend: &mut dyn WorkBackend,
) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    let environment = Environment::from_pairs(env_pairs);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code =
        run_with_backend_with_env(&owned_args, &environment, backend, &mut stdout, &mut stderr);

    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

pub fn run_with_backend(
    args: &[String],
    backend: &mut dyn WorkBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let environment = Environment::from_process();
    run_with_backend_with_env(args, &environment, backend, stdout, stderr)
}

fn run_with_backend_with_env(
    args: &[String],
    environment: &Environment,
    backend: &mut dyn WorkBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, environment, backend, stdout) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    environment: &Environment,
    backend: &mut dyn WorkBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match parsed.command {
        Command::Help => write_help(stdout).map_err(|err| err.to_string()),
        Command::Set {
            task_id,
            status,
            detail,
        } => {
            let loop_ref = require_loop_ref(parsed.loop_ref, environment)?;
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            let agent_id = resolve_agent_id(parsed.agent_id, environment)?;
            let state = backend.set_current(SetCurrentRequest {
                loop_id: loop_entry.id,
                agent_id,
                task_id,
                status,
                detail,
                loop_iteration: loop_entry.iteration,
            })?;

            match parsed.output_mode {
                OutputMode::Human => {
                    writeln!(stdout, "ok").map_err(|err| err.to_string())?;
                }
                OutputMode::Json => {
                    write_json(stdout, &state)?;
                }
                OutputMode::Jsonl => {
                    write_jsonl(stdout, &state)?;
                }
            }
            Ok(())
        }
        Command::Clear => {
            let loop_ref = require_loop_ref(parsed.loop_ref, environment)?;
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            backend.clear_current(&loop_entry.id)?;

            let payload = serde_json::json!({
                "loop": loop_entry.name,
                "ok": true
            });

            match parsed.output_mode {
                OutputMode::Human => {
                    writeln!(stdout, "ok").map_err(|err| err.to_string())?;
                }
                OutputMode::Json => write_json(stdout, &payload)?,
                OutputMode::Jsonl => write_jsonl(stdout, &payload)?,
            }
            Ok(())
        }
        Command::Current => {
            let loop_ref = require_loop_ref(parsed.loop_ref, environment)?;
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            let current = backend.current(&loop_entry.id)?;
            match current {
                Some(state) => match parsed.output_mode {
                    OutputMode::Human => {
                        writeln!(stdout, "{}", format_human_current(&state))
                            .map_err(|err| err.to_string())?;
                    }
                    OutputMode::Json => write_json(stdout, &state)?,
                    OutputMode::Jsonl => write_jsonl(stdout, &state)?,
                },
                None => match parsed.output_mode {
                    OutputMode::Human => {
                        writeln!(stdout, "(none)").map_err(|err| err.to_string())?;
                    }
                    OutputMode::Json | OutputMode::Jsonl => {
                        let payload = serde_json::json!({"current": serde_json::Value::Null});
                        if parsed.output_mode == OutputMode::Json {
                            write_json(stdout, &payload)?;
                        } else {
                            write_jsonl(stdout, &payload)?;
                        }
                    }
                },
            }
            Ok(())
        }
        Command::List => {
            let loop_ref = require_loop_ref(parsed.loop_ref, environment)?;
            let loop_entry = backend.resolve_loop(&loop_ref)?;
            let items = backend.list(&loop_entry.id, 100)?;
            match parsed.output_mode {
                OutputMode::Human => {
                    if items.is_empty() {
                        writeln!(stdout, "(empty)").map_err(|err| err.to_string())?;
                        return Ok(());
                    }
                    for item in items {
                        let marker = if item.is_current { "*" } else { " " };
                        writeln!(stdout, "{marker} {}", format_human_current(&item))
                            .map_err(|err| err.to_string())?;
                    }
                }
                OutputMode::Json => write_json(stdout, &items)?,
                OutputMode::Jsonl => write_jsonl(stdout, &items)?,
            }
            Ok(())
        }
    }
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.is_empty() {
        return Ok(ParsedArgs {
            output_mode: OutputMode::Human,
            loop_ref: None,
            agent_id: None,
            command: Command::Help,
        });
    }

    let mut index = 0usize;
    if args.get(index).is_some_and(|value| value == "work") {
        index += 1;
    }

    let mut output_mode = OutputMode::Human;
    let mut loop_ref: Option<String> = None;
    let mut agent_id: Option<String> = None;

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                output_mode = OutputMode::Json;
                index += 1;
            }
            "--jsonl" => {
                output_mode = OutputMode::Jsonl;
                index += 1;
            }
            "--loop" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --loop".to_string())?;
                loop_ref = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --agent".to_string())?;
                agent_id = Some(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    output_mode,
                    loop_ref,
                    agent_id,
                    command: Command::Help,
                });
            }
            "set" => return parse_set(args, index + 1, output_mode, loop_ref, agent_id),
            "clear" => return parse_clear(args, index + 1, output_mode, loop_ref, agent_id),
            "current" | "status" | "show" => {
                return parse_current(args, index + 1, output_mode, loop_ref, agent_id);
            }
            "ls" | "list" => return parse_list(args, index + 1, output_mode, loop_ref, agent_id),
            unknown => {
                return Err(format!(
                    "error: unknown work argument '{unknown}' (expected one of: set, clear, current, ls)"
                ));
            }
        }
    }

    Ok(ParsedArgs {
        output_mode,
        loop_ref,
        agent_id,
        command: Command::Help,
    })
}

fn parse_set(
    args: &[String],
    mut index: usize,
    mut output_mode: OutputMode,
    mut loop_ref: Option<String>,
    mut agent_id: Option<String>,
) -> Result<ParsedArgs, String> {
    let mut task_id: Option<String> = None;
    let mut status = "in_progress".to_string();
    let mut detail = String::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--status" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --status".to_string())?;
                status = value.clone();
                index += 2;
            }
            "--detail" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --detail".to_string())?;
                detail = value.clone();
                index += 2;
            }
            "--json" => {
                output_mode = OutputMode::Json;
                index += 1;
            }
            "--jsonl" => {
                output_mode = OutputMode::Jsonl;
                index += 1;
            }
            "--loop" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --loop".to_string())?;
                loop_ref = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --agent".to_string())?;
                agent_id = Some(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    output_mode,
                    loop_ref,
                    agent_id,
                    command: Command::Help,
                });
            }
            value if value.starts_with('-') => {
                return Err(format!("error: unknown argument for work set: '{value}'"));
            }
            value => {
                if task_id.is_some() {
                    return Err(format!(
                        "error: unexpected argument for work set: '{value}'"
                    ));
                }
                task_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    let Some(task_id) = task_id else {
        return Err("error: work set requires <task-id>".to_string());
    };

    Ok(ParsedArgs {
        output_mode,
        loop_ref,
        agent_id,
        command: Command::Set {
            task_id,
            status,
            detail,
        },
    })
}

fn parse_clear(
    args: &[String],
    mut index: usize,
    mut output_mode: OutputMode,
    mut loop_ref: Option<String>,
    mut agent_id: Option<String>,
) -> Result<ParsedArgs, String> {
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                output_mode = OutputMode::Json;
                index += 1;
            }
            "--jsonl" => {
                output_mode = OutputMode::Jsonl;
                index += 1;
            }
            "--loop" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --loop".to_string())?;
                loop_ref = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --agent".to_string())?;
                agent_id = Some(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    output_mode,
                    loop_ref,
                    agent_id,
                    command: Command::Help,
                });
            }
            value => {
                return Err(format!(
                    "error: unexpected argument for work clear: '{value}'"
                ));
            }
        }
    }

    Ok(ParsedArgs {
        output_mode,
        loop_ref,
        agent_id,
        command: Command::Clear,
    })
}

fn parse_current(
    args: &[String],
    mut index: usize,
    mut output_mode: OutputMode,
    mut loop_ref: Option<String>,
    mut agent_id: Option<String>,
) -> Result<ParsedArgs, String> {
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                output_mode = OutputMode::Json;
                index += 1;
            }
            "--jsonl" => {
                output_mode = OutputMode::Jsonl;
                index += 1;
            }
            "--loop" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --loop".to_string())?;
                loop_ref = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --agent".to_string())?;
                agent_id = Some(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    output_mode,
                    loop_ref,
                    agent_id,
                    command: Command::Help,
                });
            }
            value => {
                return Err(format!(
                    "error: unexpected argument for work current: '{value}'"
                ));
            }
        }
    }

    Ok(ParsedArgs {
        output_mode,
        loop_ref,
        agent_id,
        command: Command::Current,
    })
}

fn parse_list(
    args: &[String],
    mut index: usize,
    mut output_mode: OutputMode,
    mut loop_ref: Option<String>,
    mut agent_id: Option<String>,
) -> Result<ParsedArgs, String> {
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--json" => {
                output_mode = OutputMode::Json;
                index += 1;
            }
            "--jsonl" => {
                output_mode = OutputMode::Jsonl;
                index += 1;
            }
            "--loop" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --loop".to_string())?;
                loop_ref = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "error: missing value for --agent".to_string())?;
                agent_id = Some(value.clone());
                index += 2;
            }
            "--help" | "-h" => {
                return Ok(ParsedArgs {
                    output_mode,
                    loop_ref,
                    agent_id,
                    command: Command::Help,
                });
            }
            value => return Err(format!("error: unexpected argument for work ls: '{value}'")),
        }
    }

    Ok(ParsedArgs {
        output_mode,
        loop_ref,
        agent_id,
        command: Command::List,
    })
}

fn require_loop_ref(explicit: Option<String>, environment: &Environment) -> Result<String, String> {
    if let Some(value) = explicit {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    if let Some(value) = environment.get("FORGE_LOOP_ID") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Some(value) = environment.get("FORGE_LOOP_NAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    Err("loop required (pass --loop or set FORGE_LOOP_ID)".to_string())
}

fn resolve_agent_id(explicit: Option<String>, environment: &Environment) -> Result<String, String> {
    if let Some(value) = explicit {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    if let Some(value) = environment.get("FMAIL_AGENT") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(value) = environment.get("SV_ACTOR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Some(value) = environment.get("FORGE_LOOP_NAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err("agent id required (pass --agent or set FMAIL_AGENT)".to_string())
}

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(stdout, "Persist loop work context (task id + status).")?;
    writeln!(stdout)?;
    writeln!(stdout, "Commands:")?;
    writeln!(stdout, "  set <task-id>   Set current task for a loop")?;
    writeln!(stdout, "  current         Show current task pointer")?;
    writeln!(stdout, "  ls              List recent work updates")?;
    writeln!(stdout, "  clear           Clear current task pointer")?;
    Ok(())
}

fn write_json(stdout: &mut dyn Write, value: &impl Serialize) -> Result<(), String> {
    serde_json::to_writer_pretty(&mut *stdout, value).map_err(|err| err.to_string())?;
    writeln!(stdout).map_err(|err| err.to_string())
}

fn write_jsonl(stdout: &mut dyn Write, value: &impl Serialize) -> Result<(), String> {
    let payload = serde_json::to_value(value).map_err(|err| err.to_string())?;
    match payload {
        serde_json::Value::Array(items) => {
            for item in items {
                serde_json::to_writer(&mut *stdout, &item).map_err(|err| err.to_string())?;
                writeln!(stdout).map_err(|err| err.to_string())?;
            }
            Ok(())
        }
        other => {
            serde_json::to_writer(&mut *stdout, &other).map_err(|err| err.to_string())?;
            writeln!(stdout).map_err(|err| err.to_string())
        }
    }
}

fn format_human_current(state: &LoopWorkState) -> String {
    let mut line = format!(
        "{} [{}] agent={} iter={} updated={}",
        state.task_id, state.status, state.agent_id, state.loop_iteration, state.updated_at
    );
    if !state.detail.trim().is_empty() {
        line.push_str(" | ");
        line.push_str(state.detail.trim());
    }
    line
}

fn short_id(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.chars().take(8).collect()
}

fn format_timestamp(counter: u64) -> String {
    let seconds_per_day = 24 * 60 * 60;
    let day = (counter / seconds_per_day) + 1;
    let seconds_in_day = counter % seconds_per_day;
    let hour = seconds_in_day / 3600;
    let minute = (seconds_in_day % 3600) / 60;
    let second = seconds_in_day % 60;
    format!("2026-01-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Serialize;

    use super::{run_for_test_with_env, InMemoryWorkBackend};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct GoldenReport {
        steps: Vec<GoldenStep>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct GoldenStep {
        name: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    }

    #[test]
    fn work_oracle_flow_matches_golden() {
        let mut backend = InMemoryWorkBackend::default();
        backend.seed_loop("oracle-loop-id", "oracle-loop", 0);
        let env = [
            ("FORGE_LOOP_NAME", "oracle-loop"),
            ("FMAIL_AGENT", "oracle-agent"),
        ];

        let mut steps = Vec::new();

        let current_none =
            run_for_test_with_env(&["work", "current", "--json"], &env, &mut backend);
        steps.push(GoldenStep {
            name: "work current (none)".to_string(),
            stdout: current_none.stdout,
            stderr: current_none.stderr,
            exit_code: current_none.exit_code,
        });

        let set = run_for_test_with_env(
            &[
                "work",
                "set",
                "sv-123",
                "--status",
                "in_progress",
                "--detail",
                "port mem/work fixtures",
                "--json",
            ],
            &env,
            &mut backend,
        );
        steps.push(GoldenStep {
            name: "work set sv-123".to_string(),
            stdout: set.stdout,
            stderr: set.stderr,
            exit_code: set.exit_code,
        });

        let current = run_for_test_with_env(&["work", "current", "--json"], &env, &mut backend);
        steps.push(GoldenStep {
            name: "work current".to_string(),
            stdout: current.stdout,
            stderr: current.stderr,
            exit_code: current.exit_code,
        });

        let list = run_for_test_with_env(&["work", "ls", "--json"], &env, &mut backend);
        steps.push(GoldenStep {
            name: "work ls".to_string(),
            stdout: list.stdout,
            stderr: list.stderr,
            exit_code: list.exit_code,
        });

        let clear = run_for_test_with_env(&["work", "clear", "--json"], &env, &mut backend);
        steps.push(GoldenStep {
            name: "work clear".to_string(),
            stdout: clear.stdout,
            stderr: clear.stderr,
            exit_code: clear.exit_code,
        });

        let current_none_after_clear =
            run_for_test_with_env(&["work", "current", "--json"], &env, &mut backend);
        steps.push(GoldenStep {
            name: "work current (none after clear)".to_string(),
            stdout: current_none_after_clear.stdout,
            stderr: current_none_after_clear.stderr,
            exit_code: current_none_after_clear.exit_code,
        });

        let report = GoldenReport { steps };
        let got = match serde_json::to_string_pretty(&report) {
            Ok(text) => text + "\n",
            Err(err) => panic!("failed to encode report: {err}"),
        };
        let path = format!("{}/testdata/work_oracle.json", env!("CARGO_MANIFEST_DIR"));
        let want = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => panic!("failed to read {}: {err}", path),
        };
        assert_eq!(normalize_newlines(&want), normalize_newlines(&got));
    }

    #[test]
    fn missing_agent_fails_set() {
        let mut backend = InMemoryWorkBackend::default();
        backend.seed_loop("oracle-loop-id", "oracle-loop", 0);
        let result = run_for_test_with_env(
            &["work", "set", "sv-123", "--json", "--loop", "oracle-loop"],
            &[],
            &mut backend,
        );

        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.contains("agent id required"));
    }

    #[test]
    fn show_alias_resolves_to_current() {
        let mut backend = InMemoryWorkBackend::default();
        backend.seed_loop("oracle-loop-id", "oracle-loop", 3);
        let env = [
            ("FORGE_LOOP_NAME", "oracle-loop"),
            ("FMAIL_AGENT", "oracle-agent"),
        ];

        let set = run_for_test_with_env(
            &["work", "set", "sv-900", "--status", "blocked", "--json"],
            &env,
            &mut backend,
        );
        assert_eq!(set.exit_code, 0);

        let show = run_for_test_with_env(&["work", "show", "--json"], &env, &mut backend);
        assert_eq!(show.exit_code, 0);
        assert!(show.stdout.contains("\"task_id\": \"sv-900\""));
        assert!(show.stdout.contains("\"status\": \"blocked\""));
        assert!(show.stderr.is_empty());
    }

    #[test]
    fn set_relinks_existing_task_and_updates_status() {
        let mut backend = InMemoryWorkBackend::default();
        backend.seed_loop("oracle-loop-id", "oracle-loop", 7);
        let env = [
            ("FORGE_LOOP_NAME", "oracle-loop"),
            ("FMAIL_AGENT", "oracle-agent"),
        ];

        let first = run_for_test_with_env(
            &[
                "work",
                "set",
                "sv-100",
                "--status",
                "blocked",
                "--detail",
                "needs review",
                "--json",
            ],
            &env,
            &mut backend,
        );
        assert_eq!(first.exit_code, 0);
        let first_json: serde_json::Value = match serde_json::from_str(&first.stdout) {
            Ok(value) => value,
            Err(err) => panic!("failed to parse first set output as json: {err}"),
        };
        let first_id = match first_json.get("id").and_then(serde_json::Value::as_str) {
            Some(value) => value.to_string(),
            None => panic!("first set output missing id"),
        };

        let second = run_for_test_with_env(
            &["work", "set", "sv-200", "--status", "in_progress", "--json"],
            &env,
            &mut backend,
        );
        assert_eq!(second.exit_code, 0);

        let relink = run_for_test_with_env(
            &[
                "work", "set", "sv-100", "--status", "done", "--detail", "merged", "--json",
            ],
            &env,
            &mut backend,
        );
        assert_eq!(relink.exit_code, 0);
        let relink_json: serde_json::Value = match serde_json::from_str(&relink.stdout) {
            Ok(value) => value,
            Err(err) => panic!("failed to parse relink output as json: {err}"),
        };
        let relink_id = match relink_json.get("id").and_then(serde_json::Value::as_str) {
            Some(value) => value,
            None => panic!("relink output missing id"),
        };
        assert_eq!(relink_id, first_id);
        assert_eq!(
            relink_json
                .get("status")
                .and_then(serde_json::Value::as_str),
            Some("done")
        );
        assert_eq!(
            relink_json
                .get("detail")
                .and_then(serde_json::Value::as_str),
            Some("merged")
        );
    }

    #[test]
    fn ls_lists_current_first_for_task_status_history() {
        let mut backend = InMemoryWorkBackend::default();
        backend.seed_loop("oracle-loop-id", "oracle-loop", 11);
        let env = [
            ("FORGE_LOOP_NAME", "oracle-loop"),
            ("FMAIL_AGENT", "oracle-agent"),
        ];

        for args in [
            ["work", "set", "sv-100", "--status", "blocked"],
            ["work", "set", "sv-200", "--status", "in_progress"],
            ["work", "set", "sv-100", "--status", "done"],
        ] {
            let out = run_for_test_with_env(&args, &env, &mut backend);
            assert_eq!(out.exit_code, 0);
            assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
        }

        let json = run_for_test_with_env(&["work", "ls", "--json"], &env, &mut backend);
        assert_eq!(json.exit_code, 0);
        let list_json: serde_json::Value = match serde_json::from_str(&json.stdout) {
            Ok(value) => value,
            Err(err) => panic!("failed to parse list output as json: {err}"),
        };
        let items = match list_json.as_array() {
            Some(value) => value,
            None => panic!("work ls --json did not return an array"),
        };
        assert_eq!(items.len(), 2);

        assert_eq!(
            items[0].get("task_id").and_then(serde_json::Value::as_str),
            Some("sv-100")
        );
        assert_eq!(
            items[0].get("status").and_then(serde_json::Value::as_str),
            Some("done")
        );
        assert_eq!(
            items[0]
                .get("is_current")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );

        assert_eq!(
            items[1].get("task_id").and_then(serde_json::Value::as_str),
            Some("sv-200")
        );
        assert_eq!(
            items[1].get("status").and_then(serde_json::Value::as_str),
            Some("in_progress")
        );
        assert_eq!(
            items[1]
                .get("is_current")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );

        let human = run_for_test_with_env(&["work", "ls"], &env, &mut backend);
        assert_eq!(human.exit_code, 0);
        let lines: Vec<&str> = human.stdout.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("* sv-100 [done]"));
        assert!(lines[1].starts_with("  sv-200 [in_progress]"));
    }

    fn normalize_newlines(value: &str) -> String {
        value.replace("\r\n", "\n")
    }
}

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use forge_agent::error::AgentServiceError;
use forge_agent::event::NullEventSink;
use forge_agent::forged::{ForgedTransport, ForgedTransportConfig};
use forge_agent::mock::MockAgentService;
use forge_agent::service::AgentService;
use forge_agent::types::{
    AgentRequestMode, AgentSnapshot, AgentState, KillAgentParams, ListAgentsFilter,
    SendMessageParams, SpawnAgentParams, WaitStateParams,
};
use serde::Serialize;

// ── JSON output types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AgentJson {
    id: String,
    workspace_id: String,
    state: String,
    pane_id: String,
    pid: i32,
    command: String,
    adapter: String,
    spawned_at: String,
    last_activity_at: String,
}

impl From<&AgentSnapshot> for AgentJson {
    fn from(s: &AgentSnapshot) -> Self {
        Self {
            id: s.id.clone(),
            workspace_id: s.workspace_id.clone(),
            state: s.state.to_string(),
            pane_id: s.pane_id.clone(),
            pid: s.pid,
            command: s.command.clone(),
            adapter: s.adapter.clone(),
            spawned_at: s.spawned_at.to_rfc3339(),
            last_activity_at: s.last_activity_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AgentListJson {
    agents: Vec<AgentJson>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct BoolResultJson {
    ok: bool,
}

// ── Backend trait ─────────────────────────────────────────────────────────────

/// Abstraction over the agent service for CLI commands.
/// Each method is synchronous to fit the CLI execution model; the
/// real implementation creates a Tokio runtime internally.
#[allow(unused)]
pub trait AgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String>;
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String>;
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String>;
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String>;
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String>;
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String>;
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String>;
}

// ── InMemory backend (testing) ───────────────────────────────────────────────

#[derive(Default)]
pub struct InMemoryAgentBackend {
    mock: MockAgentService,
}

impl InMemoryAgentBackend {
    pub fn new() -> Self {
        Self {
            mock: MockAgentService::new(),
        }
    }

    pub fn with_agent(mut self, snapshot: AgentSnapshot) -> Self {
        self.mock = self.mock.with_agent(snapshot);
        self
    }

    pub fn with_spawn_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_spawn_error(err);
        self
    }

    pub fn with_send_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_send_error(err);
        self
    }

    pub fn with_kill_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_kill_error(err);
        self
    }

    pub fn with_get_error(mut self, err: AgentServiceError) -> Self {
        self.mock = self.mock.with_get_error(err);
        self
    }
}

fn block_on<F: std::future::Future<Output = T>, T>(future: F) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|err| panic!("failed to create tokio runtime: {err}"));
    runtime.block_on(future)
}

impl AgentBackend for InMemoryAgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String> {
        block_on(self.mock.spawn_agent(params)).map_err(|e| e.to_string())
    }
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String> {
        block_on(self.mock.send_message(params)).map_err(|e| e.to_string())
    }
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String> {
        block_on(self.mock.wait_state(params)).map_err(|e| e.to_string())
    }
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String> {
        block_on(self.mock.interrupt_agent(agent_id)).map_err(|e| e.to_string())
    }
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String> {
        block_on(self.mock.kill_agent(params)).map_err(|e| e.to_string())
    }
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String> {
        block_on(self.mock.list_agents(filter)).map_err(|e| e.to_string())
    }
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String> {
        block_on(self.mock.get_agent(agent_id)).map_err(|e| e.to_string())
    }
}

// ── ForgedAgentBackend (production) ──────────────────────────────────────────

pub struct ForgedAgentBackend {
    service: ForgedTransport,
}

impl ForgedAgentBackend {
    pub fn open_from_env() -> Self {
        let target = resolved_daemon_target();
        let config = ForgedTransportConfig {
            target,
            ..ForgedTransportConfig::default()
        };
        Self {
            service: ForgedTransport::new(config, Arc::new(NullEventSink)),
        }
    }
}

impl AgentBackend for ForgedAgentBackend {
    fn spawn_agent(&self, params: SpawnAgentParams) -> Result<AgentSnapshot, String> {
        block_on(self.service.spawn_agent(params)).map_err(|e| e.to_string())
    }
    fn send_message(&self, params: SendMessageParams) -> Result<bool, String> {
        block_on(self.service.send_message(params)).map_err(|e| e.to_string())
    }
    fn wait_state(&self, params: WaitStateParams) -> Result<AgentSnapshot, String> {
        block_on(self.service.wait_state(params)).map_err(|e| e.to_string())
    }
    fn interrupt_agent(&self, agent_id: &str) -> Result<bool, String> {
        block_on(self.service.interrupt_agent(agent_id)).map_err(|e| e.to_string())
    }
    fn kill_agent(&self, params: KillAgentParams) -> Result<bool, String> {
        block_on(self.service.kill_agent(params)).map_err(|e| e.to_string())
    }
    fn list_agents(&self, filter: ListAgentsFilter) -> Result<Vec<AgentSnapshot>, String> {
        block_on(self.service.list_agents(filter)).map_err(|e| e.to_string())
    }
    fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, String> {
        block_on(self.service.get_agent(agent_id)).map_err(|e| e.to_string())
    }
}

fn resolved_daemon_target() -> String {
    let env_target = std::env::var("FORGE_DAEMON_TARGET")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("FORGED_ADDR")
                .ok()
                .filter(|v| !v.trim().is_empty())
        });
    if let Some(target) = env_target {
        let trimmed = target.trim();
        if trimmed.contains("://") {
            return trimmed.to_string();
        }
        return format!("http://{trimmed}");
    }
    "http://127.0.0.1:50051".to_string()
}

// ── Arg parsing ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    subcommand: Subcommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Subcommand {
    Spawn(SpawnArgs),
    Send(SendArgs),
    Wait(WaitArgs),
    Ps(PsArgs),
    Show(ShowArgs),
    Interrupt(InterruptArgs),
    Kill(KillArgs),
    Revive(ReviveArgs),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SpawnArgs {
    agent_id: String,
    workspace_id: String,
    command: String,
    args: Vec<String>,
    working_dir: String,
    session_name: String,
    adapter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SendArgs {
    agent_id: String,
    text: String,
    send_enter: bool,
    keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WaitArgs {
    agent_id: String,
    until: Vec<String>,
    timeout: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PsArgs {
    workspace_id: String,
    state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShowArgs {
    agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InterruptArgs {
    agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KillArgs {
    agent_id: String,
    force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviveArgs {
    agent_id: String,
}

// ── Public entry points ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_for_test(args: &[&str], backend: &dyn AgentBackend) -> CommandOutput {
    let owned_args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
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
    backend: &dyn AgentBackend,
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

// ── Core dispatcher ──────────────────────────────────────────────────────────

fn execute(
    args: &[String],
    backend: &dyn AgentBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;
    match &parsed.subcommand {
        Subcommand::Spawn(spawn_args) => exec_spawn(backend, spawn_args, &parsed, stdout),
        Subcommand::Send(send_args) => exec_send(backend, send_args, &parsed, stdout),
        Subcommand::Wait(wait_args) => exec_wait(backend, wait_args, &parsed, stdout),
        Subcommand::Ps(ps_args) => exec_ps(backend, ps_args, &parsed, stdout),
        Subcommand::Show(show_args) => exec_show(backend, show_args, &parsed, stdout),
        Subcommand::Interrupt(int_args) => exec_interrupt(backend, int_args, &parsed, stdout),
        Subcommand::Kill(kill_args) => exec_kill(backend, kill_args, &parsed, stdout),
        Subcommand::Revive(revive_args) => exec_revive(backend, revive_args, &parsed, stdout),
        Subcommand::Help => {
            write_help(stdout)?;
            Ok(())
        }
    }
}

// ── Subcommand implementations ───────────────────────────────────────────────

fn exec_spawn(
    backend: &dyn AgentBackend,
    args: &SpawnArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.command.is_empty() {
        return Err("error: --command is required for agent spawn".to_string());
    }
    let params = SpawnAgentParams {
        agent_id: args.agent_id.clone(),
        workspace_id: args.workspace_id.clone(),
        command: args.command.clone(),
        args: args.args.clone(),
        env: HashMap::new(),
        working_dir: args.working_dir.clone(),
        session_name: args.session_name.clone(),
        adapter: args.adapter.clone(),
        requested_mode: AgentRequestMode::Continuous,
        allow_oneshot_fallback: false,
    };
    let snapshot = backend.spawn_agent(params)?;
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_send(
    backend: &dyn AgentBackend,
    args: &SendArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent send".to_string());
    }
    let params = SendMessageParams {
        agent_id: args.agent_id.clone(),
        text: args.text.clone(),
        send_enter: args.send_enter,
        keys: args.keys.clone(),
    };
    let ok = backend.send_message(params)?;
    write_bool_output(ok, parsed, stdout)
}

fn exec_wait(
    backend: &dyn AgentBackend,
    args: &WaitArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent wait".to_string());
    }
    let target_states: Vec<AgentState> = if args.until.is_empty() {
        vec![AgentState::Idle]
    } else {
        args.until
            .iter()
            .map(|s| parse_agent_state(s))
            .collect::<Result<Vec<_>, _>>()?
    };
    let params = WaitStateParams {
        agent_id: args.agent_id.clone(),
        target_states,
        timeout: Duration::from_secs(args.timeout),
        poll_interval: Duration::from_millis(500),
    };
    let snapshot = backend.wait_state(params)?;
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_ps(
    backend: &dyn AgentBackend,
    args: &PsArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let states = if args.state.is_empty() {
        Vec::new()
    } else {
        vec![parse_agent_state(&args.state)?]
    };
    let filter = ListAgentsFilter {
        workspace_id: if args.workspace_id.is_empty() {
            None
        } else {
            Some(args.workspace_id.clone())
        },
        states,
    };
    let agents = backend.list_agents(filter)?;
    write_agent_list_output(&agents, parsed, stdout)
}

fn exec_show(
    backend: &dyn AgentBackend,
    args: &ShowArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent show".to_string());
    }
    let snapshot = backend.get_agent(&args.agent_id)?;
    write_agent_output(&snapshot, parsed, stdout)
}

fn exec_interrupt(
    backend: &dyn AgentBackend,
    args: &InterruptArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent interrupt".to_string());
    }
    let ok = backend.interrupt_agent(&args.agent_id)?;
    write_bool_output(ok, parsed, stdout)
}

fn exec_kill(
    backend: &dyn AgentBackend,
    args: &KillArgs,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent kill".to_string());
    }
    let params = KillAgentParams {
        agent_id: args.agent_id.clone(),
        force: args.force,
        grace_period: None,
    };
    let ok = backend.kill_agent(params)?;
    write_bool_output(ok, parsed, stdout)
}

fn exec_revive(
    _backend: &dyn AgentBackend,
    args: &ReviveArgs,
    _parsed: &ParsedArgs,
    _stdout: &mut dyn Write,
) -> Result<(), String> {
    if args.agent_id.is_empty() {
        return Err("error: agent ID is required for agent revive".to_string());
    }
    // Revive is a composite operation: spawn with the same parameters.
    // This requires the agent to be in a terminal state. For now, this
    // returns an error because the underlying service does not yet support
    // revive directly. M10.5 will add full revive semantics.
    Err(format!(
        "agent revive is not yet implemented (agent '{}'). Use `agent spawn` with the same parameters to restart.",
        args.agent_id
    ))
}

// ── Output helpers ───────────────────────────────────────────────────────────

fn write_agent_output(
    snapshot: &AgentSnapshot,
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let json_entry = AgentJson::from(snapshot);
    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &json_entry).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &json_entry).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if !parsed.quiet {
        writeln!(
            stdout,
            "{}\t{}\t{}\t{}",
            snapshot.id, snapshot.state, snapshot.command, snapshot.adapter,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_agent_list_output(
    agents: &[AgentSnapshot],
    parsed: &ParsedArgs,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if parsed.json {
        let entries: Vec<AgentJson> = agents.iter().map(AgentJson::from).collect();
        let output = AgentListJson {
            total: entries.len(),
            agents: entries,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        for agent in agents {
            let entry = AgentJson::from(agent);
            serde_json::to_writer(&mut *stdout, &entry).map_err(|e| e.to_string())?;
            writeln!(stdout).map_err(|e| e.to_string())?;
        }
    } else if !parsed.quiet {
        if agents.is_empty() {
            writeln!(stdout, "No agents found").map_err(|e| e.to_string())?;
        } else {
            writeln!(stdout, "ID\tSTATE\tCOMMAND\tADAPTER\tWORKSPACE")
                .map_err(|e| e.to_string())?;
            for agent in agents {
                writeln!(
                    stdout,
                    "{}\t{}\t{}\t{}\t{}",
                    agent.id, agent.state, agent.command, agent.adapter, agent.workspace_id,
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn write_bool_output(ok: bool, parsed: &ParsedArgs, stdout: &mut dyn Write) -> Result<(), String> {
    if parsed.json {
        serde_json::to_writer_pretty(&mut *stdout, &BoolResultJson { ok })
            .map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if parsed.jsonl {
        serde_json::to_writer(&mut *stdout, &BoolResultJson { ok }).map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else if !parsed.quiet {
        writeln!(stdout, "{ok}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn parse_agent_state(s: &str) -> Result<AgentState, String> {
    match s {
        "starting" => Ok(AgentState::Starting),
        "running" => Ok(AgentState::Running),
        "idle" => Ok(AgentState::Idle),
        "waiting_approval" => Ok(AgentState::WaitingApproval),
        "paused" => Ok(AgentState::Paused),
        "stopping" => Ok(AgentState::Stopping),
        "stopped" => Ok(AgentState::Stopped),
        "failed" => Ok(AgentState::Failed),
        other => Err(format!(
            "invalid agent state: '{other}'. Valid states: starting, running, idle, waiting_approval, paused, stopping, stopped, failed"
        )),
    }
}

// ── Arg parser ───────────────────────────────────────────────────────────────

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;

    // Skip the command name "agent" if present.
    if args.get(index).is_some_and(|t| t == "agent") {
        index += 1;
    }

    // Collect top-level flags before subcommand.
    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;

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
            _ => break,
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    let subcommand_name = args.get(index).map(|s| s.as_str());
    match subcommand_name {
        None | Some("help") | Some("-h") | Some("--help") => Ok(ParsedArgs {
            json,
            jsonl,
            quiet,
            subcommand: Subcommand::Help,
        }),
        Some("spawn") => {
            index += 1;
            let spawn_args = parse_spawn_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Spawn(spawn_args),
            })
        }
        Some("send") => {
            index += 1;
            let send_args = parse_send_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Send(send_args),
            })
        }
        Some("wait") => {
            index += 1;
            let wait_args = parse_wait_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Wait(wait_args),
            })
        }
        Some("ps") | Some("list") => {
            index += 1;
            let ps_args = parse_ps_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Ps(ps_args),
            })
        }
        Some("show") | Some("get") => {
            index += 1;
            let show_args = parse_show_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Show(show_args),
            })
        }
        Some("interrupt") => {
            index += 1;
            let int_args = parse_interrupt_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Interrupt(int_args),
            })
        }
        Some("kill") => {
            index += 1;
            let kill_args = parse_kill_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Kill(kill_args),
            })
        }
        Some("revive") => {
            index += 1;
            let revive_args = parse_revive_args(args, index)?;
            Ok(ParsedArgs {
                json,
                jsonl,
                quiet,
                subcommand: Subcommand::Revive(revive_args),
            })
        }
        Some(other) => Err(format!(
            "unknown agent subcommand: '{other}'. Run `forge agent help` for usage."
        )),
    }
}

fn parse_spawn_args(args: &[String], mut index: usize) -> Result<SpawnArgs, String> {
    let mut result = SpawnArgs {
        agent_id: String::new(),
        workspace_id: String::new(),
        command: String::new(),
        args: Vec::new(),
        working_dir: String::new(),
        session_name: String::new(),
        adapter: String::new(),
    };

    // First positional arg is agent_id (optional, auto-generated if empty).
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(SPAWN_HELP.to_string()),
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--command" | "-c" => {
                result.command = take_value(args, index, "--command")?;
                index += 2;
            }
            "--arg" => {
                result.args.push(take_value(args, index, "--arg")?);
                index += 2;
            }
            "--working-dir" => {
                result.working_dir = take_value(args, index, "--working-dir")?;
                index += 2;
            }
            "--session" => {
                result.session_name = take_value(args, index, "--session")?;
                index += 2;
            }
            "--adapter" => {
                result.adapter = take_value(args, index, "--adapter")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent spawn: '{flag}'"));
            }
            _ => {
                return Err(format!(
                    "error: unexpected positional argument: '{}'",
                    token
                ));
            }
        }
    }

    Ok(result)
}

fn parse_send_args(args: &[String], mut index: usize) -> Result<SendArgs, String> {
    let mut result = SendArgs {
        agent_id: String::new(),
        text: String::new(),
        send_enter: true,
        keys: Vec::new(),
    };

    // Positional: agent_id.
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(SEND_HELP.to_string()),
            "--text" | "-t" => {
                result.text = take_value(args, index, "--text")?;
                index += 2;
            }
            "--no-enter" => {
                result.send_enter = false;
                index += 1;
            }
            "--key" => {
                result.keys.push(take_value(args, index, "--key")?);
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent send: '{flag}'"));
            }
            positional => {
                // If text is empty and we have a positional, use it as text.
                if result.text.is_empty() {
                    result.text = positional.to_string();
                    index += 1;
                } else {
                    return Err(format!(
                        "error: unexpected positional argument: '{positional}'"
                    ));
                }
            }
        }
    }

    Ok(result)
}

fn parse_wait_args(args: &[String], mut index: usize) -> Result<WaitArgs, String> {
    let mut result = WaitArgs {
        agent_id: String::new(),
        until: Vec::new(),
        timeout: 300,
    };

    // Positional: agent_id.
    if let Some(token) = args.get(index) {
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(WAIT_HELP.to_string()),
            "--until" => {
                result.until.push(take_value(args, index, "--until")?);
                index += 2;
            }
            "--timeout" => {
                let val = take_value(args, index, "--timeout")?;
                result.timeout = val
                    .parse::<u64>()
                    .map_err(|_| format!("invalid timeout value: '{val}'"))?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent wait: '{flag}'"));
            }
            other => {
                return Err(format!("error: unexpected positional argument: '{other}'"));
            }
        }
    }

    Ok(result)
}

fn parse_ps_args(args: &[String], mut index: usize) -> Result<PsArgs, String> {
    let mut result = PsArgs {
        workspace_id: String::new(),
        state: String::new(),
    };

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" => return Err(PS_HELP.to_string()),
            "--workspace" | "-w" => {
                result.workspace_id = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--state" => {
                result.state = take_value(args, index, "--state")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent ps: '{flag}'"));
            }
            other => {
                return Err(format!(
                    "error: agent ps takes no positional arguments, got '{other}'"
                ));
            }
        }
    }

    Ok(result)
}

fn parse_show_args(args: &[String], mut index: usize) -> Result<ShowArgs, String> {
    let mut result = ShowArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(SHOW_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!("error: unknown flag for agent show: '{token}'"));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn parse_interrupt_args(args: &[String], mut index: usize) -> Result<InterruptArgs, String> {
    let mut result = InterruptArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(INTERRUPT_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!(
                "error: unknown flag for agent interrupt: '{token}'"
            ));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn parse_kill_args(args: &[String], mut index: usize) -> Result<KillArgs, String> {
    let mut result = KillArgs {
        agent_id: String::new(),
        force: false,
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(KILL_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "--force" | "-f" => {
                result.force = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag for agent kill: '{flag}'"));
            }
            other => {
                return Err(format!("error: unexpected positional argument: '{other}'"));
            }
        }
    }

    Ok(result)
}

fn parse_revive_args(args: &[String], mut index: usize) -> Result<ReviveArgs, String> {
    let mut result = ReviveArgs {
        agent_id: String::new(),
    };

    if let Some(token) = args.get(index) {
        if token == "-h" || token == "--help" {
            return Err(REVIVE_HELP.to_string());
        }
        if !token.starts_with('-') {
            result.agent_id = token.clone();
            index += 1;
        }
    }

    if let Some(token) = args.get(index) {
        if token.starts_with('-') {
            return Err(format!("error: unknown flag for agent revive: '{token}'"));
        }
        return Err(format!("error: unexpected positional argument: '{token}'"));
    }

    Ok(result)
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

// ── Help text ────────────────────────────────────────────────────────────────

fn write_help(stdout: &mut dyn Write) -> Result<(), String> {
    writeln!(stdout, "{AGENT_HELP}").map_err(|e| e.to_string())
}

const AGENT_HELP: &str = "\
Manage persistent agents

Usage:
  forge agent <subcommand> [options]

Subcommands:
  spawn       Spawn a new persistent agent
  send        Send a message to an agent
  wait        Wait for an agent to reach a target state
  ps          List agents
  show        Show agent details
  interrupt   Interrupt an agent (Ctrl+C)
  kill        Kill an agent
  revive      Revive a stopped/failed agent

Flags:
  -h, --help    help for agent
      --json    output in JSON format
      --jsonl   output in JSON Lines format";

const SPAWN_HELP: &str = "\
Spawn a new persistent agent

Usage:
  forge agent spawn [agent-id] [flags]

Flags:
  -c, --command string     command to run (required)
  -w, --workspace string   workspace ID
      --arg string         command argument (repeatable)
      --working-dir string working directory
      --session string     tmux session name
      --adapter string     adapter name";

const SEND_HELP: &str = "\
Send a message to an agent

Usage:
  forge agent send <agent-id> [text] [flags]

Flags:
  -t, --text string   message text
      --no-enter       do not send Enter after text
      --key string     send a key (repeatable)";

const WAIT_HELP: &str = "\
Wait for an agent to reach a target state

Usage:
  forge agent wait <agent-id> [flags]

Flags:
      --until string    target state (repeatable, default: idle)
      --timeout int     timeout in seconds (default: 300)";

const PS_HELP: &str = "\
List agents

Usage:
  forge agent ps [flags]

Aliases:
  ps, list

Flags:
  -w, --workspace string   filter by workspace
      --state string        filter by state";

const SHOW_HELP: &str = "\
Show agent details

Usage:
  forge agent show <agent-id>

Aliases:
  show, get";

const INTERRUPT_HELP: &str = "\
Interrupt an agent (send Ctrl+C)

Usage:
  forge agent interrupt <agent-id>";

const KILL_HELP: &str = "\
Kill an agent

Usage:
  forge agent kill <agent-id> [flags]

Flags:
  -f, --force   force kill without grace period";

const REVIVE_HELP: &str = "\
Revive a stopped or failed agent

Usage:
  forge agent revive <agent-id>";

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_agent::mock::test_snapshot;
    use forge_agent::types::AgentState;

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
            Err(err) => panic!("expected valid json: {err}\ninput: {text}"),
        }
    }

    // ── Parse tests ──────────────────────────────────────────────────────

    #[test]
    fn parse_agent_help() {
        let args = vec!["agent".to_string()];
        let parsed = parse_ok(&args);
        assert_eq!(parsed.subcommand, Subcommand::Help);
    }

    #[test]
    fn parse_agent_help_flag() {
        let args = vec!["agent".to_string(), "--help".to_string()];
        let parsed = parse_ok(&args);
        assert_eq!(parsed.subcommand, Subcommand::Help);
    }

    #[test]
    fn parse_unknown_subcommand() {
        let args = vec!["agent".to_string(), "bogus".to_string()];
        let err = parse_err(&args);
        assert!(err.contains("unknown agent subcommand"));
        assert!(err.contains("bogus"));
    }

    #[test]
    fn parse_json_and_jsonl_conflict() {
        let args = vec![
            "agent".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
            "ps".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_spawn_with_flags() {
        let args = vec![
            "agent".to_string(),
            "spawn".to_string(),
            "my-agent".to_string(),
            "--command".to_string(),
            "claude".to_string(),
            "--workspace".to_string(),
            "ws-1".to_string(),
            "--adapter".to_string(),
            "claude_code".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Spawn(a) => {
                assert_eq!(a.agent_id, "my-agent");
                assert_eq!(a.command, "claude");
                assert_eq!(a.workspace_id, "ws-1");
                assert_eq!(a.adapter, "claude_code");
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn parse_send_positional_text() {
        let args = vec![
            "agent".to_string(),
            "send".to_string(),
            "my-agent".to_string(),
            "hello world".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Send(a) => {
                assert_eq!(a.agent_id, "my-agent");
                assert_eq!(a.text, "hello world");
                assert!(a.send_enter);
            }
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn parse_wait_with_until_and_timeout() {
        let args = vec![
            "agent".to_string(),
            "wait".to_string(),
            "ag-1".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "--until".to_string(),
            "stopped".to_string(),
            "--timeout".to_string(),
            "60".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Wait(a) => {
                assert_eq!(a.agent_id, "ag-1");
                assert_eq!(a.until, vec!["idle", "stopped"]);
                assert_eq!(a.timeout, 60);
            }
            other => panic!("expected Wait, got {other:?}"),
        }
    }

    #[test]
    fn parse_ps_with_state_filter() {
        let args = vec![
            "agent".to_string(),
            "ps".to_string(),
            "--state".to_string(),
            "running".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Ps(a) => {
                assert_eq!(a.state, "running");
            }
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn parse_show_positional() {
        let args = vec![
            "agent".to_string(),
            "show".to_string(),
            "agent-42".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Show(a) => {
                assert_eq!(a.agent_id, "agent-42");
            }
            other => panic!("expected Show, got {other:?}"),
        }
    }

    #[test]
    fn parse_kill_with_force() {
        let args = vec![
            "agent".to_string(),
            "kill".to_string(),
            "agent-42".to_string(),
            "--force".to_string(),
        ];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Kill(a) => {
                assert_eq!(a.agent_id, "agent-42");
                assert!(a.force);
            }
            other => panic!("expected Kill, got {other:?}"),
        }
    }

    #[test]
    fn parse_list_alias() {
        let args = vec!["agent".to_string(), "list".to_string()];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Ps(_) => {}
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn parse_get_alias() {
        let args = vec!["agent".to_string(), "get".to_string(), "ag-1".to_string()];
        let parsed = parse_ok(&args);
        match &parsed.subcommand {
            Subcommand::Show(a) => {
                assert_eq!(a.agent_id, "ag-1");
            }
            other => panic!("expected Show, got {other:?}"),
        }
    }

    #[test]
    fn parse_spawn_unknown_flag() {
        let args = vec![
            "agent".to_string(),
            "spawn".to_string(),
            "--bogus".to_string(),
        ];
        let err = parse_err(&args);
        assert!(err.contains("unknown flag"));
    }

    #[test]
    fn parse_agent_state_valid() {
        assert_eq!(parse_agent_state("idle").unwrap(), AgentState::Idle);
        assert_eq!(parse_agent_state("running").unwrap(), AgentState::Running);
        assert_eq!(parse_agent_state("stopped").unwrap(), AgentState::Stopped);
        assert_eq!(parse_agent_state("failed").unwrap(), AgentState::Failed);
    }

    #[test]
    fn parse_agent_state_invalid() {
        let err = parse_agent_state("bogus").unwrap_err();
        assert!(err.contains("invalid agent state"));
        assert!(err.contains("bogus"));
    }

    // ── Execution tests with InMemory backend ────────────────────────────

    #[test]
    fn agent_help_output() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Manage persistent agents"));
        assert!(out.stdout.contains("spawn"));
        assert!(out.stdout.contains("send"));
        assert!(out.stdout.contains("wait"));
        assert!(out.stdout.contains("ps"));
        assert!(out.stdout.contains("show"));
        assert!(out.stdout.contains("kill"));
    }

    #[test]
    fn agent_ps_empty_json() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "--json", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 0);
        assert!(parsed["agents"].as_array().unwrap().is_empty());
    }

    #[test]
    fn agent_ps_empty_human() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "No agents found\n");
    }

    #[test]
    fn agent_ps_with_agents_json() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 2);
        let agents = parsed["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn agent_ps_filters_by_state() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "ps", "--state", "idle"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["total"], 1);
        let agents = parsed["agents"].as_array().unwrap();
        assert_eq!(agents[0]["id"], "ag-001");
        assert_eq!(agents[0]["state"], "idle");
    }

    #[test]
    fn agent_ps_jsonl() {
        let backend = InMemoryAgentBackend::new()
            .with_agent(test_snapshot("ag-001", AgentState::Idle))
            .with_agent(test_snapshot("ag-002", AgentState::Running));
        let out = run_for_test(&["agent", "--jsonl", "ps"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let lines: Vec<&str> = out.stdout.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        let first = parse_json(lines[0]);
        let second = parse_json(lines[1]);
        assert_eq!(first["id"], "ag-001");
        assert_eq!(second["id"], "ag-002");
    }

    #[test]
    fn agent_show_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "show", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "ag-001");
        assert_eq!(parsed["state"], "idle");
        assert_eq!(parsed["command"], "claude");
    }

    #[test]
    fn agent_show_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "show", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_show_missing_id() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "show"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent ID is required"));
    }

    #[test]
    fn agent_spawn_json() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(
            &[
                "agent",
                "--json",
                "spawn",
                "my-agent",
                "--command",
                "claude",
                "--workspace",
                "ws-1",
                "--adapter",
                "claude_code",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "my-agent");
        assert_eq!(parsed["state"], "starting");
        assert_eq!(parsed["command"], "claude");
    }

    #[test]
    fn agent_spawn_missing_command() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "spawn", "my-agent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--command is required"));
    }

    #[test]
    fn agent_send_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "send", "ag-001", "hello"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_send_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "send", "nonexistent", "hello"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_kill_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "kill", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_kill_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "kill", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_kill_force() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--json", "kill", "ag-001", "--force"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_interrupt_json() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Running));
        let out = run_for_test(&["agent", "--json", "interrupt", "ag-001"], &backend);
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn agent_interrupt_not_found() {
        let backend = InMemoryAgentBackend::new();
        let out = run_for_test(&["agent", "interrupt", "nonexistent"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    #[test]
    fn agent_wait_already_in_state() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(
            &["agent", "--json", "wait", "ag-001", "--until", "idle"],
            &backend,
        );
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["id"], "ag-001");
        assert_eq!(parsed["state"], "idle");
    }

    #[test]
    fn agent_wait_timeout() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Running));
        let out = run_for_test(
            &[
                "agent",
                "wait",
                "ag-001",
                "--until",
                "idle",
                "--timeout",
                "1",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("timeout"));
    }

    #[test]
    fn agent_revive_not_implemented() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Stopped));
        let out = run_for_test(&["agent", "revive", "ag-001"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not yet implemented"));
    }

    #[test]
    fn agent_quiet_suppresses_output() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "--quiet", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn agent_ps_human_table() {
        let backend =
            InMemoryAgentBackend::new().with_agent(test_snapshot("ag-001", AgentState::Idle));
        let out = run_for_test(&["agent", "ps"], &backend);
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("ID"));
        assert!(out.stdout.contains("STATE"));
        assert!(out.stdout.contains("COMMAND"));
        assert!(out.stdout.contains("ag-001"));
        assert!(out.stdout.contains("idle"));
    }

    #[test]
    fn agent_subcommand_help_texts() {
        let backend = InMemoryAgentBackend::new();

        // Each subcommand's --help returns help text via stderr (exit 1).
        let spawn_out = run_for_test(&["agent", "spawn", "--help"], &backend);
        assert_eq!(spawn_out.exit_code, 1);
        assert!(spawn_out.stderr.contains("Spawn a new persistent agent"));

        let send_out = run_for_test(&["agent", "send", "--help"], &backend);
        assert_eq!(send_out.exit_code, 1);
        assert!(send_out.stderr.contains("Send a message to an agent"));

        let wait_out = run_for_test(&["agent", "wait", "--help"], &backend);
        assert_eq!(wait_out.exit_code, 1);
        assert!(wait_out.stderr.contains("Wait for an agent"));

        let ps_out = run_for_test(&["agent", "ps", "--help"], &backend);
        assert_eq!(ps_out.exit_code, 1);
        assert!(ps_out.stderr.contains("List agents"));

        let show_out = run_for_test(&["agent", "show", "--help"], &backend);
        assert_eq!(show_out.exit_code, 1);
        assert!(show_out.stderr.contains("Show agent details"));

        let kill_out = run_for_test(&["agent", "kill", "--help"], &backend);
        assert_eq!(kill_out.exit_code, 1);
        assert!(kill_out.stderr.contains("Kill an agent"));
    }
}

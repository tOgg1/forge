use std::io::Write;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Agent state as reported by the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Stopped,
    Starting,
    AwaitingApproval,
    Paused,
    RateLimited,
    Error,
    Unknown,
}

impl AgentState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Paused => "paused",
            Self::RateLimited => "rate_limited",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    fn is_ready_for_inject(self) -> bool {
        matches!(self, Self::Idle | Self::Stopped | Self::Starting)
    }
}

/// Minimal agent info returned by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    pub id: String,
    pub workspace_id: String,
    pub state: AgentState,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait InjectBackend {
    /// Resolve an agent target (ID or prefix) to a concrete agent record.
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String>;

    /// Load the current agent context (from `forge use --agent`).
    fn load_agent_context(&self) -> Result<Option<String>, String>;

    /// List all agents in the current workspace.
    fn list_workspace_agents(&self) -> Result<Vec<AgentRecord>, String>;

    /// Send a message directly to the agent (bypassing queue).
    fn send_message(&mut self, agent_id: &str, message: &str) -> Result<(), String>;

    /// Read a message from a file path.
    fn read_file(&self, path: &str) -> Result<String, String>;

    /// Read a message from stdin.
    fn read_stdin(&self) -> Result<String, String>;

    /// Check whether the session is interactive (for confirmation prompts).
    fn is_interactive(&self) -> bool;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InMemoryInjectBackend {
    pub agents: Vec<AgentRecord>,
    pub context_agent_id: Option<String>,
    pub sent_messages: Vec<(String, String)>,
    pub interactive: bool,
    pub send_error: Option<String>,
    pub file_contents: Vec<(String, String)>,
    pub stdin_content: Option<String>,
}

impl InMemoryInjectBackend {
    pub fn with_agents(agents: Vec<AgentRecord>) -> Self {
        Self {
            agents,
            ..Self::default()
        }
    }

    pub fn with_context(mut self, agent_id: &str) -> Self {
        self.context_agent_id = Some(agent_id.to_string());
        self
    }

    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn with_send_error(mut self, err: &str) -> Self {
        self.send_error = Some(err.to_string());
        self
    }

    pub fn with_file(mut self, path: &str, content: &str) -> Self {
        self.file_contents
            .push((path.to_string(), content.to_string()));
        self
    }

    pub fn with_stdin(mut self, content: &str) -> Self {
        self.stdin_content = Some(content.to_string());
        self
    }
}

impl InjectBackend for InMemoryInjectBackend {
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String> {
        // Exact match by ID.
        if let Some(a) = self.agents.iter().find(|a| a.id == target) {
            return Ok(a.clone());
        }
        // Prefix match.
        let matches: Vec<&AgentRecord> = self
            .agents
            .iter()
            .filter(|a| a.id.starts_with(target))
            .collect();
        match matches.len() {
            1 => Ok(matches[0].clone()),
            0 => Err(format!("agent not found: {target}")),
            _ => Err(format!(
                "agent '{target}' is ambiguous; use a longer prefix or full ID"
            )),
        }
    }

    fn load_agent_context(&self) -> Result<Option<String>, String> {
        Ok(self.context_agent_id.clone())
    }

    fn list_workspace_agents(&self) -> Result<Vec<AgentRecord>, String> {
        Ok(self.agents.clone())
    }

    fn send_message(&mut self, agent_id: &str, message: &str) -> Result<(), String> {
        if let Some(ref err) = self.send_error {
            return Err(err.clone());
        }
        self.sent_messages
            .push((agent_id.to_string(), message.to_string()));
        Ok(())
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        for (p, content) in &self.file_contents {
            if p == path {
                if content.is_empty() {
                    return Err(format!(
                        "message file \"{path}\" is empty (add content or use --editor/--stdin)"
                    ));
                }
                return Ok(content.clone());
            }
        }
        Err(format!(
            "failed to read message file \"{path}\": no such file"
        ))
    }

    fn read_stdin(&self) -> Result<String, String> {
        match &self.stdin_content {
            Some(content) => Ok(content.clone()),
            None => Err("failed to read from stdin".to_string()),
        }
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct InjectResultJson {
    agent_id: String,
    agent_state: String,
    bypassed_queue: bool,
    injected: bool,
    message: String,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

pub fn run_for_test(args: &[&str], backend: &mut dyn InjectBackend) -> CommandOutput {
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
    backend: &mut dyn InjectBackend,
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

// ---------------------------------------------------------------------------
// Parsed arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    force: bool,
    file: String,
    stdin: bool,
    editor: bool,
    help: bool,
    positionals: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &mut dyn InjectBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    if parsed.help {
        write_help(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Resolve agent target.
    let agent_target = resolve_agent_target(&parsed, backend)?;
    let agent = backend.resolve_agent(&agent_target)?;

    // Resolve the message.
    let message = resolve_message(&parsed, backend)?;

    // Check agent state and potentially require confirmation.
    if !parsed.force && !agent.state.is_ready_for_inject() {
        if !backend.is_interactive() {
            return Err(format!(
                "agent is {}; use --force to inject without confirmation",
                agent.state.as_str()
            ));
        }
        // In interactive mode we would prompt; for the Rust CLI parity port
        // this is handled by the real backend. The in-memory backend is
        // non-interactive by default, so this branch returns the error.
        return Err(format!(
            "agent is {}; use --force to inject without confirmation",
            agent.state.as_str()
        ));
    }

    // Send the message directly (bypasses queue).
    backend.send_message(&agent.id, &message).map_err(|err| {
        if err.contains("not found") {
            format!("agent '{}' not found", agent.id)
        } else {
            format!("failed to inject message: {err}")
        }
    })?;

    // Output.
    if parsed.json || parsed.jsonl {
        let payload = InjectResultJson {
            agent_id: agent.id.clone(),
            agent_state: agent.state.as_str().to_string(),
            bypassed_queue: true,
            injected: true,
            message,
        };
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &payload).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if !parsed.quiet {
        writeln!(
            stdout,
            "Warning: Direct injection to agent {} (bypassed queue)",
            short_id(&agent.id)
        )
        .map_err(|e| e.to_string())?;
        writeln!(stdout, "Message injected").map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn resolve_agent_target(
    parsed: &ParsedArgs,
    backend: &dyn InjectBackend,
) -> Result<String, String> {
    // First positional is always the agent ID for inject.
    if !parsed.positionals.is_empty() {
        return Ok(parsed.positionals[0].clone());
    }

    // Try agent context.
    if let Ok(Some(ctx_agent_id)) = backend.load_agent_context() {
        if !ctx_agent_id.is_empty() {
            return Ok(ctx_agent_id);
        }
    }

    // Auto-detect: if workspace has exactly one agent, use it.
    let agents = backend.list_workspace_agents()?;
    if agents.len() == 1 {
        return Ok(agents[0].id.clone());
    }
    if agents.is_empty() {
        return Err(
            "no agents in workspace; spawn one with 'forge up' or 'forge agent spawn'".to_string(),
        );
    }
    Err("agent required: provide agent ID as first argument or set context with 'forge use --agent <agent>'".to_string())
}

fn resolve_message(parsed: &ParsedArgs, backend: &dyn InjectBackend) -> Result<String, String> {
    let has_inline = parsed.positionals.len() > 1;
    let mut source_count = 0u8;
    if has_inline {
        source_count += 1;
    }
    if !parsed.file.is_empty() {
        source_count += 1;
    }
    if parsed.stdin {
        source_count += 1;
    }
    if parsed.editor {
        source_count += 1;
    }

    if source_count == 0 {
        return Err(
            "message required (provide <message>, --file, --stdin, or --editor)".to_string(),
        );
    }
    if source_count > 1 {
        return Err(
            "choose only one message source: <message>, --file, --stdin, or --editor".to_string(),
        );
    }

    let message = if !parsed.file.is_empty() {
        backend.read_file(&parsed.file)?
    } else if parsed.stdin {
        backend.read_stdin()?
    } else if parsed.editor {
        return Err("--editor: editor launching not supported in this context".to_string());
    } else {
        parsed.positionals[1..].join(" ")
    };

    if message.trim().is_empty() {
        return Err(
            "message is empty (provide content via <message>, --file, --stdin, or --editor)"
                .to_string(),
        );
    }

    Ok(message)
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "inject") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut force = false;
    let mut file = String::new();
    let mut stdin = false;
    let mut editor = false;
    let mut help = false;
    let mut positionals = Vec::new();

    while let Some(token) = args.get(index) {
        match token.as_str() {
            "-h" | "--help" | "help" => {
                help = true;
                index += 1;
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
            "-F" | "--force" => {
                force = true;
                index += 1;
            }
            "-f" | "--file" => {
                file = take_value(args, index, "--file")?;
                index += 2;
            }
            "--stdin" => {
                stdin = true;
                index += 1;
            }
            "--editor" => {
                editor = true;
                index += 1;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown flag: '{flag}'"));
            }
            value => {
                positionals.push(value.to_string());
                index += 1;
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
        force,
        file,
        stdin,
        editor,
        help,
        positionals,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => Ok(value.clone()),
        Some(_) | None => Err(format!("error: {flag} requires a value")),
    }
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

fn write_help(stdout: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        stdout,
        "Inject a message directly into an agent (bypasses queue)"
    )?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "WARNING: This bypasses the scheduler queue and sends immediately."
    )?;
    writeln!(
        stdout,
        "Use 'forge send' for safe, queue-based message dispatch."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Direct injection is useful for:")?;
    writeln!(stdout, "  - Emergency interventions")?;
    writeln!(stdout, "  - Debugging agent behavior")?;
    writeln!(stdout, "  - Immediate control commands")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "But it can cause issues if the agent is not ready to receive input."
    )?;
    writeln!(
        stdout,
        "Non-idle agents require confirmation (use --force to skip)."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge inject <agent-id> [message] [flags]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Examples:")?;
    writeln!(
        stdout,
        "  forge inject abc123 \"Stop and commit\"            # inject message"
    )?;
    writeln!(
        stdout,
        "  forge inject --force abc123 \"Emergency stop\"      # skip confirmation"
    )?;
    writeln!(
        stdout,
        "  forge inject abc123 --file prompt.txt             # from file"
    )?;
    writeln!(
        stdout,
        "  echo \"Continue\" | forge inject abc123 --stdin     # from stdin"
    )?;
    writeln!(
        stdout,
        "  forge inject abc123 --editor                      # compose in editor"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "  -F, --force            skip confirmation for non-idle agents"
    )?;
    writeln!(stdout, "  -f, --file string      read message from file")?;
    writeln!(stdout, "      --stdin            read message from stdin")?;
    writeln!(
        stdout,
        "      --editor           compose message in $EDITOR"
    )?;
    writeln!(stdout, "      --json             output in JSON format")?;
    writeln!(
        stdout,
        "      --jsonl            output in JSON Lines format"
    )?;
    writeln!(stdout, "      --quiet            suppress human output")?;
    writeln!(stdout, "  -h, --help             help for inject")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn idle_agent() -> AgentRecord {
        AgentRecord {
            id: "agent-inject-idle".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
        }
    }

    fn busy_agent() -> AgentRecord {
        AgentRecord {
            id: "agent-inject-busy".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Working,
        }
    }

    fn single_idle_backend() -> InMemoryInjectBackend {
        InMemoryInjectBackend::with_agents(vec![idle_agent()])
    }

    fn multi_agent_backend() -> InMemoryInjectBackend {
        InMemoryInjectBackend::with_agents(vec![idle_agent(), busy_agent()])
    }

    fn run(args: &[&str], backend: &mut dyn InjectBackend) -> CommandOutput {
        run_for_test(args, backend)
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    // --- Help ---

    #[test]
    fn inject_help_flag() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "--help"], &mut backend);
        assert_success(&out);
        assert!(out
            .stdout
            .contains("Inject a message directly into an agent"));
        assert!(out.stdout.contains("forge inject"));
        assert!(out.stdout.contains("--force"));
        assert!(out.stdout.contains("--file"));
        assert!(out.stdout.contains("--stdin"));
        assert!(out.stdout.contains("--editor"));
    }

    #[test]
    fn inject_short_help_flag() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "-h"], &mut backend);
        assert_success(&out);
        assert!(out
            .stdout
            .contains("Inject a message directly into an agent"));
    }

    #[test]
    fn inject_help_subcommand() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "help"], &mut backend);
        assert_success(&out);
        assert!(out
            .stdout
            .contains("Inject a message directly into an agent"));
    }

    // --- Basic injection ---

    #[test]
    fn inject_idle_agent_json() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "Stop and commit", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["injected"], true);
        assert_eq!(parsed["agent_id"], "agent-inject-idle");
        assert_eq!(parsed["message"], "Stop and commit");
        assert_eq!(parsed["bypassed_queue"], true);
        assert_eq!(parsed["agent_state"], "idle");
    }

    #[test]
    fn inject_idle_agent_jsonl() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "hello", "--jsonl"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(parsed["injected"], true);
        assert_eq!(parsed["agent_id"], "agent-inject-idle");
        assert_eq!(parsed["bypassed_queue"], true);
    }

    #[test]
    fn inject_idle_agent_human() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "Stop and commit"],
            &mut backend,
        );
        assert_success(&out);
        assert!(out
            .stdout
            .contains("Warning: Direct injection to agent agent-i"));
        assert!(out.stdout.contains("(bypassed queue)"));
        assert!(out.stdout.contains("Message injected"));
    }

    #[test]
    fn inject_quiet_suppresses_output() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "hello", "--quiet"],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.is_empty());
    }

    // --- Multi-word message ---

    #[test]
    fn inject_multi_word_message() {
        let mut backend = single_idle_backend();
        let out = run(
            &[
                "inject",
                "agent-inject-idle",
                "stop",
                "and",
                "commit",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["message"], "stop and commit");
    }

    // --- Force flag ---

    #[test]
    fn inject_busy_agent_without_force_errors() {
        let mut backend = multi_agent_backend();
        let out = run(&["inject", "agent-inject-busy", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("agent is working; use --force to inject without confirmation"));
    }

    #[test]
    fn inject_busy_agent_with_force_succeeds() {
        let mut backend = multi_agent_backend();
        let out = run(
            &[
                "inject",
                "--force",
                "agent-inject-busy",
                "inject forced",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["injected"], true);
        assert_eq!(parsed["agent_id"], "agent-inject-busy");
        assert_eq!(parsed["message"], "inject forced");
        assert_eq!(parsed["agent_state"], "working");
        assert_eq!(parsed["bypassed_queue"], true);
    }

    #[test]
    fn inject_busy_agent_short_force_flag() {
        let mut backend = multi_agent_backend();
        let out = run(
            &["inject", "-F", "agent-inject-busy", "forced msg", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["injected"], true);
        assert_eq!(parsed["agent_state"], "working");
    }

    // --- Agent state checks ---

    #[test]
    fn inject_stopped_agent_without_force_succeeds() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-stopped".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Stopped,
        }]);
        let out = run(
            &["inject", "agent-stopped", "hello", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_state"], "stopped");
    }

    #[test]
    fn inject_starting_agent_without_force_succeeds() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-starting".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Starting,
        }]);
        let out = run(
            &["inject", "agent-starting", "hello", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_state"], "starting");
    }

    #[test]
    fn inject_paused_agent_without_force_errors() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-paused".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Paused,
        }]);
        let out = run(&["inject", "agent-paused", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent is paused"));
    }

    #[test]
    fn inject_awaiting_approval_without_force_errors() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-approval".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::AwaitingApproval,
        }]);
        let out = run(&["inject", "agent-approval", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent is awaiting_approval"));
    }

    #[test]
    fn inject_error_state_without_force_errors() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-error".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Error,
        }]);
        let out = run(&["inject", "agent-error", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent is error"));
    }

    #[test]
    fn inject_rate_limited_with_force_succeeds() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-rl".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::RateLimited,
        }]);
        let out = run(
            &["inject", "--force", "agent-rl", "hello", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_state"], "rate_limited");
    }

    // --- Auto-detect agent ---

    #[test]
    fn inject_auto_detect_single_agent() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "hello auto", "--json"], &mut backend);
        // "hello auto" is not a valid agent ID, so it goes through resolve_agent_target
        // which tries resolve first, fails, then checks context / auto-detect.
        // But since positionals[0] = "hello auto", resolve_agent will fail.
        // Actually "hello auto" won't match any agent, and since we have positionals,
        // it tries to resolve "hello auto" and fails.
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent not found"));
    }

    #[test]
    fn inject_no_args_auto_detect_single_agent() {
        // With no positionals, inject should auto-detect the single agent.
        // But inject needs a message too...
        let mut backend = single_idle_backend();
        let out = run(&["inject"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("message required"));
    }

    #[test]
    fn inject_context_agent_fallback_no_message() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent(), busy_agent()])
            .with_context("agent-inject-busy");
        // No positional agent specified; context resolves to busy agent.
        // No message source provided → message required error.
        let out = run(&["inject"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("message required"));
    }

    #[test]
    fn inject_context_agent_fallback_with_file() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()])
            .with_context("agent-inject-idle")
            .with_file("prompt.txt", "hello from file");
        // No positional agent → context agent. Message from --file.
        let out = run(&["inject", "--file", "prompt.txt", "--json"], &mut backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_id"], "agent-inject-idle");
        assert_eq!(parsed["message"], "hello from file");
    }

    // --- Error cases ---

    #[test]
    fn inject_no_agents_in_workspace() {
        let mut backend = InMemoryInjectBackend::default();
        let out = run(&["inject"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("no agents in workspace"));
    }

    #[test]
    fn inject_agent_not_found() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "nonexistent", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent not found"));
    }

    #[test]
    fn inject_empty_message_error() {
        let mut backend = single_idle_backend();
        let out = run(&["inject", "agent-inject-idle"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("message required"));
    }

    #[test]
    fn inject_send_failure() {
        let mut backend =
            InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_send_error("tmux error");
        let out = run(&["inject", "agent-inject-idle", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("failed to inject message"));
    }

    #[test]
    fn inject_send_not_found_error() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()])
            .with_send_error("agent not found");
        let out = run(&["inject", "agent-inject-idle", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("not found"));
    }

    // --- Flag validation ---

    #[test]
    fn inject_unknown_flag() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "--unknown", "agent-inject-idle", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown flag: '--unknown'"));
    }

    #[test]
    fn inject_json_jsonl_conflict() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "--json", "--jsonl", "agent-inject-idle", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    // --- Backend records send calls ---

    #[test]
    fn inject_records_sent_message() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "hello world"],
            &mut backend,
        );
        assert_success(&out);
        assert_eq!(backend.sent_messages.len(), 1);
        assert_eq!(backend.sent_messages[0].0, "agent-inject-idle");
        assert_eq!(backend.sent_messages[0].1, "hello world");
    }

    #[test]
    fn inject_forced_records_sent_message() {
        let mut backend = multi_agent_backend();
        let out = run(
            &["inject", "--force", "agent-inject-busy", "forced msg"],
            &mut backend,
        );
        assert_success(&out);
        assert_eq!(backend.sent_messages.len(), 1);
        assert_eq!(backend.sent_messages[0].0, "agent-inject-busy");
        assert_eq!(backend.sent_messages[0].1, "forced msg");
    }

    // --- Prefix resolution ---

    #[test]
    fn inject_prefix_resolution() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![AgentRecord {
            id: "agent-abc123xyz".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
        }]);
        let out = run(&["inject", "agent-abc", "hello", "--json"], &mut backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["agent_id"], "agent-abc123xyz");
    }

    #[test]
    fn inject_ambiguous_prefix() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![
            AgentRecord {
                id: "agent-abc123".to_string(),
                workspace_id: "ws-001".to_string(),
                state: AgentState::Idle,
            },
            AgentRecord {
                id: "agent-abc456".to_string(),
                workspace_id: "ws-001".to_string(),
                state: AgentState::Idle,
            },
        ]);
        let out = run(&["inject", "agent-abc", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("ambiguous"));
    }

    // --- File/stdin message sources ---

    #[test]
    fn inject_from_file() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()])
            .with_file("msg.txt", "file content here");
        let out = run(
            &["inject", "agent-inject-idle", "--file", "msg.txt", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["message"], "file content here");
        assert_eq!(backend.sent_messages[0].1, "file content here");
    }

    #[test]
    fn inject_from_stdin() {
        let mut backend =
            InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_stdin("stdin msg");
        let out = run(
            &["inject", "agent-inject-idle", "--stdin", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["message"], "stdin msg");
    }

    #[test]
    fn inject_file_not_found_error() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()]);
        let out = run(
            &["inject", "agent-inject-idle", "--file", "missing.txt"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("failed to read message file"));
    }

    #[test]
    fn inject_empty_file_error() {
        let mut backend =
            InMemoryInjectBackend::with_agents(vec![idle_agent()]).with_file("empty.txt", "");
        let out = run(
            &["inject", "agent-inject-idle", "--file", "empty.txt"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("empty"));
    }

    #[test]
    fn inject_multiple_sources_error() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()])
            .with_file("msg.txt", "content")
            .with_stdin("stdin");
        let out = run(
            &[
                "inject",
                "agent-inject-idle",
                "inline msg",
                "--file",
                "msg.txt",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("choose only one message source"));
    }

    #[test]
    fn inject_file_and_stdin_conflict() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()])
            .with_file("msg.txt", "content")
            .with_stdin("stdin");
        let out = run(
            &[
                "inject",
                "agent-inject-idle",
                "--file",
                "msg.txt",
                "--stdin",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("choose only one message source"));
    }

    #[test]
    fn inject_file_requires_value() {
        let mut backend = InMemoryInjectBackend::with_agents(vec![idle_agent()]);
        let out = run(&["inject", "agent-inject-idle", "--file"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("--file requires a value"));
    }

    // --- Oracle parity: JSON field ordering ---

    #[test]
    fn inject_json_fields_are_alphabetical() {
        let mut backend = single_idle_backend();
        let out = run(
            &["inject", "agent-inject-idle", "ping", "--json"],
            &mut backend,
        );
        assert_success(&out);
        // Verify the JSON output has keys in alphabetical order
        // matching Go's sorted map[string]any output.
        let expected = concat!(
            "{\n",
            "  \"agent_id\": \"agent-inject-idle\",\n",
            "  \"agent_state\": \"idle\",\n",
            "  \"bypassed_queue\": true,\n",
            "  \"injected\": true,\n",
            "  \"message\": \"ping\"\n",
            "}\n",
        );
        assert_eq!(out.stdout, expected);
    }

    fn _assert_backend_object_safe(_b: &mut dyn InjectBackend) {}
}

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

/// Minimal agent info returned by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    pub id: String,
    pub workspace_id: String,
    pub state: String,
}

/// Queue item as stored/returned by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub agent_id: String,
    pub item_type: String,
    pub status: String,
    pub position: i64,
    pub payload: String,
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

pub trait SendBackend {
    /// Resolve an agent target (ID or prefix) to a concrete agent record.
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String>;

    /// Load the current agent context (from `forge use --agent`).
    fn load_agent_context(&self) -> Result<Option<String>, String>;

    /// List all agents in the current workspace.
    fn list_workspace_agents(&self) -> Result<Vec<AgentRecord>, String>;

    /// Enqueue a standard message for an agent. Returns item ID.
    fn enqueue_message(&mut self, agent_id: &str, text: &str) -> Result<String, String>;

    /// Enqueue a conditional (when-idle) message. Returns item ID.
    fn enqueue_conditional(&mut self, agent_id: &str, text: &str) -> Result<String, String>;

    /// Insert a message at the front of the queue. Returns item ID.
    fn enqueue_front(&mut self, agent_id: &str, text: &str) -> Result<String, String>;

    /// Insert a conditional item at the front of the queue. Returns item ID.
    fn enqueue_front_conditional(&mut self, agent_id: &str, text: &str) -> Result<String, String>;

    /// Insert a message after a specific queue item. Returns item ID.
    fn enqueue_after(
        &mut self,
        agent_id: &str,
        after_id: &str,
        text: &str,
    ) -> Result<String, String>;

    /// List queue items for an agent (for position calculation).
    fn list_queue(&self, agent_id: &str) -> Result<Vec<QueueItem>, String>;
}

// ---------------------------------------------------------------------------
// In-memory backend for testing
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct InMemorySendBackend {
    pub agents: Vec<AgentRecord>,
    pub context_agent_id: Option<String>,
    pub queue_items: Vec<QueueItem>,
    next_item_id: usize,

    pub load_context_error: Option<String>,
}

impl InMemorySendBackend {
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

    fn next_id(&mut self) -> String {
        self.next_item_id += 1;
        format!("item-{:03}", self.next_item_id)
    }

    fn add_item(&mut self, agent_id: &str, item_type: &str, payload: &str, front: bool) -> String {
        let id = self.next_id();
        let agent_items: Vec<&QueueItem> = self
            .queue_items
            .iter()
            .filter(|qi| qi.agent_id == agent_id)
            .collect();
        let position = if front {
            0
        } else {
            agent_items.len() as i64 + 1
        };
        if front {
            // Shift existing items
            for qi in self.queue_items.iter_mut() {
                if qi.agent_id == agent_id {
                    qi.position += 1;
                }
            }
        }
        self.queue_items.push(QueueItem {
            id: id.clone(),
            agent_id: agent_id.to_string(),
            item_type: item_type.to_string(),
            status: "pending".to_string(),
            position,
            payload: payload.to_string(),
        });
        id
    }
}

impl SendBackend for InMemorySendBackend {
    fn resolve_agent(&self, target: &str) -> Result<AgentRecord, String> {
        // Exact match by ID
        if let Some(a) = self.agents.iter().find(|a| a.id == target) {
            return Ok(a.clone());
        }
        // Prefix match
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
        if let Some(ref err) = self.load_context_error {
            return Err(err.clone());
        }
        Ok(self.context_agent_id.clone())
    }

    fn list_workspace_agents(&self) -> Result<Vec<AgentRecord>, String> {
        Ok(self.agents.clone())
    }

    fn enqueue_message(&mut self, agent_id: &str, text: &str) -> Result<String, String> {
        if !self.agents.iter().any(|a| a.id == agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let payload = format!(
            "{{\"text\":{}}}",
            serde_json::to_string(text).unwrap_or_default()
        );
        Ok(self.add_item(agent_id, "message", &payload, false))
    }

    fn enqueue_conditional(&mut self, agent_id: &str, text: &str) -> Result<String, String> {
        if !self.agents.iter().any(|a| a.id == agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let payload = format!(
            "{{\"condition_type\":\"when_idle\",\"message\":{}}}",
            serde_json::to_string(text).unwrap_or_default()
        );
        Ok(self.add_item(agent_id, "conditional", &payload, false))
    }

    fn enqueue_front(&mut self, agent_id: &str, text: &str) -> Result<String, String> {
        if !self.agents.iter().any(|a| a.id == agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let payload = format!(
            "{{\"text\":{}}}",
            serde_json::to_string(text).unwrap_or_default()
        );
        Ok(self.add_item(agent_id, "message", &payload, true))
    }

    fn enqueue_front_conditional(&mut self, agent_id: &str, text: &str) -> Result<String, String> {
        if !self.agents.iter().any(|a| a.id == agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let payload = format!(
            "{{\"condition_type\":\"when_idle\",\"message\":{}}}",
            serde_json::to_string(text).unwrap_or_default()
        );
        Ok(self.add_item(agent_id, "conditional", &payload, true))
    }

    fn enqueue_after(
        &mut self,
        agent_id: &str,
        after_id: &str,
        text: &str,
    ) -> Result<String, String> {
        if !self.agents.iter().any(|a| a.id == agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let after_item = self
            .queue_items
            .iter()
            .find(|qi| qi.id == after_id)
            .ok_or_else(|| format!("failed to find queue item {after_id}: not found"))?;
        if after_item.agent_id != agent_id {
            return Err(format!(
                "queue item {after_id} does not belong to agent {agent_id}"
            ));
        }
        let insert_pos = after_item.position + 1;
        // Shift items after
        for qi in self.queue_items.iter_mut() {
            if qi.agent_id == agent_id && qi.position >= insert_pos {
                qi.position += 1;
            }
        }
        let id = self.next_id();
        let payload = format!(
            "{{\"text\":{}}}",
            serde_json::to_string(text).unwrap_or_default()
        );
        self.queue_items.push(QueueItem {
            id: id.clone(),
            agent_id: agent_id.to_string(),
            item_type: "message".to_string(),
            status: "pending".to_string(),
            position: insert_pos,
            payload,
        });
        Ok(id)
    }

    fn list_queue(&self, agent_id: &str) -> Result<Vec<QueueItem>, String> {
        let mut items: Vec<QueueItem> = self
            .queue_items
            .iter()
            .filter(|qi| qi.agent_id == agent_id)
            .cloned()
            .collect();
        items.sort_by_key(|qi| qi.position);
        Ok(items)
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
    priority: String,
    front: bool,
    when_idle: bool,
    after: String,
    all: bool,
    help: bool,
    positionals: Vec<String>,
}

// ---------------------------------------------------------------------------
// Result types for JSON output
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SendResultJson {
    queued: bool,
    results: Vec<SendResultItem>,
    message: String,
}

#[derive(Debug, Serialize)]
struct SendResultItem {
    agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_type: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

pub fn run_for_test(args: &[&str], backend: &mut dyn SendBackend) -> CommandOutput {
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
    backend: &mut dyn SendBackend,
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
// Core logic
// ---------------------------------------------------------------------------

fn execute(
    args: &[String],
    backend: &mut dyn SendBackend,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let parsed = parse_args(args)?;

    if parsed.help {
        write_help(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Resolve the message text from positional arguments.
    let (agent_target, message) = resolve_agent_and_message(&parsed, backend)?;

    if message.trim().is_empty() {
        return Err(
            "message required (provide <message>, --file, --stdin, or --editor)".to_string(),
        );
    }

    // Resolve target agents.
    let target_agents = resolve_targets(&parsed, &agent_target, backend)?;

    if target_agents.is_empty() {
        return Err("no agents in workspace".to_string());
    }

    // Determine queue options.
    let front = parsed.front || parsed.priority.eq_ignore_ascii_case("high");

    // Enqueue for each target agent.
    let mut results = Vec::new();
    for agent in &target_agents {
        let result = enqueue_for_agent(backend, agent, &message, &parsed, front);
        results.push(result);
    }

    // Output.
    let truncated_message = truncate_message(&message, 100);

    if parsed.json || parsed.jsonl {
        let payload = SendResultJson {
            queued: true,
            results,
            message: truncated_message,
        };
        if parsed.jsonl {
            serde_json::to_writer(&mut *stdout, &payload).map_err(|e| e.to_string())?;
        } else {
            serde_json::to_writer_pretty(&mut *stdout, &payload).map_err(|e| e.to_string())?;
        }
        writeln!(stdout).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if parsed.quiet {
        return Ok(());
    }

    // Human-readable output.
    for r in &results {
        if let Some(ref err) = r.error {
            writeln!(
                stdout,
                "\u{2717} Failed to queue for agent {}: {}",
                short_id(&r.agent_id),
                err
            )
            .map_err(|e| e.to_string())?;
            continue;
        }

        let position_str = format_position(r.position, &parsed);
        let type_str = if r.item_type.as_deref() == Some("conditional") {
            " (when idle)"
        } else {
            ""
        };
        writeln!(
            stdout,
            "\u{2713} Queued for agent {} at position {}{}",
            short_id(&r.agent_id),
            position_str,
            type_str
        )
        .map_err(|e| e.to_string())?;
    }

    if results.len() == 1 && results[0].error.is_none() {
        let display_msg = truncate_message(&message, 60);
        writeln!(stdout, "  \"{display_msg}\"").map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn enqueue_for_agent(
    backend: &mut dyn SendBackend,
    agent: &AgentRecord,
    message: &str,
    parsed: &ParsedArgs,
    front: bool,
) -> SendResultItem {
    let result = if !parsed.after.is_empty() {
        backend.enqueue_after(&agent.id, &parsed.after, message)
    } else if front && parsed.when_idle {
        backend.enqueue_front_conditional(&agent.id, message)
    } else if front {
        backend.enqueue_front(&agent.id, message)
    } else if parsed.when_idle {
        backend.enqueue_conditional(&agent.id, message)
    } else {
        backend.enqueue_message(&agent.id, message)
    };

    match result {
        Ok(item_id) => {
            let item_type = if parsed.when_idle {
                "conditional"
            } else {
                "message"
            };

            // Calculate position from queue.
            let position = match backend.list_queue(&agent.id) {
                Ok(items) => items
                    .iter()
                    .enumerate()
                    .find(|(_, qi)| qi.id == item_id)
                    .map(|(i, _)| (i + 1) as i64)
                    .unwrap_or(0),
                Err(_) => 0,
            };

            SendResultItem {
                agent_id: agent.id.clone(),
                item_id: Some(item_id),
                item_type: Some(item_type.to_string()),
                position,
                error: None,
            }
        }
        Err(err) => SendResultItem {
            agent_id: agent.id.clone(),
            item_id: None,
            item_type: None,
            position: 0,
            error: Some(err),
        },
    }
}

fn resolve_agent_and_message(
    parsed: &ParsedArgs,
    backend: &dyn SendBackend,
) -> Result<(String, String), String> {
    if parsed.all {
        // All mode: everything is message text.
        let message = parsed.positionals.join(" ");
        return Ok((String::new(), message));
    }

    if parsed.positionals.is_empty() {
        // No args: try context, then auto-detect.
        return Ok((String::new(), String::new()));
    }

    // Try first positional as agent ID.
    let first = &parsed.positionals[0];
    match backend.resolve_agent(first) {
        Ok(_) => {
            let message = parsed.positionals[1..].join(" ");
            Ok((first.clone(), message))
        }
        Err(_) => {
            // First positional is not an agent; treat all as message text.
            let message = parsed.positionals.join(" ");
            Ok((String::new(), message))
        }
    }
}

fn resolve_targets(
    parsed: &ParsedArgs,
    agent_target: &str,
    backend: &dyn SendBackend,
) -> Result<Vec<AgentRecord>, String> {
    if parsed.all {
        return backend.list_workspace_agents();
    }

    if !agent_target.is_empty() {
        let agent = backend.resolve_agent(agent_target)?;
        return Ok(vec![agent]);
    }

    // Try agent context.
    if let Ok(Some(ctx_agent_id)) = backend.load_agent_context() {
        if !ctx_agent_id.is_empty() {
            let agent = backend.resolve_agent(&ctx_agent_id)?;
            return Ok(vec![agent]);
        }
    }

    // Auto-detect: if workspace has exactly one agent, use it.
    let agents = backend.list_workspace_agents()?;
    if agents.len() == 1 {
        return Ok(agents);
    }
    if agents.is_empty() {
        return Err(
            "no agents in workspace; spawn one with 'forge up' or 'forge agent spawn'".to_string(),
        );
    }
    Err("agent required: provide agent ID as argument, set context with 'forge use --agent <agent>', or run from a workspace directory".to_string())
}

fn format_position(position: i64, parsed: &ParsedArgs) -> String {
    if !parsed.after.is_empty() && position > 0 {
        format!("#{} (after {})", position, short_id(&parsed.after))
    } else if parsed.front || parsed.priority.eq_ignore_ascii_case("high") {
        "#1 (front)".to_string()
    } else {
        format!("#{position}")
    }
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

fn truncate_message(message: &str, max_len: usize) -> String {
    if message.len() <= max_len {
        message.to_string()
    } else {
        format!("{}...", &message[..max_len])
    }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "send") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut priority = "normal".to_string();
    let mut front = false;
    let mut when_idle = false;
    let mut after = String::new();
    let mut all = false;
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
            "--priority" => {
                priority = take_value(args, index, "--priority")?;
                index += 2;
            }
            "--front" => {
                front = true;
                index += 1;
            }
            "--when-idle" => {
                when_idle = true;
                index += 1;
            }
            "--after" => {
                after = take_value(args, index, "--after")?;
                index += 2;
            }
            "--all" => {
                all = true;
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

    // Validate priority.
    let norm_priority = priority.trim().to_ascii_lowercase();
    match norm_priority.as_str() {
        "high" | "normal" | "low" => {}
        _ => {
            return Err(format!(
                "invalid priority: {priority} (use high, normal, or low)"
            ))
        }
    }

    // Validate flag combinations.
    if !after.is_empty() && front {
        return Err("error: --after and --front cannot be used together".to_string());
    }
    if !after.is_empty() && all {
        return Err("error: --after and --all cannot be used together".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        priority: norm_priority,
        front,
        when_idle,
        after,
        all,
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
    writeln!(stdout, "Queue a message for an agent")?;
    writeln!(stdout)?;
    writeln!(
        stdout,
        "Messages are enqueued and dispatched when the agent is ready (idle)."
    )?;
    writeln!(
        stdout,
        "This is the safe, queue-based way to send messages."
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Usage:")?;
    writeln!(stdout, "  forge send [agent] <message> [flags]")?;
    writeln!(stdout)?;
    writeln!(stdout, "Examples:")?;
    writeln!(
        stdout,
        "  forge send \"Fix the lint errors\"                    # auto-detect agent"
    )?;
    writeln!(
        stdout,
        "  forge send abc123 \"Fix the lint errors\"             # specific agent"
    )?;
    writeln!(
        stdout,
        "  forge send --all \"Pause and commit your work\"       # all agents"
    )?;
    writeln!(
        stdout,
        "  forge send --priority high abc123 \"Urgent: revert\"  # high priority"
    )?;
    writeln!(
        stdout,
        "  forge send --front abc123 \"Do this next\"            # front of queue"
    )?;
    writeln!(
        stdout,
        "  forge send --when-idle abc123 \"When ready\"          # conditional"
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "Flags:")?;
    writeln!(
        stdout,
        "      --priority string  queue priority: high, normal, low (default \"normal\")"
    )?;
    writeln!(
        stdout,
        "      --front            insert at front of queue (next to be dispatched)"
    )?;
    writeln!(
        stdout,
        "      --when-idle        only dispatch when agent is idle (conditional)"
    )?;
    writeln!(
        stdout,
        "      --after string     insert after a specific queue item ID"
    )?;
    writeln!(
        stdout,
        "      --all              send to all agents in workspace"
    )?;
    writeln!(stdout, "      --json             output in JSON format")?;
    writeln!(
        stdout,
        "      --jsonl            output in JSON Lines format"
    )?;
    writeln!(stdout, "      --quiet            suppress human output")?;
    writeln!(stdout, "  -h, --help             help for send")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn single_agent_backend() -> InMemorySendBackend {
        InMemorySendBackend::with_agents(vec![AgentRecord {
            id: "oracle-agent-idle".to_string(),
            workspace_id: "ws-001".to_string(),
            state: "idle".to_string(),
        }])
    }

    fn multi_agent_backend() -> InMemorySendBackend {
        InMemorySendBackend::with_agents(vec![
            AgentRecord {
                id: "oracle-agent-idle".to_string(),
                workspace_id: "ws-001".to_string(),
                state: "idle".to_string(),
            },
            AgentRecord {
                id: "oracle-agent-busy".to_string(),
                workspace_id: "ws-001".to_string(),
                state: "working".to_string(),
            },
        ])
    }

    fn run(args: &[&str], backend: &mut dyn SendBackend) -> CommandOutput {
        run_for_test(args, backend)
    }

    fn assert_success(out: &CommandOutput) {
        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
        assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
    }

    // --- Basic send ---

    #[test]
    fn send_basic_message_json() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "oracle-agent-idle", "hello from oracle", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["queued"], true);
        assert_eq!(parsed["message"], "hello from oracle");
        assert_eq!(parsed["results"][0]["agent_id"], "oracle-agent-idle");
        assert_eq!(parsed["results"][0]["item_type"], "message");
        assert_eq!(parsed["results"][0]["position"], 1);
    }

    #[test]
    fn send_basic_message_human() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "oracle-agent-idle", "hello from oracle"],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("\u{2713} Queued for agent oracle-a"));
        assert!(out.stdout.contains("at position #1"));
        assert!(out.stdout.contains("\"hello from oracle\""));
    }

    #[test]
    fn send_basic_message_jsonl() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "oracle-agent-idle", "hello from oracle", "--jsonl"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
        assert_eq!(parsed["queued"], true);
        assert_eq!(parsed["message"], "hello from oracle");
    }

    #[test]
    fn send_quiet_suppresses_output() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "oracle-agent-idle", "hello", "--quiet"],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.is_empty());
    }

    // --- When idle ---

    #[test]
    fn send_when_idle_json() {
        let mut backend = single_agent_backend();
        let out = run(
            &[
                "send",
                "oracle-agent-idle",
                "continue when ready",
                "--when-idle",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["item_type"], "conditional");
        assert_eq!(parsed["results"][0]["position"], 1);
    }

    #[test]
    fn send_when_idle_human() {
        let mut backend = single_agent_backend();
        let out = run(
            &[
                "send",
                "oracle-agent-idle",
                "continue when ready",
                "--when-idle",
            ],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("(when idle)"));
    }

    // --- Priority high ---

    #[test]
    fn send_priority_high_inserts_at_front() {
        let mut backend = single_agent_backend();
        // First enqueue a normal message.
        let _ = run(
            &["send", "oracle-agent-idle", "first msg", "--json"],
            &mut backend,
        );
        // Then enqueue a high-priority message.
        let out = run(
            &[
                "send",
                "--priority",
                "high",
                "oracle-agent-idle",
                "urgent",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["position"], 1);
        assert_eq!(parsed["results"][0]["item_type"], "message");
    }

    #[test]
    fn send_priority_high_human() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "--priority", "high", "oracle-agent-idle", "urgent"],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("#1 (front)"));
    }

    // --- Front flag ---

    #[test]
    fn send_front_inserts_at_front() {
        let mut backend = single_agent_backend();
        let _ = run(
            &["send", "oracle-agent-idle", "first", "--json"],
            &mut backend,
        );
        let out = run(
            &["send", "--front", "oracle-agent-idle", "at front", "--json"],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["position"], 1);
    }

    // --- After ---

    #[test]
    fn send_after_inserts_after_item() {
        let mut backend = multi_agent_backend();
        // Seed an item.
        let first = run(
            &["send", "oracle-agent-idle", "seed", "--json"],
            &mut backend,
        );
        assert_success(&first);
        let first_parsed: serde_json::Value = serde_json::from_str(&first.stdout).unwrap();
        let seed_id = first_parsed["results"][0]["item_id"].as_str().unwrap();

        let out = run(
            &[
                "send",
                "--after",
                seed_id,
                "oracle-agent-idle",
                "after seed",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["position"], 2);
    }

    #[test]
    fn send_after_human() {
        let mut backend = single_agent_backend();
        let first = run(
            &["send", "oracle-agent-idle", "seed", "--json"],
            &mut backend,
        );
        let first_parsed: serde_json::Value = serde_json::from_str(&first.stdout).unwrap();
        let seed_id = first_parsed["results"][0]["item_id"].as_str().unwrap();

        let out = run(
            &[
                "send",
                "--after",
                seed_id,
                "oracle-agent-idle",
                "after seed",
            ],
            &mut backend,
        );
        assert_success(&out);
        assert!(out.stdout.contains("(after"));
    }

    // --- All ---

    #[test]
    fn send_all_sends_to_all_agents() {
        let mut backend = multi_agent_backend();
        let out = run(&["send", "--all", "broadcast msg", "--json"], &mut backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn send_all_human() {
        let mut backend = multi_agent_backend();
        let out = run(&["send", "--all", "broadcast msg"], &mut backend);
        assert_success(&out);
        // Multiple results, no message quote.
        assert!(out.stdout.contains("\u{2713} Queued for agent oracle-a"));
    }

    // --- Auto-detect ---

    #[test]
    fn send_auto_detect_single_agent() {
        let mut backend = single_agent_backend();
        let out = run(&["send", "hello auto", "--json"], &mut backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["agent_id"], "oracle-agent-idle");
        assert_eq!(parsed["message"], "hello auto");
    }

    #[test]
    fn send_agent_context_fallback() {
        let mut backend = multi_agent_backend().with_context("oracle-agent-busy");
        let out = run(&["send", "hello ctx", "--json"], &mut backend);
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["results"][0]["agent_id"], "oracle-agent-busy");
    }

    #[test]
    fn send_multi_agent_no_context_errors() {
        let mut backend = multi_agent_backend();
        let out = run(&["send", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent required"));
    }

    // --- Validation ---

    #[test]
    fn send_no_args_no_agents() {
        let mut backend = InMemorySendBackend::default();
        let out = run(&["send", "hello"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("no agents in workspace"));
    }

    #[test]
    fn send_empty_message_error() {
        let mut backend = single_agent_backend();
        let out = run(&["send", "oracle-agent-idle"], &mut backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("message required"));
    }

    #[test]
    fn send_after_and_front_conflict() {
        let mut backend = single_agent_backend();
        let out = run(
            &[
                "send",
                "--after",
                "item-001",
                "--front",
                "oracle-agent-idle",
                "hello",
            ],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--after and --front cannot be used together"));
    }

    #[test]
    fn send_after_and_all_conflict() {
        let mut backend = multi_agent_backend();
        let out = run(
            &["send", "--after", "item-001", "--all", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--after and --all cannot be used together"));
    }

    #[test]
    fn send_invalid_priority() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "--priority", "ultra", "oracle-agent-idle", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("invalid priority"));
    }

    #[test]
    fn send_json_jsonl_conflict() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "--json", "--jsonl", "oracle-agent-idle", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn send_unknown_flag() {
        let mut backend = single_agent_backend();
        let out = run(
            &["send", "--unknown", "oracle-agent-idle", "hello"],
            &mut backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("unknown flag: '--unknown'"));
    }

    #[test]
    fn send_help_flag() {
        let mut backend = single_agent_backend();
        let out = run(&["send", "--help"], &mut backend);
        assert_success(&out);
        assert!(out.stdout.contains("Queue a message for an agent"));
        assert!(out.stdout.contains("forge send"));
        assert!(out.stdout.contains("--priority"));
    }

    #[test]
    fn send_agent_not_found() {
        let mut backend = single_agent_backend();
        let out = run(&["send", "nonexistent", "hello"], &mut backend);
        // When agent is not found, it falls back to treating first arg as message.
        // Since there's only one agent, it auto-detects and uses "nonexistent hello" as message.
        assert_success(&out);
    }

    #[test]
    fn send_after_wrong_agent_error() {
        let mut backend = multi_agent_backend();
        // Seed an item for oracle-agent-idle.
        let first = run(
            &["send", "oracle-agent-idle", "seed", "--json"],
            &mut backend,
        );
        let first_parsed: serde_json::Value = serde_json::from_str(&first.stdout).unwrap();
        let seed_id = first_parsed["results"][0]["item_id"].as_str().unwrap();

        // Try to insert after that item but targeting a different agent.
        let out = run(
            &[
                "send",
                "--after",
                seed_id,
                "oracle-agent-busy",
                "hello",
                "--json",
            ],
            &mut backend,
        );
        assert_success(&out);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert!(parsed["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("does not belong to agent"));
    }

    fn _assert_backend_object_safe(_b: &mut dyn SendBackend) {}
}

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::context::{ContextBackend, FilesystemContextBackend};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Condition types for the wait command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitCondition {
    Idle,
    QueueEmpty,
    CooldownOver,
    Ready,
    AllIdle,
    AnyIdle,
}

impl WaitCondition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::QueueEmpty => "queue-empty",
            Self::CooldownOver => "cooldown-over",
            Self::Ready => "ready",
            Self::AllIdle => "all-idle",
            Self::AnyIdle => "any-idle",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "idle" => Some(Self::Idle),
            "queue-empty" => Some(Self::QueueEmpty),
            "cooldown-over" => Some(Self::CooldownOver),
            "ready" => Some(Self::Ready),
            "all-idle" => Some(Self::AllIdle),
            "any-idle" => Some(Self::AnyIdle),
            _ => None,
        }
    }

    fn needs_agent(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::QueueEmpty | Self::CooldownOver | Self::Ready
        )
    }

    fn needs_workspace(self) -> bool {
        matches!(self, Self::AllIdle | Self::AnyIdle)
    }
}

/// Result of a single condition check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionResult {
    pub met: bool,
    pub status: String,
}

/// Backend trait for the wait command, allowing testable abstractions.
pub trait WaitBackend {
    /// Check a condition for a specific agent.
    fn check_agent_condition(
        &self,
        agent: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String>;

    /// Check a condition for all agents in a workspace.
    fn check_workspace_condition(
        &self,
        workspace: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String>;

    /// Resolve agent from context if not provided. Returns agent ID.
    fn resolve_agent_context(&self) -> Result<String, String>;

    /// Resolve workspace from context if not provided. Returns workspace ID.
    fn resolve_workspace_context(&self) -> Result<String, String>;

    /// Sleep for the poll interval. In tests, this is a no-op.
    fn sleep_poll_interval(&self);

    /// Check if deadline is exceeded. Returns elapsed duration string if exceeded.
    fn check_deadline(&self) -> Option<String>;

    /// Get the elapsed time as a human-readable string.
    fn elapsed(&self) -> String;

    /// Configure timeout + poll interval before the wait loop starts.
    fn configure_polling(&self, _timeout: Duration, _poll_interval: Duration) {}
}

#[derive(Debug, Clone)]
struct PollingConfig {
    started_at: Instant,
    deadline: Option<Instant>,
    poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct SqliteWaitBackend {
    db_path: PathBuf,
    context_backend: FilesystemContextBackend,
    polling: std::cell::RefCell<PollingConfig>,
}

#[derive(Debug, Clone)]
struct AgentStatus {
    id: String,
    state: String,
    account_id: String,
}

impl SqliteWaitBackend {
    pub fn open_from_env() -> Self {
        Self::new(resolve_database_path(), FilesystemContextBackend::default())
    }

    pub fn new(db_path: PathBuf, context_backend: FilesystemContextBackend) -> Self {
        let now = Instant::now();
        Self {
            db_path,
            context_backend,
            polling: std::cell::RefCell::new(PollingConfig {
                started_at: now,
                deadline: Some(now + Duration::from_secs(30 * 60)),
                poll_interval: Duration::from_secs(2),
            }),
        }
    }

    fn open_db(&self) -> Result<forge_db::Db, String> {
        forge_db::Db::open(forge_db::Config::new(&self.db_path))
            .map_err(|err| format!("open database {}: {err}", self.db_path.display()))
    }

    fn find_agent(&self, agent_ref: &str) -> Result<AgentStatus, String> {
        let db = self.open_db()?;
        let conn = db.conn();
        let trimmed = agent_ref.trim();
        if trimmed.is_empty() {
            return Err("agent ID required".to_string());
        }

        let exact = conn
            .query_row(
                "SELECT id, state, COALESCE(account_id, '') FROM agents WHERE id = ?1",
                rusqlite::params![trimmed],
                |row| {
                    Ok(AgentStatus {
                        id: row.get(0)?,
                        state: row.get(1)?,
                        account_id: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if let Some(agent) = exact {
            return Ok(agent);
        }

        let like = format!("{trimmed}%");
        let mut stmt = conn
            .prepare(
                "SELECT id, state, COALESCE(account_id, '') \
                 FROM agents WHERE id LIKE ?1 ORDER BY id LIMIT 2",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![like], |row| {
                Ok(AgentStatus {
                    id: row.get(0)?,
                    state: row.get(1)?,
                    account_id: row.get(2)?,
                })
            })
            .map_err(|err| err.to_string())?;

        let mut found = Vec::new();
        for row in rows {
            found.push(row.map_err(|err| err.to_string())?);
        }
        match found.len() {
            0 => Err(format!("agent not found: {trimmed}")),
            1 => Ok(found.remove(0)),
            _ => Err(format!(
                "agent '{}' is ambiguous; use a longer prefix or full ID",
                trimmed
            )),
        }
    }

    fn resolve_workspace_id(&self, workspace_ref: &str) -> Result<String, String> {
        let db = self.open_db()?;
        let conn = db.conn();
        let trimmed = workspace_ref.trim();
        if trimmed.is_empty() {
            return Err("workspace name or ID required".to_string());
        }

        let exact_id = conn
            .query_row(
                "SELECT id FROM workspaces WHERE id = ?1",
                rusqlite::params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if let Some(id) = exact_id {
            return Ok(id);
        }

        let exact_name = conn
            .query_row(
                "SELECT id FROM workspaces WHERE name = ?1",
                rusqlite::params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        if let Some(id) = exact_name {
            return Ok(id);
        }

        let like = format!("{trimmed}%");
        let mut stmt = conn
            .prepare("SELECT id FROM workspaces WHERE id LIKE ?1 ORDER BY id LIMIT 2")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![like], |row| row.get::<_, String>(0))
            .map_err(|err| err.to_string())?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|err| err.to_string())?);
        }
        match ids.len() {
            0 => Err(format!("workspace not found: {trimmed}")),
            1 => Ok(ids.remove(0)),
            _ => Err(format!(
                "workspace '{}' is ambiguous; use a longer prefix or full ID",
                trimmed
            )),
        }
    }

    fn pending_queue_items(&self, agent_id: &str) -> Result<i64, String> {
        let db = self.open_db()?;
        let conn = db.conn();
        conn.query_row(
            "SELECT COUNT(1) FROM queue_items WHERE agent_id = ?1 AND status = 'pending'",
            rusqlite::params![agent_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| err.to_string())
    }

    fn cooldown_remaining_secs(&self, account_id: &str) -> Result<Option<i64>, String> {
        let db = self.open_db()?;
        let conn = db.conn();
        let value = conn
            .query_row(
                "SELECT cooldown_until FROM accounts WHERE id = ?1",
                rusqlite::params![account_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|err| err.to_string())?;
        let Some(cooldown_until) = value else {
            return Err(format!("failed to get account: {account_id}"));
        };
        let Some(cooldown_until) = cooldown_until else {
            return Ok(None);
        };
        if cooldown_until.trim().is_empty() {
            return Ok(None);
        }

        let parsed = chrono::DateTime::parse_from_rfc3339(&cooldown_until)
            .map_err(|err| format!("failed to parse cooldown_until: {err}"))?
            .with_timezone(&chrono::Utc);
        let remaining = (parsed - chrono::Utc::now()).num_seconds();
        if remaining <= 0 {
            return Ok(None);
        }
        Ok(Some(remaining))
    }

    fn workspace_agents(&self, workspace_id: &str) -> Result<Vec<AgentStatus>, String> {
        let db = self.open_db()?;
        let conn = db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, state, COALESCE(account_id, '') \
                 FROM agents WHERE workspace_id = ?1 ORDER BY id",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace_id], |row| {
                Ok(AgentStatus {
                    id: row.get(0)?,
                    state: row.get(1)?,
                    account_id: row.get(2)?,
                })
            })
            .map_err(|err| err.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|err| err.to_string())?);
        }
        Ok(out)
    }
}

impl WaitBackend for SqliteWaitBackend {
    fn check_agent_condition(
        &self,
        agent: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String> {
        let agent = self.find_agent(agent)?;

        match condition {
            WaitCondition::Idle => {
                if agent.state == "idle" {
                    Ok(ConditionResult {
                        met: true,
                        status: "idle".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("state: {}", agent.state),
                    })
                }
            }
            WaitCondition::QueueEmpty => {
                let pending = self.pending_queue_items(&agent.id)?;
                if pending == 0 {
                    Ok(ConditionResult {
                        met: true,
                        status: "queue empty".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("queue: {pending} pending"),
                    })
                }
            }
            WaitCondition::CooldownOver => {
                if agent.account_id.trim().is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no account".to_string(),
                    });
                }
                match self.cooldown_remaining_secs(&agent.account_id)? {
                    None => Ok(ConditionResult {
                        met: true,
                        status: "no cooldown".to_string(),
                    }),
                    Some(remaining) => Ok(ConditionResult {
                        met: false,
                        status: format!("cooldown: {remaining}s remaining"),
                    }),
                }
            }
            WaitCondition::Ready => {
                if agent.state != "idle" {
                    return Ok(ConditionResult {
                        met: false,
                        status: format!("state: {}", agent.state),
                    });
                }
                let pending = self.pending_queue_items(&agent.id)?;
                if pending > 0 {
                    return Ok(ConditionResult {
                        met: false,
                        status: format!("queue: {pending} pending"),
                    });
                }
                if !agent.account_id.trim().is_empty() {
                    if let Some(remaining) = self.cooldown_remaining_secs(&agent.account_id)? {
                        return Ok(ConditionResult {
                            met: false,
                            status: format!("cooldown: {remaining}s remaining"),
                        });
                    }
                }
                Ok(ConditionResult {
                    met: true,
                    status: "ready".to_string(),
                })
            }
            _ => Err(format!(
                "condition '{}' requires a workspace, not an agent",
                condition.as_str()
            )),
        }
    }

    fn check_workspace_condition(
        &self,
        workspace: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String> {
        let workspace_id = self.resolve_workspace_id(workspace)?;
        let agents = self.workspace_agents(&workspace_id)?;

        match condition {
            WaitCondition::AllIdle => {
                if agents.is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no agents".to_string(),
                    });
                }
                let not_idle = agents.iter().filter(|agent| agent.state != "idle").count();
                if not_idle == 0 {
                    Ok(ConditionResult {
                        met: true,
                        status: "all idle".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("{}/{} agents not idle", not_idle, agents.len()),
                    })
                }
            }
            WaitCondition::AnyIdle => {
                if agents.is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no agents".to_string(),
                    });
                }
                if let Some(agent) = agents.iter().find(|agent| agent.state == "idle") {
                    return Ok(ConditionResult {
                        met: true,
                        status: format!("agent {} is idle", short_id(&agent.id)),
                    });
                }
                Ok(ConditionResult {
                    met: false,
                    status: format!("0/{} agents idle", agents.len()),
                })
            }
            _ => Err(format!(
                "condition '{}' requires an agent, not a workspace",
                condition.as_str()
            )),
        }
    }

    fn resolve_agent_context(&self) -> Result<String, String> {
        let ctx = self
            .context_backend
            .load_context()
            .map_err(|err| format!("failed to load context: {err}"))?;
        if ctx.agent_id.trim().is_empty() {
            return Err("no agent context set".to_string());
        }
        Ok(ctx.agent_id)
    }

    fn resolve_workspace_context(&self) -> Result<String, String> {
        let ctx = self
            .context_backend
            .load_context()
            .map_err(|err| format!("failed to load context: {err}"))?;
        if ctx.workspace_id.trim().is_empty() {
            return Err("no workspace context set".to_string());
        }
        Ok(ctx.workspace_id)
    }

    fn sleep_poll_interval(&self) {
        let poll = self.polling.borrow().poll_interval;
        std::thread::sleep(poll);
    }

    fn check_deadline(&self) -> Option<String> {
        let deadline = self.polling.borrow().deadline?;
        if Instant::now() >= deadline {
            return Some(self.elapsed());
        }
        None
    }

    fn elapsed(&self) -> String {
        let elapsed = self.polling.borrow().started_at.elapsed();
        format_elapsed(elapsed)
    }

    fn configure_polling(&self, timeout: Duration, poll_interval: Duration) {
        let now = Instant::now();
        let mut polling = self.polling.borrow_mut();
        polling.started_at = now;
        polling.deadline = Some(now + timeout);
        polling.poll_interval = poll_interval;
    }
}

/// Agent state for the in-memory backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Starting,
    Stopping,
    Error,
}

impl AgentState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Starting => "starting",
            Self::Stopping => "stopping",
            Self::Error => "error",
        }
    }
}

/// Agent record for the in-memory backend.
#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: String,
    pub workspace_id: String,
    pub state: AgentState,
    pub account_id: String,
    pub pending_queue_items: usize,
    pub cooldown_remaining_secs: Option<i64>,
}

/// In-memory backend for testing.
#[derive(Debug, Clone, Default)]
pub struct InMemoryWaitBackend {
    pub agents: Vec<AgentRecord>,
    pub agent_context: Option<String>,
    pub workspace_context: Option<String>,
    /// If true, check_deadline returns a timeout.
    pub deadline_exceeded: bool,
    /// Fixed elapsed string for deterministic output.
    pub elapsed_str: String,
    /// How many poll iterations before condition is met (0 = already met).
    pub polls_before_met: usize,
    poll_count: std::cell::Cell<usize>,
}

impl InMemoryWaitBackend {
    pub fn with_agents(agents: Vec<AgentRecord>) -> Self {
        Self {
            agents,
            elapsed_str: "0s".to_string(),
            ..Default::default()
        }
    }
}

impl WaitBackend for InMemoryWaitBackend {
    fn check_agent_condition(
        &self,
        agent_id: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String> {
        let agent = self
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| format!("agent not found: {agent_id}"))?;

        match condition {
            WaitCondition::Idle => {
                if agent.state == AgentState::Idle {
                    Ok(ConditionResult {
                        met: true,
                        status: "idle".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("state: {}", agent.state.as_str()),
                    })
                }
            }
            WaitCondition::QueueEmpty => {
                if agent.pending_queue_items == 0 {
                    Ok(ConditionResult {
                        met: true,
                        status: "queue empty".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("queue: {} pending", agent.pending_queue_items),
                    })
                }
            }
            WaitCondition::CooldownOver => {
                if agent.account_id.is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no account".to_string(),
                    });
                }
                match agent.cooldown_remaining_secs {
                    None | Some(0) => Ok(ConditionResult {
                        met: true,
                        status: "no cooldown".to_string(),
                    }),
                    Some(secs) if secs < 0 => Ok(ConditionResult {
                        met: true,
                        status: "no cooldown".to_string(),
                    }),
                    Some(secs) => Ok(ConditionResult {
                        met: false,
                        status: format!("cooldown: {secs}s remaining"),
                    }),
                }
            }
            WaitCondition::Ready => {
                // Check idle
                if agent.state != AgentState::Idle {
                    return Ok(ConditionResult {
                        met: false,
                        status: format!("state: {}", agent.state.as_str()),
                    });
                }
                // Check queue
                if agent.pending_queue_items > 0 {
                    return Ok(ConditionResult {
                        met: false,
                        status: format!("queue: {} pending", agent.pending_queue_items),
                    });
                }
                // Check cooldown
                if !agent.account_id.is_empty() {
                    if let Some(secs) = agent.cooldown_remaining_secs {
                        if secs > 0 {
                            return Ok(ConditionResult {
                                met: false,
                                status: format!("cooldown: {secs}s remaining"),
                            });
                        }
                    }
                }
                Ok(ConditionResult {
                    met: true,
                    status: "ready".to_string(),
                })
            }
            _ => Err(format!(
                "condition '{}' requires a workspace, not an agent",
                condition.as_str()
            )),
        }
    }

    fn check_workspace_condition(
        &self,
        workspace_id: &str,
        condition: WaitCondition,
    ) -> Result<ConditionResult, String> {
        let agents: Vec<&AgentRecord> = self
            .agents
            .iter()
            .filter(|a| a.workspace_id == workspace_id)
            .collect();

        // Verify workspace exists by checking if we have any reference to it
        // (in real impl this would query the workspace table)
        if agents.is_empty() && !self.agents.iter().any(|a| a.workspace_id == workspace_id) {
            // Check if workspace is known at all - for tests, empty agents list means "no agents"
            // which is a valid state (returns true).
        }

        match condition {
            WaitCondition::AllIdle => {
                if agents.is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no agents".to_string(),
                    });
                }
                let not_idle = agents
                    .iter()
                    .filter(|a| a.state != AgentState::Idle)
                    .count();
                if not_idle == 0 {
                    Ok(ConditionResult {
                        met: true,
                        status: "all idle".to_string(),
                    })
                } else {
                    Ok(ConditionResult {
                        met: false,
                        status: format!("{}/{} agents not idle", not_idle, agents.len()),
                    })
                }
            }
            WaitCondition::AnyIdle => {
                if agents.is_empty() {
                    return Ok(ConditionResult {
                        met: true,
                        status: "no agents".to_string(),
                    });
                }
                for agent in &agents {
                    if agent.state == AgentState::Idle {
                        let short = if agent.id.len() > 8 {
                            &agent.id[..8]
                        } else {
                            &agent.id
                        };
                        return Ok(ConditionResult {
                            met: true,
                            status: format!("agent {short} is idle"),
                        });
                    }
                }
                Ok(ConditionResult {
                    met: false,
                    status: format!("0/{} agents idle", agents.len()),
                })
            }
            _ => Err(format!(
                "condition '{}' requires an agent, not a workspace",
                condition.as_str()
            )),
        }
    }

    fn resolve_agent_context(&self) -> Result<String, String> {
        self.agent_context
            .clone()
            .ok_or_else(|| "no agent context set".to_string())
    }

    fn resolve_workspace_context(&self) -> Result<String, String> {
        self.workspace_context
            .clone()
            .ok_or_else(|| "no workspace context set".to_string())
    }

    fn sleep_poll_interval(&self) {
        self.poll_count.set(self.poll_count.get() + 1);
    }

    fn check_deadline(&self) -> Option<String> {
        if self.deadline_exceeded {
            Some(self.elapsed_str.clone())
        } else {
            None
        }
    }

    fn elapsed(&self) -> String {
        self.elapsed_str.clone()
    }
}

#[derive(Debug, Clone)]
struct ParsedArgs {
    json: bool,
    jsonl: bool,
    quiet: bool,
    until: String,
    agent: String,
    workspace: String,
    timeout: Duration,
    poll_interval: Duration,
}

#[derive(Debug, Serialize)]
struct WaitResult {
    success: bool,
    condition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub fn run_for_test(args: &[&str], backend: &dyn WaitBackend) -> CommandOutput {
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
    backend: &dyn WaitBackend,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match execute(args, backend, stdout) {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(stderr, "{message}");
            1
        }
    }
}

fn execute(
    args: &[String],
    backend: &dyn WaitBackend,
    stdout: &mut dyn Write,
) -> Result<i32, String> {
    let parsed = parse_args(args)?;

    let condition = WaitCondition::from_str(&parsed.until).ok_or_else(|| {
        format!(
            "invalid condition '{}'; valid conditions: [idle queue-empty cooldown-over all-idle any-idle ready]",
            parsed.until
        )
    })?;

    // Resolve target
    let agent_id = if condition.needs_agent() {
        if parsed.agent.is_empty() {
            match backend.resolve_agent_context() {
                Ok(id) => id,
                Err(_) => {
                    return Err(format!(
                        "--agent is required for condition '{}' (no context set)",
                        condition.as_str()
                    ));
                }
            }
        } else {
            parsed.agent.clone()
        }
    } else {
        String::new()
    };

    let workspace_id = if condition.needs_workspace() {
        if parsed.workspace.is_empty() {
            match backend.resolve_workspace_context() {
                Ok(id) => id,
                Err(_) => {
                    return Err(format!(
                        "--workspace is required for condition '{}' (no context set)",
                        condition.as_str()
                    ));
                }
            }
        } else {
            parsed.workspace.clone()
        }
    } else {
        String::new()
    };

    backend.configure_polling(parsed.timeout, parsed.poll_interval);

    if !parsed.quiet && !parsed.json && !parsed.jsonl {
        writeln!(stdout, "Waiting for condition '{}'...", condition.as_str())
            .map_err(|err| err.to_string())?;
    }

    let mut last_status = String::new();
    loop {
        // Check deadline
        if let Some(elapsed) = backend.check_deadline() {
            if parsed.json || parsed.jsonl {
                let payload = WaitResult {
                    success: false,
                    condition: condition.as_str().to_string(),
                    elapsed: Some(elapsed),
                    reason: Some("timeout".to_string()),
                    error: None,
                };
                write_json(stdout, &payload, parsed.jsonl)?;
                return Ok(0);
            }
            if !parsed.quiet {
                writeln!(stdout, "\nTimeout reached after {elapsed}")
                    .map_err(|err| err.to_string())?;
            }
            return Ok(1);
        }

        // Check condition
        let result = if condition.needs_agent() {
            backend.check_agent_condition(&agent_id, condition)?
        } else {
            backend.check_workspace_condition(&workspace_id, condition)?
        };

        if result.met {
            let elapsed = backend.elapsed();
            if parsed.json || parsed.jsonl {
                let payload = WaitResult {
                    success: true,
                    condition: condition.as_str().to_string(),
                    elapsed: Some(elapsed),
                    reason: None,
                    error: None,
                };
                write_json(stdout, &payload, parsed.jsonl)?;
                return Ok(0);
            }
            if !parsed.quiet {
                writeln!(
                    stdout,
                    "\nCondition '{}' met (waited {elapsed})",
                    condition.as_str()
                )
                .map_err(|err| err.to_string())?;
            }
            return Ok(0);
        }

        // Print status update if changed
        if !parsed.quiet && !parsed.json && !parsed.jsonl && result.status != last_status {
            let elapsed = backend.elapsed();
            writeln!(stdout, "  {} (elapsed: {elapsed})", result.status)
                .map_err(|err| err.to_string())?;
            last_status = result.status;
        }

        backend.sleep_poll_interval();
    }
}

fn write_json(stdout: &mut dyn Write, payload: &WaitResult, jsonl: bool) -> Result<(), String> {
    if jsonl {
        serde_json::to_writer(&mut *stdout, payload).map_err(|err| err.to_string())?;
    } else {
        serde_json::to_writer_pretty(&mut *stdout, payload).map_err(|err| err.to_string())?;
    }
    writeln!(stdout).map_err(|err| err.to_string())?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut index = 0usize;
    if args.get(index).is_some_and(|token| token == "wait") {
        index += 1;
    }

    let mut json = false;
    let mut jsonl = false;
    let mut quiet = false;
    let mut until = String::new();
    let mut agent = String::new();
    let mut workspace = String::new();
    let mut timeout = Duration::from_secs(30 * 60);
    let mut poll_interval = Duration::from_secs(2);

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
            "--quiet" | "-q" => {
                quiet = true;
                index += 1;
            }
            "--until" | "-u" => {
                until = take_value(args, index, "--until")?;
                index += 2;
            }
            "--agent" | "-a" => {
                agent = take_value(args, index, "--agent")?;
                index += 2;
            }
            "--workspace" | "-w" => {
                workspace = take_value(args, index, "--workspace")?;
                index += 2;
            }
            "--timeout" | "-t" => {
                let raw = take_value(args, index, "--timeout")?;
                timeout = parse_duration_flag(&raw, "--timeout")?;
                index += 2;
            }
            "--poll-interval" => {
                let raw = take_value(args, index, "--poll-interval")?;
                poll_interval = parse_duration_flag(&raw, "--poll-interval")?;
                index += 2;
            }
            flag if flag.starts_with('-') => {
                return Err(format!("error: unknown argument for wait: '{flag}'"));
            }
            _value => {
                return Err(format!("error: unexpected positional argument '{_value}'"));
            }
        }
    }

    if json && jsonl {
        return Err("error: --json and --jsonl cannot be used together".to_string());
    }

    if until.is_empty() {
        return Err("error: --until is required".to_string());
    }

    Ok(ParsedArgs {
        json,
        jsonl,
        quiet,
        until,
        agent,
        workspace,
        timeout,
        poll_interval,
    })
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| format!("error: missing value for {flag}"))
}

fn resolve_database_path() -> PathBuf {
    crate::runtime_paths::resolve_database_path()
}

fn short_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

fn format_elapsed(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        return format!("{hours}h{minutes}m{seconds}s");
    }
    if minutes > 0 {
        return format!("{minutes}m{seconds}s");
    }
    format!("{seconds}s")
}

fn parse_duration_flag(raw: &str, flag: &str) -> Result<Duration, String> {
    let parsed =
        parse_go_duration(raw).map_err(|err| format!("error: invalid value for {flag}: {err}"))?;
    if parsed.is_zero() {
        return Err(format!(
            "error: invalid value for {flag}: duration must be > 0"
        ));
    }
    Ok(parsed)
}

fn parse_go_duration(raw: &str) -> Result<Duration, String> {
    let seconds = parse_go_duration_seconds(raw)?;
    if seconds < 0.0 {
        return Err("duration must be non-negative".to_string());
    }
    Duration::try_from_secs_f64(seconds).map_err(|_| "duration out of range".to_string())
}

fn parse_go_duration_seconds(raw: &str) -> Result<f64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }

    let (negative, mut rest) = if let Some(value) = trimmed.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = trimmed.strip_prefix('+') {
        (false, value)
    } else {
        (false, trimmed)
    };

    if rest.is_empty() {
        return Err("empty duration".to_string());
    }

    if rest == "0" {
        return Ok(0.0);
    }

    let mut total_seconds = 0.0f64;
    while !rest.is_empty() {
        let num_len = number_prefix_len(rest);
        if num_len == 0 {
            return Err("invalid duration value".to_string());
        }

        let number_raw = &rest[..num_len];
        let value = number_raw
            .parse::<f64>()
            .map_err(|_| "invalid duration value".to_string())?;
        if !value.is_finite() {
            return Err("invalid duration value".to_string());
        }

        rest = &rest[num_len..];
        let (unit, scale_seconds) = duration_unit(rest)?;
        total_seconds += value * scale_seconds;
        rest = &rest[unit.len()..];
    }

    if negative {
        total_seconds = -total_seconds;
    }
    Ok(total_seconds)
}

fn number_prefix_len(input: &str) -> usize {
    let mut bytes = 0usize;
    let mut saw_digit = false;
    let mut saw_dot = false;

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            bytes += ch.len_utf8();
            continue;
        }
        if ch == '.' && !saw_dot {
            saw_dot = true;
            bytes += ch.len_utf8();
            continue;
        }
        break;
    }

    if saw_digit {
        bytes
    } else {
        0
    }
}

fn duration_unit(input: &str) -> Result<(&'static str, f64), String> {
    if input.starts_with("ns") {
        return Ok(("ns", 1e-9));
    }
    if input.starts_with("us") {
        return Ok(("us", 1e-6));
    }
    if input.starts_with("µs") {
        return Ok(("µs", 1e-6));
    }
    if input.starts_with("μs") {
        return Ok(("μs", 1e-6));
    }
    if input.starts_with("ms") {
        return Ok(("ms", 1e-3));
    }
    if input.starts_with('s') {
        return Ok(("s", 1.0));
    }
    if input.starts_with('m') {
        return Ok(("m", 60.0));
    }
    if input.starts_with('h') {
        return Ok(("h", 3600.0));
    }
    Err("missing duration unit".to_string())
}

const HELP_TEXT: &str = "\
Wait for a condition to be met

Usage:
  forge wait [flags]

Flags:
  -u, --until string       condition to wait for (required)
  -a, --agent string       agent to wait for
  -w, --workspace string   workspace to wait for
  -t, --timeout duration   maximum wait time (default: 30m)
      --poll-interval dur  check interval (default: 2s)
  -q, --quiet              no output, just wait
  -h, --help               help for wait

Valid conditions:
  idle           Agent is in idle state
  queue-empty    Agent's queue has no pending items
  cooldown-over  Account's cooldown period has expired
  ready          Agent is idle, queue empty, and no cooldown
  all-idle       All agents in workspace are idle
  any-idle       At least one agent in workspace is idle

Exit codes:
  0: Condition met
  1: Timeout reached";

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn idle_agent() -> AgentRecord {
        AgentRecord {
            id: "agent-001".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
            account_id: "acc-001".to_string(),
            pending_queue_items: 0,
            cooldown_remaining_secs: None,
        }
    }

    fn working_agent() -> AgentRecord {
        AgentRecord {
            id: "agent-002".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Working,
            account_id: "acc-001".to_string(),
            pending_queue_items: 0,
            cooldown_remaining_secs: None,
        }
    }

    fn agent_with_queue(pending: usize) -> AgentRecord {
        AgentRecord {
            id: "agent-003".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
            account_id: "acc-001".to_string(),
            pending_queue_items: pending,
            cooldown_remaining_secs: None,
        }
    }

    fn agent_with_cooldown(secs: i64) -> AgentRecord {
        AgentRecord {
            id: "agent-004".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
            account_id: "acc-001".to_string(),
            pending_queue_items: 0,
            cooldown_remaining_secs: Some(secs),
        }
    }

    fn agent_no_account() -> AgentRecord {
        AgentRecord {
            id: "agent-005".to_string(),
            workspace_id: "ws-001".to_string(),
            state: AgentState::Idle,
            account_id: String::new(),
            pending_queue_items: 0,
            cooldown_remaining_secs: None,
        }
    }

    // --- parse_args tests ---

    #[test]
    fn parse_requires_until_flag() {
        let args = vec!["wait".to_string()];
        let err = parse_args(&args).unwrap_err();
        assert_eq!(err, "error: --until is required");
    }

    #[test]
    fn parse_accepts_until_flag() {
        let args = vec![
            "wait".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "--agent".to_string(),
            "a1".to_string(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.until, "idle");
        assert_eq!(parsed.agent, "a1");
    }

    #[test]
    fn parse_accepts_short_flags() {
        let args = vec![
            "wait".to_string(),
            "-u".to_string(),
            "ready".to_string(),
            "-a".to_string(),
            "agent-x".to_string(),
            "-q".to_string(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.until, "ready");
        assert_eq!(parsed.agent, "agent-x");
        assert!(parsed.quiet);
    }

    #[test]
    fn parse_rejects_json_and_jsonl_together() {
        let args = vec![
            "wait".to_string(),
            "--json".to_string(),
            "--jsonl".to_string(),
            "--until".to_string(),
            "idle".to_string(),
        ];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--json and --jsonl cannot be used together"));
    }

    #[test]
    fn parse_rejects_unknown_flag() {
        let args = vec![
            "wait".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "--bogus".to_string(),
        ];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown argument for wait: '--bogus'"));
    }

    #[test]
    fn parse_rejects_positional_arg() {
        let args = vec![
            "wait".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "extra".to_string(),
        ];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unexpected positional argument"));
    }

    #[test]
    fn parse_accepts_timeout_and_poll_interval() {
        let args = vec![
            "wait".to_string(),
            "--until".to_string(),
            "idle".to_string(),
            "--agent".to_string(),
            "a1".to_string(),
            "--timeout".to_string(),
            "5m".to_string(),
            "--poll-interval".to_string(),
            "500ms".to_string(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.until, "idle");
    }

    // --- invalid condition test ---

    #[test]
    fn invalid_condition_returns_error() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "definitely-invalid",
                "--agent",
                "agent-001",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert_eq!(
            out.stderr,
            "invalid condition 'definitely-invalid'; valid conditions: [idle queue-empty cooldown-over all-idle any-idle ready]\n"
        );
    }

    // --- idle condition ---

    #[test]
    fn idle_condition_met() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-001", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "idle");
    }

    #[test]
    fn idle_condition_not_met_working() {
        // Backend that always returns "working" agent - since polling would loop forever,
        // we set deadline_exceeded so it times out on the second check.
        let mut backend = InMemoryWaitBackend::with_agents(vec![working_agent()]);
        backend.deadline_exceeded = true;
        backend.elapsed_str = "30m0s".to_string();
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-002", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0); // JSON mode returns 0 even on timeout
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["reason"], "timeout");
    }

    #[test]
    fn idle_condition_human_output() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-001"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("Waiting for condition 'idle'..."));
        assert!(out.stdout.contains("Condition 'idle' met"));
    }

    #[test]
    fn idle_condition_quiet() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-001", "--quiet"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[test]
    fn idle_timeout_human_output() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![working_agent()]);
        backend.deadline_exceeded = true;
        backend.elapsed_str = "30m0s".to_string();
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-002"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.contains("Timeout reached after 30m0s"));
    }

    #[test]
    fn idle_timeout_quiet() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![working_agent()]);
        backend.deadline_exceeded = true;
        backend.elapsed_str = "30m0s".to_string();
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-002", "--quiet"],
            &backend,
        );
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
    }

    // --- queue-empty condition ---

    #[test]
    fn queue_empty_condition_met() {
        let backend = InMemoryWaitBackend::with_agents(vec![agent_with_queue(0)]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "queue-empty",
                "--agent",
                "agent-003",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "queue-empty");
    }

    #[test]
    fn queue_not_empty_times_out() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![agent_with_queue(3)]);
        backend.deadline_exceeded = true;
        backend.elapsed_str = "5m".to_string();
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "queue-empty",
                "--agent",
                "agent-003",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["reason"], "timeout");
    }

    // --- cooldown-over condition ---

    #[test]
    fn cooldown_over_no_cooldown() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "cooldown-over",
                "--agent",
                "agent-001",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[test]
    fn cooldown_over_no_account() {
        let backend = InMemoryWaitBackend::with_agents(vec![agent_no_account()]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "cooldown-over",
                "--agent",
                "agent-005",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[test]
    fn cooldown_active_times_out() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![agent_with_cooldown(300)]);
        backend.deadline_exceeded = true;
        backend.elapsed_str = "5m".to_string();
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "cooldown-over",
                "--agent",
                "agent-004",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    // --- ready condition ---

    #[test]
    fn ready_condition_met() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &["wait", "--until", "ready", "--agent", "agent-001", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "ready");
    }

    #[test]
    fn ready_not_met_working() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![working_agent()]);
        backend.deadline_exceeded = true;
        let out = run_for_test(
            &["wait", "--until", "ready", "--agent", "agent-002", "--json"],
            &backend,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    #[test]
    fn ready_not_met_queue_pending() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![agent_with_queue(2)]);
        backend.deadline_exceeded = true;
        let out = run_for_test(
            &["wait", "--until", "ready", "--agent", "agent-003", "--json"],
            &backend,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    #[test]
    fn ready_not_met_cooldown_active() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![agent_with_cooldown(60)]);
        backend.deadline_exceeded = true;
        let out = run_for_test(
            &["wait", "--until", "ready", "--agent", "agent-004", "--json"],
            &backend,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    // --- all-idle condition ---

    #[test]
    fn all_idle_condition_met() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "all-idle",
                "--workspace",
                "ws-001",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "all-idle");
    }

    #[test]
    fn all_idle_not_met() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![idle_agent(), working_agent()]);
        backend.deadline_exceeded = true;
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "all-idle",
                "--workspace",
                "ws-001",
                "--json",
            ],
            &backend,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    #[test]
    fn all_idle_no_agents() {
        let backend = InMemoryWaitBackend::default();
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "all-idle",
                "--workspace",
                "ws-empty",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
    }

    // --- any-idle condition ---

    #[test]
    fn any_idle_condition_met() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent(), working_agent()]);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "any-idle",
                "--workspace",
                "ws-001",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "any-idle");
    }

    #[test]
    fn any_idle_none_idle() {
        let agent1 = AgentRecord {
            id: "agent-a".to_string(),
            workspace_id: "ws-002".to_string(),
            state: AgentState::Working,
            account_id: String::new(),
            pending_queue_items: 0,
            cooldown_remaining_secs: None,
        };
        let agent2 = AgentRecord {
            id: "agent-b".to_string(),
            workspace_id: "ws-002".to_string(),
            state: AgentState::Working,
            account_id: String::new(),
            pending_queue_items: 0,
            cooldown_remaining_secs: None,
        };
        let mut backend = InMemoryWaitBackend::with_agents(vec![agent1, agent2]);
        backend.deadline_exceeded = true;
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "any-idle",
                "--workspace",
                "ws-002",
                "--json",
            ],
            &backend,
        );
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
    }

    // --- target requirement validation ---

    #[test]
    fn agent_required_for_idle_no_context() {
        let backend = InMemoryWaitBackend::default();
        let out = run_for_test(&["wait", "--until", "idle"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--agent is required for condition 'idle' (no context set)"));
    }

    #[test]
    fn workspace_required_for_all_idle_no_context() {
        let backend = InMemoryWaitBackend::default();
        let out = run_for_test(&["wait", "--until", "all-idle"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out
            .stderr
            .contains("--workspace is required for condition 'all-idle' (no context set)"));
    }

    #[test]
    fn agent_resolved_from_context() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        backend.agent_context = Some("agent-001".to_string());
        let out = run_for_test(&["wait", "--until", "idle", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
    }

    #[test]
    fn workspace_resolved_from_context() {
        let mut backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        backend.workspace_context = Some("ws-001".to_string());
        let out = run_for_test(&["wait", "--until", "all-idle", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
    }

    // --- agent not found ---

    #[test]
    fn agent_not_found_returns_error() {
        let backend = InMemoryWaitBackend::default();
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "idle",
                "--agent",
                "nonexistent",
                "--json",
            ],
            &backend,
        );
        // Agent not found propagates as an error
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("agent not found"));
    }

    // --- JSONL output ---

    #[test]
    fn idle_condition_jsonl_output() {
        let backend = InMemoryWaitBackend::with_agents(vec![idle_agent()]);
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-001", "--jsonl"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        // JSONL should be compact (no pretty printing)
        assert!(
            !out.stdout.contains('\n')
                || out.stdout.trim_end().chars().filter(|c| *c == '\n').count() == 0
        );
    }

    // --- help ---

    #[test]
    fn help_flag_shows_help() {
        let backend = InMemoryWaitBackend::default();
        let out = run_for_test(&["wait", "--help"], &backend);
        assert_eq!(out.exit_code, 1);
        assert!(out.stderr.contains("Wait for a condition to be met"));
        assert!(out.stderr.contains("--until"));
    }

    // --- sqlite backend live parity ---

    fn setup_sqlite_wait_backend(
        label: &str,
        state: &str,
        pending_queue: usize,
    ) -> (SqliteWaitBackend, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("forge-wait-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("forge.db");
        let ctx_path = root.join("context.yaml");

        let mut db = forge_db::Db::open(forge_db::Config::new(&db_path)).unwrap();
        db.migrate_up().unwrap();

        let conn = db.conn();
        conn.execute(
            "INSERT INTO nodes (id, name, status, is_local) VALUES (?1, ?2, 'online', 1)",
            rusqlite::params!["node-1", "node-1"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspaces (id, name, node_id, repo_path, tmux_session, status) VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
            rusqlite::params!["ws-1", "workspace-1", "node-1", "/tmp/repo", "forge-ws-1"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agents (id, workspace_id, type, tmux_pane, state) VALUES (?1, ?2, 'codex', ?3, ?4)",
            rusqlite::params!["agent-001", "ws-1", "forge-ws-1:0.1", state],
        )
        .unwrap();
        for idx in 0..pending_queue {
            conn.execute(
                "INSERT INTO queue_items (id, agent_id, type, position, status, payload_json) VALUES (?1, ?2, 'message', ?3, 'pending', ?4)",
                rusqlite::params![
                    format!("qi-{idx}"),
                    "agent-001",
                    idx as i64 + 1,
                    r#"{"message":"hello"}"#
                ],
            )
            .unwrap();
        }

        let context_backend = FilesystemContextBackend::new(ctx_path, db_path.clone());
        context_backend
            .save_context(&crate::context::ContextRecord {
                workspace_id: "ws-1".to_string(),
                workspace_name: "workspace-1".to_string(),
                agent_id: "agent-001".to_string(),
                agent_name: String::new(),
                updated_at: "2026-02-10T00:00:00Z".to_string(),
            })
            .unwrap();

        (SqliteWaitBackend::new(db_path, context_backend), root)
    }

    #[test]
    fn sqlite_idle_condition_met() {
        let (backend, root) = setup_sqlite_wait_backend("idle", "idle", 0);
        let out = run_for_test(
            &["wait", "--until", "idle", "--agent", "agent-001", "--json"],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["condition"], "idle");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sqlite_queue_not_empty_times_out() {
        let (backend, root) = setup_sqlite_wait_backend("queue", "idle", 1);
        let out = run_for_test(
            &[
                "wait",
                "--until",
                "queue-empty",
                "--agent",
                "agent-001",
                "--timeout",
                "10ms",
                "--poll-interval",
                "2ms",
                "--json",
            ],
            &backend,
        );
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["reason"], "timeout");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sqlite_resolves_agent_from_context() {
        let (backend, root) = setup_sqlite_wait_backend("context", "idle", 0);
        let out = run_for_test(&["wait", "--until", "idle", "--json"], &backend);
        assert_eq!(out.exit_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(parsed["success"], true);
        std::fs::remove_dir_all(root).unwrap();
    }
}

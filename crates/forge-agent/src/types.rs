//! Transport-agnostic agent types.
//!
//! These types provide a clean domain model for agent operations,
//! decoupled from proto/gRPC specifics.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

/// Agent state mirroring proto `AgentState` but as a Rust enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentState {
    Unspecified,
    Starting,
    Running,
    Idle,
    WaitingApproval,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

impl AgentState {
    pub fn from_proto_i32(v: i32) -> Self {
        match v {
            1 => Self::Starting,
            2 => Self::Running,
            3 => Self::Idle,
            4 => Self::WaitingApproval,
            5 => Self::Paused,
            6 => Self::Stopping,
            7 => Self::Stopped,
            8 => Self::Failed,
            _ => Self::Unspecified,
        }
    }

    pub fn to_proto_i32(self) -> i32 {
        match self {
            Self::Unspecified => 0,
            Self::Starting => 1,
            Self::Running => 2,
            Self::Idle => 3,
            Self::WaitingApproval => 4,
            Self::Paused => 5,
            Self::Stopping => 6,
            Self::Stopped => 7,
            Self::Failed => 8,
        }
    }

    /// Whether the agent is in a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }

    /// Whether the agent is in an active (non-terminal) state.
    pub fn is_active(self) -> bool {
        !self.is_terminal() && self != Self::Unspecified
    }

    /// Parse a state from its string representation.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unspecified" => Some(Self::Unspecified),
            "starting" => Some(Self::Starting),
            "running" => Some(Self::Running),
            "idle" => Some(Self::Idle),
            "waiting_approval" => Some(Self::WaitingApproval),
            "paused" => Some(Self::Paused),
            "stopping" => Some(Self::Stopping),
            "stopped" => Some(Self::Stopped),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Unspecified => "unspecified",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::WaitingApproval => "waiting_approval",
            Self::Paused => "paused",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        };
        f.write_str(s)
    }
}

/// Snapshot of an agent returned by service operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub id: String,
    pub workspace_id: String,
    pub state: AgentState,
    pub pane_id: String,
    pub pid: i32,
    pub command: String,
    pub adapter: String,
    pub spawned_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// Requested agent mode from caller intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRequestMode {
    Continuous,
    OneShot,
}

impl AgentRequestMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::OneShot => "one-shot",
        }
    }
}

/// Parameters for spawning a new agent.
#[derive(Debug, Clone)]
pub struct SpawnAgentParams {
    pub agent_id: String,
    pub workspace_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: String,
    pub session_name: String,
    pub adapter: String,
    pub requested_mode: AgentRequestMode,
    pub allow_oneshot_fallback: bool,
}

/// Parameters for sending a message/input to an agent.
#[derive(Debug, Clone)]
pub struct SendMessageParams {
    pub agent_id: String,
    pub text: String,
    pub send_enter: bool,
    pub keys: Vec<String>,
}

/// Parameters for waiting on an agent to reach a target state.
#[derive(Debug, Clone)]
pub struct WaitStateParams {
    pub agent_id: String,
    pub target_states: Vec<AgentState>,
    pub timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for WaitStateParams {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            target_states: vec![AgentState::Idle],
            timeout: Duration::from_secs(300),
            poll_interval: Duration::from_millis(500),
        }
    }
}

/// Parameters for killing an agent.
#[derive(Debug, Clone)]
pub struct KillAgentParams {
    pub agent_id: String,
    pub force: bool,
    pub grace_period: Option<Duration>,
}

/// Filter criteria for listing agents.
#[derive(Debug, Clone, Default)]
pub struct ListAgentsFilter {
    pub workspace_id: Option<String>,
    pub states: Vec<AgentState>,
}

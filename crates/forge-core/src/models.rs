//! Domain model types for the Forge system.
//!
//! Mirrors Go `internal/models/` — entity types, state enums, and
//! associated metadata used across all Forge crates.

use std::fmt;

// ---------------------------------------------------------------------------
// Loop
// ---------------------------------------------------------------------------

/// Runtime state of a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopState {
    Running,
    Sleeping,
    Waiting,
    Stopped,
    Error,
}

impl fmt::Display for LoopState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Waiting => "waiting",
            Self::Stopped => "stopped",
            Self::Error => "error",
        };
        f.write_str(s)
    }
}

/// Status of a single loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopRunStatus {
    Running,
    Success,
    Error,
    Killed,
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// Observed state of an agent process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentState {
    Working,
    Idle,
    AwaitingApproval,
    RateLimited,
    Error,
    Paused,
    Starting,
    Stopped,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Working => "working",
            Self::Idle => "idle",
            Self::AwaitingApproval => "awaiting_approval",
            Self::RateLimited => "rate_limited",
            Self::Error => "error",
            Self::Paused => "paused",
            Self::Starting => "starting",
            Self::Stopped => "stopped",
        };
        f.write_str(s)
    }
}

/// CLI agent harness type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentType {
    OpenCode,
    ClaudeCode,
    Codex,
    Gemini,
    Generic,
}

/// Confidence level for state detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateConfidence {
    High,
    Medium,
    Low,
}

/// Adapter integration tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterTier {
    Generic,
    Telemetry,
    Native,
}

// ---------------------------------------------------------------------------
// Provider / Harness / Profile
// ---------------------------------------------------------------------------

/// AI provider backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Anthropic,
    OpenAI,
    Google,
    Custom,
}

/// Agent harness type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Harness {
    Pi,
    OpenCode,
    Codex,
    Claude,
    Droid,
}

/// How prompts are delivered to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptMode {
    Env,
    Stdin,
    Path,
}

/// Profile pool selection strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolStrategy {
    RoundRobin,
    Lru,
}

// ---------------------------------------------------------------------------
// Node / Execution
// ---------------------------------------------------------------------------

/// SSH backend implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SshBackend {
    Native,
    System,
    Auto,
}

/// Command execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionMode {
    Auto,
    Forged,
    Ssh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_state_display() {
        assert_eq!(LoopState::Running.to_string(), "running");
        assert_eq!(LoopState::Stopped.to_string(), "stopped");
        assert_eq!(LoopState::Error.to_string(), "error");
    }

    #[test]
    fn agent_state_display() {
        assert_eq!(AgentState::Working.to_string(), "working");
        assert_eq!(
            AgentState::AwaitingApproval.to_string(),
            "awaiting_approval"
        );
        assert_eq!(AgentState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn enum_equality() {
        assert_eq!(Provider::Anthropic, Provider::Anthropic);
        assert_ne!(Provider::Anthropic, Provider::OpenAI);
        assert_eq!(Harness::Claude, Harness::Claude);
        assert_ne!(PoolStrategy::RoundRobin, PoolStrategy::Lru);
    }
}

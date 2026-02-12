//! Normalized error types for agent service operations.
//!
//! Transport-agnostic errors that hide gRPC/tonic details and provide
//! actionable error categories for callers.

use std::fmt;

/// Normalized error for agent service operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentServiceError {
    /// Agent not found by id.
    NotFound { agent_id: String },

    /// Agent already exists (duplicate spawn).
    AlreadyExists { agent_id: String },

    /// Request validation failed (missing required fields, bad arguments).
    InvalidArgument { message: String },

    /// Agent is in a state that does not allow the requested operation.
    /// For example, sending input to a stopped agent.
    InvalidState {
        agent_id: String,
        current_state: String,
        operation: String,
    },

    /// The forged daemon is unreachable or connection failed.
    TransportUnavailable { message: String },

    /// An RPC call returned an internal/unexpected error.
    Internal { message: String },

    /// A wait operation timed out before the target state was reached.
    WaitTimeout {
        agent_id: String,
        target_state: String,
        last_observed_state: String,
    },

    /// A wait operation was cancelled via cancellation token.
    WaitCancelled {
        agent_id: String,
        last_observed_state: String,
    },

    /// Requested agent mode does not match harness command capability.
    CapabilityMismatch {
        adapter: String,
        requested_mode: String,
        command_mode: String,
        hint: String,
    },
}

impl fmt::Display for AgentServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { agent_id } => write!(f, "agent {agent_id:?} not found"),
            Self::AlreadyExists { agent_id } => {
                write!(f, "agent {agent_id:?} already exists")
            }
            Self::InvalidArgument { message } => write!(f, "invalid argument: {message}"),
            Self::InvalidState {
                agent_id,
                current_state,
                operation,
            } => write!(
                f,
                "agent {agent_id:?} in state {current_state:?} does not support {operation:?}"
            ),
            Self::TransportUnavailable { message } => {
                write!(f, "forged daemon unavailable: {message}")
            }
            Self::Internal { message } => write!(f, "internal error: {message}"),
            Self::WaitTimeout {
                agent_id,
                target_state,
                last_observed_state,
            } => write!(
                f,
                "wait timeout for agent {agent_id:?}: target state {target_state:?}, last observed {last_observed_state:?}"
            ),
            Self::WaitCancelled {
                agent_id,
                last_observed_state,
            } => write!(
                f,
                "wait cancelled for agent {agent_id:?}: last observed state {last_observed_state:?}"
            ),
            Self::CapabilityMismatch {
                adapter,
                requested_mode,
                command_mode,
                hint,
            } => write!(
                f,
                "capability mismatch: adapter {adapter:?} requested {requested_mode:?} but command mode is {command_mode:?}; {hint}"
            ),
        }
    }
}

impl std::error::Error for AgentServiceError {}

impl AgentServiceError {
    /// Whether this error is retryable (transport failures, timeouts).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TransportUnavailable { .. }
                | Self::WaitTimeout { .. }
                | Self::WaitCancelled { .. }
        )
    }
}

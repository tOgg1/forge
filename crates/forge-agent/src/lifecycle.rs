//! Agent lifecycle transition and operation guardrails.

use crate::error::AgentServiceError;
use crate::types::AgentState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentOperation {
    SendMessage,
    Interrupt,
    Kill,
    Revive,
}

impl AgentOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SendMessage => "send_message",
            Self::Interrupt => "interrupt_agent",
            Self::Kill => "kill_agent",
            Self::Revive => "revive_agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub from: AgentState,
    pub to: AgentState,
}

impl TransitionError {
    pub fn to_service_error(self, agent_id: &str) -> AgentServiceError {
        AgentServiceError::InvalidState {
            agent_id: agent_id.to_string(),
            current_state: self.from.to_string(),
            operation: format!("transition_to_{}", self.to),
        }
    }
}

pub fn is_valid_transition(from: AgentState, to: AgentState) -> bool {
    if from == to {
        return true;
    }

    matches!(
        (from, to),
        (AgentState::Unspecified, AgentState::Starting)
            | (AgentState::Starting, AgentState::Running)
            | (AgentState::Starting, AgentState::Idle)
            | (AgentState::Starting, AgentState::WaitingApproval)
            | (AgentState::Starting, AgentState::Stopping)
            | (AgentState::Starting, AgentState::Stopped)
            | (AgentState::Starting, AgentState::Failed)
            | (AgentState::Running, AgentState::Idle)
            | (AgentState::Running, AgentState::WaitingApproval)
            | (AgentState::Running, AgentState::Paused)
            | (AgentState::Running, AgentState::Stopping)
            | (AgentState::Running, AgentState::Stopped)
            | (AgentState::Running, AgentState::Failed)
            | (AgentState::Idle, AgentState::Running)
            | (AgentState::Idle, AgentState::WaitingApproval)
            | (AgentState::Idle, AgentState::Paused)
            | (AgentState::Idle, AgentState::Stopping)
            | (AgentState::Idle, AgentState::Stopped)
            | (AgentState::Idle, AgentState::Failed)
            | (AgentState::WaitingApproval, AgentState::Running)
            | (AgentState::WaitingApproval, AgentState::Idle)
            | (AgentState::WaitingApproval, AgentState::Paused)
            | (AgentState::WaitingApproval, AgentState::Stopping)
            | (AgentState::WaitingApproval, AgentState::Stopped)
            | (AgentState::WaitingApproval, AgentState::Failed)
            | (AgentState::Paused, AgentState::Running)
            | (AgentState::Paused, AgentState::Idle)
            | (AgentState::Paused, AgentState::WaitingApproval)
            | (AgentState::Paused, AgentState::Stopping)
            | (AgentState::Paused, AgentState::Stopped)
            | (AgentState::Paused, AgentState::Failed)
            | (AgentState::Stopping, AgentState::Stopped)
            | (AgentState::Stopping, AgentState::Failed)
            // Revive path.
            | (AgentState::Stopped, AgentState::Starting)
            | (AgentState::Failed, AgentState::Starting)
            // Failed can also settle into stopped if supervisor cleanup runs.
            | (AgentState::Failed, AgentState::Stopped)
    )
}

pub fn validate_transition(from: AgentState, to: AgentState) -> Result<(), TransitionError> {
    if is_valid_transition(from, to) {
        Ok(())
    } else {
        Err(TransitionError { from, to })
    }
}

pub fn operation_allows_state(operation: AgentOperation, state: AgentState) -> bool {
    match operation {
        AgentOperation::SendMessage => matches!(
            state,
            AgentState::Starting
                | AgentState::Running
                | AgentState::Idle
                | AgentState::WaitingApproval
                | AgentState::Paused
        ),
        AgentOperation::Interrupt => {
            matches!(
                state,
                AgentState::Running | AgentState::WaitingApproval | AgentState::Paused
            )
        }
        AgentOperation::Kill => !matches!(state, AgentState::Unspecified),
        AgentOperation::Revive => matches!(state, AgentState::Stopped | AgentState::Failed),
    }
}

pub fn validate_operation_state(
    agent_id: &str,
    operation: AgentOperation,
    state: AgentState,
) -> Result<(), AgentServiceError> {
    if operation_allows_state(operation, state) {
        Ok(())
    } else {
        Err(AgentServiceError::InvalidState {
            agent_id: agent_id.to_string(),
            current_state: state.to_string(),
            operation: operation.as_str().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_nominal_loop_transitions() {
        let path = [
            (AgentState::Unspecified, AgentState::Starting),
            (AgentState::Starting, AgentState::Running),
            (AgentState::Running, AgentState::Idle),
            (AgentState::Idle, AgentState::WaitingApproval),
            (AgentState::WaitingApproval, AgentState::Idle),
            (AgentState::Idle, AgentState::Stopping),
            (AgentState::Stopping, AgentState::Stopped),
        ];
        for (from, to) in path {
            assert!(is_valid_transition(from, to), "expected {from}->{to} valid");
        }
    }

    #[test]
    fn rejects_ambiguous_backwards_transition() {
        let err = match validate_transition(AgentState::Idle, AgentState::Starting) {
            Ok(()) => panic!("expected invalid transition"),
            Err(err) => err,
        };
        assert_eq!(err.from, AgentState::Idle);
        assert_eq!(err.to, AgentState::Starting);
    }

    #[test]
    fn allows_revive_transitions_only_from_terminal_states() {
        assert!(is_valid_transition(
            AgentState::Stopped,
            AgentState::Starting
        ));
        assert!(is_valid_transition(
            AgentState::Failed,
            AgentState::Starting
        ));
        assert!(!is_valid_transition(AgentState::Idle, AgentState::Starting));
        assert!(!is_valid_transition(
            AgentState::Running,
            AgentState::Starting
        ));
    }

    #[test]
    fn send_requires_interactive_state() {
        for state in [
            AgentState::Starting,
            AgentState::Running,
            AgentState::Idle,
            AgentState::WaitingApproval,
            AgentState::Paused,
        ] {
            assert!(operation_allows_state(AgentOperation::SendMessage, state));
        }
        for state in [
            AgentState::Unspecified,
            AgentState::Stopping,
            AgentState::Stopped,
            AgentState::Failed,
        ] {
            assert!(!operation_allows_state(AgentOperation::SendMessage, state));
        }
    }

    #[test]
    fn kill_allows_terminal_states_but_rejects_unspecified() {
        assert!(validate_operation_state("a1", AgentOperation::Kill, AgentState::Running).is_ok());
        assert!(validate_operation_state("a1", AgentOperation::Kill, AgentState::Stopped).is_ok());
        let err =
            match validate_operation_state("a1", AgentOperation::Kill, AgentState::Unspecified) {
                Ok(()) => panic!("expected invalid kill state"),
                Err(err) => err,
            };
        match err {
            AgentServiceError::InvalidState {
                current_state,
                operation,
                ..
            } => {
                assert_eq!(current_state, "unspecified");
                assert_eq!(operation, "kill_agent");
            }
            other => panic!("expected InvalidState, got {other:?}"),
        }
    }

    #[test]
    fn revive_requires_terminal_state() {
        assert!(validate_operation_state("a1", AgentOperation::Revive, AgentState::Failed).is_ok());
        let err = match validate_operation_state("a1", AgentOperation::Revive, AgentState::Idle) {
            Ok(()) => panic!("expected invalid revive state"),
            Err(err) => err,
        };
        assert!(matches!(err, AgentServiceError::InvalidState { .. }));
    }
}

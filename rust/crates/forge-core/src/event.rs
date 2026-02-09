//! Event system types for the append-only log.
//!
//! Mirrors Go `internal/models/event.go` — event types, entity types,
//! and associated classification enums.

use std::fmt;

/// Classification of events in the append-only log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    NodeCreated,
    NodeUpdated,
    NodeDeleted,
    WorkspaceCreated,
    WorkspaceUpdated,
    WorkspaceDeleted,
    AgentStarted,
    AgentStopped,
    AgentStateChanged,
    MessageQueued,
    MessageDispatched,
    ApprovalRequested,
    ApprovalGranted,
    RateLimitHit,
    CooldownStarted,
    CooldownEnded,
    AccountRotated,
    AccountCooldown,
    Error,
    Warning,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NodeCreated => "node.created",
            Self::NodeUpdated => "node.updated",
            Self::NodeDeleted => "node.deleted",
            Self::WorkspaceCreated => "workspace.created",
            Self::WorkspaceUpdated => "workspace.updated",
            Self::WorkspaceDeleted => "workspace.deleted",
            Self::AgentStarted => "agent.started",
            Self::AgentStopped => "agent.stopped",
            Self::AgentStateChanged => "agent.state_changed",
            Self::MessageQueued => "message.queued",
            Self::MessageDispatched => "message.dispatched",
            Self::ApprovalRequested => "approval.requested",
            Self::ApprovalGranted => "approval.granted",
            Self::RateLimitHit => "rate_limit.hit",
            Self::CooldownStarted => "cooldown.started",
            Self::CooldownEnded => "cooldown.ended",
            Self::AccountRotated => "account.rotated",
            Self::AccountCooldown => "account.cooldown",
            Self::Error => "error",
            Self::Warning => "warning",
        };
        f.write_str(s)
    }
}

/// Entity type classification for events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Node,
    Workspace,
    Agent,
    Queue,
    Account,
    System,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Node => "node",
            Self::Workspace => "workspace",
            Self::Agent => "agent",
            Self::Queue => "queue",
            Self::Account => "account",
            Self::System => "system",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_display() {
        assert_eq!(EventType::NodeCreated.to_string(), "node.created");
        assert_eq!(
            EventType::AgentStateChanged.to_string(),
            "agent.state_changed"
        );
        assert_eq!(EventType::Error.to_string(), "error");
    }

    #[test]
    fn entity_type_display() {
        assert_eq!(EntityType::Node.to_string(), "node");
        assert_eq!(EntityType::System.to_string(), "system");
    }
}

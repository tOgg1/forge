//! Agent event recording for audit and debugging.
//!
//! Each service operation emits an event that can be stored for later
//! querying (audit trail, explain support, debugging).

use chrono::{DateTime, Utc};

/// The kind of agent operation that generated an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentEventKind {
    Spawn,
    SendMessage,
    WaitState,
    Interrupt,
    Kill,
    GetAgent,
    ListAgents,
}

impl std::fmt::Display for AgentEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Spawn => "spawn",
            Self::SendMessage => "send_message",
            Self::WaitState => "wait_state",
            Self::Interrupt => "interrupt",
            Self::Kill => "kill",
            Self::GetAgent => "get_agent",
            Self::ListAgents => "list_agents",
        };
        f.write_str(s)
    }
}

/// Outcome of an agent service operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEventOutcome {
    Success,
    Error(String),
}

impl std::fmt::Display for AgentEventOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => f.write_str("success"),
            Self::Error(msg) => write!(f, "error: {msg}"),
        }
    }
}

/// An event emitted by the agent service for each operation.
#[derive(Debug, Clone)]
pub struct AgentEvent {
    pub timestamp: DateTime<Utc>,
    pub agent_id: Option<String>,
    pub kind: AgentEventKind,
    pub outcome: AgentEventOutcome,
    pub detail: String,
}

impl AgentEvent {
    pub fn new(
        agent_id: Option<String>,
        kind: AgentEventKind,
        outcome: AgentEventOutcome,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            agent_id,
            kind,
            outcome,
            detail: detail.into(),
        }
    }
}

/// Trait for event sinks that receive agent events.
///
/// Implementations can store events in a database, log them, or broadcast
/// them to subscribers.
pub trait AgentEventSink: Send + Sync {
    fn record(&self, event: AgentEvent);
}

/// In-memory event sink for testing.
#[derive(Default)]
pub struct InMemoryEventSink {
    events: std::sync::Mutex<Vec<AgentEvent>>,
}

impl InMemoryEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<AgentEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn count(&self) -> usize {
        match self.events.lock() {
            Ok(guard) => guard.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }
}

impl AgentEventSink for InMemoryEventSink {
    fn record(&self, event: AgentEvent) {
        match self.events.lock() {
            Ok(mut guard) => guard.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
    }
}

/// No-op event sink that discards all events.
pub struct NullEventSink;

impl AgentEventSink for NullEventSink {
    fn record(&self, _event: AgentEvent) {}
}

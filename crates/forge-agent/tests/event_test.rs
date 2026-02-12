#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tests for the agent event recording system.

use forge_agent::event::{
    AgentEvent, AgentEventKind, AgentEventOutcome, AgentEventSink, InMemoryEventSink, NullEventSink,
};

#[test]
fn in_memory_sink_records_events() {
    let sink = InMemoryEventSink::new();
    assert_eq!(sink.count(), 0);

    sink.record(AgentEvent::new(
        Some("a1".into()),
        AgentEventKind::Spawn,
        AgentEventOutcome::Success,
        "spawned",
    ));

    assert_eq!(sink.count(), 1);

    let events = sink.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].agent_id, Some("a1".into()));
    assert_eq!(events[0].kind, AgentEventKind::Spawn);
}

#[test]
fn in_memory_sink_records_multiple_events() {
    let sink = InMemoryEventSink::new();

    for kind in &[
        AgentEventKind::Spawn,
        AgentEventKind::SendMessage,
        AgentEventKind::WaitState,
        AgentEventKind::Kill,
    ] {
        sink.record(AgentEvent::new(
            Some("a1".into()),
            *kind,
            AgentEventOutcome::Success,
            "test",
        ));
    }

    assert_eq!(sink.count(), 4);
}

#[test]
fn null_sink_discards_events() {
    let sink = NullEventSink;
    sink.record(AgentEvent::new(
        Some("a1".into()),
        AgentEventKind::Spawn,
        AgentEventOutcome::Success,
        "spawned",
    ));
    // No way to count—just verify no panic.
}

#[test]
fn event_kind_display() {
    assert_eq!(AgentEventKind::Spawn.to_string(), "spawn");
    assert_eq!(AgentEventKind::SendMessage.to_string(), "send_message");
    assert_eq!(AgentEventKind::WaitState.to_string(), "wait_state");
    assert_eq!(AgentEventKind::Interrupt.to_string(), "interrupt");
    assert_eq!(AgentEventKind::Kill.to_string(), "kill");
    assert_eq!(AgentEventKind::GetAgent.to_string(), "get_agent");
    assert_eq!(AgentEventKind::ListAgents.to_string(), "list_agents");
}

#[test]
fn event_outcome_display() {
    assert_eq!(AgentEventOutcome::Success.to_string(), "success");
    assert_eq!(
        AgentEventOutcome::Error("boom".into()).to_string(),
        "error: boom"
    );
}

#[test]
fn event_has_timestamp() {
    let event = AgentEvent::new(
        None,
        AgentEventKind::ListAgents,
        AgentEventOutcome::Success,
        "listed",
    );
    // Timestamp should be recent (within last second).
    let elapsed = chrono::Utc::now() - event.timestamp;
    assert!(elapsed.num_seconds() < 2);
}

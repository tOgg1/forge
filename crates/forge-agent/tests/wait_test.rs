#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tests for M10.7: Wait semantics and state synchronization.
//!
//! Covers:
//! - wait_for_state with simulated state streams (MockStateStream)
//! - Target state reached immediately
//! - Target state reached after transitions
//! - Timeout expiry
//! - Cancellation via CancellationToken
//! - Terminal state detection
//! - Multiple target states
//! - Stream exhaustion (agent gone)
//! - Error propagation from stream
//! - Delayed transitions (integration-style tests)
//! - PollingStateStream with MockAgentService

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use forge_agent::error::AgentServiceError;
use forge_agent::mock::{test_snapshot, MockAgentService};
use forge_agent::types::AgentState;
use forge_agent::wait::{wait_for_state, MockStateStream, PollingStateStream};

// ── Helper ──

fn snapshot_ok(
    id: &str,
    state: AgentState,
) -> Result<forge_agent::types::AgentSnapshot, AgentServiceError> {
    Ok(test_snapshot(id, state))
}

// ── wait_for_state: target state reached immediately ──

#[tokio::test]
async fn wait_returns_immediately_when_already_in_target_state() {
    let mut stream = MockStateStream::from_snapshots(vec![snapshot_ok("a1", AgentState::Idle)]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.id, "a1");
    assert_eq!(snapshot.state, AgentState::Idle);
}

// ── wait_for_state: target state reached after transitions ──

#[tokio::test]
async fn wait_returns_after_state_transitions_to_target() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Starting),
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::Idle),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::Idle);
}

// ── wait_for_state: multiple target states ──

#[tokio::test]
async fn wait_returns_when_any_target_state_is_reached() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Starting),
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::WaitingApproval),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle, AgentState::WaitingApproval],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::WaitingApproval);
}

// ── wait_for_state: timeout ──

#[tokio::test]
async fn wait_returns_timeout_when_deadline_expires() {
    // Stream with delays that exceed the timeout
    let mut stream = MockStateStream::with_delays(vec![
        (
            Duration::from_millis(10),
            snapshot_ok("a1", AgentState::Running),
        ),
        (
            Duration::from_millis(200),
            snapshot_ok("a1", AgentState::Running),
        ),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_millis(50),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::WaitTimeout {
            agent_id,
            target_state,
            last_observed_state,
        }) => {
            assert_eq!(agent_id, "a1");
            assert_eq!(target_state, "idle");
            assert_eq!(last_observed_state, "running");
        }
        other => panic!("expected WaitTimeout, got {other:?}"),
    }
}

// ── wait_for_state: cancellation ──

#[tokio::test]
async fn wait_returns_cancelled_when_token_is_triggered() {
    // Stream that would take a long time — but we cancel immediately
    let mut stream = MockStateStream::with_delays(vec![(
        Duration::from_secs(60),
        snapshot_ok("a1", AgentState::Running),
    )]);
    let cancel = CancellationToken::new();
    cancel.cancel(); // Cancel before the stream yields

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(300),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::WaitCancelled {
            agent_id,
            last_observed_state,
        }) => {
            assert_eq!(agent_id, "a1");
            assert_eq!(last_observed_state, "unspecified");
        }
        other => panic!("expected WaitCancelled, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_returns_cancelled_mid_wait() {
    let mut stream = MockStateStream::with_delays(vec![
        (
            Duration::from_millis(5),
            snapshot_ok("a1", AgentState::Running),
        ),
        (Duration::from_secs(60), snapshot_ok("a1", AgentState::Idle)),
    ]);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Cancel after a short delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel_clone.cancel();
    });

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(300),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::WaitCancelled {
            agent_id,
            last_observed_state,
        }) => {
            assert_eq!(agent_id, "a1");
            assert_eq!(last_observed_state, "running");
        }
        other => panic!("expected WaitCancelled, got {other:?}"),
    }
}

// ── wait_for_state: terminal state detection ──

#[tokio::test]
async fn wait_returns_invalid_state_when_agent_enters_terminal_state() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::Stopped),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::InvalidState {
            agent_id,
            current_state,
            operation,
        }) => {
            assert_eq!(agent_id, "a1");
            assert_eq!(current_state, "stopped");
            assert_eq!(operation, "wait_state");
        }
        other => panic!("expected InvalidState, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_returns_ok_when_terminal_state_is_target() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::Stopped),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Stopped, AgentState::Failed],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::Stopped);
}

#[tokio::test]
async fn wait_detects_failed_terminal_state() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Starting),
        snapshot_ok("a1", AgentState::Failed),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::InvalidState { current_state, .. }) => {
            assert_eq!(current_state, "failed");
        }
        other => panic!("expected InvalidState, got {other:?}"),
    }
}

// ── wait_for_state: stream exhaustion ──

#[tokio::test]
async fn wait_returns_not_found_when_stream_ends() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Running),
        // Stream ends without reaching target
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::NotFound { agent_id }) => {
            assert_eq!(agent_id, "a1");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_returns_not_found_when_stream_is_empty() {
    let mut stream = MockStateStream::from_snapshots(vec![]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::NotFound { agent_id }) => {
            assert_eq!(agent_id, "a1");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ── wait_for_state: error propagation ──

#[tokio::test]
async fn wait_propagates_stream_errors() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Running),
        Err(AgentServiceError::TransportUnavailable {
            message: "connection lost".into(),
        }),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::TransportUnavailable { message }) => {
            assert_eq!(message, "connection lost");
        }
        other => panic!("expected TransportUnavailable, got {other:?}"),
    }
}

// ── wait_for_state: waiting for approval ──

#[tokio::test]
async fn wait_for_approval_needed_state() {
    let mut stream = MockStateStream::from_snapshots(vec![
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::Running),
        snapshot_ok("a1", AgentState::WaitingApproval),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::WaitingApproval],
        Duration::from_secs(5),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::WaitingApproval);
}

// ── Integration-style: delayed transitions ──

#[tokio::test]
async fn delayed_transition_reaches_target_before_timeout() {
    let mut stream = MockStateStream::with_delays(vec![
        (
            Duration::from_millis(5),
            snapshot_ok("a1", AgentState::Starting),
        ),
        (
            Duration::from_millis(10),
            snapshot_ok("a1", AgentState::Running),
        ),
        (
            Duration::from_millis(10),
            snapshot_ok("a1", AgentState::Idle),
        ),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(2),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::Idle);
}

#[tokio::test]
async fn delayed_transition_times_out_before_target() {
    let mut stream = MockStateStream::with_delays(vec![
        (
            Duration::from_millis(5),
            snapshot_ok("a1", AgentState::Starting),
        ),
        (
            Duration::from_millis(100),
            snapshot_ok("a1", AgentState::Idle),
        ),
    ]);
    let cancel = CancellationToken::new();

    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_millis(30),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::WaitTimeout { .. }) => {}
        other => panic!("expected WaitTimeout, got {other:?}"),
    }
}

// ── PollingStateStream with MockAgentService ──

#[tokio::test]
async fn polling_stream_yields_snapshots_from_service() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Idle));
    let svc = Arc::new(svc);

    let mut stream =
        PollingStateStream::new(svc.clone(), "a1".to_string(), Duration::from_millis(10));

    let cancel = CancellationToken::new();
    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(2),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::Idle);
}

#[tokio::test]
async fn polling_stream_detects_delayed_state_transition() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Running));
    let svc = Arc::new(svc);

    // Transition agent state after a short delay
    let svc_clone = svc.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        svc_clone.set_agent_state("a1", AgentState::Idle);
    });

    let mut stream =
        PollingStateStream::new(svc.clone(), "a1".to_string(), Duration::from_millis(10));

    let cancel = CancellationToken::new();
    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_secs(2),
        cancel,
        &mut stream,
    )
    .await;

    let snapshot = result.unwrap();
    assert_eq!(snapshot.state, AgentState::Idle);
}

#[tokio::test]
async fn polling_stream_timeout_when_state_never_changes() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Running));
    let svc = Arc::new(svc);

    let mut stream =
        PollingStateStream::new(svc.clone(), "a1".to_string(), Duration::from_millis(10));

    let cancel = CancellationToken::new();
    let result = wait_for_state(
        "a1",
        &[AgentState::Idle],
        Duration::from_millis(50),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::WaitTimeout {
            agent_id,
            last_observed_state,
            ..
        }) => {
            assert_eq!(agent_id, "a1");
            assert_eq!(last_observed_state, "running");
        }
        other => panic!("expected WaitTimeout, got {other:?}"),
    }
}

#[tokio::test]
async fn polling_stream_not_found_for_missing_agent() {
    let svc = MockAgentService::new();
    let svc = Arc::new(svc);

    let mut stream = PollingStateStream::new(
        svc.clone(),
        "missing".to_string(),
        Duration::from_millis(10),
    );

    let cancel = CancellationToken::new();
    let result = wait_for_state(
        "missing",
        &[AgentState::Idle],
        Duration::from_secs(2),
        cancel,
        &mut stream,
    )
    .await;

    match result {
        Err(AgentServiceError::NotFound { agent_id }) => {
            assert_eq!(agent_id, "missing");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// ── Error type tests for new variants ──

#[test]
fn wait_cancelled_error_display() {
    let err = AgentServiceError::WaitCancelled {
        agent_id: "a1".into(),
        last_observed_state: "running".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("cancelled"));
    assert!(msg.contains("a1"));
    assert!(msg.contains("running"));
}

#[test]
fn wait_cancelled_is_retryable() {
    let err = AgentServiceError::WaitCancelled {
        agent_id: "a1".into(),
        last_observed_state: "running".into(),
    };
    assert!(err.is_retryable());
}

#[test]
fn capability_mismatch_error_display() {
    let err = AgentServiceError::CapabilityMismatch {
        adapter: "codex".into(),
        requested_mode: "continuous".into(),
        command_mode: "one-shot".into(),
        hint: "use interactive mode".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("codex"));
    assert!(msg.contains("continuous"));
    assert!(msg.contains("one-shot"));
}

#[test]
fn agent_state_from_str_round_trip() {
    let states = [
        AgentState::Unspecified,
        AgentState::Starting,
        AgentState::Running,
        AgentState::Idle,
        AgentState::WaitingApproval,
        AgentState::Paused,
        AgentState::Stopping,
        AgentState::Stopped,
        AgentState::Failed,
    ];
    for state in states {
        let s = state.to_string();
        let parsed =
            AgentState::from_str(&s).unwrap_or_else(|| panic!("failed to parse state string: {s}"));
        assert_eq!(parsed, state);
    }
}

#[test]
fn agent_state_from_str_invalid_returns_none() {
    assert!(AgentState::from_str("bogus").is_none());
    assert!(AgentState::from_str("").is_none());
    assert!(AgentState::from_str("IDLE").is_none());
}

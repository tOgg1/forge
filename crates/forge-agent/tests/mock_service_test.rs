#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Unit tests for the agent service using the mock implementation.
//!
//! These tests verify the `AgentService` trait contract and the mock
//! behavior without requiring a running forged daemon.

use std::collections::HashMap;
use std::time::Duration;

use forge_agent::error::AgentServiceError;
use forge_agent::mock::{test_snapshot, MockAgentService};
use forge_agent::service::AgentService;
use forge_agent::types::{
    AgentRequestMode, AgentState, KillAgentParams, ListAgentsFilter, SendMessageParams,
    SpawnAgentParams, WaitStateParams,
};

fn test_spawn_params(id: &str) -> SpawnAgentParams {
    SpawnAgentParams {
        agent_id: id.to_string(),
        workspace_id: "test-ws".to_string(),
        command: "claude".to_string(),
        args: vec!["--prompt".to_string(), "test prompt".to_string()],
        env: HashMap::new(),
        working_dir: "/tmp".to_string(),
        session_name: String::new(),
        adapter: "claude_code".to_string(),
        requested_mode: AgentRequestMode::Continuous,
        allow_oneshot_fallback: false,
    }
}

// ── Spawn tests ──

#[tokio::test]
async fn spawn_agent_returns_starting_state() {
    let svc = MockAgentService::new();
    let snapshot = svc.spawn_agent(test_spawn_params("a1")).await.unwrap();
    assert_eq!(snapshot.id, "a1");
    assert_eq!(snapshot.state, AgentState::Starting);
    assert_eq!(snapshot.workspace_id, "test-ws");
    assert_eq!(snapshot.command, "claude");
}

#[tokio::test]
async fn spawn_duplicate_agent_returns_already_exists() {
    let svc = MockAgentService::new();
    svc.spawn_agent(test_spawn_params("a1")).await.unwrap();

    let err = svc.spawn_agent(test_spawn_params("a1")).await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::AlreadyExists {
            agent_id: "a1".into()
        }
    );
}

#[tokio::test]
async fn spawn_error_is_returned_when_configured() {
    let svc = MockAgentService::new().with_spawn_error(AgentServiceError::TransportUnavailable {
        message: "daemon down".into(),
    });

    let err = svc.spawn_agent(test_spawn_params("a1")).await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::TransportUnavailable {
            message: "daemon down".into()
        }
    );
}

// ── SendMessage tests ──

#[tokio::test]
async fn send_message_succeeds_for_existing_agent() {
    let svc = MockAgentService::new();
    svc.spawn_agent(test_spawn_params("a1")).await.unwrap();

    let ok = svc
        .send_message(SendMessageParams {
            agent_id: "a1".into(),
            text: "do something".into(),
            send_enter: true,
            keys: vec![],
        })
        .await
        .unwrap();

    assert!(ok);
}

#[tokio::test]
async fn send_message_returns_not_found_for_missing_agent() {
    let svc = MockAgentService::new();

    let err = svc
        .send_message(SendMessageParams {
            agent_id: "missing".into(),
            text: "hello".into(),
            send_enter: true,
            keys: vec![],
        })
        .await
        .unwrap_err();

    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "missing".into()
        }
    );
}

#[tokio::test]
async fn send_message_error_is_returned_when_configured() {
    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_send_error(AgentServiceError::Internal {
            message: "pane error".into(),
        });

    let err = svc
        .send_message(SendMessageParams {
            agent_id: "a1".into(),
            text: "hello".into(),
            send_enter: true,
            keys: vec![],
        })
        .await
        .unwrap_err();

    match err {
        AgentServiceError::Internal { message } => assert_eq!(message, "pane error"),
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── WaitState tests ──

#[tokio::test]
async fn wait_state_returns_immediately_when_already_in_target() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Idle));

    let snapshot = svc
        .wait_state(WaitStateParams {
            agent_id: "a1".into(),
            target_states: vec![AgentState::Idle],
            timeout: Duration::from_millis(100),
            poll_interval: Duration::from_millis(10),
        })
        .await
        .unwrap();

    assert_eq!(snapshot.state, AgentState::Idle);
}

#[tokio::test]
async fn wait_state_returns_timeout_when_not_in_target() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Running));

    let err = svc
        .wait_state(WaitStateParams {
            agent_id: "a1".into(),
            target_states: vec![AgentState::Idle],
            timeout: Duration::from_millis(100),
            poll_interval: Duration::from_millis(10),
        })
        .await
        .unwrap_err();

    match err {
        AgentServiceError::WaitTimeout {
            agent_id,
            target_state,
            last_observed_state,
        } => {
            assert_eq!(agent_id, "a1");
            assert_eq!(target_state, "idle");
            assert_eq!(last_observed_state, "running");
        }
        other => panic!("expected WaitTimeout, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_state_returns_not_found_for_missing_agent() {
    let svc = MockAgentService::new();

    let err = svc
        .wait_state(WaitStateParams {
            agent_id: "missing".into(),
            target_states: vec![AgentState::Idle],
            timeout: Duration::from_millis(100),
            poll_interval: Duration::from_millis(10),
        })
        .await
        .unwrap_err();

    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "missing".into()
        }
    );
}

// ── Interrupt tests ──

#[tokio::test]
async fn interrupt_agent_succeeds_for_existing_agent() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Running));

    assert!(svc.interrupt_agent("a1").await.unwrap());
}

#[tokio::test]
async fn interrupt_agent_returns_not_found_for_missing_agent() {
    let svc = MockAgentService::new();

    let err = svc.interrupt_agent("missing").await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "missing".into()
        }
    );
}

// ── Kill tests ──

#[tokio::test]
async fn kill_agent_removes_from_registry() {
    let svc = MockAgentService::new();
    svc.spawn_agent(test_spawn_params("a1")).await.unwrap();

    let ok = svc
        .kill_agent(KillAgentParams {
            agent_id: "a1".into(),
            force: false,
            grace_period: None,
        })
        .await
        .unwrap();

    assert!(ok);

    // Verify agent is gone.
    let err = svc.get_agent("a1").await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "a1".into()
        }
    );
}

#[tokio::test]
async fn kill_missing_agent_returns_not_found() {
    let svc = MockAgentService::new();

    let err = svc
        .kill_agent(KillAgentParams {
            agent_id: "missing".into(),
            force: false,
            grace_period: None,
        })
        .await
        .unwrap_err();

    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "missing".into()
        }
    );
}

#[tokio::test]
async fn kill_error_is_returned_when_configured() {
    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_kill_error(AgentServiceError::Internal {
            message: "tmux error".into(),
        });

    let err = svc
        .kill_agent(KillAgentParams {
            agent_id: "a1".into(),
            force: true,
            grace_period: None,
        })
        .await
        .unwrap_err();

    match err {
        AgentServiceError::Internal { message } => assert_eq!(message, "tmux error"),
        other => panic!("expected Internal, got {other:?}"),
    }
}

// ── ListAgents tests ──

#[tokio::test]
async fn list_agents_returns_all_when_no_filter() {
    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_agent(test_snapshot("a2", AgentState::Idle))
        .with_agent(test_snapshot("a3", AgentState::Stopped));

    let agents = svc.list_agents(ListAgentsFilter::default()).await.unwrap();

    assert_eq!(agents.len(), 3);
}

#[tokio::test]
async fn list_agents_filters_by_state() {
    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_agent(test_snapshot("a2", AgentState::Idle))
        .with_agent(test_snapshot("a3", AgentState::Stopped));

    let agents = svc
        .list_agents(ListAgentsFilter {
            workspace_id: None,
            states: vec![AgentState::Running, AgentState::Idle],
        })
        .await
        .unwrap();

    assert_eq!(agents.len(), 2);
    assert!(agents.iter().all(|a| a.state != AgentState::Stopped));
}

#[tokio::test]
async fn list_agents_filters_by_workspace() {
    let mut a2 = test_snapshot("a2", AgentState::Running);
    a2.workspace_id = "other-ws".to_string();

    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_agent(a2);

    let agents = svc
        .list_agents(ListAgentsFilter {
            workspace_id: Some("test-ws".into()),
            states: vec![],
        })
        .await
        .unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "a1");
}

#[tokio::test]
async fn list_agents_empty_result_when_no_match() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Running));

    let agents = svc
        .list_agents(ListAgentsFilter {
            workspace_id: None,
            states: vec![AgentState::Failed],
        })
        .await
        .unwrap();

    assert!(agents.is_empty());
}

// ── GetAgent tests ──

#[tokio::test]
async fn get_agent_returns_snapshot() {
    let svc = MockAgentService::new().with_agent(test_snapshot("a1", AgentState::Idle));

    let snapshot = svc.get_agent("a1").await.unwrap();
    assert_eq!(snapshot.id, "a1");
    assert_eq!(snapshot.state, AgentState::Idle);
}

#[tokio::test]
async fn get_agent_returns_not_found() {
    let svc = MockAgentService::new();

    let err = svc.get_agent("missing").await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::NotFound {
            agent_id: "missing".into()
        }
    );
}

#[tokio::test]
async fn get_agent_error_is_returned_when_configured() {
    let svc = MockAgentService::new()
        .with_agent(test_snapshot("a1", AgentState::Running))
        .with_get_error(AgentServiceError::TransportUnavailable {
            message: "timeout".into(),
        });

    let err = svc.get_agent("a1").await.unwrap_err();
    assert_eq!(
        err,
        AgentServiceError::TransportUnavailable {
            message: "timeout".into()
        }
    );
}

// ── Call recording tests ──

#[tokio::test]
async fn calls_are_recorded() {
    let svc = MockAgentService::new();
    svc.spawn_agent(test_spawn_params("a1")).await.unwrap();
    svc.get_agent("a1").await.unwrap();
    svc.send_message(SendMessageParams {
        agent_id: "a1".into(),
        text: "hello".into(),
        send_enter: true,
        keys: vec![],
    })
    .await
    .unwrap();

    assert_eq!(svc.call_count(), 3);
}

// ── Concurrent operation tests ──

#[tokio::test]
async fn multiple_agents_managed_concurrently() {
    let svc = MockAgentService::new();

    // Spawn three agents.
    for i in 0..3 {
        let id = format!("agent-{i}");
        svc.spawn_agent(test_spawn_params(&id)).await.unwrap();
    }

    // List all.
    let agents = svc.list_agents(ListAgentsFilter::default()).await.unwrap();
    assert_eq!(agents.len(), 3);

    // Kill one.
    svc.kill_agent(KillAgentParams {
        agent_id: "agent-1".into(),
        force: false,
        grace_period: None,
    })
    .await
    .unwrap();

    // List again.
    let agents = svc.list_agents(ListAgentsFilter::default()).await.unwrap();
    assert_eq!(agents.len(), 2);
    assert!(agents.iter().all(|a| a.id != "agent-1"));
}

// ── Error type tests ──

#[test]
fn error_display_messages() {
    let not_found = AgentServiceError::NotFound {
        agent_id: "a1".into(),
    };
    assert!(not_found.to_string().contains("a1"));
    assert!(not_found.to_string().contains("not found"));

    let timeout = AgentServiceError::WaitTimeout {
        agent_id: "a2".into(),
        target_state: "idle".into(),
        last_observed_state: "running".into(),
    };
    assert!(timeout.to_string().contains("timeout"));
    assert!(timeout.to_string().contains("idle"));
    assert!(timeout.to_string().contains("running"));
}

#[test]
fn error_retryable_classification() {
    assert!(AgentServiceError::TransportUnavailable { message: "".into() }.is_retryable());
    assert!(AgentServiceError::WaitTimeout {
        agent_id: "".into(),
        target_state: "".into(),
        last_observed_state: "".into(),
    }
    .is_retryable());
    assert!(!AgentServiceError::NotFound {
        agent_id: "".into()
    }
    .is_retryable());
    assert!(!AgentServiceError::AlreadyExists {
        agent_id: "".into()
    }
    .is_retryable());
    assert!(!AgentServiceError::CapabilityMismatch {
        adapter: "codex".into(),
        requested_mode: "continuous".into(),
        command_mode: "one-shot".into(),
        hint: "use interactive codex command".into(),
    }
    .is_retryable());
}

// ── AgentState type tests ──

#[test]
fn agent_state_terminal_classification() {
    assert!(AgentState::Stopped.is_terminal());
    assert!(AgentState::Failed.is_terminal());
    assert!(!AgentState::Running.is_terminal());
    assert!(!AgentState::Idle.is_terminal());
    assert!(!AgentState::Starting.is_terminal());
    assert!(!AgentState::WaitingApproval.is_terminal());
}

#[test]
fn agent_state_active_classification() {
    assert!(AgentState::Running.is_active());
    assert!(AgentState::Idle.is_active());
    assert!(AgentState::Starting.is_active());
    assert!(!AgentState::Stopped.is_active());
    assert!(!AgentState::Failed.is_active());
    assert!(!AgentState::Unspecified.is_active());
}

#[test]
fn agent_state_display() {
    assert_eq!(AgentState::Running.to_string(), "running");
    assert_eq!(AgentState::WaitingApproval.to_string(), "waiting_approval");
    assert_eq!(AgentState::Stopped.to_string(), "stopped");
}

#[tokio::test]
async fn spawn_rejects_oneshot_command_for_continuous_mode() {
    let svc = MockAgentService::new();
    let mut params = test_spawn_params("a1");
    params.adapter = "codex".to_string();
    params.command = "codex exec".to_string();

    let err = svc.spawn_agent(params).await.unwrap_err();
    match err {
        AgentServiceError::CapabilityMismatch {
            adapter,
            requested_mode,
            command_mode,
            hint,
        } => {
            assert_eq!(adapter, "codex");
            assert_eq!(requested_mode, "continuous");
            assert_eq!(command_mode, "one-shot");
            assert!(hint.contains("interactive codex command"));
        }
        other => panic!("expected CapabilityMismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn spawn_allows_oneshot_command_with_explicit_override() {
    let svc = MockAgentService::new();
    let mut params = test_spawn_params("a1");
    params.adapter = "codex".to_string();
    params.command = "codex exec".to_string();
    params
        .env
        .insert("FORGE_AGENT_ALLOW_ONESHOT".to_string(), "1".to_string());

    let snapshot = svc.spawn_agent(params).await.unwrap();
    assert_eq!(snapshot.id, "a1");
}

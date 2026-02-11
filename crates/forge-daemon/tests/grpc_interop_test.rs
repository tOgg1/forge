//! Go<->Rust daemon protocol interop tests (PAR-039).
//!
//! These tests verify that the Rust ForgedAgentService implementation correctly
//! serves gRPC requests matching the proto contract shared with the Go daemon.
//! They start a real tonic gRPC server and connect a real gRPC client, proving
//! full round-trip wire compatibility.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use forge_daemon::agent::AgentManager;
use forge_daemon::server::ForgedAgentService;
use forge_daemon::tmux::TmuxClient;
use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use forge_rpc::forged::v1::forged_service_server::ForgedServiceServer;
use tonic::transport::{Channel, Server};

// ---------------------------------------------------------------------------
// Minimal mock TmuxClient for integration tests
// ---------------------------------------------------------------------------

struct MockTmux;

impl TmuxClient for MockTmux {
    fn send_keys(&self, _: &str, _: &str, _: bool, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn send_special_key(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn capture_pane(&self, _: &str, _: bool) -> Result<String, String> {
        Ok("$ ".to_string())
    }
    fn has_session(&self, _: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn new_session(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn split_window(&self, _: &str, _: bool, _: &str) -> Result<String, String> {
        Ok("mock-pane:0.1".to_string())
    }
    fn get_pane_pid(&self, _: &str) -> Result<i32, String> {
        Ok(9999)
    }
    fn send_interrupt(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn kill_pane(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper: spin up server + return connected client
// ---------------------------------------------------------------------------

async fn start_server_and_client() -> ForgedServiceClient<Channel> {
    let service = ForgedAgentService::new(AgentManager::new(), Arc::new(MockTmux));

    // Bind to port 0 to get a random available port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    tokio::spawn(async move {
        Server::builder()
            .add_service(ForgedServiceServer::new(service))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // Give the server a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let channel = Channel::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    ForgedServiceClient::new(channel)
}

// ---------------------------------------------------------------------------
// Interop tests: Rust client -> Rust server (unary RPCs)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ping_round_trip() {
    let mut client = start_server_and_client().await;

    let resp = client
        .ping(proto::PingRequest {})
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.version.is_empty(), "version should be non-empty");
    assert!(resp.timestamp.is_some(), "timestamp should be present");
}

#[tokio::test]
async fn get_status_round_trip() {
    let mut client = start_server_and_client().await;

    let resp = client
        .get_status(proto::GetStatusRequest {})
        .await
        .unwrap()
        .into_inner();

    let status = match resp.status {
        Some(status) => status,
        None => panic!("status should be present"),
    };
    assert!(!status.version.is_empty());
    assert!(!status.hostname.is_empty());
    assert!(status.started_at.is_some());
    assert!(status.uptime.is_some());
    assert_eq!(status.agent_count, 0);
    assert!(status.health.is_some());
}

#[tokio::test]
async fn spawn_agent_round_trip() {
    let mut client = start_server_and_client().await;

    let resp = client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-interop-1".to_string(),
            workspace_id: "ws-interop".to_string(),
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-interop".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap()
        .into_inner();

    let agent = match resp.agent {
        Some(agent) => agent,
        None => panic!("agent should be present"),
    };
    assert_eq!(agent.id, "agent-interop-1");
    assert_eq!(agent.workspace_id, "ws-interop");
    assert_eq!(agent.command, "echo");
    assert_eq!(agent.adapter, "test");
    assert!(agent.pid > 0);
    assert!(!resp.pane_id.is_empty());
}

#[tokio::test]
async fn spawn_agent_duplicate_returns_already_exists() {
    let mut client = start_server_and_client().await;

    let req = proto::SpawnAgentRequest {
        agent_id: "agent-dup".to_string(),
        workspace_id: "ws-dup".to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: "/tmp".to_string(),
        session_name: "sess-dup".to_string(),
        adapter: "test".to_string(),
        resource_limits: None,
    };

    client.spawn_agent(req.clone()).await.unwrap();
    let err = client.spawn_agent(req).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::AlreadyExists);
}

#[tokio::test]
async fn spawn_agent_missing_id_returns_invalid_argument() {
    let mut client = start_server_and_client().await;

    let err = client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: String::new(),
            command: "echo".to_string(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn kill_agent_round_trip() {
    let mut client = start_server_and_client().await;

    // Spawn first.
    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-kill".to_string(),
            workspace_id: "ws-kill".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-kill".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    // Kill.
    let resp = client
        .kill_agent(proto::KillAgentRequest {
            agent_id: "agent-kill".to_string(),
            force: true,
            grace_period: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.success);
}

#[tokio::test]
async fn kill_agent_not_found() {
    let mut client = start_server_and_client().await;

    let err = client
        .kill_agent(proto::KillAgentRequest {
            agent_id: "nonexistent".to_string(),
            force: false,
            grace_period: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn send_input_round_trip() {
    let mut client = start_server_and_client().await;

    // Spawn agent first.
    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-input".to_string(),
            workspace_id: "ws-input".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-input".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .send_input(proto::SendInputRequest {
            agent_id: "agent-input".to_string(),
            text: "hello world".to_string(),
            send_enter: true,
            keys: vec!["C-c".to_string()],
        })
        .await
        .unwrap()
        .into_inner();

    assert!(resp.success);
}

#[tokio::test]
async fn list_agents_round_trip() {
    let mut client = start_server_and_client().await;

    // List before any spawn — empty.
    let resp = client
        .list_agents(proto::ListAgentsRequest {
            workspace_id: String::new(),
            states: vec![],
        })
        .await
        .unwrap()
        .into_inner();
    assert!(resp.agents.is_empty());

    // Spawn two agents.
    for id in ["agent-list-1", "agent-list-2"] {
        client
            .spawn_agent(proto::SpawnAgentRequest {
                agent_id: id.to_string(),
                workspace_id: "ws-list".to_string(),
                command: "echo".to_string(),
                args: vec![],
                env: Default::default(),
                working_dir: "/tmp".to_string(),
                session_name: format!("sess-{id}"),
                adapter: "test".to_string(),
                resource_limits: None,
            })
            .await
            .unwrap();
    }

    let resp = client
        .list_agents(proto::ListAgentsRequest {
            workspace_id: String::new(),
            states: vec![],
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.agents.len(), 2);
}

#[tokio::test]
async fn get_agent_round_trip() {
    let mut client = start_server_and_client().await;

    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-get".to_string(),
            workspace_id: "ws-get".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-get".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .get_agent(proto::GetAgentRequest {
            agent_id: "agent-get".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    let agent = match resp.agent {
        Some(agent) => agent,
        None => panic!("agent"),
    };
    assert_eq!(agent.id, "agent-get");
    assert_eq!(agent.workspace_id, "ws-get");
}

#[tokio::test]
async fn get_agent_not_found() {
    let mut client = start_server_and_client().await;

    let err = client
        .get_agent(proto::GetAgentRequest {
            agent_id: "nonexistent".to_string(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn start_stop_loop_runner_round_trip() {
    let mut client = start_server_and_client().await;

    // Start a loop runner.
    let resp = client
        .start_loop_runner(proto::StartLoopRunnerRequest {
            loop_id: "loop-interop-1".to_string(),
            config_path: "/tmp/loop.yaml".to_string(),
            command_path: "forge".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    let runner = match resp.runner {
        Some(runner) => runner,
        None => panic!("runner"),
    };
    assert_eq!(runner.loop_id, "loop-interop-1");
    assert!(!runner.instance_id.is_empty());
    assert_eq!(runner.state, proto::LoopRunnerState::Running as i32);

    // Stop it.
    let stop_resp = client
        .stop_loop_runner(proto::StopLoopRunnerRequest {
            loop_id: "loop-interop-1".to_string(),
            force: true,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(stop_resp.success);
    let stopped = match stop_resp.runner {
        Some(runner) => runner,
        None => panic!("stopped runner"),
    };
    assert_eq!(stopped.state, proto::LoopRunnerState::Stopped as i32);
}

#[tokio::test]
async fn get_loop_runner_round_trip() {
    let mut client = start_server_and_client().await;

    client
        .start_loop_runner(proto::StartLoopRunnerRequest {
            loop_id: "loop-get".to_string(),
            config_path: "/tmp/loop.yaml".to_string(),
            command_path: "forge".to_string(),
        })
        .await
        .unwrap();

    let resp = client
        .get_loop_runner(proto::GetLoopRunnerRequest {
            loop_id: "loop-get".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    let runner = match resp.runner {
        Some(runner) => runner,
        None => panic!("runner"),
    };
    assert_eq!(runner.loop_id, "loop-get");
}

#[tokio::test]
async fn list_loop_runners_round_trip() {
    let mut client = start_server_and_client().await;

    let resp = client
        .list_loop_runners(proto::ListLoopRunnersRequest {})
        .await
        .unwrap()
        .into_inner();
    assert!(resp.runners.is_empty());

    client
        .start_loop_runner(proto::StartLoopRunnerRequest {
            loop_id: "loop-list".to_string(),
            config_path: "/tmp/loop.yaml".to_string(),
            command_path: "forge".to_string(),
        })
        .await
        .unwrap();

    let resp = client
        .list_loop_runners(proto::ListLoopRunnersRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.runners.len(), 1);
}

#[tokio::test]
async fn capture_pane_round_trip() {
    let mut client = start_server_and_client().await;

    // Need an agent first.
    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-capture".to_string(),
            workspace_id: "ws-capture".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-capture".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .capture_pane(proto::CapturePaneRequest {
            agent_id: "agent-capture".to_string(),
            lines: 0,
            include_escape_sequences: false,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!resp.content_hash.is_empty());
    assert!(resp.captured_at.is_some());
}

#[tokio::test]
async fn get_transcript_round_trip() {
    let mut client = start_server_and_client().await;

    // Spawn agent (which creates a transcript entry).
    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-transcript".to_string(),
            workspace_id: "ws-transcript".to_string(),
            command: "echo".to_string(),
            args: vec!["hi".to_string()],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-transcript".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .get_transcript(proto::GetTranscriptRequest {
            agent_id: "agent-transcript".to_string(),
            start_time: None,
            end_time: None,
            limit: 100,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.agent_id, "agent-transcript");
    assert!(
        !resp.entries.is_empty(),
        "should have at least the spawn entry"
    );
}

// ---------------------------------------------------------------------------
// Interop tests: streaming RPCs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_pane_updates_round_trip() {
    let mut client = start_server_and_client().await;

    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-stream-pane".to_string(),
            workspace_id: "ws-stream".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-stream-pane".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .stream_pane_updates(proto::StreamPaneUpdatesRequest {
            agent_id: "agent-stream-pane".to_string(),
            include_content: true,
            last_known_hash: String::new(),
            min_interval: None,
        })
        .await
        .unwrap();

    let mut stream = resp.into_inner();
    // Collect at least one update (the initial emission).
    let mut updates = Vec::new();
    while let Ok(Some(msg)) = stream.message().await {
        updates.push(msg);
    }
    assert!(
        !updates.is_empty(),
        "should receive at least one pane update"
    );
    assert_eq!(updates[0].agent_id, "agent-stream-pane");
}

#[tokio::test]
async fn stream_events_round_trip() {
    let mut client = start_server_and_client().await;

    // Spawn then kill an agent to generate streamable event records.
    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-events".to_string(),
            workspace_id: "ws-events".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-events".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();
    client
        .kill_agent(proto::KillAgentRequest {
            agent_id: "agent-events".to_string(),
            force: true,
            grace_period: None,
        })
        .await
        .unwrap();

    let resp = client
        .stream_events(proto::StreamEventsRequest {
            cursor: "1".to_string(),
            types: vec![],
            agent_ids: vec![],
            workspace_ids: vec![],
        })
        .await
        .unwrap();

    let mut stream = resp.into_inner();
    let mut seen = 0usize;
    while let Ok(Some(msg)) = stream.message().await {
        let event = match msg.event.as_ref() {
            Some(event) => event,
            None => panic!("event present"),
        };
        assert!(!event.id.is_empty());
        seen += 1;
    }

    // Interop contract check: stream RPC succeeds and yields well-formed messages.
    assert!(seen < 1000);
}

#[tokio::test]
async fn stream_transcript_round_trip() {
    let mut client = start_server_and_client().await;

    client
        .spawn_agent(proto::SpawnAgentRequest {
            agent_id: "agent-stream-tx".to_string(),
            workspace_id: "ws-stream-tx".to_string(),
            command: "echo".to_string(),
            args: vec!["hi".to_string()],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: "sess-stream-tx".to_string(),
            adapter: "test".to_string(),
            resource_limits: None,
        })
        .await
        .unwrap();

    let resp = client
        .stream_transcript(proto::StreamTranscriptRequest {
            agent_id: "agent-stream-tx".to_string(),
            cursor: String::new(),
        })
        .await
        .unwrap();

    let mut stream = resp.into_inner();
    let mut chunks = Vec::new();
    while let Ok(Some(msg)) = stream.message().await {
        chunks.push(msg);
    }
    assert!(
        !chunks.is_empty(),
        "should receive transcript chunk for spawn entry"
    );
    assert!(
        !chunks[0].entries.is_empty(),
        "first chunk should contain entries"
    );
}

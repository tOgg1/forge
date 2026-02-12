//! Forged daemon transport — implements `AgentService` via gRPC calls to forged.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use tonic::transport::Endpoint;

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;

use crate::capability::validate_spawn_guardrails;
use crate::error::AgentServiceError;
use crate::event::{AgentEvent, AgentEventKind, AgentEventOutcome, AgentEventSink};
use crate::service::AgentService;
use crate::types::{
    AgentSnapshot, AgentState, KillAgentParams, ListAgentsFilter, SendMessageParams,
    SpawnAgentParams, WaitStateParams,
};

/// Configuration for the forged transport.
#[derive(Debug, Clone)]
pub struct ForgedTransportConfig {
    pub target: String,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for ForgedTransportConfig {
    fn default() -> Self {
        Self {
            target: default_daemon_target(),
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// Agent service implementation backed by the forged daemon gRPC transport.
pub struct ForgedTransport {
    config: ForgedTransportConfig,
    event_sink: Arc<dyn AgentEventSink>,
}

impl ForgedTransport {
    pub fn new(config: ForgedTransportConfig, event_sink: Arc<dyn AgentEventSink>) -> Self {
        Self { config, event_sink }
    }

    async fn connect(
        &self,
    ) -> Result<ForgedServiceClient<tonic::transport::Channel>, AgentServiceError> {
        let endpoint = Endpoint::from_shared(self.config.target.clone())
            .map_err(|e| AgentServiceError::TransportUnavailable {
                message: format!("invalid target: {e}"),
            })?
            .connect_timeout(self.config.connect_timeout)
            .timeout(self.config.request_timeout);

        let channel =
            endpoint
                .connect()
                .await
                .map_err(|e| AgentServiceError::TransportUnavailable {
                    message: e.to_string(),
                })?;

        Ok(ForgedServiceClient::new(channel))
    }

    fn emit_event(
        &self,
        agent_id: Option<String>,
        kind: AgentEventKind,
        outcome: AgentEventOutcome,
        detail: impl Into<String>,
    ) {
        self.event_sink
            .record(AgentEvent::new(agent_id, kind, outcome, detail));
    }
}

#[async_trait]
impl AgentService for ForgedTransport {
    async fn spawn_agent(
        &self,
        params: SpawnAgentParams,
    ) -> Result<AgentSnapshot, AgentServiceError> {
        if params.agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }
        if params.command.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "command is required".into(),
            });
        }
        let capability = validate_spawn_guardrails(&params)?;

        let mut client = self.connect().await?;

        let request = proto::SpawnAgentRequest {
            agent_id: params.agent_id.clone(),
            workspace_id: params.workspace_id.clone(),
            command: params.command.clone(),
            args: params.args.clone(),
            env: params.env.clone(),
            working_dir: params.working_dir.clone(),
            session_name: params.session_name.clone(),
            adapter: params.adapter.clone(),
            resource_limits: None,
        };

        let response = client
            .spawn_agent(request)
            .await
            .map_err(|s| map_tonic_status(s, &params.agent_id))?
            .into_inner();

        let agent = response.agent.ok_or_else(|| AgentServiceError::Internal {
            message: "daemon returned empty agent in spawn response".into(),
        })?;

        let snapshot = proto_agent_to_snapshot(&agent);

        self.emit_event(
            Some(params.agent_id),
            AgentEventKind::Spawn,
            AgentEventOutcome::Success,
            format!(
                "spawned with command {:?} ({})",
                params.command,
                capability.detail_line()
            ),
        );

        Ok(snapshot)
    }

    async fn send_message(&self, params: SendMessageParams) -> Result<bool, AgentServiceError> {
        if params.agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }

        let mut client = self.connect().await?;

        let request = proto::SendInputRequest {
            agent_id: params.agent_id.clone(),
            text: params.text.clone(),
            send_enter: params.send_enter,
            keys: params.keys.clone(),
        };

        let response = client
            .send_input(request)
            .await
            .map_err(|s| map_tonic_status(s, &params.agent_id))?
            .into_inner();

        self.emit_event(
            Some(params.agent_id),
            AgentEventKind::SendMessage,
            AgentEventOutcome::Success,
            format!("sent {} bytes", params.text.len()),
        );

        Ok(response.success)
    }

    async fn wait_state(
        &self,
        params: WaitStateParams,
    ) -> Result<AgentSnapshot, AgentServiceError> {
        if params.agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }
        if params.target_states.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "at least one target state is required".into(),
            });
        }

        let deadline = tokio::time::Instant::now() + params.timeout;

        loop {
            let snapshot = self.get_agent(&params.agent_id).await?;
            let last_state = snapshot.state;

            if params.target_states.contains(&snapshot.state) {
                self.emit_event(
                    Some(params.agent_id),
                    AgentEventKind::WaitState,
                    AgentEventOutcome::Success,
                    format!("reached target state {}", snapshot.state),
                );
                return Ok(snapshot);
            }

            // If agent entered a terminal state that is not our target, stop waiting.
            if snapshot.state.is_terminal() {
                self.emit_event(
                    Some(params.agent_id.clone()),
                    AgentEventKind::WaitState,
                    AgentEventOutcome::Error(format!(
                        "agent entered terminal state {}",
                        snapshot.state
                    )),
                    format!("terminal state {} is not a target", snapshot.state),
                );
                return Err(AgentServiceError::InvalidState {
                    agent_id: params.agent_id,
                    current_state: snapshot.state.to_string(),
                    operation: "wait_state".into(),
                });
            }

            if tokio::time::Instant::now() >= deadline {
                let target_str = params
                    .target_states
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("|");

                self.emit_event(
                    Some(params.agent_id.clone()),
                    AgentEventKind::WaitState,
                    AgentEventOutcome::Error("timeout".into()),
                    format!("timed out waiting for {target_str}"),
                );

                return Err(AgentServiceError::WaitTimeout {
                    agent_id: params.agent_id,
                    target_state: target_str,
                    last_observed_state: last_state.to_string(),
                });
            }

            tokio::time::sleep(params.poll_interval).await;
        }
    }

    async fn interrupt_agent(&self, agent_id: &str) -> Result<bool, AgentServiceError> {
        if agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }

        let mut client = self.connect().await?;

        // Send Ctrl+C via SendInput with special key.
        let request = proto::SendInputRequest {
            agent_id: agent_id.to_string(),
            text: String::new(),
            send_enter: false,
            keys: vec!["C-c".to_string()],
        };

        let response = client
            .send_input(request)
            .await
            .map_err(|s| map_tonic_status(s, agent_id))?
            .into_inner();

        self.emit_event(
            Some(agent_id.to_string()),
            AgentEventKind::Interrupt,
            AgentEventOutcome::Success,
            "sent Ctrl+C",
        );

        Ok(response.success)
    }

    async fn kill_agent(&self, params: KillAgentParams) -> Result<bool, AgentServiceError> {
        if params.agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }

        let mut client = self.connect().await?;

        let request = proto::KillAgentRequest {
            agent_id: params.agent_id.clone(),
            force: params.force,
            grace_period: params.grace_period.map(|d| prost_types::Duration {
                seconds: d.as_secs() as i64,
                nanos: d.subsec_nanos() as i32,
            }),
        };

        let response = client
            .kill_agent(request)
            .await
            .map_err(|s| map_tonic_status(s, &params.agent_id))?
            .into_inner();

        self.emit_event(
            Some(params.agent_id),
            AgentEventKind::Kill,
            AgentEventOutcome::Success,
            format!("killed (force={})", params.force),
        );

        Ok(response.success)
    }

    async fn list_agents(
        &self,
        filter: ListAgentsFilter,
    ) -> Result<Vec<AgentSnapshot>, AgentServiceError> {
        let mut client = self.connect().await?;

        let request = proto::ListAgentsRequest {
            workspace_id: filter.workspace_id.clone().unwrap_or_default(),
            states: filter.states.iter().map(|s| s.to_proto_i32()).collect(),
        };

        let response = client
            .list_agents(request)
            .await
            .map_err(|s| map_tonic_status(s, ""))?
            .into_inner();

        let snapshots: Vec<AgentSnapshot> = response
            .agents
            .iter()
            .map(proto_agent_to_snapshot)
            .collect();

        self.emit_event(
            None,
            AgentEventKind::ListAgents,
            AgentEventOutcome::Success,
            format!("returned {} agents", snapshots.len()),
        );

        Ok(snapshots)
    }

    async fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, AgentServiceError> {
        if agent_id.is_empty() {
            return Err(AgentServiceError::InvalidArgument {
                message: "agent_id is required".into(),
            });
        }

        let mut client = self.connect().await?;

        let request = proto::GetAgentRequest {
            agent_id: agent_id.to_string(),
        };

        let response = client
            .get_agent(request)
            .await
            .map_err(|s| map_tonic_status(s, agent_id))?
            .into_inner();

        let agent = response.agent.ok_or_else(|| AgentServiceError::Internal {
            message: "daemon returned empty agent in get response".into(),
        })?;

        Ok(proto_agent_to_snapshot(&agent))
    }
}

// -- helpers --

fn default_daemon_target() -> String {
    std::env::var("FORGE_DAEMON_TARGET")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_target(&s))
        .unwrap_or_else(|| "http://127.0.0.1:50051".to_string())
}

fn normalize_target(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn map_tonic_status(status: tonic::Status, agent_id: &str) -> AgentServiceError {
    match status.code() {
        tonic::Code::NotFound => AgentServiceError::NotFound {
            agent_id: agent_id.to_string(),
        },
        tonic::Code::AlreadyExists => AgentServiceError::AlreadyExists {
            agent_id: agent_id.to_string(),
        },
        tonic::Code::InvalidArgument => AgentServiceError::InvalidArgument {
            message: status.message().to_string(),
        },
        tonic::Code::Unavailable => AgentServiceError::TransportUnavailable {
            message: status.message().to_string(),
        },
        _ => AgentServiceError::Internal {
            message: format!("{}: {}", status.code(), status.message()),
        },
    }
}

fn proto_agent_to_snapshot(agent: &proto::Agent) -> AgentSnapshot {
    AgentSnapshot {
        id: agent.id.clone(),
        workspace_id: agent.workspace_id.clone(),
        state: AgentState::from_proto_i32(agent.state),
        pane_id: agent.pane_id.clone(),
        pid: agent.pid,
        command: agent.command.clone(),
        adapter: agent.adapter.clone(),
        spawned_at: proto_timestamp_to_chrono(agent.spawned_at.as_ref()),
        last_activity_at: proto_timestamp_to_chrono(agent.last_activity_at.as_ref()),
    }
}

fn proto_timestamp_to_chrono(ts: Option<&prost_types::Timestamp>) -> DateTime<Utc> {
    match ts {
        Some(ts) => Utc
            .timestamp_opt(ts.seconds, ts.nanos as u32)
            .single()
            .unwrap_or_else(Utc::now),
        None => Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_target_adds_scheme() {
        assert_eq!(
            normalize_target("127.0.0.1:50051"),
            "http://127.0.0.1:50051"
        );
    }

    #[test]
    fn normalize_target_preserves_scheme() {
        assert_eq!(
            normalize_target("https://daemon.local:9999"),
            "https://daemon.local:9999"
        );
    }

    #[test]
    fn normalize_target_trims_whitespace() {
        assert_eq!(
            normalize_target("  http://localhost:50051  "),
            "http://localhost:50051"
        );
    }

    #[test]
    fn map_tonic_not_found() {
        let status = tonic::Status::not_found("agent-1 not found");
        let err = map_tonic_status(status, "agent-1");
        assert_eq!(
            err,
            AgentServiceError::NotFound {
                agent_id: "agent-1".into()
            }
        );
    }

    #[test]
    fn map_tonic_already_exists() {
        let status = tonic::Status::already_exists("duplicate");
        let err = map_tonic_status(status, "agent-1");
        assert_eq!(
            err,
            AgentServiceError::AlreadyExists {
                agent_id: "agent-1".into()
            }
        );
    }

    #[test]
    fn map_tonic_unavailable() {
        let status = tonic::Status::unavailable("connection refused");
        let err = map_tonic_status(status, "");
        assert_eq!(
            err,
            AgentServiceError::TransportUnavailable {
                message: "connection refused".into()
            }
        );
    }

    #[test]
    fn map_tonic_invalid_argument() {
        let status = tonic::Status::invalid_argument("missing field");
        let err = map_tonic_status(status, "");
        assert_eq!(
            err,
            AgentServiceError::InvalidArgument {
                message: "missing field".into()
            }
        );
    }

    #[test]
    fn map_tonic_internal() {
        let status = tonic::Status::internal("unexpected");
        let err = map_tonic_status(status, "agent-1");
        match err {
            AgentServiceError::Internal { message } => {
                assert!(message.contains("unexpected"));
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn proto_agent_snapshot_conversion() {
        let agent = proto::Agent {
            id: "a1".into(),
            workspace_id: "ws1".into(),
            state: 3, // Idle
            pane_id: "a1:0.0".into(),
            pid: 1234,
            command: "claude".into(),
            adapter: "claude_code".into(),
            spawned_at: Some(prost_types::Timestamp {
                seconds: 1700000000,
                nanos: 0,
            }),
            last_activity_at: Some(prost_types::Timestamp {
                seconds: 1700000100,
                nanos: 0,
            }),
            content_hash: String::new(),
            resource_limits: None,
            resource_usage: None,
        };

        let snapshot = proto_agent_to_snapshot(&agent);
        assert_eq!(snapshot.id, "a1");
        assert_eq!(snapshot.workspace_id, "ws1");
        assert_eq!(snapshot.state, AgentState::Idle);
        assert_eq!(snapshot.pid, 1234);
        assert_eq!(snapshot.command, "claude");
    }

    #[test]
    fn agent_state_round_trip() {
        for v in 0..=8 {
            let state = AgentState::from_proto_i32(v);
            assert_eq!(state.to_proto_i32(), v);
        }
    }
}

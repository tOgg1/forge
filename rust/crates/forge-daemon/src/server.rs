//! gRPC server implementation for ForgedService agent RPCs.
//!
//! Implements SendInput, ListAgents, GetAgent with parity to Go daemon
//! (`internal/forged/server.go`).

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};

use forge_rpc::forged::v1 as proto;

use crate::agent::{Agent, AgentManager, AgentState};
use crate::tmux::TmuxClient;
use crate::transcript::TranscriptEntryType;

/// Holds agent registry + tmux client for gRPC handlers.
pub struct ForgedAgentService {
    agents: AgentManager,
    tmux: Arc<dyn TmuxClient>,
}

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl ForgedAgentService {
    pub fn new(agents: AgentManager, tmux: Arc<dyn TmuxClient>) -> Self {
        Self { agents, tmux }
    }

    /// Access the agent manager (used by other service components).
    pub fn agents(&self) -> &AgentManager {
        &self.agents
    }

    // -- RPC handlers --

    /// SendInput sends keystrokes or text to an agent's pane.
    #[allow(clippy::result_large_err)]
    pub fn send_input(
        &self,
        req: Request<proto::SendInputRequest>,
    ) -> Result<Response<proto::SendInputResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        // Lookup agent (read path).
        let agent = self
            .agents
            .get(&req.agent_id)
            .ok_or_else(|| Status::not_found(format!("agent {:?} not found", req.agent_id)))?;

        // Send special keys first (interpreted, no -l flag).
        for key in &req.keys {
            self.tmux
                .send_special_key(&agent.pane_id, key)
                .map_err(|e| Status::internal(format!("failed to send key {key:?}: {e}")))?;
        }

        // Send text (literal mode, optionally followed by Enter).
        if !req.text.is_empty() {
            self.tmux
                .send_keys(&agent.pane_id, &req.text, true, req.send_enter)
                .map_err(|e| Status::internal(format!("failed to send text: {e}")))?;
        }

        // Update last_activity + record transcript.
        self.agents.touch(&req.agent_id);

        let mut input_content = String::new();
        if !req.keys.is_empty() {
            input_content.push_str(&format!("[keys: {:?}] ", req.keys));
        }
        input_content.push_str(&req.text);

        if !input_content.is_empty() {
            self.agents.add_transcript_entry(
                &req.agent_id,
                TranscriptEntryType::UserInput,
                &input_content,
            );
        }

        Ok(Response::new(proto::SendInputResponse { success: true }))
    }

    /// ListAgents returns all agents, optionally filtered by workspace and state.
    #[allow(clippy::result_large_err)]
    pub fn list_agents(
        &self,
        req: Request<proto::ListAgentsRequest>,
    ) -> Result<Response<proto::ListAgentsResponse>, Status> {
        let req = req.into_inner();

        let workspace_filter = if req.workspace_id.is_empty() {
            None
        } else {
            Some(req.workspace_id.as_str())
        };

        let state_filter: Vec<AgentState> = req
            .states
            .iter()
            .map(|s| AgentState::from_proto_i32(*s))
            .collect();

        let agents = self.agents.list(workspace_filter, &state_filter);
        let proto_agents: Vec<proto::Agent> = agents.iter().map(agent_to_proto).collect();

        Ok(Response::new(proto::ListAgentsResponse {
            agents: proto_agents,
        }))
    }

    /// GetAgent returns details for a specific agent.
    #[allow(clippy::result_large_err)]
    pub fn get_agent(
        &self,
        req: Request<proto::GetAgentRequest>,
    ) -> Result<Response<proto::GetAgentResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let agent = self
            .agents
            .get(&req.agent_id)
            .ok_or_else(|| Status::not_found(format!("agent {:?} not found", req.agent_id)))?;

        Ok(Response::new(proto::GetAgentResponse {
            agent: Some(agent_to_proto(&agent)),
        }))
    }

    /// CapturePane returns current content for an agent pane.
    #[allow(clippy::result_large_err)]
    pub fn capture_pane(
        &self,
        req: Request<proto::CapturePaneRequest>,
    ) -> Result<Response<proto::CapturePaneResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let agent = self
            .agents
            .get(&req.agent_id)
            .ok_or_else(|| Status::not_found(format!("agent {:?} not found", req.agent_id)))?;

        let include_history = req.lines < 0;
        let content = self
            .tmux
            .capture_pane(&agent.pane_id, include_history)
            .map_err(|e| Status::internal(format!("failed to capture pane: {e}")))?;

        let content_hash = hash_snapshot(&content);
        self.agents
            .update_snapshot(&req.agent_id, content_hash.clone(), None);

        Ok(Response::new(proto::CapturePaneResponse {
            content,
            content_hash,
            width: 0,
            height: 0,
            cursor_x: 0,
            cursor_y: 0,
            captured_at: Some(datetime_to_timestamp(Utc::now())),
        }))
    }

    /// StreamPaneUpdates parity helper.
    ///
    /// Runs `max_polls` iterations and returns updates matching Go stream logic:
    /// emit only when content changed, unless this is the first emission.
    #[allow(clippy::result_large_err)]
    pub fn stream_pane_updates(
        &self,
        req: Request<proto::StreamPaneUpdatesRequest>,
        max_polls: usize,
    ) -> Result<Vec<proto::StreamPaneUpdatesResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        if !self.agents.contains(&req.agent_id) {
            return Err(Status::not_found(format!(
                "agent {:?} not found",
                req.agent_id
            )));
        }

        let mut updates = Vec::new();
        let mut last_hash = req.last_known_hash;
        let poll_interval =
            positive_duration(req.min_interval.as_ref()).unwrap_or(DEFAULT_POLL_INTERVAL);

        for _ in 0..max_polls {
            std::thread::sleep(poll_interval);

            let agent = match self.agents.get(&req.agent_id) {
                Some(agent) => agent,
                None => {
                    return Err(Status::not_found(format!(
                        "agent {:?} no longer exists",
                        req.agent_id
                    )))
                }
            };

            let content = match self.tmux.capture_pane(&agent.pane_id, false) {
                Ok(content) => content,
                Err(_) => continue,
            };

            let content_hash = hash_snapshot(&content);
            let changed = content_hash != last_hash;

            if changed || last_hash.is_empty() {
                let detected_state = detect_agent_state(&content, &agent.adapter);
                self.agents.update_snapshot(
                    &req.agent_id,
                    content_hash.clone(),
                    Some(detected_state),
                );

                let mut update = proto::StreamPaneUpdatesResponse {
                    agent_id: req.agent_id.clone(),
                    content_hash: content_hash.clone(),
                    content: String::new(),
                    changed,
                    timestamp: Some(datetime_to_timestamp(Utc::now())),
                    detected_state: detected_state.to_proto_i32(),
                };
                if req.include_content {
                    update.content = content;
                }

                updates.push(update);
                last_hash = content_hash;
            }
        }

        Ok(updates)
    }
}

/// Convert domain Agent to proto Agent.
fn agent_to_proto(agent: &Agent) -> proto::Agent {
    proto::Agent {
        id: agent.id.clone(),
        workspace_id: agent.workspace_id.clone(),
        state: agent.state.to_proto_i32(),
        pane_id: agent.pane_id.clone(),
        pid: agent.pid,
        command: agent.command.clone(),
        adapter: agent.adapter.clone(),
        spawned_at: Some(datetime_to_timestamp(agent.spawned_at)),
        last_activity_at: Some(datetime_to_timestamp(agent.last_activity_at)),
        content_hash: agent.content_hash.clone(),
        resource_limits: None,
        resource_usage: None,
    }
}

fn datetime_to_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

fn positive_duration(d: Option<&prost_types::Duration>) -> Option<Duration> {
    let d = d?;
    if d.seconds < 0 || d.nanos < 0 {
        return None;
    }
    if d.seconds == 0 && d.nanos == 0 {
        return None;
    }

    let secs = u64::try_from(d.seconds).ok()?;
    let nanos = u32::try_from(d.nanos).ok()?;
    Some(Duration::new(secs, nanos))
}

fn hash_snapshot(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let sum = hasher.finalize();
    format!("{sum:x}")
}

fn detect_agent_state(content: &str, _adapter: &str) -> AgentState {
    if contains_any(
        content,
        &[
            "Do you want to",
            "Proceed?",
            "[y/n]",
            "[Y/n]",
            "approve",
            "confirm",
            "Allow?",
        ],
    ) {
        return AgentState::WaitingApproval;
    }

    if contains_any(content, &["$", "❯", "→", ">", "claude>", "opencode>"]) {
        let lines = split_lines(content);
        if let Some(last_line) = lines.last() {
            if contains_any(last_line, &["$", "❯", "→", ">"]) {
                return AgentState::Idle;
            }
        }
    }

    if contains_any(
        content,
        &[
            "Thinking...",
            "Working...",
            "Processing...",
            "⠋",
            "⠙",
            "⠹",
            "⠸",
            "⠼",
            "⠴",
            "⠦",
            "⠧",
            "⠇",
            "⠏",
        ],
    ) {
        return AgentState::Running;
    }

    if contains_any(
        content,
        &[
            "error:", "Error:", "ERROR", "fatal:", "Fatal:", "panic:", "Panic:",
        ],
    ) {
        return AgentState::Failed;
    }

    AgentState::Running
}

fn contains_any(content: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| !needle.is_empty() && content.contains(needle))
}

fn split_lines(content: &str) -> Vec<&str> {
    let trimmed = content.trim_end_matches('\n');
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed.split('\n').collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::agent::AgentInfo;
    use crate::tmux::TmuxClient;
    use crate::transcript::TranscriptStore;
    use std::sync::Mutex;

    /// Mock tmux client that records calls instead of executing.
    struct MockTmux {
        calls: Mutex<Vec<TmuxCall>>,
        fail_on: Mutex<Option<String>>,
        capture_outputs: Mutex<Vec<String>>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TmuxCall {
        SendKeys {
            target: String,
            keys: String,
            literal: bool,
            enter: bool,
        },
        SpecialKey {
            target: String,
            key: String,
        },
        CapturePane {
            target: String,
            include_history: bool,
        },
    }

    impl MockTmux {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_on: Mutex::new(None),
                capture_outputs: Mutex::new(vec![String::new()]),
            }
        }

        fn with_failure(msg: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_on: Mutex::new(Some(msg.to_string())),
                capture_outputs: Mutex::new(vec![String::new()]),
            }
        }

        fn with_capture(output: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_on: Mutex::new(None),
                capture_outputs: Mutex::new(vec![output.to_string()]),
            }
        }

        fn with_capture_sequence(outputs: &[&str]) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_on: Mutex::new(None),
                capture_outputs: Mutex::new(outputs.iter().map(|s| s.to_string()).collect()),
            }
        }

        fn calls(&self) -> Vec<TmuxCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl TmuxClient for MockTmux {
        fn send_keys(
            &self,
            target: &str,
            keys: &str,
            literal: bool,
            enter: bool,
        ) -> Result<(), String> {
            if let Some(msg) = self.fail_on.lock().unwrap().as_ref() {
                return Err(msg.clone());
            }
            self.calls.lock().unwrap().push(TmuxCall::SendKeys {
                target: target.to_string(),
                keys: keys.to_string(),
                literal,
                enter,
            });
            Ok(())
        }

        fn send_special_key(&self, target: &str, key: &str) -> Result<(), String> {
            if let Some(msg) = self.fail_on.lock().unwrap().as_ref() {
                return Err(msg.clone());
            }
            self.calls.lock().unwrap().push(TmuxCall::SpecialKey {
                target: target.to_string(),
                key: key.to_string(),
            });
            Ok(())
        }

        fn capture_pane(&self, target: &str, include_history: bool) -> Result<String, String> {
            if let Some(msg) = self.fail_on.lock().unwrap().as_ref() {
                return Err(msg.clone());
            }
            self.calls.lock().unwrap().push(TmuxCall::CapturePane {
                target: target.to_string(),
                include_history,
            });

            let mut outputs = self.capture_outputs.lock().unwrap();
            if outputs.is_empty() {
                return Ok(String::new());
            }
            if outputs.len() == 1 {
                return Ok(outputs[0].clone());
            }

            Ok(outputs.remove(0))
        }
    }

    fn make_service(tmux: Arc<dyn TmuxClient>) -> ForgedAgentService {
        ForgedAgentService::new(AgentManager::new(), tmux)
    }

    fn register_agent(svc: &ForgedAgentService, id: &str, ws: &str, state: AgentState) {
        let now = Utc::now();
        svc.agents.register(AgentInfo {
            id: id.to_string(),
            workspace_id: ws.to_string(),
            state,
            pane_id: format!("sess:{id}.0"),
            pid: 42,
            command: "claude".to_string(),
            adapter: "claude_code".to_string(),
            spawned_at: now,
            last_activity_at: now,
            content_hash: String::new(),
            transcript: TranscriptStore::new(),
        });
    }

    // -- SendInput tests --

    #[test]
    fn send_input_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: String::new(),
            text: "hello".to_string(),
            send_enter: false,
            keys: vec![],
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn send_input_agent_not_found() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: "missing".to_string(),
            text: "hello".to_string(),
            send_enter: false,
            keys: vec![],
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn send_input_sends_text() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: "a1".to_string(),
            text: "hello world".to_string(),
            send_enter: true,
            keys: vec![],
        }));
        assert!(result.is_ok());
        assert!(result.unwrap().into_inner().success);

        let calls = tmux.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            TmuxCall::SendKeys {
                target: "sess:a1.0".to_string(),
                keys: "hello world".to_string(),
                literal: true,
                enter: true,
            }
        );
    }

    #[test]
    fn send_input_sends_special_keys_first() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: "a1".to_string(),
            text: "y".to_string(),
            send_enter: false,
            keys: vec!["C-c".to_string(), "C-d".to_string()],
        }));
        assert!(result.is_ok());

        let calls = tmux.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0],
            TmuxCall::SpecialKey {
                target: "sess:a1.0".to_string(),
                key: "C-c".to_string(),
            }
        );
        assert_eq!(
            calls[1],
            TmuxCall::SpecialKey {
                target: "sess:a1.0".to_string(),
                key: "C-d".to_string(),
            }
        );
        assert_eq!(
            calls[2],
            TmuxCall::SendKeys {
                target: "sess:a1.0".to_string(),
                keys: "y".to_string(),
                literal: true,
                enter: false,
            }
        );
    }

    #[test]
    fn send_input_tmux_failure_returns_internal() {
        let tmux = Arc::new(MockTmux::with_failure("tmux error"));
        let svc = make_service(tmux);
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: "a1".to_string(),
            text: "hello".to_string(),
            send_enter: false,
            keys: vec![],
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[test]
    fn send_input_empty_text_with_keys_only() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.send_input(Request::new(proto::SendInputRequest {
            agent_id: "a1".to_string(),
            text: String::new(),
            send_enter: false,
            keys: vec!["C-c".to_string()],
        }));
        assert!(result.is_ok());

        let calls = tmux.calls();
        // Only the special key, no SendKeys call since text is empty
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            TmuxCall::SpecialKey {
                target: "sess:a1.0".to_string(),
                key: "C-c".to_string(),
            }
        );
    }

    // -- ListAgents tests --

    #[test]
    fn list_agents_empty() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.list_agents(Request::new(proto::ListAgentsRequest {
            workspace_id: String::new(),
            states: vec![],
        }));
        let resp = result.unwrap().into_inner();
        assert!(resp.agents.is_empty());
    }

    #[test]
    fn list_agents_returns_all() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        register_agent(&svc, "a2", "ws2", AgentState::Idle);

        let result = svc.list_agents(Request::new(proto::ListAgentsRequest {
            workspace_id: String::new(),
            states: vec![],
        }));
        let resp = result.unwrap().into_inner();
        assert_eq!(resp.agents.len(), 2);
    }

    #[test]
    fn list_agents_workspace_filter() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        register_agent(&svc, "a2", "ws2", AgentState::Running);

        let result = svc.list_agents(Request::new(proto::ListAgentsRequest {
            workspace_id: "ws1".to_string(),
            states: vec![],
        }));
        let resp = result.unwrap().into_inner();
        assert_eq!(resp.agents.len(), 1);
        assert_eq!(resp.agents[0].id, "a1");
    }

    #[test]
    fn list_agents_state_filter() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        register_agent(&svc, "a2", "ws1", AgentState::Idle);
        register_agent(&svc, "a3", "ws1", AgentState::Stopped);

        let result = svc.list_agents(Request::new(proto::ListAgentsRequest {
            workspace_id: String::new(),
            states: vec![
                AgentState::Running.to_proto_i32(),
                AgentState::Idle.to_proto_i32(),
            ],
        }));
        let resp = result.unwrap().into_inner();
        assert_eq!(resp.agents.len(), 2);
    }

    // -- GetAgent tests --

    #[test]
    fn get_agent_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.get_agent(Request::new(proto::GetAgentRequest {
            agent_id: String::new(),
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn get_agent_not_found() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.get_agent(Request::new(proto::GetAgentRequest {
            agent_id: "missing".to_string(),
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn get_agent_returns_proto() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.get_agent(Request::new(proto::GetAgentRequest {
            agent_id: "a1".to_string(),
        }));
        let resp = result.unwrap().into_inner();
        let agent = resp.agent.unwrap();
        assert_eq!(agent.id, "a1");
        assert_eq!(agent.workspace_id, "ws1");
        assert_eq!(agent.state, AgentState::Running.to_proto_i32());
        assert_eq!(agent.pane_id, "sess:a1.0");
        assert_eq!(agent.pid, 42);
        assert_eq!(agent.command, "claude");
        assert_eq!(agent.adapter, "claude_code");
        assert!(agent.spawned_at.is_some());
        assert!(agent.last_activity_at.is_some());
    }

    // -- CapturePane tests --

    #[test]
    fn capture_pane_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::with_capture("hello")));
        let result = svc.capture_pane(Request::new(proto::CapturePaneRequest {
            agent_id: String::new(),
            include_escape_sequences: false,
            lines: 0,
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn capture_pane_not_found() {
        let svc = make_service(Arc::new(MockTmux::with_capture("hello")));
        let result = svc.capture_pane(Request::new(proto::CapturePaneRequest {
            agent_id: "missing".to_string(),
            include_escape_sequences: false,
            lines: 0,
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn capture_pane_returns_content_hash_and_updates_agent_snapshot() {
        let tmux = Arc::new(MockTmux::with_capture("steady output"));
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.capture_pane(Request::new(proto::CapturePaneRequest {
            agent_id: "a1".to_string(),
            include_escape_sequences: false,
            lines: 0,
        }));
        assert!(result.is_ok());
        let resp = result.unwrap().into_inner();

        let expected_hash = hash_snapshot("steady output");
        assert_eq!(resp.content, "steady output");
        assert_eq!(resp.content_hash, expected_hash);
        assert!(resp.captured_at.is_some());

        let agent = svc.agents.get("a1").unwrap();
        assert_eq!(agent.content_hash, expected_hash);

        let calls = tmux.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            TmuxCall::CapturePane {
                target: "sess:a1.0".to_string(),
                include_history: false,
            }
        );
    }

    #[test]
    fn capture_pane_lines_negative_requests_history() {
        let tmux = Arc::new(MockTmux::with_capture("with history"));
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let result = svc.capture_pane(Request::new(proto::CapturePaneRequest {
            agent_id: "a1".to_string(),
            include_escape_sequences: false,
            lines: -1,
        }));
        assert!(result.is_ok());

        let calls = tmux.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            TmuxCall::CapturePane {
                target: "sess:a1.0".to_string(),
                include_history: true,
            }
        );
    }

    // -- StreamPaneUpdates tests --

    #[test]
    fn stream_pane_updates_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::with_capture("steady output")));
        let result = svc.stream_pane_updates(
            Request::new(proto::StreamPaneUpdatesRequest {
                agent_id: String::new(),
                min_interval: None,
                last_known_hash: String::new(),
                include_content: false,
            }),
            1,
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn stream_pane_updates_not_found() {
        let svc = make_service(Arc::new(MockTmux::with_capture("steady output")));
        let result = svc.stream_pane_updates(
            Request::new(proto::StreamPaneUpdatesRequest {
                agent_id: "missing".to_string(),
                min_interval: None,
                last_known_hash: String::new(),
                include_content: false,
            }),
            1,
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn stream_pane_updates_skips_unchanged_content() {
        let svc = make_service(Arc::new(MockTmux::with_capture_sequence(&[
            "steady output",
            "steady output",
            "steady output",
        ])));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let updates = svc
            .stream_pane_updates(
                Request::new(proto::StreamPaneUpdatesRequest {
                    agent_id: "a1".to_string(),
                    min_interval: Some(prost_types::Duration {
                        seconds: 0,
                        nanos: 1,
                    }),
                    last_known_hash: String::new(),
                    include_content: false,
                }),
                3,
            )
            .unwrap();

        assert_eq!(updates.len(), 1);
        assert!(updates[0].changed);
        assert_eq!(updates[0].content_hash, hash_snapshot("steady output"));
        assert!(updates[0].content.is_empty());
    }

    #[test]
    fn stream_pane_updates_respects_last_known_hash() {
        let stable = "no change";
        let svc = make_service(Arc::new(MockTmux::with_capture(stable)));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let updates = svc
            .stream_pane_updates(
                Request::new(proto::StreamPaneUpdatesRequest {
                    agent_id: "a1".to_string(),
                    min_interval: Some(prost_types::Duration {
                        seconds: 0,
                        nanos: 1,
                    }),
                    last_known_hash: hash_snapshot(stable),
                    include_content: true,
                }),
                2,
            )
            .unwrap();

        assert!(updates.is_empty());
    }

    // -- State-detection helpers --

    #[test]
    fn detect_agent_state_parity_smoke() {
        assert_eq!(
            detect_agent_state("Do you want to proceed? [y/n]", ""),
            AgentState::WaitingApproval
        );
        assert_eq!(detect_agent_state("output\n$", ""), AgentState::Idle);
        assert_eq!(detect_agent_state("Thinking...", ""), AgentState::Running);
        assert_eq!(
            detect_agent_state("error: something broke", ""),
            AgentState::Failed
        );
    }

    #[test]
    fn split_lines_drops_trailing_newline_only() {
        assert_eq!(split_lines("line1\nline2\n"), vec!["line1", "line2"]);
        assert_eq!(split_lines(""), Vec::<&str>::new());
    }

    // -- Proto conversion tests --

    #[test]
    fn agent_state_round_trip() {
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
            let proto_val = state.to_proto_i32();
            let back = AgentState::from_proto_i32(proto_val);
            assert_eq!(state, back, "round-trip failed for {state:?}");
        }
    }

    #[test]
    fn agent_state_unknown_maps_to_unspecified() {
        assert_eq!(AgentState::from_proto_i32(99), AgentState::Unspecified);
        assert_eq!(AgentState::from_proto_i32(-1), AgentState::Unspecified);
    }

    #[test]
    fn datetime_to_timestamp_converts_correctly() {
        use chrono::TimeZone;
        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let ts = datetime_to_timestamp(dt);
        assert_eq!(ts.seconds, dt.timestamp());
        assert_eq!(ts.nanos, 0);
    }
}

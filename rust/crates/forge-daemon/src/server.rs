//! gRPC server implementation for ForgedService agent RPCs.
//!
//! Implements SpawnAgent, KillAgent, SendInput, ListAgents, GetAgent with parity
//! to Go daemon (`internal/forged/server.go`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};

use forge_rpc::forged::v1 as proto;

use crate::agent::{Agent, AgentInfo, AgentManager, AgentState};
use crate::events::EventBus;
use crate::loop_runner::{
    LoopRunner, LoopRunnerError, LoopRunnerManager, LoopRunnerState, StartLoopRunnerRequest,
};
use crate::status::StatusService;
use crate::tmux::TmuxClient;
use crate::transcript::{TranscriptEntry, TranscriptEntryType, TranscriptStore};

/// Holds agent registry + tmux client for gRPC handlers.
pub struct ForgedAgentService {
    agents: AgentManager,
    tmux: Arc<dyn TmuxClient>,
    events: Arc<EventBus>,
    loop_runners: LoopRunnerManager,
    status: StatusService,
}

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);

impl ForgedAgentService {
    pub fn new(agents: AgentManager, tmux: Arc<dyn TmuxClient>) -> Self {
        Self::new_with_loop_runners(agents, tmux, LoopRunnerManager::new())
    }

    pub fn new_with_loop_runners(
        agents: AgentManager,
        tmux: Arc<dyn TmuxClient>,
        loop_runners: LoopRunnerManager,
    ) -> Self {
        let hostname = nix::unistd::gethostname()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            agents,
            tmux,
            events: Arc::new(EventBus::new()),
            loop_runners,
            status: StatusService::new("dev", hostname),
        }
    }

    /// Access the agent manager (used by other service components).
    pub fn agents(&self) -> &AgentManager {
        &self.agents
    }

    /// Access the loop runner manager (used by daemon shutdown flow).
    pub fn loop_runner_manager(&self) -> LoopRunnerManager {
        self.loop_runners.clone()
    }

    // -- RPC handlers --

    /// SpawnAgent creates a new agent in a tmux pane.
    ///
    /// Parity with Go `(*Server).SpawnAgent` in `internal/forged/server.go`.
    #[allow(clippy::result_large_err)]
    pub fn spawn_agent(
        &self,
        req: Request<proto::SpawnAgentRequest>,
    ) -> Result<Response<proto::SpawnAgentResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }
        if req.command.is_empty() {
            return Err(Status::invalid_argument("command is required"));
        }

        // Check if agent already exists.
        if self.agents.contains(&req.agent_id) {
            return Err(Status::already_exists(format!(
                "agent {:?} already exists",
                req.agent_id
            )));
        }

        // Determine session name (default: forge-{workspace_id}).
        let session_name = if req.session_name.is_empty() {
            format!("forge-{}", req.workspace_id)
        } else {
            req.session_name.clone()
        };

        // Determine working directory.
        let work_dir = if req.working_dir.is_empty() {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        } else {
            req.working_dir.clone()
        };

        // Ensure session exists.
        let has_session = self
            .tmux
            .has_session(&session_name)
            .map_err(|e| Status::internal(format!("failed to check session: {e}")))?;

        if !has_session {
            self.tmux
                .new_session(&session_name, &work_dir)
                .map_err(|e| Status::internal(format!("failed to create session: {e}")))?;
        }

        // Create a new pane by splitting the window.
        let pane_id = self
            .tmux
            .split_window(&session_name, true, &work_dir)
            .map_err(|e| Status::internal(format!("failed to create pane: {e}")))?;

        // Build the command line with args.
        let mut cmd_line = req.command.clone();
        for arg in &req.args {
            cmd_line.push(' ');
            cmd_line.push_str(arg);
        }

        // Set environment variables in the pane.
        for (k, v) in &req.env {
            let env_cmd = format!("export {k}={v:?}");
            // Best-effort: ignore errors setting env vars (matches Go behavior).
            let _ = self.tmux.send_keys(&pane_id, &env_cmd, true, true);
        }

        // Send the command to the pane.
        if let Err(e) = self.tmux.send_keys(&pane_id, &cmd_line, true, true) {
            // Try to clean up the pane on failure.
            let _ = self.tmux.kill_pane(&pane_id);
            return Err(Status::internal(format!("failed to send command: {e}")));
        }

        // Get the PID of the process in the pane.
        // Continue without PID on failure (resource monitoring will be limited).
        let pid = self.tmux.get_pane_pid(&pane_id).unwrap_or(0);

        let now = Utc::now();
        let info = AgentInfo {
            id: req.agent_id.clone(),
            workspace_id: req.workspace_id.clone(),
            state: AgentState::Starting,
            pane_id: pane_id.clone(),
            pid,
            command: req.command.clone(),
            adapter: req.adapter.clone(),
            spawned_at: now,
            last_activity_at: now,
            content_hash: String::new(),
            transcript: TranscriptStore::new(),
        };
        let agent = self.agents.register(info);

        // Record spawn event in transcript.
        let mut metadata = HashMap::new();
        metadata.insert("event".to_string(), "spawn".to_string());
        metadata.insert("adapter".to_string(), req.adapter.clone());
        metadata.insert("workspace".to_string(), req.workspace_id.clone());

        self.agents.add_transcript_entry_full(
            &req.agent_id,
            TranscriptEntry {
                entry_type: TranscriptEntryType::Command,
                content: cmd_line,
                timestamp: Utc::now(),
                metadata,
            },
        );

        self.events.publish_agent_state_changed(
            &req.agent_id,
            &req.workspace_id,
            AgentState::Unspecified.to_proto_i32(),
            AgentState::Starting.to_proto_i32(),
            "agent spawned",
        );

        Ok(Response::new(proto::SpawnAgentResponse {
            agent: Some(agent_to_proto(&agent)),
            pane_id,
        }))
    }

    /// KillAgent terminates an agent's process.
    ///
    /// Parity with Go `(*Server).KillAgent` in `internal/forged/server.go`.
    #[allow(clippy::result_large_err)]
    pub fn kill_agent(
        &self,
        req: Request<proto::KillAgentRequest>,
    ) -> Result<Response<proto::KillAgentResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        // Look up agent — must exist.
        let agent = self
            .agents
            .get(&req.agent_id)
            .ok_or_else(|| Status::not_found(format!("agent {:?} not found", req.agent_id)))?;

        // Send interrupt first (Ctrl+C) unless force is set.
        if !req.force {
            let _ = self.tmux.send_interrupt(&agent.pane_id);

            // Wait for grace period if specified.
            if let Some(ref gp) = req.grace_period {
                let grace = positive_duration(Some(gp));
                if let Some(dur) = grace {
                    std::thread::sleep(dur);
                }
            }
        }

        // Record state change in transcript before killing.
        let mut metadata = HashMap::new();
        metadata.insert("event".to_string(), "kill".to_string());
        metadata.insert("force".to_string(), format!("{}", req.force));
        metadata.insert("previous".to_string(), format!("{:?}", agent.state));

        self.agents.add_transcript_entry_full(
            &req.agent_id,
            TranscriptEntry {
                entry_type: TranscriptEntryType::StateChange,
                content: "stopped".to_string(),
                timestamp: Utc::now(),
                metadata,
            },
        );

        // Kill the pane.
        let _ = self.tmux.kill_pane(&agent.pane_id);

        // Remove agent from registry.
        self.agents.remove(&req.agent_id);

        let reason = if req.force {
            "agent force killed"
        } else {
            "agent killed"
        };
        self.events.publish_agent_state_changed(
            &req.agent_id,
            &agent.workspace_id,
            agent.state.to_proto_i32(),
            AgentState::Stopped.to_proto_i32(),
            reason,
        );

        Ok(Response::new(proto::KillAgentResponse { success: true }))
    }

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

    /// StartLoopRunner starts a daemon-owned loop runner process.
    #[allow(clippy::result_large_err)]
    pub fn start_loop_runner(
        &self,
        req: Request<proto::StartLoopRunnerRequest>,
    ) -> Result<Response<proto::StartLoopRunnerResponse>, Status> {
        let req = req.into_inner();

        let runner = self
            .loop_runners
            .start_loop_runner(StartLoopRunnerRequest {
                loop_id: req.loop_id,
                config_path: req.config_path,
                command_path: req.command_path,
            })
            .map_err(loop_runner_error_to_status)?;

        Ok(Response::new(proto::StartLoopRunnerResponse {
            runner: Some(loop_runner_to_proto(&runner)),
        }))
    }

    /// StopLoopRunner stops a daemon-owned loop runner process.
    #[allow(clippy::result_large_err)]
    pub fn stop_loop_runner(
        &self,
        req: Request<proto::StopLoopRunnerRequest>,
    ) -> Result<Response<proto::StopLoopRunnerResponse>, Status> {
        let req = req.into_inner();

        let stopped = self
            .loop_runners
            .stop_loop_runner(&req.loop_id, req.force)
            .map_err(loop_runner_error_to_status)?;

        Ok(Response::new(proto::StopLoopRunnerResponse {
            success: stopped.success,
            runner: Some(loop_runner_to_proto(&stopped.runner)),
        }))
    }

    /// GetLoopRunner returns one daemon-owned loop runner.
    #[allow(clippy::result_large_err)]
    pub fn get_loop_runner(
        &self,
        req: Request<proto::GetLoopRunnerRequest>,
    ) -> Result<Response<proto::GetLoopRunnerResponse>, Status> {
        let req = req.into_inner();

        let runner = self
            .loop_runners
            .get_loop_runner(&req.loop_id)
            .map_err(loop_runner_error_to_status)?;

        Ok(Response::new(proto::GetLoopRunnerResponse {
            runner: Some(loop_runner_to_proto(&runner)),
        }))
    }

    /// ListLoopRunners lists daemon-owned loop runners.
    #[allow(clippy::result_large_err)]
    pub fn list_loop_runners(
        &self,
        _req: Request<proto::ListLoopRunnersRequest>,
    ) -> Result<Response<proto::ListLoopRunnersResponse>, Status> {
        let runners = self.loop_runners.list_loop_runners();
        let runners: Vec<proto::LoopRunner> = runners.iter().map(loop_runner_to_proto).collect();

        Ok(Response::new(proto::ListLoopRunnersResponse { runners }))
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
                let previous_state = agent.state;

                let output_content = tail_utf8(&content, 4096);
                let mut output_metadata = HashMap::new();
                output_metadata.insert("content_hash".to_string(), content_hash.clone());
                self.agents.add_transcript_entry_full(
                    &req.agent_id,
                    TranscriptEntry {
                        entry_type: TranscriptEntryType::Output,
                        content: output_content,
                        timestamp: Utc::now(),
                        metadata: output_metadata,
                    },
                );

                let mut state_changed = false;
                if detected_state != AgentState::Unspecified && previous_state != detected_state {
                    state_changed = true;
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "previous".to_string(),
                        agent_state_name(previous_state).to_string(),
                    );
                    self.agents.add_transcript_entry_full(
                        &req.agent_id,
                        TranscriptEntry {
                            entry_type: TranscriptEntryType::StateChange,
                            content: agent_state_name(detected_state).to_string(),
                            timestamp: Utc::now(),
                            metadata,
                        },
                    );
                }

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
                let lines_changed = split_lines(&content).len() as i32;
                if req.include_content {
                    update.content = content;
                }

                if state_changed {
                    self.events.publish_agent_state_changed(
                        &req.agent_id,
                        &agent.workspace_id,
                        previous_state.to_proto_i32(),
                        detected_state.to_proto_i32(),
                        "state detected from pane content",
                    );
                }
                if changed {
                    self.events.publish_pane_content_changed(
                        &req.agent_id,
                        &agent.workspace_id,
                        &content_hash,
                        lines_changed,
                    );
                }

                updates.push(update);
                last_hash = content_hash;
            }
        }

        Ok(updates)
    }

    /// StreamEvents parity helper.
    ///
    /// Runs `max_polls` iterations and emits replay/cursor-filtered events.
    #[allow(clippy::result_large_err)]
    pub fn stream_events(
        &self,
        req: Request<proto::StreamEventsRequest>,
        max_polls: usize,
    ) -> Result<Vec<proto::StreamEventsResponse>, Status> {
        let req = req.into_inner();
        let (sub_id, mut rx, replay) = self.events.subscribe(&req)?;
        let mut updates = Vec::with_capacity(replay.len());
        for event in replay {
            updates.push(proto::StreamEventsResponse { event: Some(event) });
        }
        let poll_interval = Duration::from_millis(100);

        for poll in 0..max_polls {
            if poll > 0 {
                std::thread::sleep(poll_interval);
            }
            while let Ok(event) = rx.try_recv() {
                updates.push(proto::StreamEventsResponse { event: Some(event) });
            }
        }

        self.events.unsubscribe(&sub_id);
        Ok(updates)
    }

    /// GetTranscript retrieves the full transcript for an agent.
    #[allow(clippy::result_large_err)]
    pub fn get_transcript(
        &self,
        req: Request<proto::GetTranscriptRequest>,
    ) -> Result<Response<proto::GetTranscriptResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let entries = self
            .agents
            .transcript_snapshot(&req.agent_id)
            .ok_or_else(|| Status::not_found(format!("agent {:?} not found", req.agent_id)))?;

        let start_time = match req.start_time.as_ref() {
            Some(ts) => Some(timestamp_to_datetime(ts)?),
            None => None,
        };
        let end_time = match req.end_time.as_ref() {
            Some(ts) => Some(timestamp_to_datetime(ts)?),
            None => None,
        };

        let limit = if req.limit <= 0 {
            1000usize
        } else {
            req.limit as usize
        };

        let mut filtered: Vec<(i64, TranscriptEntry)> = Vec::new();
        for (id, entry) in entries {
            if let Some(start) = start_time {
                if entry.timestamp < start {
                    continue;
                }
            }
            if let Some(end) = end_time {
                if entry.timestamp > end {
                    continue;
                }
            }
            filtered.push((id, entry));
        }

        let has_more = filtered.len() > limit;
        if has_more {
            filtered.truncate(limit);
        }

        let proto_entries: Vec<proto::TranscriptEntry> = filtered
            .iter()
            .map(|(_, entry)| transcript_entry_to_proto(entry))
            .collect();

        let mut next_cursor = String::new();
        if has_more {
            if let Some((last_id, _)) = filtered.last() {
                next_cursor = format!("{}", last_id + 1);
            }
        }

        Ok(Response::new(proto::GetTranscriptResponse {
            agent_id: req.agent_id,
            entries: proto_entries,
            has_more,
            next_cursor,
        }))
    }

    /// StreamTranscript parity helper.
    ///
    /// Runs `max_polls` iterations and returns chunks matching Go stream logic:
    /// only emit when new entries exist for the current cursor.
    #[allow(clippy::result_large_err)]
    pub fn stream_transcript(
        &self,
        req: Request<proto::StreamTranscriptRequest>,
        max_polls: usize,
    ) -> Result<Vec<proto::StreamTranscriptResponse>, Status> {
        let req = req.into_inner();

        if req.agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let mut cursor = if req.cursor.is_empty() {
            0i64
        } else {
            parse_cursor_i64(&req.cursor)?
        };

        let mut updates = Vec::new();
        let poll_interval = Duration::from_millis(100);

        for poll in 0..max_polls {
            if poll > 0 {
                std::thread::sleep(poll_interval);
            }

            let entries = self
                .agents
                .transcript_snapshot(&req.agent_id)
                .ok_or_else(|| {
                    Status::not_found(format!("agent {:?} no longer exists", req.agent_id))
                })?;

            let mut new_entries: Vec<proto::TranscriptEntry> = Vec::new();
            let mut last_id: Option<i64> = None;
            for (id, entry) in entries {
                if id >= cursor {
                    last_id = Some(id);
                    new_entries.push(transcript_entry_to_proto(&entry));
                }
            }

            if let Some(last_id) = last_id {
                cursor = last_id + 1;
            }

            if !new_entries.is_empty() {
                updates.push(proto::StreamTranscriptResponse {
                    entries: new_entries,
                    cursor: format!("{cursor}"),
                });
            }
        }

        Ok(updates)
    }
}

// -- tonic async trait wiring --

use forge_rpc::forged::v1::forged_service_server::ForgedService;

type BoxStream<T> =
    std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<T, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl ForgedService for ForgedAgentService {
    async fn spawn_agent(
        &self,
        request: Request<proto::SpawnAgentRequest>,
    ) -> Result<Response<proto::SpawnAgentResponse>, Status> {
        self.spawn_agent(request)
    }

    async fn kill_agent(
        &self,
        request: Request<proto::KillAgentRequest>,
    ) -> Result<Response<proto::KillAgentResponse>, Status> {
        self.kill_agent(request)
    }

    async fn send_input(
        &self,
        request: Request<proto::SendInputRequest>,
    ) -> Result<Response<proto::SendInputResponse>, Status> {
        self.send_input(request)
    }

    async fn list_agents(
        &self,
        request: Request<proto::ListAgentsRequest>,
    ) -> Result<Response<proto::ListAgentsResponse>, Status> {
        self.list_agents(request)
    }

    async fn get_agent(
        &self,
        request: Request<proto::GetAgentRequest>,
    ) -> Result<Response<proto::GetAgentResponse>, Status> {
        self.get_agent(request)
    }

    async fn start_loop_runner(
        &self,
        request: Request<proto::StartLoopRunnerRequest>,
    ) -> Result<Response<proto::StartLoopRunnerResponse>, Status> {
        self.start_loop_runner(request)
    }

    async fn stop_loop_runner(
        &self,
        request: Request<proto::StopLoopRunnerRequest>,
    ) -> Result<Response<proto::StopLoopRunnerResponse>, Status> {
        self.stop_loop_runner(request)
    }

    async fn get_loop_runner(
        &self,
        request: Request<proto::GetLoopRunnerRequest>,
    ) -> Result<Response<proto::GetLoopRunnerResponse>, Status> {
        self.get_loop_runner(request)
    }

    async fn list_loop_runners(
        &self,
        request: Request<proto::ListLoopRunnersRequest>,
    ) -> Result<Response<proto::ListLoopRunnersResponse>, Status> {
        self.list_loop_runners(request)
    }

    async fn capture_pane(
        &self,
        request: Request<proto::CapturePaneRequest>,
    ) -> Result<Response<proto::CapturePaneResponse>, Status> {
        self.capture_pane(request)
    }

    type StreamPaneUpdatesStream = BoxStream<proto::StreamPaneUpdatesResponse>;

    async fn stream_pane_updates(
        &self,
        request: Request<proto::StreamPaneUpdatesRequest>,
    ) -> Result<Response<Self::StreamPaneUpdatesStream>, Status> {
        let updates = self.stream_pane_updates(request, 5)?;
        let stream = tokio_stream::iter(updates.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    type StreamEventsStream = BoxStream<proto::StreamEventsResponse>;

    async fn stream_events(
        &self,
        request: Request<proto::StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let updates = self.stream_events(request, 5)?;
        let stream = tokio_stream::iter(updates.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_transcript(
        &self,
        request: Request<proto::GetTranscriptRequest>,
    ) -> Result<Response<proto::GetTranscriptResponse>, Status> {
        self.get_transcript(request)
    }

    type StreamTranscriptStream = BoxStream<proto::StreamTranscriptResponse>;

    async fn stream_transcript(
        &self,
        request: Request<proto::StreamTranscriptRequest>,
    ) -> Result<Response<Self::StreamTranscriptStream>, Status> {
        let updates = self.stream_transcript(request, 5)?;
        let stream = tokio_stream::iter(updates.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_status(
        &self,
        _request: Request<proto::GetStatusRequest>,
    ) -> Result<Response<proto::GetStatusResponse>, Status> {
        let agent_count = self.agents.list(None, &[]).len();
        Ok(Response::new(self.status.get_status(agent_count)))
    }

    async fn ping(
        &self,
        _request: Request<proto::PingRequest>,
    ) -> Result<Response<proto::PingResponse>, Status> {
        Ok(Response::new(self.status.ping()))
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

fn loop_runner_to_proto(runner: &LoopRunner) -> proto::LoopRunner {
    proto::LoopRunner {
        loop_id: runner.loop_id.clone(),
        instance_id: runner.instance_id.clone(),
        config_path: runner.config_path.clone(),
        command_path: runner.command_path.clone(),
        pid: runner.pid,
        state: loop_runner_state_to_proto_i32(&runner.state),
        last_error: runner.last_error.clone(),
        started_at: Some(datetime_to_timestamp(runner.started_at)),
        stopped_at: runner.stopped_at.map(datetime_to_timestamp),
    }
}

fn loop_runner_state_to_proto_i32(state: &LoopRunnerState) -> i32 {
    match state {
        LoopRunnerState::Running => proto::LoopRunnerState::Running as i32,
        LoopRunnerState::Stopped => proto::LoopRunnerState::Stopped as i32,
        LoopRunnerState::Error => proto::LoopRunnerState::Error as i32,
    }
}

#[allow(clippy::result_large_err)]
fn loop_runner_error_to_status(err: LoopRunnerError) -> Status {
    match err {
        LoopRunnerError::InvalidArgument => Status::invalid_argument("loop_id is required"),
        LoopRunnerError::AlreadyExists(loop_id) => {
            Status::already_exists(format!("loop runner {:?} already running", loop_id))
        }
        LoopRunnerError::NotFound(loop_id) => {
            Status::not_found(format!("loop runner {:?} not found", loop_id))
        }
        LoopRunnerError::StartFailed(cause) => {
            Status::internal(format!("failed to start loop runner: {cause}"))
        }
        LoopRunnerError::StopFailed(loop_id, cause) => {
            Status::internal(format!("failed to stop loop runner {:?}: {cause}", loop_id))
        }
        LoopRunnerError::NoProcessHandle(loop_id) => {
            Status::internal(format!("loop runner {:?} has no process handle", loop_id))
        }
    }
}

fn datetime_to_timestamp(dt: DateTime<Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

#[allow(clippy::result_large_err)]
fn timestamp_to_datetime(ts: &prost_types::Timestamp) -> Result<DateTime<Utc>, Status> {
    if ts.nanos < 0 || ts.nanos >= 1_000_000_000 {
        return Err(Status::invalid_argument("invalid timestamp nanos"));
    }
    let nanos = ts.nanos as u32;
    match Utc.timestamp_opt(ts.seconds, nanos).single() {
        Some(dt) => Ok(dt),
        None => Err(Status::invalid_argument("invalid timestamp")),
    }
}

#[allow(clippy::result_large_err)]
fn parse_cursor_i64(cursor: &str) -> Result<i64, Status> {
    let mut result: i64 = 0;
    for ch in cursor.chars() {
        if !ch.is_ascii_digit() {
            return Err(Status::invalid_argument(format!(
                "invalid cursor: invalid character: {ch}"
            )));
        }
        result = result
            .wrapping_mul(10)
            .wrapping_add(i64::from(ch as u8 - b'0'));
    }
    Ok(result)
}

fn transcript_entry_to_proto(entry: &TranscriptEntry) -> proto::TranscriptEntry {
    proto::TranscriptEntry {
        timestamp: Some(datetime_to_timestamp(entry.timestamp)),
        r#type: entry.entry_type.to_proto_i32(),
        content: entry.content.clone(),
        metadata: entry.metadata.clone(),
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

fn agent_state_name(state: AgentState) -> &'static str {
    match state {
        AgentState::Unspecified => "unspecified",
        AgentState::Starting => "starting",
        AgentState::Running => "running",
        AgentState::Idle => "idle",
        AgentState::WaitingApproval => "waiting_approval",
        AgentState::Paused => "paused",
        AgentState::Stopping => "stopping",
        AgentState::Stopped => "stopped",
        AgentState::Failed => "failed",
    }
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

fn tail_utf8(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_string();
    }

    let raw_start = content.len() - max_bytes;
    let start = content
        .char_indices()
        .find(|(idx, _)| *idx >= raw_start)
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    content[start..].to_string()
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
        /// Whether `has_session` returns true.
        session_exists: Mutex<bool>,
        /// Pane ID returned by `split_window`.
        split_pane_id: Mutex<String>,
        /// PID returned by `get_pane_pid`.
        pane_pid: Mutex<i32>,
        /// Methods that should fail (by method name).
        fail_methods: Mutex<Vec<String>>,
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
        HasSession {
            session_name: String,
        },
        NewSession {
            session_name: String,
            working_dir: String,
        },
        SplitWindow {
            session_name: String,
            horizontal: bool,
            working_dir: String,
        },
        GetPanePid {
            pane_id: String,
        },
        SendInterrupt {
            pane_id: String,
        },
        KillPane {
            pane_id: String,
        },
    }

    #[allow(dead_code)]
    impl MockTmux {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_on: Mutex::new(None),
                capture_outputs: Mutex::new(vec![String::new()]),
                session_exists: Mutex::new(false),
                split_pane_id: Mutex::new("forge-ws1:0.1".to_string()),
                pane_pid: Mutex::new(12345),
                fail_methods: Mutex::new(Vec::new()),
            }
        }

        fn with_failure(msg: &str) -> Self {
            Self {
                fail_on: Mutex::new(Some(msg.to_string())),
                ..Self::new()
            }
        }

        fn with_capture(output: &str) -> Self {
            Self {
                capture_outputs: Mutex::new(vec![output.to_string()]),
                ..Self::new()
            }
        }

        fn with_capture_sequence(outputs: &[&str]) -> Self {
            Self {
                capture_outputs: Mutex::new(outputs.iter().map(|s| s.to_string()).collect()),
                ..Self::new()
            }
        }

        fn with_session_exists(self) -> Self {
            *self.session_exists.lock().unwrap() = true;
            self
        }

        fn with_split_pane_id(self, pane_id: &str) -> Self {
            *self.split_pane_id.lock().unwrap() = pane_id.to_string();
            self
        }

        fn with_pane_pid(self, pid: i32) -> Self {
            *self.pane_pid.lock().unwrap() = pid;
            self
        }

        fn with_fail_method(self, method: &str) -> Self {
            self.fail_methods.lock().unwrap().push(method.to_string());
            self
        }

        fn calls(&self) -> Vec<TmuxCall> {
            self.calls.lock().unwrap().clone()
        }

        fn should_fail(&self, method: &str) -> Option<String> {
            let methods = self.fail_methods.lock().unwrap();
            if methods.contains(&method.to_string()) {
                Some(format!("mock failure: {method}"))
            } else {
                None
            }
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

        fn has_session(&self, session_name: &str) -> Result<bool, String> {
            if let Some(msg) = self.should_fail("has_session") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::HasSession {
                session_name: session_name.to_string(),
            });
            Ok(*self.session_exists.lock().unwrap())
        }

        fn new_session(&self, session_name: &str, working_dir: &str) -> Result<(), String> {
            if let Some(msg) = self.should_fail("new_session") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::NewSession {
                session_name: session_name.to_string(),
                working_dir: working_dir.to_string(),
            });
            Ok(())
        }

        fn split_window(
            &self,
            session_name: &str,
            horizontal: bool,
            working_dir: &str,
        ) -> Result<String, String> {
            if let Some(msg) = self.should_fail("split_window") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::SplitWindow {
                session_name: session_name.to_string(),
                horizontal,
                working_dir: working_dir.to_string(),
            });
            Ok(self.split_pane_id.lock().unwrap().clone())
        }

        fn get_pane_pid(&self, pane_id: &str) -> Result<i32, String> {
            if let Some(msg) = self.should_fail("get_pane_pid") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::GetPanePid {
                pane_id: pane_id.to_string(),
            });
            Ok(*self.pane_pid.lock().unwrap())
        }

        fn send_interrupt(&self, pane_id: &str) -> Result<(), String> {
            if let Some(msg) = self.should_fail("send_interrupt") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::SendInterrupt {
                pane_id: pane_id.to_string(),
            });
            Ok(())
        }

        fn kill_pane(&self, pane_id: &str) -> Result<(), String> {
            if let Some(msg) = self.should_fail("kill_pane") {
                return Err(msg);
            }
            self.calls.lock().unwrap().push(TmuxCall::KillPane {
                pane_id: pane_id.to_string(),
            });
            Ok(())
        }
    }

    fn make_service(tmux: Arc<dyn TmuxClient>) -> ForgedAgentService {
        ForgedAgentService::new(AgentManager::new(), tmux)
    }

    fn make_service_with_loop_runners(
        tmux: Arc<dyn TmuxClient>,
        loop_runners: LoopRunnerManager,
    ) -> ForgedAgentService {
        ForgedAgentService::new_with_loop_runners(AgentManager::new(), tmux, loop_runners)
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

    // -- SpawnAgent tests --

    fn make_spawn_request(agent_id: &str, command: &str) -> proto::SpawnAgentRequest {
        proto::SpawnAgentRequest {
            agent_id: agent_id.to_string(),
            workspace_id: "ws1".to_string(),
            command: command.to_string(),
            args: vec![],
            env: Default::default(),
            working_dir: "/tmp".to_string(),
            session_name: String::new(),
            adapter: "claude_code".to_string(),
            resource_limits: None,
        }
    }

    #[test]
    fn spawn_agent_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .spawn_agent(Request::new(make_spawn_request("", "claude")))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn spawn_agent_requires_command() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "")))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn spawn_agent_duplicate_returns_already_exists() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        let err = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::AlreadyExists);
    }

    #[test]
    fn spawn_agent_creates_session_when_missing() {
        let tmux = Arc::new(
            MockTmux::new()
                .with_split_pane_id("forge-ws1:0.1")
                .with_pane_pid(9999),
        );
        let svc = make_service(tmux.clone());

        let resp = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap()
            .into_inner();

        assert_eq!(resp.pane_id, "forge-ws1:0.1");
        let agent = resp.agent.unwrap();
        assert_eq!(agent.id, "a1");
        assert_eq!(agent.workspace_id, "ws1");
        assert_eq!(agent.state, AgentState::Starting.to_proto_i32());
        assert_eq!(agent.pid, 9999);
        assert_eq!(agent.command, "claude");
        assert_eq!(agent.adapter, "claude_code");

        let calls = tmux.calls();
        assert!(calls.contains(&TmuxCall::HasSession {
            session_name: "forge-ws1".to_string(),
        }));
        assert!(calls.contains(&TmuxCall::NewSession {
            session_name: "forge-ws1".to_string(),
            working_dir: "/tmp".to_string(),
        }));
        assert!(calls.contains(&TmuxCall::SplitWindow {
            session_name: "forge-ws1".to_string(),
            horizontal: true,
            working_dir: "/tmp".to_string(),
        }));
        assert!(calls.contains(&TmuxCall::GetPanePid {
            pane_id: "forge-ws1:0.1".to_string(),
        }));
    }

    #[test]
    fn spawn_agent_reuses_existing_session() {
        let tmux = Arc::new(MockTmux::new().with_session_exists());
        let svc = make_service(tmux.clone());
        let _resp = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap();

        let calls = tmux.calls();
        assert!(calls.contains(&TmuxCall::HasSession {
            session_name: "forge-ws1".to_string(),
        }));
        assert!(!calls
            .iter()
            .any(|c| matches!(c, TmuxCall::NewSession { .. })));
    }

    #[test]
    fn spawn_agent_custom_session_name() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        let mut req = make_spawn_request("a1", "claude");
        req.session_name = "my-session".to_string();
        let _resp = svc.spawn_agent(Request::new(req)).unwrap();

        let calls = tmux.calls();
        assert!(calls.contains(&TmuxCall::HasSession {
            session_name: "my-session".to_string(),
        }));
    }

    #[test]
    fn spawn_agent_with_args() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        let mut req = make_spawn_request("a1", "claude");
        req.args = vec!["--dangerously-skip-permissions".to_string()];
        let _resp = svc.spawn_agent(Request::new(req)).unwrap();

        let calls = tmux.calls();
        let cmd_call = calls.iter().find(|c| {
            if let TmuxCall::SendKeys { keys, .. } = c {
                keys.contains("claude")
            } else {
                false
            }
        });
        assert!(cmd_call.is_some());
        if let Some(TmuxCall::SendKeys { keys, .. }) = cmd_call {
            assert!(keys.contains("claude --dangerously-skip-permissions"));
        }
    }

    #[test]
    fn spawn_agent_with_env_vars() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        let mut req = make_spawn_request("a1", "claude");
        req.env
            .insert("FORGE_LOOP_ID".to_string(), "loop-1".to_string());
        let _resp = svc.spawn_agent(Request::new(req)).unwrap();

        let calls = tmux.calls();
        let env_call = calls.iter().find(|c| {
            if let TmuxCall::SendKeys { keys, .. } = c {
                keys.contains("export FORGE_LOOP_ID")
            } else {
                false
            }
        });
        assert!(env_call.is_some());
    }

    #[test]
    fn spawn_agent_registers_in_agent_manager() {
        let svc = make_service(Arc::new(MockTmux::new().with_pane_pid(42)));
        svc.spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap();

        assert!(svc.agents.contains("a1"));
        let agent = svc.agents.get("a1").unwrap();
        assert_eq!(agent.state, AgentState::Starting);
        assert_eq!(agent.workspace_id, "ws1");
        assert_eq!(agent.pid, 42);
    }

    #[test]
    fn spawn_agent_session_check_failure_returns_internal() {
        let tmux = Arc::new(MockTmux::new().with_fail_method("has_session"));
        let svc = make_service(tmux);
        let err = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[test]
    fn spawn_agent_split_window_failure_returns_internal() {
        let tmux = Arc::new(
            MockTmux::new()
                .with_session_exists()
                .with_fail_method("split_window"),
        );
        let svc = make_service(tmux);
        let err = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[test]
    fn spawn_agent_pid_failure_continues_with_zero() {
        let tmux = Arc::new(MockTmux::new().with_fail_method("get_pane_pid"));
        let svc = make_service(tmux);
        let resp = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap()
            .into_inner();
        let agent = resp.agent.unwrap();
        assert_eq!(agent.pid, 0);
    }

    // -- KillAgent tests --

    #[test]
    fn kill_agent_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: String::new(),
                force: false,
                grace_period: None,
            }))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn kill_agent_not_found() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: "missing".to_string(),
                force: false,
                grace_period: None,
            }))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn kill_agent_graceful_sends_interrupt_then_kills() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let resp = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: "a1".to_string(),
                force: false,
                grace_period: None,
            }))
            .unwrap()
            .into_inner();
        assert!(resp.success);

        let calls = tmux.calls();
        assert!(calls.contains(&TmuxCall::SendInterrupt {
            pane_id: "sess:a1.0".to_string(),
        }));
        assert!(calls.contains(&TmuxCall::KillPane {
            pane_id: "sess:a1.0".to_string(),
        }));
        assert!(!svc.agents.contains("a1"));
    }

    #[test]
    fn kill_agent_force_skips_interrupt() {
        let tmux = Arc::new(MockTmux::new());
        let svc = make_service(tmux.clone());
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let resp = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: "a1".to_string(),
                force: true,
                grace_period: None,
            }))
            .unwrap()
            .into_inner();
        assert!(resp.success);

        let calls = tmux.calls();
        assert!(!calls
            .iter()
            .any(|c| matches!(c, TmuxCall::SendInterrupt { .. })));
        assert!(calls.contains(&TmuxCall::KillPane {
            pane_id: "sess:a1.0".to_string(),
        }));
        assert!(!svc.agents.contains("a1"));
    }

    #[test]
    fn kill_agent_removes_from_registry() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        assert_eq!(svc.agents.count(), 1);

        svc.kill_agent(Request::new(proto::KillAgentRequest {
            agent_id: "a1".to_string(),
            force: true,
            grace_period: None,
        }))
        .unwrap();
        assert_eq!(svc.agents.count(), 0);
    }

    #[test]
    fn spawn_then_kill_lifecycle() {
        let tmux = Arc::new(MockTmux::new().with_split_pane_id("forge-ws1:0.1"));
        let svc = make_service(tmux.clone());

        // Spawn
        let spawn_resp = svc
            .spawn_agent(Request::new(make_spawn_request("a1", "claude")))
            .unwrap()
            .into_inner();
        assert!(svc.agents.contains("a1"));
        assert_eq!(spawn_resp.pane_id, "forge-ws1:0.1");

        // Kill
        let kill_resp = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: "a1".to_string(),
                force: false,
                grace_period: None,
            }))
            .unwrap()
            .into_inner();
        assert!(kill_resp.success);
        assert!(!svc.agents.contains("a1"));

        // Kill again → NotFound
        let err = svc
            .kill_agent(Request::new(proto::KillAgentRequest {
                agent_id: "a1".to_string(),
                force: false,
                grace_period: None,
            }))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
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

    // -- LoopRunner RPC tests --

    fn make_loop_runner_manager_for_tests() -> LoopRunnerManager {
        LoopRunnerManager::with_command_builder(Arc::new(|_, _| {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", "sleep 60"]);
            cmd
        }))
    }

    #[test]
    fn start_loop_runner_requires_loop_id() {
        let svc = make_service_with_loop_runners(
            Arc::new(MockTmux::new()),
            make_loop_runner_manager_for_tests(),
        );
        let result = svc.start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
            loop_id: "   ".to_string(),
            config_path: String::new(),
            command_path: String::new(),
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn loop_runner_lifecycle_via_rpc_handlers() {
        let loop_runners = make_loop_runner_manager_for_tests();
        let svc = make_service_with_loop_runners(Arc::new(MockTmux::new()), loop_runners.clone());

        let start = svc
            .start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
                loop_id: " loop-a ".to_string(),
                config_path: " cfg.toml ".to_string(),
                command_path: String::new(),
            }))
            .unwrap()
            .into_inner();
        let started = start.runner.unwrap();
        assert_eq!(started.loop_id, "loop-a");
        assert_eq!(started.config_path, "cfg.toml");
        assert_eq!(started.command_path, "forge");
        assert_eq!(started.state, proto::LoopRunnerState::Running as i32);
        assert!(!started.instance_id.is_empty());

        let get = svc
            .get_loop_runner(Request::new(proto::GetLoopRunnerRequest {
                loop_id: "loop-a".to_string(),
            }))
            .unwrap()
            .into_inner();
        assert_eq!(get.runner.unwrap().loop_id, "loop-a");

        let list = svc
            .list_loop_runners(Request::new(proto::ListLoopRunnersRequest {}))
            .unwrap()
            .into_inner();
        assert_eq!(list.runners.len(), 1);
        assert_eq!(list.runners[0].loop_id, "loop-a");

        let stop = svc
            .stop_loop_runner(Request::new(proto::StopLoopRunnerRequest {
                loop_id: "loop-a".to_string(),
                force: true,
            }))
            .unwrap()
            .into_inner();
        assert!(stop.success);
        let stopped = stop.runner.unwrap();
        assert_eq!(stopped.loop_id, "loop-a");
        assert_eq!(stopped.state, proto::LoopRunnerState::Stopped as i32);
        assert!(stopped.stopped_at.is_some());

        loop_runners.stop_all_loop_runners(true);
    }

    #[test]
    fn start_loop_runner_duplicate_returns_already_exists() {
        let loop_runners = make_loop_runner_manager_for_tests();
        let svc = make_service_with_loop_runners(Arc::new(MockTmux::new()), loop_runners.clone());

        svc.start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
            loop_id: "loop-dupe".to_string(),
            config_path: String::new(),
            command_path: String::new(),
        }))
        .unwrap();

        let result = svc.start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
            loop_id: "loop-dupe".to_string(),
            config_path: String::new(),
            command_path: String::new(),
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::AlreadyExists);

        loop_runners.stop_all_loop_runners(true);
    }

    #[test]
    fn get_loop_runner_not_found() {
        let svc = make_service_with_loop_runners(
            Arc::new(MockTmux::new()),
            make_loop_runner_manager_for_tests(),
        );
        let result = svc.get_loop_runner(Request::new(proto::GetLoopRunnerRequest {
            loop_id: "missing".to_string(),
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn stop_loop_runner_not_found() {
        let svc = make_service_with_loop_runners(
            Arc::new(MockTmux::new()),
            make_loop_runner_manager_for_tests(),
        );
        let result = svc.stop_loop_runner(Request::new(proto::StopLoopRunnerRequest {
            loop_id: "missing".to_string(),
            force: true,
        }));
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn list_loop_runners_returns_sorted_ids() {
        let loop_runners = make_loop_runner_manager_for_tests();
        let svc = make_service_with_loop_runners(Arc::new(MockTmux::new()), loop_runners.clone());

        svc.start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
            loop_id: "b-loop".to_string(),
            config_path: String::new(),
            command_path: String::new(),
        }))
        .unwrap();
        svc.start_loop_runner(Request::new(proto::StartLoopRunnerRequest {
            loop_id: "a-loop".to_string(),
            config_path: String::new(),
            command_path: String::new(),
        }))
        .unwrap();

        let list = svc
            .list_loop_runners(Request::new(proto::ListLoopRunnersRequest {}))
            .unwrap()
            .into_inner();
        let ids: Vec<&str> = list
            .runners
            .iter()
            .map(|runner| runner.loop_id.as_str())
            .collect();
        assert_eq!(ids, vec!["a-loop", "b-loop"]);

        loop_runners.stop_all_loop_runners(true);
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

    #[test]
    fn stream_pane_updates_records_transcript_and_events() {
        let svc = make_service(Arc::new(MockTmux::with_capture("work complete\n$")));
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
                1,
            )
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].detected_state, proto::AgentState::Idle as i32);

        assert_eq!(svc.events.stored_count(), 2);

        let transcript = svc
            .get_transcript(Request::new(proto::GetTranscriptRequest {
                agent_id: "a1".to_string(),
                start_time: None,
                end_time: None,
                limit: 0,
            }))
            .unwrap()
            .into_inner();

        assert_eq!(transcript.entries.len(), 2);
        assert_eq!(
            transcript.entries[0].r#type,
            proto::TranscriptEntryType::Output as i32
        );
        assert!(transcript.entries[0].metadata.contains_key("content_hash"));
        assert_eq!(
            transcript.entries[1].r#type,
            proto::TranscriptEntryType::StateChange as i32
        );
        assert_eq!(transcript.entries[1].content, "idle");
    }

    // -- StreamEvents tests --

    #[test]
    fn stream_events_rejects_invalid_cursor() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let result = svc.stream_events(
            Request::new(proto::StreamEventsRequest {
                cursor: "abc".to_string(),
                types: vec![],
                agent_ids: vec![],
                workspace_ids: vec![],
            }),
            1,
        );
        let err = result.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn stream_events_replay_and_cursor_progression() {
        let svc = make_service(Arc::new(MockTmux::new()));
        svc.events
            .publish_agent_state_changed("a1", "ws1", 0, 1, "first");
        svc.events
            .publish_agent_state_changed("a2", "ws1", 0, 1, "second");

        let from_now = svc
            .stream_events(
                Request::new(proto::StreamEventsRequest {
                    cursor: String::new(),
                    types: vec![],
                    agent_ids: vec![],
                    workspace_ids: vec![],
                }),
                1,
            )
            .unwrap();
        assert!(from_now.is_empty());

        let replay = svc
            .stream_events(
                Request::new(proto::StreamEventsRequest {
                    cursor: "1".to_string(),
                    types: vec![],
                    agent_ids: vec![],
                    workspace_ids: vec![],
                }),
                1,
            )
            .unwrap();

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event.as_ref().unwrap().id, "1");

        let events = Arc::clone(&svc.events);
        let publisher = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            events.publish_agent_state_changed("a3", "ws2", 0, 1, "live");
        });

        let live = svc
            .stream_events(
                Request::new(proto::StreamEventsRequest {
                    cursor: String::new(),
                    types: vec![],
                    agent_ids: vec![],
                    workspace_ids: vec![],
                }),
                3,
            )
            .unwrap();
        publisher.join().unwrap();

        assert_eq!(live.len(), 1);
        assert_eq!(live[0].event.as_ref().unwrap().agent_id, "a3");
        assert_eq!(live[0].event.as_ref().unwrap().id, "2");
    }

    #[test]
    fn stream_events_filters_by_agent_workspace_and_type() {
        let svc = make_service(Arc::new(MockTmux::new()));
        svc.events
            .publish_agent_state_changed("seed", "ws0", 0, 1, "seed");
        svc.events
            .publish_agent_state_changed("a1", "ws1", 0, 1, "a1");
        svc.events
            .publish_error("a2", "ws2", "ERR", "a2 error", true);

        let by_agent = svc
            .stream_events(
                Request::new(proto::StreamEventsRequest {
                    cursor: "1".to_string(),
                    types: vec![],
                    agent_ids: vec!["a1".to_string()],
                    workspace_ids: vec![],
                }),
                1,
            )
            .unwrap();
        assert_eq!(by_agent.len(), 1);
        assert_eq!(by_agent[0].event.as_ref().unwrap().agent_id, "a1");

        let by_workspace_and_type = svc
            .stream_events(
                Request::new(proto::StreamEventsRequest {
                    cursor: "1".to_string(),
                    types: vec![proto::EventType::Error as i32],
                    agent_ids: vec![],
                    workspace_ids: vec!["ws2".to_string()],
                }),
                1,
            )
            .unwrap();
        assert_eq!(by_workspace_and_type.len(), 1);
        let evt = by_workspace_and_type[0].event.as_ref().unwrap();
        assert_eq!(evt.workspace_id, "ws2");
        assert_eq!(evt.r#type, proto::EventType::Error as i32);
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

    // -- Transcript RPC parity tests --

    #[test]
    fn get_transcript_requires_agent_id() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .get_transcript(Request::new(proto::GetTranscriptRequest {
                agent_id: String::new(),
                start_time: None,
                end_time: None,
                limit: 0,
            }))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn get_transcript_not_found() {
        let svc = make_service(Arc::new(MockTmux::new()));
        let err = svc
            .get_transcript(Request::new(proto::GetTranscriptRequest {
                agent_id: "missing".to_string(),
                start_time: None,
                end_time: None,
                limit: 0,
            }))
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[test]
    fn get_transcript_limit_and_cursor_match_go_shape() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        for idx in 0..3 {
            svc.agents.add_transcript_entry_full(
                "a1",
                TranscriptEntry {
                    entry_type: TranscriptEntryType::Output,
                    content: format!("line-{idx}"),
                    timestamp: Utc::now(),
                    metadata: HashMap::new(),
                },
            );
        }

        let resp = svc
            .get_transcript(Request::new(proto::GetTranscriptRequest {
                agent_id: "a1".to_string(),
                start_time: None,
                end_time: None,
                limit: 2,
            }))
            .unwrap()
            .into_inner();

        assert_eq!(resp.agent_id, "a1");
        assert_eq!(resp.entries.len(), 2);
        assert!(resp.has_more);
        assert_eq!(resp.next_cursor, "2");
        assert_eq!(resp.entries[0].content, "line-0");
        assert_eq!(resp.entries[1].content, "line-1");
    }

    #[test]
    fn get_transcript_applies_time_filters_inclusive() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        let t1 = Utc.with_ymd_and_hms(2026, 2, 9, 18, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 2, 9, 18, 1, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 2, 9, 18, 2, 0).unwrap();

        svc.agents.add_transcript_entry_full(
            "a1",
            TranscriptEntry {
                entry_type: TranscriptEntryType::Output,
                content: "one".to_string(),
                timestamp: t1,
                metadata: HashMap::new(),
            },
        );
        svc.agents.add_transcript_entry_full(
            "a1",
            TranscriptEntry {
                entry_type: TranscriptEntryType::Output,
                content: "two".to_string(),
                timestamp: t2,
                metadata: HashMap::new(),
            },
        );
        svc.agents.add_transcript_entry_full(
            "a1",
            TranscriptEntry {
                entry_type: TranscriptEntryType::Output,
                content: "three".to_string(),
                timestamp: t3,
                metadata: HashMap::new(),
            },
        );

        let resp = svc
            .get_transcript(Request::new(proto::GetTranscriptRequest {
                agent_id: "a1".to_string(),
                start_time: Some(datetime_to_timestamp(t2)),
                end_time: Some(datetime_to_timestamp(t2)),
                limit: 0,
            }))
            .unwrap()
            .into_inner();

        assert_eq!(resp.entries.len(), 1);
        assert_eq!(resp.entries[0].content, "two");
    }

    #[test]
    fn stream_transcript_emits_only_when_new_entries_exist() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);

        svc.agents
            .add_transcript_entry("a1", TranscriptEntryType::Output, "hello world");
        svc.agents
            .add_transcript_entry("a1", TranscriptEntryType::Output, "second");

        let updates = svc
            .stream_transcript(
                Request::new(proto::StreamTranscriptRequest {
                    agent_id: "a1".to_string(),
                    cursor: String::new(),
                }),
                1,
            )
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].entries.len(), 2);
        assert_eq!(updates[0].cursor, "2");

        let updates2 = svc
            .stream_transcript(
                Request::new(proto::StreamTranscriptRequest {
                    agent_id: "a1".to_string(),
                    cursor: "2".to_string(),
                }),
                1,
            )
            .unwrap();
        assert!(updates2.is_empty());
    }

    #[test]
    fn stream_transcript_invalid_cursor_is_invalid_argument() {
        let svc = make_service(Arc::new(MockTmux::new()));
        register_agent(&svc, "a1", "ws1", AgentState::Running);
        let err = svc
            .stream_transcript(
                Request::new(proto::StreamTranscriptRequest {
                    agent_id: "a1".to_string(),
                    cursor: "bad".to_string(),
                }),
                1,
            )
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }
}

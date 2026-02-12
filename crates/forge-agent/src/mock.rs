//! Mock agent service for unit testing.
//!
//! Provides a configurable mock that records all calls and returns
//! pre-configured responses.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;

use crate::error::AgentServiceError;
use crate::service::AgentService;
use crate::types::{
    AgentSnapshot, AgentState, KillAgentParams, ListAgentsFilter, SendMessageParams,
    SpawnAgentParams, WaitStateParams,
};

/// A recorded call to the mock service.
#[derive(Debug, Clone)]
pub enum MockCall {
    Spawn(SpawnAgentParams),
    SendMessage(SendMessageParams),
    WaitState(WaitStateParams),
    Interrupt(String),
    Kill(KillAgentParams),
    ListAgents(ListAgentsFilter),
    GetAgent(String),
}

/// Mock implementation of `AgentService` for testing.
pub struct MockAgentService {
    agents: Mutex<HashMap<String, AgentSnapshot>>,
    calls: Mutex<Vec<MockCall>>,
    spawn_error: Mutex<Option<AgentServiceError>>,
    send_error: Mutex<Option<AgentServiceError>>,
    kill_error: Mutex<Option<AgentServiceError>>,
    get_error: Mutex<Option<AgentServiceError>>,
}

impl Default for MockAgentService {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAgentService {
    pub fn new() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            spawn_error: Mutex::new(None),
            send_error: Mutex::new(None),
            kill_error: Mutex::new(None),
            get_error: Mutex::new(None),
        }
    }

    /// Pre-populate an agent in the mock registry.
    pub fn with_agent(self, snapshot: AgentSnapshot) -> Self {
        match self.agents.lock() {
            Ok(mut agents) => {
                agents.insert(snapshot.id.clone(), snapshot);
            }
            Err(poisoned) => {
                poisoned.into_inner().insert(snapshot.id.clone(), snapshot);
            }
        }
        self
    }

    /// Configure spawn to return an error.
    pub fn with_spawn_error(self, err: AgentServiceError) -> Self {
        match self.spawn_error.lock() {
            Ok(mut e) => *e = Some(err),
            Err(poisoned) => *poisoned.into_inner() = Some(err),
        }
        self
    }

    /// Configure send_message to return an error.
    pub fn with_send_error(self, err: AgentServiceError) -> Self {
        match self.send_error.lock() {
            Ok(mut e) => *e = Some(err),
            Err(poisoned) => *poisoned.into_inner() = Some(err),
        }
        self
    }

    /// Configure kill to return an error.
    pub fn with_kill_error(self, err: AgentServiceError) -> Self {
        match self.kill_error.lock() {
            Ok(mut e) => *e = Some(err),
            Err(poisoned) => *poisoned.into_inner() = Some(err),
        }
        self
    }

    /// Configure get_agent to return an error.
    pub fn with_get_error(self, err: AgentServiceError) -> Self {
        match self.get_error.lock() {
            Ok(mut e) => *e = Some(err),
            Err(poisoned) => *poisoned.into_inner() = Some(err),
        }
        self
    }

    /// Return all recorded calls.
    pub fn calls(&self) -> Vec<MockCall> {
        match self.calls.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Return the number of recorded calls.
    pub fn call_count(&self) -> usize {
        match self.calls.lock() {
            Ok(guard) => guard.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }

    fn record(&self, call: MockCall) {
        match self.calls.lock() {
            Ok(mut guard) => guard.push(call),
            Err(poisoned) => poisoned.into_inner().push(call),
        }
    }

    fn take_error(lock: &Mutex<Option<AgentServiceError>>) -> Option<AgentServiceError> {
        match lock.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }
}

/// Helper to create a test snapshot with sensible defaults.
pub fn test_snapshot(id: &str, state: AgentState) -> AgentSnapshot {
    let now = Utc::now();
    AgentSnapshot {
        id: id.to_string(),
        workspace_id: "test-ws".to_string(),
        state,
        pane_id: format!("{id}:0.0"),
        pid: 1000,
        command: "claude".to_string(),
        adapter: "claude_code".to_string(),
        spawned_at: now,
        last_activity_at: now,
    }
}

#[async_trait]
impl AgentService for MockAgentService {
    async fn spawn_agent(
        &self,
        params: SpawnAgentParams,
    ) -> Result<AgentSnapshot, AgentServiceError> {
        self.record(MockCall::Spawn(params.clone()));

        if let Some(err) = Self::take_error(&self.spawn_error) {
            return Err(err);
        }

        // Check for duplicate.
        {
            let agents = match self.agents.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if agents.contains_key(&params.agent_id) {
                return Err(AgentServiceError::AlreadyExists {
                    agent_id: params.agent_id,
                });
            }
        }

        let now = Utc::now();
        let snapshot = AgentSnapshot {
            id: params.agent_id.clone(),
            workspace_id: params.workspace_id,
            state: AgentState::Starting,
            pane_id: format!("{}:0.0", params.agent_id),
            pid: 0,
            command: params.command,
            adapter: params.adapter,
            spawned_at: now,
            last_activity_at: now,
        };

        match self.agents.lock() {
            Ok(mut agents) => {
                agents.insert(snapshot.id.clone(), snapshot.clone());
            }
            Err(poisoned) => {
                poisoned
                    .into_inner()
                    .insert(snapshot.id.clone(), snapshot.clone());
            }
        }

        Ok(snapshot)
    }

    async fn send_message(&self, params: SendMessageParams) -> Result<bool, AgentServiceError> {
        self.record(MockCall::SendMessage(params.clone()));

        if let Some(err) = Self::take_error(&self.send_error) {
            return Err(err);
        }

        // Verify agent exists.
        let agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !agents.contains_key(&params.agent_id) {
            return Err(AgentServiceError::NotFound {
                agent_id: params.agent_id,
            });
        }

        Ok(true)
    }

    async fn wait_state(
        &self,
        params: WaitStateParams,
    ) -> Result<AgentSnapshot, AgentServiceError> {
        self.record(MockCall::WaitState(params.clone()));

        let agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let agent = agents
            .get(&params.agent_id)
            .ok_or_else(|| AgentServiceError::NotFound {
                agent_id: params.agent_id.clone(),
            })?;

        if params.target_states.contains(&agent.state) {
            return Ok(agent.clone());
        }

        Err(AgentServiceError::WaitTimeout {
            agent_id: params.agent_id,
            target_state: params
                .target_states
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join("|"),
            last_observed_state: agent.state.to_string(),
        })
    }

    async fn interrupt_agent(&self, agent_id: &str) -> Result<bool, AgentServiceError> {
        self.record(MockCall::Interrupt(agent_id.to_string()));

        let agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !agents.contains_key(agent_id) {
            return Err(AgentServiceError::NotFound {
                agent_id: agent_id.to_string(),
            });
        }

        Ok(true)
    }

    async fn kill_agent(&self, params: KillAgentParams) -> Result<bool, AgentServiceError> {
        self.record(MockCall::Kill(params.clone()));

        if let Some(err) = Self::take_error(&self.kill_error) {
            return Err(err);
        }

        let mut agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if agents.remove(&params.agent_id).is_none() {
            return Err(AgentServiceError::NotFound {
                agent_id: params.agent_id,
            });
        }

        Ok(true)
    }

    async fn list_agents(
        &self,
        filter: ListAgentsFilter,
    ) -> Result<Vec<AgentSnapshot>, AgentServiceError> {
        self.record(MockCall::ListAgents(filter.clone()));

        let agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let mut result: Vec<AgentSnapshot> = agents
            .values()
            .filter(|a| {
                if let Some(ref ws) = filter.workspace_id {
                    if !ws.is_empty() && a.workspace_id != *ws {
                        return false;
                    }
                }
                if !filter.states.is_empty() && !filter.states.contains(&a.state) {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }

    async fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, AgentServiceError> {
        self.record(MockCall::GetAgent(agent_id.to_string()));

        if let Some(err) = Self::take_error(&self.get_error) {
            return Err(err);
        }

        let agents = match self.agents.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| AgentServiceError::NotFound {
                agent_id: agent_id.to_string(),
            })
    }
}

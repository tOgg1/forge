//! Agent service trait — the primary abstraction for agent lifecycle operations.
//!
//! Implementations can run against a local forged daemon (gRPC) or be mocked
//! for testing.

use async_trait::async_trait;

use crate::error::AgentServiceError;
use crate::types::{
    AgentSnapshot, KillAgentParams, ListAgentsFilter, SendMessageParams, SpawnAgentParams,
    WaitStateParams,
};

/// The agent service interface.
///
/// All operations are async to support both local daemon calls and future
/// remote/distributed transports. Each method maps to one of the M10.3
/// acceptance-criteria operations.
#[async_trait]
pub trait AgentService: Send + Sync {
    /// Spawn a new agent. Returns the initial snapshot.
    async fn spawn_agent(
        &self,
        params: SpawnAgentParams,
    ) -> Result<AgentSnapshot, AgentServiceError>;

    /// Send a message (text/keys) to a running agent.
    async fn send_message(&self, params: SendMessageParams) -> Result<bool, AgentServiceError>;

    /// Wait for an agent to reach one of the target states.
    /// Returns the snapshot when the target state is reached, or a
    /// `WaitTimeout` error if the timeout expires first.
    async fn wait_state(&self, params: WaitStateParams)
        -> Result<AgentSnapshot, AgentServiceError>;

    /// Interrupt an agent (send Ctrl+C).
    async fn interrupt_agent(&self, agent_id: &str) -> Result<bool, AgentServiceError>;

    /// Kill an agent (terminate its process).
    async fn kill_agent(&self, params: KillAgentParams) -> Result<bool, AgentServiceError>;

    /// List agents with optional filters.
    async fn list_agents(
        &self,
        filter: ListAgentsFilter,
    ) -> Result<Vec<AgentSnapshot>, AgentServiceError>;

    /// Get a single agent by id.
    async fn get_agent(&self, agent_id: &str) -> Result<AgentSnapshot, AgentServiceError>;
}

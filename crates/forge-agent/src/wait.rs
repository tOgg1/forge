//! Wait semantics and state synchronization for persistent agents.
//!
//! Provides a `StateStream` trait for receiving agent state updates
//! (event-stream or polling fallback) and a `wait_for_state` function
//! that consumes the stream until the target state is reached, the
//! agent enters a terminal state, the timeout expires, or the
//! operation is cancelled.

use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::error::AgentServiceError;
use crate::types::{AgentSnapshot, AgentState};

/// A stream of agent state snapshots.
///
/// Implementations can be backed by:
/// - gRPC server-side streaming (event-stream)
/// - polling via `get_agent` calls (polling fallback)
/// - in-memory sequences for testing
#[async_trait]
pub trait StateStream: Send {
    /// Wait for and return the next state snapshot.
    ///
    /// Returns `None` when the stream is exhausted (agent gone, connection
    /// closed, etc.).
    async fn next(&mut self) -> Option<Result<AgentSnapshot, AgentServiceError>>;
}

/// Polling-based `StateStream` backed by an `AgentService`.
///
/// Calls `get_agent` at a fixed interval to produce snapshots.
pub struct PollingStateStream<S: crate::service::AgentService> {
    service: std::sync::Arc<S>,
    agent_id: String,
    poll_interval: Duration,
    first: bool,
}

impl<S: crate::service::AgentService> PollingStateStream<S> {
    pub fn new(service: std::sync::Arc<S>, agent_id: String, poll_interval: Duration) -> Self {
        Self {
            service,
            agent_id,
            poll_interval,
            first: true,
        }
    }
}

#[async_trait]
impl<S: crate::service::AgentService + 'static> StateStream for PollingStateStream<S> {
    async fn next(&mut self) -> Option<Result<AgentSnapshot, AgentServiceError>> {
        if self.first {
            self.first = false;
        } else {
            tokio::time::sleep(self.poll_interval).await;
        }
        Some(self.service.get_agent(&self.agent_id).await)
    }
}

/// In-memory state stream for testing.
///
/// Yields pre-configured snapshots in order, with optional delays
/// between them to simulate real state transitions.
pub struct MockStateStream {
    snapshots: Vec<(Duration, Result<AgentSnapshot, AgentServiceError>)>,
    index: usize,
}

impl MockStateStream {
    /// Create a stream that yields snapshots immediately.
    pub fn from_snapshots(snapshots: Vec<Result<AgentSnapshot, AgentServiceError>>) -> Self {
        Self {
            snapshots: snapshots.into_iter().map(|s| (Duration::ZERO, s)).collect(),
            index: 0,
        }
    }

    /// Create a stream with explicit delays before each snapshot.
    pub fn with_delays(
        snapshots: Vec<(Duration, Result<AgentSnapshot, AgentServiceError>)>,
    ) -> Self {
        Self {
            snapshots,
            index: 0,
        }
    }
}

#[async_trait]
impl StateStream for MockStateStream {
    async fn next(&mut self) -> Option<Result<AgentSnapshot, AgentServiceError>> {
        if self.index >= self.snapshots.len() {
            return None;
        }
        let (delay, ref result) = self.snapshots[self.index];
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let out = result.clone();
        self.index += 1;
        Some(out)
    }
}

/// Wait for an agent to reach one of the target states.
///
/// Consumes snapshots from `stream` until:
/// - The agent reaches a state in `target_states` → returns the snapshot.
/// - The agent enters a terminal state not in `target_states` → returns `InvalidState`.
/// - The `timeout` expires → returns `WaitTimeout`.
/// - The `cancel` token is triggered → returns `WaitCancelled`.
/// - The stream ends unexpectedly → returns `NotFound`.
pub async fn wait_for_state(
    agent_id: &str,
    target_states: &[AgentState],
    timeout: Duration,
    cancel: CancellationToken,
    stream: &mut dyn StateStream,
) -> Result<AgentSnapshot, AgentServiceError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_observed_state = AgentState::Unspecified;

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(AgentServiceError::WaitCancelled {
                    agent_id: agent_id.to_string(),
                    last_observed_state: last_observed_state.to_string(),
                });
            }
            _ = tokio::time::sleep_until(deadline) => {
                let target_str = target_states
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("|");
                return Err(AgentServiceError::WaitTimeout {
                    agent_id: agent_id.to_string(),
                    target_state: target_str,
                    last_observed_state: last_observed_state.to_string(),
                });
            }
            next = stream.next() => {
                match next {
                    Some(Ok(snapshot)) => {
                        last_observed_state = snapshot.state;

                        if target_states.contains(&snapshot.state) {
                            return Ok(snapshot);
                        }

                        if snapshot.state.is_terminal() {
                            return Err(AgentServiceError::InvalidState {
                                agent_id: agent_id.to_string(),
                                current_state: snapshot.state.to_string(),
                                operation: "wait_state".into(),
                            });
                        }
                    }
                    Some(Err(e)) => return Err(e),
                    None => {
                        return Err(AgentServiceError::NotFound {
                            agent_id: agent_id.to_string(),
                        });
                    }
                }
            }
        }
    }
}

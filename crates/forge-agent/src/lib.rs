//! forge-agent: Agent service abstraction for persistent agent lifecycle operations.
//!
//! Provides a transport-agnostic `AgentService` trait with implementations for:
//! - `ForgedTransport`: gRPC-backed service using the forged daemon
//! - `MockAgentService`: Configurable mock for unit testing
//!
//! Each operation emits an `AgentEvent` for audit/debugging via the `AgentEventSink` trait.

pub mod error;
pub mod event;
pub mod forged;
pub mod mock;
pub mod service;
pub mod types;

/// Stable crate label used for bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-agent"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-agent");
    }
}

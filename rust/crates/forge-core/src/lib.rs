//! forge-core: domain types, policies, and validation for the Forge system.
//!
//! This crate contains the foundational domain model shared across all Forge
//! crates. It defines entities (Node, Workspace, Loop, Agent, etc.),
//! configuration types, event types, error/validation framework, and repository
//! traits.

pub mod config;
pub mod error;
pub mod event;
pub mod models;
pub mod queue;
pub mod validation;

/// Crate identity label used for parity verification.
pub fn crate_label() -> &'static str {
    "forge-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-core");
    }

    #[test]
    fn modules_are_accessible() {
        // Verify all public modules compile and are reachable.
        let _ = models::LoopState::Running;
        let _ = models::AgentState::Idle;
        let _ = event::EventType::NodeCreated;
        let _ = event::EntityType::Node;
        let _ = config::Config::default();
        let _ = queue::LoopQueueItemType::Message;
        let _ = error::ForgeError::Validation("test".into());
        let _ = validation::ValidationErrors::new();
    }
}

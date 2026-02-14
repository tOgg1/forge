//! forge-daemon: Rust daemon surface for Forge.

pub mod agent;
pub mod bootstrap;
pub mod events;
pub mod loop_runner;
pub mod node_registry;
pub mod server;
pub mod status;
pub mod tmux;
pub mod transcript;

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-daemon"
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-daemon");
    }
}

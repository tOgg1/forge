//! forge-loop: loop runtime primitives and orchestration entrypoints.

pub mod harness_wrapper;
pub mod ledger_writer;
pub mod log_io;
pub mod log_tail;
pub mod profile_selection;
pub mod prompt_composition;
pub mod queue_interactions;
pub mod runtime_limits;
pub mod stale_runner;
pub mod state_machine;
pub mod stop_rules;

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-loop"
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-loop");
    }
}

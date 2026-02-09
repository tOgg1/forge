//! forge-loop: loop runtime primitives and orchestration entrypoints.

pub mod profile_selection;
pub mod prompt_composition;
pub mod queue_interactions;
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

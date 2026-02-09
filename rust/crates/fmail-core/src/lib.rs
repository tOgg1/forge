pub fn crate_label() -> &'static str {
    "fmail-core"
}

pub mod agent_registry;
pub mod constants;
pub mod format;
pub mod names;
pub mod root;
pub mod store;
pub mod validate;

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "fmail-core");
    }
}

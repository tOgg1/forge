//! forge-ftui-adapter: boundary layer around FrankenTUI integration points.

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-ftui-adapter"
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-ftui-adapter");
    }
}

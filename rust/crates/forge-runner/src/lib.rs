pub fn crate_label() -> &'static str {
    "forge-runner"
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-runner");
    }
}

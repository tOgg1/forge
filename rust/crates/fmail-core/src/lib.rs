pub fn crate_label() -> &'static str {
    "fmail-core"
}

#[cfg(test)]
mod tests {
    use super::crate_label;

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "fmail-core");
    }
}

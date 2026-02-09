pub fn parity_stub_label() -> &'static str {
    "forge-rust-parity-stub"
}

#[cfg(test)]
mod tests {
    use super::parity_stub_label;

    #[test]
    fn label_is_stable() {
        assert_eq!(parity_stub_label(), "forge-rust-parity-stub");
    }
}

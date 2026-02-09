//! forge-tui: terminal user interface surface for Forge operators.

use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-tui"
}

/// Forge TUI default theme comes from the local FrankenTUI adapter abstraction.
#[must_use]
pub fn default_theme() -> ThemeSpec {
    ThemeSpec::for_kind(ThemeKind::Dark)
}

#[cfg(test)]
mod tests {
    use super::{crate_label, default_theme};
    use forge_ftui_adapter::style::{StyleToken, ThemeKind};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-tui");
    }

    #[test]
    fn uses_adapter_theme_abstraction() {
        let theme = default_theme();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.color(StyleToken::Accent), 39);
    }
}

//! forge-ftui-adapter: boundary layer around FrankenTUI integration points.
//!
//! This crate keeps TUI crates insulated from direct FrankenTUI style/theme APIs.
//! Only this local abstraction is imported by app crates.

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-ftui-adapter"
}

/// Style and theme primitives consumed by Forge TUI crates.
pub mod style {
    /// Logical theme choices supported by the adapter.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ThemeKind {
        Dark,
        Light,
        HighContrast,
    }

    /// Stable style tokens exposed to application crates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StyleToken {
        Background,
        Surface,
        Foreground,
        Muted,
        Accent,
        Success,
        Danger,
    }

    /// Adapter palette uses terminal 256-color indexes for portability.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Palette {
        pub background: u8,
        pub surface: u8,
        pub foreground: u8,
        pub muted: u8,
        pub accent: u8,
        pub success: u8,
        pub danger: u8,
    }

    /// Theme specification exposed to target TUI crates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ThemeSpec {
        pub kind: ThemeKind,
        pub palette: Palette,
    }

    impl ThemeSpec {
        /// Returns the color index for a stable style token.
        #[must_use]
        pub fn color(self, token: StyleToken) -> u8 {
            match token {
                StyleToken::Background => self.palette.background,
                StyleToken::Surface => self.palette.surface,
                StyleToken::Foreground => self.palette.foreground,
                StyleToken::Muted => self.palette.muted,
                StyleToken::Accent => self.palette.accent,
                StyleToken::Success => self.palette.success,
                StyleToken::Danger => self.palette.danger,
            }
        }
    }

    impl Default for ThemeSpec {
        fn default() -> Self {
            Self::for_kind(ThemeKind::Dark)
        }
    }

    impl ThemeSpec {
        /// Builds a theme for the requested style family.
        #[must_use]
        pub fn for_kind(kind: ThemeKind) -> Self {
            let palette = match kind {
                ThemeKind::Dark => Palette {
                    background: 16,
                    surface: 235,
                    foreground: 252,
                    muted: 244,
                    accent: 39,
                    success: 41,
                    danger: 196,
                },
                ThemeKind::Light => Palette {
                    background: 255,
                    surface: 252,
                    foreground: 234,
                    muted: 244,
                    accent: 25,
                    success: 28,
                    danger: 160,
                },
                ThemeKind::HighContrast => Palette {
                    background: 16,
                    surface: 232,
                    foreground: 231,
                    muted: 250,
                    accent: 51,
                    success: 118,
                    danger: 203,
                },
            };
            Self { kind, palette }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::crate_label;
    use super::style::{StyleToken, ThemeKind, ThemeSpec};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-ftui-adapter");
    }

    #[test]
    fn default_theme_is_dark() {
        let theme = ThemeSpec::default();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.color(StyleToken::Accent), 39);
    }

    #[test]
    fn high_contrast_theme_snapshot() {
        let theme = ThemeSpec::for_kind(ThemeKind::HighContrast);
        let snapshot = format!(
            "kind={:?} bg={} surface={} fg={} muted={} accent={} success={} danger={}",
            theme.kind,
            theme.color(StyleToken::Background),
            theme.color(StyleToken::Surface),
            theme.color(StyleToken::Foreground),
            theme.color(StyleToken::Muted),
            theme.color(StyleToken::Accent),
            theme.color(StyleToken::Success),
            theme.color(StyleToken::Danger),
        );
        assert_eq!(
            snapshot,
            "kind=HighContrast bg=16 surface=232 fg=231 muted=250 accent=51 success=118 danger=203"
        );
    }
}

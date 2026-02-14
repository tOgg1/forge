//! forge-ftui-adapter: boundary layer around FrankenTUI integration points.
//!
//! This crate keeps TUI crates insulated from direct FrankenTUI style/theme APIs.
//! Only this local abstraction is imported by app crates.

/// Stable crate label used by bootstrap smoke tests.
pub fn crate_label() -> &'static str {
    "forge-ftui-adapter"
}

/// Pinned upstream FrankenTUI commit used for reproducible adapter integration.
pub const FRANKENTUI_PIN: &str = "23429fac0e739635c7b8e0b995bde09401ff6ea0";

#[cfg(feature = "frankentui-upstream")]
pub use ftui as upstream_ftui;

/// Upstream FrankenTUI bridge helpers used by rewrite runtime/view code.
#[cfg(feature = "frankentui-upstream")]
pub mod upstream_bridge {
    use super::render::{CellStyle, TermColor};
    use super::style::{StyleToken, ThemeSpec};
    use super::upstream_ftui as ftui;

    /// Convert adapter terminal color into upstream ftui color.
    #[must_use]
    pub fn term_color_to_ftui_color(color: TermColor) -> ftui::Color {
        match color {
            TermColor::Ansi256(idx) => ftui::Color::Ansi256(idx),
            TermColor::Rgb(r, g, b) => ftui::Color::rgb(r, g, b),
        }
    }

    /// Convert adapter terminal color into ftui PackedRgba for style pipelines.
    #[must_use]
    pub fn term_color_to_packed_rgba(color: TermColor) -> ftui::render::cell::PackedRgba {
        match term_color_to_ftui_color(color) {
            ftui::Color::Rgb(rgb) => ftui::render::cell::PackedRgba::rgb(rgb.r, rgb.g, rgb.b),
            ftui::Color::Ansi256(idx) => {
                let rgb = ftui::Color::Ansi256(idx).to_rgb();
                ftui::render::cell::PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
            }
            ftui::Color::Ansi16(ansi16) => {
                let rgb = ftui::Color::Ansi16(ansi16).to_rgb();
                ftui::render::cell::PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
            }
            ftui::Color::Mono(mono) => {
                let rgb = ftui::Color::Mono(mono).to_rgb();
                ftui::render::cell::PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
            }
        }
    }

    /// Convert adapter cell style into upstream ftui style.
    #[must_use]
    pub fn cell_style_to_ftui_style(style: CellStyle) -> ftui::Style {
        let mut attrs = ftui::StyleFlags::NONE;
        if style.bold {
            attrs.insert(ftui::StyleFlags::BOLD);
        }
        if style.dim {
            attrs.insert(ftui::StyleFlags::DIM);
        }
        if style.underline {
            attrs.insert(ftui::StyleFlags::UNDERLINE);
        }

        let mut out = ftui::Style::new()
            .fg(term_color_to_packed_rgba(style.fg))
            .bg(term_color_to_packed_rgba(style.bg));

        if !attrs.is_empty() {
            out = out.attrs(attrs);
        }

        out
    }

    /// Build an upstream style for one semantic theme token.
    #[must_use]
    pub fn token_style(theme: ThemeSpec, token: StyleToken) -> ftui::Style {
        let fg = term_color_to_packed_rgba(TermColor::Ansi256(theme.color(token)));
        let bg = term_color_to_packed_rgba(TermColor::Ansi256(theme.color(StyleToken::Background)));
        let mut attrs = ftui::StyleFlags::NONE;

        match token {
            StyleToken::Accent if theme.typography.accent_bold => {
                attrs.insert(ftui::StyleFlags::BOLD)
            }
            StyleToken::Success if theme.typography.success_bold => {
                attrs.insert(ftui::StyleFlags::BOLD)
            }
            StyleToken::Danger if theme.typography.danger_bold => {
                attrs.insert(ftui::StyleFlags::BOLD)
            }
            StyleToken::Warning if theme.typography.warning_bold => {
                attrs.insert(ftui::StyleFlags::BOLD)
            }
            StyleToken::Muted if theme.typography.muted_dim => attrs.insert(ftui::StyleFlags::DIM),
            StyleToken::Focus if theme.typography.focus_underline => {
                attrs.insert(ftui::StyleFlags::UNDERLINE)
            }
            _ => {}
        }

        let mut style = ftui::Style::new().fg(fg).bg(bg);
        if !attrs.is_empty() {
            style = style.attrs(attrs);
        }
        style
    }

    /// Resolve a [`SpanStyle`] to an ftui [`Style`] using the given theme.
    #[must_use]
    pub fn span_style_to_ftui_style(
        theme: super::style::ThemeSpec,
        span_style: super::render::SpanStyle,
    ) -> ftui::Style {
        use super::render::{SpanStyle, TextRole};
        match span_style {
            SpanStyle::Cell(cs) => cell_style_to_ftui_style(cs),
            SpanStyle::Token(token) => token_style(theme, token),
            SpanStyle::Role(role) => {
                let token = match role {
                    TextRole::Primary => StyleToken::Foreground,
                    TextRole::Muted => StyleToken::Muted,
                    TextRole::Accent => StyleToken::Accent,
                    TextRole::Success => StyleToken::Success,
                    TextRole::Danger => StyleToken::Danger,
                    TextRole::Warning => StyleToken::Warning,
                    TextRole::Info => StyleToken::Info,
                    TextRole::Focus => StyleToken::Focus,
                };
                token_style(theme, token)
            }
        }
    }

    /// Convert a [`StyledSpan`] into an ftui [`Span`].
    #[must_use]
    pub fn styled_span_to_ftui_span<'a>(
        theme: super::style::ThemeSpec,
        span: &super::render::StyledSpan<'a>,
    ) -> ftui::text::Span<'a> {
        let style = span_style_to_ftui_style(theme, span.style);
        ftui::text::Span::styled(span.text, style)
    }

    /// Convert a [`StyledLine`] into an ftui [`Line`].
    #[must_use]
    pub fn styled_line_to_ftui_line(
        theme: super::style::ThemeSpec,
        line: &super::render::StyledLine,
    ) -> ftui::text::Line {
        let spans: Vec<ftui::text::Span<'static>> = line
            .spans
            .iter()
            .map(|owned| {
                let style = span_style_to_ftui_style(theme, owned.style);
                ftui::text::Span::styled(owned.text.clone(), style)
            })
            .collect();
        ftui::text::Line::from_spans(spans)
    }

    /// Convert a [`StyledText`] into an ftui [`Text`].
    #[must_use]
    pub fn styled_text_to_ftui_text(
        theme: super::style::ThemeSpec,
        text: &super::render::StyledText,
    ) -> ftui::text::Text {
        let mut out = ftui::text::Text::default();
        for line in &text.lines {
            out.push_line(styled_line_to_ftui_line(theme, line));
        }
        out
    }
}

/// Re-export supporting upstream widgets used by the rewrite path.
#[cfg(feature = "frankentui-upstream")]
pub mod upstream_widgets {
    use super::upstream_ftui as ftui;

    pub use ftui::widgets::list::{List, ListItem, ListState};
    pub use ftui::widgets::log_viewer::{LogViewer, LogViewerState, LogWrapMode};
    pub use ftui::widgets::modal::{
        Modal, ModalConfig, ModalPosition, ModalSizeConstraints, ModalState,
    };
    pub use ftui::widgets::notification_queue::{NotificationQueue, QueueConfig};
    pub use ftui::widgets::Panel;

    /// Build a `LogViewer` with explicit capacity.
    #[must_use]
    pub fn log_viewer(max_lines: usize) -> LogViewer {
        LogViewer::new(max_lines)
    }

    /// Build a `ListState`.
    #[must_use]
    pub fn list_state() -> ListState {
        ListState::default()
    }

    /// Build a `LogViewerState`.
    #[must_use]
    pub fn log_viewer_state() -> LogViewerState {
        LogViewerState::default()
    }

    /// Build a `NotificationQueue` using provided config.
    #[must_use]
    pub fn notification_queue(config: QueueConfig) -> NotificationQueue {
        NotificationQueue::new(config)
    }

    /// Build a default `ModalState` in open state.
    #[must_use]
    pub fn modal_state() -> ModalState {
        ModalState::default()
    }
}

/// Minimum primitive surface used by the immediate rewrite scope.
#[cfg(feature = "frankentui-upstream")]
pub mod upstream_primitives {
    use super::style::{StyleToken, ThemeSpec};
    use super::upstream_bridge::token_style;
    use super::upstream_ftui as ftui;

    pub use ftui::layout::{Constraint, Direction, Flex};
    pub use ftui::widgets::table::{Row as TableRow, Table, TableState};
    pub use ftui::widgets::{Badge, StatusItem, StatusLine};

    /// Build a default table state.
    #[must_use]
    pub fn table_state() -> TableState {
        TableState::default()
    }

    /// Build an empty status line.
    #[must_use]
    pub fn status_line<'a>() -> StatusLine<'a> {
        StatusLine::new()
    }

    /// Build a status line with Forge default foreground/background style.
    #[must_use]
    pub fn forge_status_line<'a>(theme: ThemeSpec) -> StatusLine<'a> {
        StatusLine::new().style(token_style(theme, StyleToken::Foreground))
    }

    /// Build a simple badge with one label.
    #[must_use]
    pub fn badge<'a>(label: &'a str) -> Badge<'a> {
        Badge::new(label)
    }

    /// Build a badge with Forge palette defaults for a semantic token.
    #[must_use]
    pub fn forge_badge<'a>(label: &'a str, theme: ThemeSpec, token: StyleToken) -> Badge<'a> {
        Badge::new(label).with_style(token_style(theme, token))
    }

    /// Build a table with Forge default base/highlight styles.
    #[must_use]
    pub fn forge_table<'a>(
        rows: impl IntoIterator<Item = TableRow>,
        widths: impl IntoIterator<Item = Constraint>,
        theme: ThemeSpec,
    ) -> Table<'a> {
        Table::new(rows, widths)
            .style(token_style(theme, StyleToken::Foreground))
            .highlight_style(token_style(theme, StyleToken::Focus))
    }

    /// Build a horizontal flex layout from constraints.
    #[must_use]
    pub fn horizontal_flex(constraints: impl IntoIterator<Item = Constraint>) -> Flex {
        Flex::horizontal().constraints(constraints)
    }

    /// Build a vertical flex layout from constraints.
    #[must_use]
    pub fn vertical_flex(constraints: impl IntoIterator<Item = Constraint>) -> Flex {
        Flex::vertical().constraints(constraints)
    }
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
        Warning,
        Info,
        Focus,
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
        pub warning: u8,
        pub info: u8,
        pub focus: u8,
    }

    /// Typography emphasis policy per theme.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TypographySpec {
        pub accent_bold: bool,
        pub success_bold: bool,
        pub danger_bold: bool,
        pub warning_bold: bool,
        pub muted_dim: bool,
        pub focus_underline: bool,
    }

    /// Theme specification exposed to target TUI crates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ThemeSpec {
        pub kind: ThemeKind,
        pub palette: Palette,
        pub typography: TypographySpec,
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
                StyleToken::Warning => self.palette.warning,
                StyleToken::Info => self.palette.info,
                StyleToken::Focus => self.palette.focus,
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
                    accent: 45,
                    success: 41,
                    danger: 197,
                    warning: 220,
                    info: 117,
                    focus: 81,
                },
                ThemeKind::Light => Palette {
                    background: 255,
                    surface: 252,
                    foreground: 234,
                    muted: 244,
                    accent: 25,
                    success: 28,
                    danger: 160,
                    warning: 172,
                    info: 31,
                    focus: 21,
                },
                ThemeKind::HighContrast => Palette {
                    background: 16,
                    surface: 232,
                    foreground: 231,
                    muted: 250,
                    accent: 51,
                    success: 118,
                    danger: 203,
                    warning: 226,
                    info: 159,
                    focus: 229,
                },
            };
            let typography = match kind {
                ThemeKind::Dark => TypographySpec {
                    accent_bold: true,
                    success_bold: false,
                    danger_bold: true,
                    warning_bold: true,
                    muted_dim: true,
                    focus_underline: true,
                },
                ThemeKind::Light => TypographySpec {
                    accent_bold: true,
                    success_bold: false,
                    danger_bold: true,
                    warning_bold: true,
                    muted_dim: false,
                    focus_underline: true,
                },
                ThemeKind::HighContrast => TypographySpec {
                    accent_bold: true,
                    success_bold: true,
                    danger_bold: true,
                    warning_bold: true,
                    muted_dim: false,
                    focus_underline: true,
                },
            };
            Self {
                kind,
                palette,
                typography,
            }
        }
    }
}

/// Render and frame primitives consumed by Forge TUI crates.
pub mod render {
    use super::style::{StyleToken, ThemeSpec};

    /// Track when deprecated legacy aliases can be deleted.
    pub const LEGACY_RENDER_FRAME_API_DELETE_GATE: &str = "forge-brp";
    use super::widgets::BorderStyle;

    /// Terminal color: ANSI256 index or 24-bit RGB.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TermColor {
        Ansi256(u8),
        Rgb(u8, u8, u8),
    }

    impl TermColor {
        /// Convert to ANSI256 index (lossy for RGB).
        #[must_use]
        pub fn as_ansi256(self) -> u8 {
            match self {
                Self::Ansi256(idx) => idx,
                Self::Rgb(r, g, b) => rgb_to_ansi256(r, g, b),
            }
        }
    }

    fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
        // Greyscale ramp check
        if r == g && g == b {
            if r < 8 {
                return 16;
            }
            if r > 248 {
                return 231;
            }
            return (((r as u16 - 8) * 24 / 247) as u8) + 232;
        }
        let ri = closest_ansi_component(r);
        let gi = closest_ansi_component(g);
        let bi = closest_ansi_component(b);
        16 + 36 * ri + 6 * gi + bi
    }

    fn closest_ansi_component(value: u8) -> u8 {
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let mut best = 0u8;
        let mut best_dist = u8::abs_diff(value, LEVELS[0]);
        for (i, level) in LEVELS.iter().enumerate().skip(1) {
            let dist = u8::abs_diff(value, *level);
            if dist < best_dist {
                best_dist = dist;
                best = i as u8;
            }
        }
        best
    }

    /// Frame dimensions in terminal cells.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FrameSize {
        pub width: usize,
        pub height: usize,
    }

    /// A rectangular region within a frame.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Rect {
        pub x: usize,
        pub y: usize,
        pub width: usize,
        pub height: usize,
    }

    impl Rect {
        /// Inner region after removing border (1 cell each side).
        #[must_use]
        pub fn inner(self) -> Self {
            if self.width < 2 || self.height < 2 {
                return Self {
                    x: self.x,
                    y: self.y,
                    width: 0,
                    height: 0,
                };
            }
            Self {
                x: self.x + 1,
                y: self.y + 1,
                width: self.width - 2,
                height: self.height - 2,
            }
        }

        /// Split into left (width=`left_width`) and right.
        #[must_use]
        pub fn split_horizontal(self, left_width: usize) -> (Self, Self) {
            let left_w = left_width.min(self.width);
            let right_w = self.width.saturating_sub(left_w);
            (
                Self {
                    x: self.x,
                    y: self.y,
                    width: left_w,
                    height: self.height,
                },
                Self {
                    x: self.x + left_w,
                    y: self.y,
                    width: right_w,
                    height: self.height,
                },
            )
        }

        /// Split into top (height=`top_height`) and bottom.
        #[must_use]
        pub fn split_vertical(self, top_height: usize) -> (Self, Self) {
            let top_h = top_height.min(self.height);
            let bot_h = self.height.saturating_sub(top_h);
            (
                Self {
                    x: self.x,
                    y: self.y,
                    width: self.width,
                    height: top_h,
                },
                Self {
                    x: self.x,
                    y: self.y + top_h,
                    width: self.width,
                    height: bot_h,
                },
            )
        }
    }

    /// Cell style represented as terminal colors and text attributes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellStyle {
        pub fg: TermColor,
        pub bg: TermColor,
        pub bold: bool,
        pub dim: bool,
        pub underline: bool,
    }

    /// A single frame cell.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FrameCell {
        pub glyph: char,
        pub style: CellStyle,
    }

    /// Semantic role for rendered text.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextRole {
        Primary,
        Muted,
        Accent,
        Success,
        Danger,
        Warning,
        Info,
        Focus,
    }

    /// Styling selector for span-oriented rendering.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SpanStyle {
        /// Semantic role translated through `ThemeSpec`.
        Role(TextRole),
        /// Direct `StyleToken` selector for markdown/syntax integration seams.
        Token(StyleToken),
        /// Explicit terminal style for callers with pre-resolved colors/attrs.
        Cell(CellStyle),
    }

    /// One text span with a style selector.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StyledSpan<'a> {
        pub text: &'a str,
        pub style: SpanStyle,
    }

    impl<'a> StyledSpan<'a> {
        /// Build a semantic role span.
        #[must_use]
        pub fn role(text: &'a str, role: TextRole) -> Self {
            Self {
                text,
                style: SpanStyle::Role(role),
            }
        }

        /// Build a style-token span.
        #[must_use]
        pub fn token(text: &'a str, token: StyleToken) -> Self {
            Self {
                text,
                style: SpanStyle::Token(token),
            }
        }

        /// Build an explicit style span.
        #[must_use]
        pub fn cell(text: &'a str, style: CellStyle) -> Self {
            Self {
                text,
                style: SpanStyle::Cell(style),
            }
        }
    }

    // -- Convenience span builders for common semantic roles --

    /// Shorthand constructors for the most common `StyledSpan` patterns.
    impl<'a> StyledSpan<'a> {
        /// Primary text (default foreground).
        #[must_use]
        pub fn primary(text: &'a str) -> Self {
            Self::role(text, TextRole::Primary)
        }

        /// Muted / secondary text.
        #[must_use]
        pub fn muted(text: &'a str) -> Self {
            Self::role(text, TextRole::Muted)
        }

        /// Accent text (bold in default typography).
        #[must_use]
        pub fn accent(text: &'a str) -> Self {
            Self::role(text, TextRole::Accent)
        }

        /// Success text.
        #[must_use]
        pub fn success(text: &'a str) -> Self {
            Self::role(text, TextRole::Success)
        }

        /// Danger / error text.
        #[must_use]
        pub fn danger(text: &'a str) -> Self {
            Self::role(text, TextRole::Danger)
        }

        /// Warning text.
        #[must_use]
        pub fn warning(text: &'a str) -> Self {
            Self::role(text, TextRole::Warning)
        }

        /// Info text.
        #[must_use]
        pub fn info(text: &'a str) -> Self {
            Self::role(text, TextRole::Info)
        }

        /// Focused / selected text.
        #[must_use]
        pub fn focus(text: &'a str) -> Self {
            Self::role(text, TextRole::Focus)
        }
    }

    // -- Owned span types for pipeline stages that produce text --

    /// Owned variant of [`StyledSpan`] for pipeline stages that need to store produced text.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OwnedStyledSpan {
        pub text: String,
        pub style: SpanStyle,
    }

    impl OwnedStyledSpan {
        /// Build an owned span from text and style.
        #[must_use]
        pub fn new(text: impl Into<String>, style: SpanStyle) -> Self {
            Self {
                text: text.into(),
                style,
            }
        }

        /// Build from a semantic role.
        #[must_use]
        pub fn role(text: impl Into<String>, role: TextRole) -> Self {
            Self::new(text, SpanStyle::Role(role))
        }

        /// Build from a style token.
        #[must_use]
        pub fn token(text: impl Into<String>, token: StyleToken) -> Self {
            Self::new(text, SpanStyle::Token(token))
        }

        /// Build from an explicit cell style.
        #[must_use]
        pub fn cell(text: impl Into<String>, style: CellStyle) -> Self {
            Self::new(text, SpanStyle::Cell(style))
        }

        /// Borrow as a [`StyledSpan`] for drawing.
        #[must_use]
        pub fn as_span(&self) -> StyledSpan<'_> {
            StyledSpan {
                text: &self.text,
                style: self.style,
            }
        }
    }

    // -- Composite types for multi-span lines and multi-line text --

    /// A single line composed of styled spans (owned).
    ///
    /// This is the primary pipeline type for passing styled line content between
    /// parsing stages (markdown, syntax highlighting) and rendering.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct StyledLine {
        pub spans: Vec<OwnedStyledSpan>,
    }

    impl StyledLine {
        /// Create an empty line.
        #[must_use]
        pub fn new() -> Self {
            Self { spans: Vec::new() }
        }

        /// Create a line from a single plain string with the given role.
        #[must_use]
        pub fn from_role(text: impl Into<String>, role: TextRole) -> Self {
            Self {
                spans: vec![OwnedStyledSpan::role(text, role)],
            }
        }

        /// Create a line from a single plain string with Primary role.
        #[must_use]
        pub fn plain(text: impl Into<String>) -> Self {
            Self::from_role(text, TextRole::Primary)
        }

        /// Push a span onto the line.
        pub fn push(&mut self, span: OwnedStyledSpan) {
            self.spans.push(span);
        }

        /// Push text with a semantic role.
        pub fn push_role(&mut self, text: impl Into<String>, role: TextRole) {
            self.spans.push(OwnedStyledSpan::role(text, role));
        }

        /// Push text with a style token.
        pub fn push_token(&mut self, text: impl Into<String>, token: StyleToken) {
            self.spans.push(OwnedStyledSpan::token(text, token));
        }

        /// Whether the line has no spans.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.spans.is_empty()
        }

        /// Number of spans in the line.
        #[must_use]
        pub fn len(&self) -> usize {
            self.spans.len()
        }

        /// Total character count across all spans.
        #[must_use]
        pub fn char_count(&self) -> usize {
            self.spans.iter().map(|s| s.text.len()).sum()
        }

        /// Borrow spans as a slice of [`StyledSpan`] for drawing.
        ///
        /// Returns a `Vec` because the borrowed spans have shorter lifetimes than
        /// the owned data, so a `&[StyledSpan]` cannot be returned directly.
        #[must_use]
        pub fn as_spans(&self) -> Vec<StyledSpan<'_>> {
            self.spans.iter().map(OwnedStyledSpan::as_span).collect()
        }

        /// Concatenate all span text (unstyled).
        #[must_use]
        pub fn plain_text(&self) -> String {
            self.spans.iter().map(|s| s.text.as_str()).collect()
        }
    }

    /// Multi-line styled text composed of [`StyledLine`]s.
    ///
    /// Used for rendering multi-line content from markdown, syntax-highlighted
    /// source code, or any pipeline that produces styled output.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct StyledText {
        pub lines: Vec<StyledLine>,
    }

    impl StyledText {
        /// Create empty styled text.
        #[must_use]
        pub fn new() -> Self {
            Self { lines: Vec::new() }
        }

        /// Create from a single line.
        #[must_use]
        pub fn single(line: StyledLine) -> Self {
            Self { lines: vec![line] }
        }

        /// Push a line.
        pub fn push(&mut self, line: StyledLine) {
            self.lines.push(line);
        }

        /// Number of lines.
        #[must_use]
        pub fn line_count(&self) -> usize {
            self.lines.len()
        }

        /// Whether there are no lines.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.lines.is_empty()
        }
    }

    // -- Pipeline trait: source of styled spans for future markdown/syntax integration --

    /// Trait for sources that produce styled spans from raw text.
    ///
    /// Implementors parse raw text (markdown, source code, log output) and
    /// produce styled spans. This is the integration seam for plugging in
    /// markdown renderers, syntax highlighters, or custom formatting logic.
    pub trait SpanSource {
        /// Parse one line of input text into styled spans.
        fn style_line(&self, input: &str) -> StyledLine;

        /// Parse multi-line input text into styled text.
        fn style_text(&self, input: &str) -> StyledText {
            StyledText {
                lines: input.lines().map(|line| self.style_line(line)).collect(),
            }
        }
    }

    /// Passthrough span source that wraps all text in Primary role.
    ///
    /// Useful as a default / fallback when no specific highlighter is configured.
    pub struct PlainSpanSource;

    impl SpanSource for PlainSpanSource {
        fn style_line(&self, input: &str) -> StyledLine {
            StyledLine::plain(input)
        }
    }

    /// Box-drawing character sets.
    struct BorderChars {
        top_left: char,
        top_right: char,
        bottom_left: char,
        bottom_right: char,
        horizontal: char,
        vertical: char,
    }

    fn border_chars(style: BorderStyle) -> BorderChars {
        match style {
            BorderStyle::Rounded => BorderChars {
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                horizontal: '─',
                vertical: '│',
            },
            BorderStyle::Plain => BorderChars {
                top_left: '┌',
                top_right: '┐',
                bottom_left: '└',
                bottom_right: '┘',
                horizontal: '─',
                vertical: '│',
            },
            BorderStyle::Heavy => BorderChars {
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
                horizontal: '━',
                vertical: '┃',
            },
        }
    }

    /// Stable frame abstraction shielding app crates from FrankenTUI internals.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RenderFrame {
        size: FrameSize,
        cells: Vec<FrameCell>,
        theme: ThemeSpec,
    }

    impl RenderFrame {
        /// Create a blank frame using the provided adapter theme.
        #[must_use]
        pub fn new(size: FrameSize, theme: ThemeSpec) -> Self {
            let default_cell = FrameCell {
                glyph: ' ',
                style: CellStyle {
                    fg: TermColor::Ansi256(theme.color(StyleToken::Foreground)),
                    bg: TermColor::Ansi256(theme.color(StyleToken::Background)),
                    bold: false,
                    dim: false,
                    underline: false,
                },
            };
            Self {
                size,
                cells: vec![default_cell; size.width.saturating_mul(size.height)],
                theme,
            }
        }

        /// Returns the theme spec for this frame.
        #[must_use]
        pub fn theme(&self) -> ThemeSpec {
            self.theme
        }

        #[must_use]
        pub fn size(&self) -> FrameSize {
            self.size
        }

        /// Legacy helper retained during adapter migration.
        #[deprecated(
            note = "use size().width instead; removal tracked by LEGACY_RENDER_FRAME_API_DELETE_GATE"
        )]
        #[must_use]
        pub fn width(&self) -> usize {
            self.size.width
        }

        /// Legacy helper retained during adapter migration.
        #[deprecated(
            note = "use size().height instead; removal tracked by LEGACY_RENDER_FRAME_API_DELETE_GATE"
        )]
        #[must_use]
        pub fn height(&self) -> usize {
            self.size.height
        }

        /// Returns one frame cell for assertions/snapshot helpers.
        #[must_use]
        pub fn cell(&self, x: usize, y: usize) -> Option<FrameCell> {
            if x >= self.size.width || y >= self.size.height {
                return None;
            }
            Some(self.cells[y * self.size.width + x])
        }

        /// Write a single cell, clipped to frame bounds.
        pub fn set_cell(&mut self, x: usize, y: usize, cell: FrameCell) {
            if x >= self.size.width || y >= self.size.height {
                return;
            }
            self.cells[y * self.size.width + x] = cell;
        }

        /// Draw text on a single row, clipped to frame width.
        ///
        /// Legacy single-span helper retained during migration to `draw_spans`.
        pub fn draw_text(&mut self, x: usize, y: usize, text: &str, role: TextRole) {
            self.draw_spans(x, y, &[StyledSpan::role(text, role)]);
        }

        /// Draw text with explicit foreground/background colors.
        ///
        /// Legacy single-span helper retained during migration to `draw_spans`.
        pub fn draw_styled_text(
            &mut self,
            x: usize,
            y: usize,
            text: &str,
            fg: TermColor,
            bg: TermColor,
            bold: bool,
        ) {
            let style = CellStyle {
                fg,
                bg,
                bold,
                dim: false,
                underline: false,
            };
            self.draw_spans(x, y, &[StyledSpan::cell(text, style)]);
        }

        /// Draw styled spans in order, clipped to frame bounds.
        pub fn draw_spans(&mut self, x: usize, y: usize, spans: &[StyledSpan<'_>]) {
            if y >= self.size.height || x >= self.size.width {
                return;
            }

            let mut col = x;
            for span in spans {
                let style = self.resolve_span_style(span.style);
                for glyph in span.text.chars() {
                    if col >= self.size.width {
                        return;
                    }
                    self.cells[y * self.size.width + col] = FrameCell { glyph, style };
                    col += 1;
                }
            }
        }

        /// Draw styled spans in order, clipped to the provided rect.
        pub fn draw_spans_in_rect(
            &mut self,
            rect: Rect,
            x_offset: usize,
            y_offset: usize,
            spans: &[StyledSpan<'_>],
        ) {
            let abs_x = rect.x + x_offset;
            let abs_y = rect.y + y_offset;
            let max_col = (rect.x + rect.width).min(self.size.width);
            if abs_y >= rect.y + rect.height || abs_y >= self.size.height || abs_x >= max_col {
                return;
            }

            let mut col = abs_x;
            for span in spans {
                let style = self.resolve_span_style(span.style);
                for glyph in span.text.chars() {
                    if col >= max_col {
                        return;
                    }
                    self.cells[abs_y * self.size.width + col] = FrameCell { glyph, style };
                    col += 1;
                }
            }
        }

        /// Draw a [`StyledLine`] at the given position, clipped to frame bounds.
        pub fn draw_styled_line(&mut self, x: usize, y: usize, line: &StyledLine) {
            let borrowed: Vec<StyledSpan<'_>> = line.as_spans();
            self.draw_spans(x, y, &borrowed);
        }

        /// Draw a [`StyledLine`] within a rect, clipped to rect bounds.
        pub fn draw_styled_line_in_rect(
            &mut self,
            rect: Rect,
            x_offset: usize,
            y_offset: usize,
            line: &StyledLine,
        ) {
            let borrowed: Vec<StyledSpan<'_>> = line.as_spans();
            self.draw_spans_in_rect(rect, x_offset, y_offset, &borrowed);
        }

        /// Draw a [`StyledText`] block starting at `(x, y)`, one line per row.
        ///
        /// Lines that fall outside the frame height are silently skipped.
        pub fn draw_styled_text_block(&mut self, x: usize, y: usize, text: &StyledText) {
            for (i, line) in text.lines.iter().enumerate() {
                let row = y + i;
                if row >= self.size.height {
                    break;
                }
                self.draw_styled_line(x, row, line);
            }
        }

        /// Draw a [`StyledText`] within a rect, one line per row.
        ///
        /// Lines that fall outside the rect height are silently skipped.
        pub fn draw_styled_text_in_rect(&mut self, rect: Rect, text: &StyledText) {
            for (i, line) in text.lines.iter().enumerate() {
                if i >= rect.height {
                    break;
                }
                self.draw_styled_line_in_rect(rect, 0, i, line);
            }
        }

        /// Draw a bordered panel with a title into a rectangular region.
        ///
        /// Returns the inner `Rect` (content area inside the border) for subsequent drawing.
        pub fn draw_panel(
            &mut self,
            rect: Rect,
            title: &str,
            border: BorderStyle,
            border_color: TermColor,
            bg: TermColor,
        ) -> Rect {
            if rect.width < 2 || rect.height < 2 {
                return Rect {
                    x: rect.x,
                    y: rect.y,
                    width: 0,
                    height: 0,
                };
            }

            let chars = border_chars(border);
            let border_style = CellStyle {
                fg: border_color,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };
            let fill_style = CellStyle {
                fg: TermColor::Ansi256(self.theme.color(StyleToken::Foreground)),
                bg,
                bold: false,
                dim: false,
                underline: false,
            };

            // Fill background
            for row in rect.y..rect.y + rect.height {
                for col in rect.x..rect.x + rect.width {
                    self.set_cell(
                        col,
                        row,
                        FrameCell {
                            glyph: ' ',
                            style: fill_style,
                        },
                    );
                }
            }

            // Top border: ╭─ Title ─╮
            self.set_cell(
                rect.x,
                rect.y,
                FrameCell {
                    glyph: chars.top_left,
                    style: border_style,
                },
            );
            // Title in top border
            let title_start = rect.x + 2;
            let title_max = rect.width.saturating_sub(4);
            let title_text: String = if !title.is_empty() {
                let truncated: String = title.chars().take(title_max).collect();
                format!(" {} ", truncated)
            } else {
                String::new()
            };
            let title_len = title_text.chars().count();
            // Fill horizontal bar
            for col in (rect.x + 1)..(rect.x + rect.width - 1) {
                self.set_cell(
                    col,
                    rect.y,
                    FrameCell {
                        glyph: chars.horizontal,
                        style: border_style,
                    },
                );
            }
            // Overlay title
            let title_style = CellStyle {
                fg: border_color,
                bg,
                bold: true,
                dim: false,
                underline: false,
            };
            for (i, ch) in title_text.chars().enumerate() {
                let col = title_start + i;
                if col >= rect.x + rect.width - 1 {
                    break;
                }
                self.set_cell(
                    col,
                    rect.y,
                    FrameCell {
                        glyph: ch,
                        style: title_style,
                    },
                );
            }
            self.set_cell(
                rect.x + rect.width - 1,
                rect.y,
                FrameCell {
                    glyph: chars.top_right,
                    style: border_style,
                },
            );

            // Side borders
            for row in (rect.y + 1)..(rect.y + rect.height - 1) {
                self.set_cell(
                    rect.x,
                    row,
                    FrameCell {
                        glyph: chars.vertical,
                        style: border_style,
                    },
                );
                self.set_cell(
                    rect.x + rect.width - 1,
                    row,
                    FrameCell {
                        glyph: chars.vertical,
                        style: border_style,
                    },
                );
            }

            // Bottom border: ╰───╯
            let bottom_y = rect.y + rect.height - 1;
            self.set_cell(
                rect.x,
                bottom_y,
                FrameCell {
                    glyph: chars.bottom_left,
                    style: border_style,
                },
            );
            for col in (rect.x + 1)..(rect.x + rect.width - 1) {
                self.set_cell(
                    col,
                    bottom_y,
                    FrameCell {
                        glyph: chars.horizontal,
                        style: border_style,
                    },
                );
            }
            self.set_cell(
                rect.x + rect.width - 1,
                bottom_y,
                FrameCell {
                    glyph: chars.bottom_right,
                    style: border_style,
                },
            );

            // Return inner rect
            let _ = title_len; // used above
            rect.inner()
        }

        /// Draw a horizontal rule across a row within a region.
        pub fn draw_horizontal_rule(&mut self, x: usize, y: usize, width: usize, role: TextRole) {
            let fg = self.color_for_role(role);
            let bg = TermColor::Ansi256(self.theme.color(StyleToken::Background));
            let style = CellStyle {
                fg,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };
            for col in x..x + width {
                if col >= self.size.width || y >= self.size.height {
                    break;
                }
                self.set_cell(
                    col,
                    y,
                    FrameCell {
                        glyph: '─', style
                    },
                );
            }
        }

        /// Draw a gauge/progress bar at (x, y) with given width.
        /// `ratio` is 0.0..=1.0. Uses block characters for sub-cell precision.
        pub fn draw_gauge(
            &mut self,
            x: usize,
            y: usize,
            width: usize,
            ratio: f64,
            filled_color: TermColor,
            empty_color: TermColor,
            bg: TermColor,
        ) {
            if width == 0 || y >= self.size.height {
                return;
            }
            let clamped = ratio.clamp(0.0, 1.0);
            let filled_exact = clamped * width as f64;
            let full_blocks = filled_exact as usize;
            let remainder = filled_exact - full_blocks as f64;

            let filled_style = CellStyle {
                fg: filled_color,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };
            let empty_style = CellStyle {
                fg: empty_color,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };

            for i in 0..width {
                let col = x + i;
                if col >= self.size.width {
                    break;
                }
                let (glyph, style) = if i < full_blocks {
                    ('\u{2588}', filled_style) // █
                } else if i == full_blocks {
                    let frac = (remainder * 8.0) as usize;
                    let ch = match frac {
                        0 => '\u{2591}', // ░
                        1 => '\u{2581}', // ▁
                        2 => '\u{2582}', // ▂
                        3 => '\u{2583}', // ▃
                        4 => '\u{2584}', // ▄
                        5 => '\u{2585}', // ▅
                        6 => '\u{2586}', // ▆
                        _ => '\u{2587}', // ▇
                    };
                    (ch, filled_style)
                } else {
                    ('\u{2591}', empty_style) // ░
                };
                self.set_cell(col, y, FrameCell { glyph, style });
            }
        }

        /// Draw a sparkline using the given data points.
        /// Data is normalized to fit in 1 row using block characters ▁▂▃▄▅▆▇█.
        pub fn draw_sparkline(
            &mut self,
            x: usize,
            y: usize,
            width: usize,
            data: &[f64],
            color: TermColor,
            bg: TermColor,
        ) {
            if width == 0 || y >= self.size.height || data.is_empty() {
                return;
            }
            let max_val = data.iter().cloned().fold(0.0f64, f64::max);
            let style = CellStyle {
                fg: color,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };
            let blocks = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            for i in 0..width {
                let col = x + i;
                if col >= self.size.width {
                    break;
                }
                let data_idx = if data.len() <= width {
                    if i < data.len() {
                        i
                    } else {
                        continue;
                    }
                } else {
                    (i * data.len()) / width
                };
                let val = data.get(data_idx).copied().unwrap_or(0.0);
                let normalized = if max_val > 0.0 {
                    (val / max_val).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let idx = (normalized * 8.0) as usize;
                let glyph = blocks[idx.min(8)];
                self.set_cell(col, y, FrameCell { glyph, style });
            }
        }

        /// Fill a rectangular region with a background color.
        pub fn fill_bg(&mut self, rect: Rect, bg: TermColor) {
            let fg = TermColor::Ansi256(self.theme.color(StyleToken::Foreground));
            let style = CellStyle {
                fg,
                bg,
                bold: false,
                dim: false,
                underline: false,
            };
            for row in rect.y..rect.y + rect.height {
                for col in rect.x..rect.x + rect.width {
                    if col < self.size.width && row < self.size.height {
                        self.set_cell(col, row, FrameCell { glyph: ' ', style });
                    }
                }
            }
        }

        /// Draw text within a rect, clipped to rect bounds.
        ///
        /// Legacy single-span helper retained during migration to `draw_spans_in_rect`.
        pub fn draw_text_in_rect(
            &mut self,
            rect: Rect,
            x_offset: usize,
            y_offset: usize,
            text: &str,
            role: TextRole,
        ) {
            self.draw_spans_in_rect(rect, x_offset, y_offset, &[StyledSpan::role(text, role)]);
        }

        #[must_use]
        pub fn row_text(&self, y: usize) -> String {
            if y >= self.size.height {
                return String::new();
            }
            let start = y * self.size.width;
            let end = start + self.size.width;
            self.cells[start..end]
                .iter()
                .map(|cell| cell.glyph)
                .collect()
        }

        /// Text-only snapshot helper for lightweight regression tests.
        #[must_use]
        pub fn snapshot(&self) -> String {
            (0..self.size.height)
                .map(|row| self.row_text(row))
                .collect::<Vec<_>>()
                .join("\n")
        }

        /// Legacy full-frame text helper retained during adapter migration.
        #[deprecated(
            note = "use snapshot() for full frame text or row_text(y) for one row; removal tracked by LEGACY_RENDER_FRAME_API_DELETE_GATE"
        )]
        #[must_use]
        pub fn to_text(&self) -> String {
            self.snapshot()
        }

        /// Returns the `TermColor` for a semantic role.
        #[must_use]
        pub fn color_for_role(&self, role: TextRole) -> TermColor {
            TermColor::Ansi256(match role {
                TextRole::Primary => self.theme.color(StyleToken::Foreground),
                TextRole::Muted => self.theme.color(StyleToken::Muted),
                TextRole::Accent => self.theme.color(StyleToken::Accent),
                TextRole::Success => self.theme.color(StyleToken::Success),
                TextRole::Danger => self.theme.color(StyleToken::Danger),
                TextRole::Warning => self.theme.color(StyleToken::Warning),
                TextRole::Info => self.theme.color(StyleToken::Info),
                TextRole::Focus => self.theme.color(StyleToken::Focus),
            })
        }

        fn style_for_role(&self, role: TextRole) -> (bool, bool, bool) {
            let typography = self.theme.typography;
            match role {
                TextRole::Primary => (false, false, false),
                TextRole::Muted => (false, typography.muted_dim, false),
                TextRole::Accent => (typography.accent_bold, false, false),
                TextRole::Success => (typography.success_bold, false, false),
                TextRole::Danger => (typography.danger_bold, false, false),
                TextRole::Warning => (typography.warning_bold, false, false),
                TextRole::Info => (false, false, false),
                TextRole::Focus => (true, false, typography.focus_underline),
            }
        }

        fn style_for_token(&self, token: StyleToken) -> (bool, bool, bool) {
            let typography = self.theme.typography;
            match token {
                StyleToken::Accent => (typography.accent_bold, false, false),
                StyleToken::Success => (typography.success_bold, false, false),
                StyleToken::Danger => (typography.danger_bold, false, false),
                StyleToken::Warning => (typography.warning_bold, false, false),
                StyleToken::Muted => (false, typography.muted_dim, false),
                StyleToken::Focus => (true, false, typography.focus_underline),
                _ => (false, false, false),
            }
        }

        fn resolve_span_style(&self, style: SpanStyle) -> CellStyle {
            match style {
                SpanStyle::Cell(style) => style,
                SpanStyle::Role(role) => {
                    let fg = self.color_for_role(role);
                    let bg = TermColor::Ansi256(self.theme.color(StyleToken::Background));
                    let (bold, dim, underline) = self.style_for_role(role);
                    CellStyle {
                        fg,
                        bg,
                        bold,
                        dim,
                        underline,
                    }
                }
                SpanStyle::Token(token) => {
                    let fg = TermColor::Ansi256(self.theme.color(token));
                    let bg = TermColor::Ansi256(self.theme.color(StyleToken::Background));
                    let (bold, dim, underline) = self.style_for_token(token);
                    CellStyle {
                        fg,
                        bg,
                        bold,
                        dim,
                        underline,
                    }
                }
            }
        }
    }
}

/// Stable widget primitives consumed by Forge TUI crates.
pub mod widgets {
    /// Border treatment exposed by the adapter.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BorderStyle {
        Plain,
        Rounded,
        Heavy,
    }

    /// Text alignment for widget headers and columns.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextAlign {
        Left,
        Center,
        Right,
    }

    /// Visual emphasis for loop surface blocks.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Emphasis {
        Subtle,
        Normal,
        Strong,
        Critical,
    }

    /// Stable padding primitive used by widget specs.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Padding {
        pub top: u8,
        pub right: u8,
        pub bottom: u8,
        pub left: u8,
    }

    impl Padding {
        pub const COMPACT: Self = Self {
            top: 0,
            right: 1,
            bottom: 0,
            left: 1,
        };

        pub const ROOMY: Self = Self {
            top: 1,
            right: 2,
            bottom: 1,
            left: 2,
        };
    }

    /// Stable block primitive for loop dashboards.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WidgetSpec {
        pub id: &'static str,
        pub title: &'static str,
        pub border: BorderStyle,
        pub align: TextAlign,
        pub emphasis: Emphasis,
        pub padding: Padding,
    }

    impl WidgetSpec {
        /// Primary loop status block.
        #[must_use]
        pub fn loop_status_panel() -> Self {
            Self {
                id: "loop.status",
                title: "Loop Status",
                border: BorderStyle::Rounded,
                align: TextAlign::Left,
                emphasis: Emphasis::Strong,
                padding: Padding::ROOMY,
            }
        }

        /// Queue block for pending/dispatched work visibility.
        #[must_use]
        pub fn loop_queue_panel() -> Self {
            Self {
                id: "loop.queue",
                title: "Queue",
                border: BorderStyle::Plain,
                align: TextAlign::Left,
                emphasis: Emphasis::Normal,
                padding: Padding::COMPACT,
            }
        }

        /// Log tail block for ongoing event stream.
        #[must_use]
        pub fn loop_log_panel() -> Self {
            Self {
                id: "loop.logs",
                title: "Recent Logs",
                border: BorderStyle::Heavy,
                align: TextAlign::Left,
                emphasis: Emphasis::Subtle,
                padding: Padding::COMPACT,
            }
        }

        /// Inbox list block for fmail TUI.
        #[must_use]
        pub fn fmail_inbox_panel() -> Self {
            Self {
                id: "fmail.inbox",
                title: "Inbox",
                border: BorderStyle::Rounded,
                align: TextAlign::Left,
                emphasis: Emphasis::Strong,
                padding: Padding::ROOMY,
            }
        }

        /// Message preview block for fmail TUI.
        #[must_use]
        pub fn fmail_message_panel() -> Self {
            Self {
                id: "fmail.message",
                title: "Message",
                border: BorderStyle::Plain,
                align: TextAlign::Left,
                emphasis: Emphasis::Normal,
                padding: Padding::COMPACT,
            }
        }

        /// Compose block for fmail TUI.
        #[must_use]
        pub fn fmail_compose_panel() -> Self {
            Self {
                id: "fmail.compose",
                title: "Compose",
                border: BorderStyle::Heavy,
                align: TextAlign::Left,
                emphasis: Emphasis::Subtle,
                padding: Padding::COMPACT,
            }
        }
    }

    /// Stable loop queue table column primitive.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TableColumnSpec {
        pub key: &'static str,
        pub title: &'static str,
        pub width: u16,
        pub align: TextAlign,
    }

    /// Queue columns consumed by loop TUI crate.
    #[must_use]
    pub fn loop_queue_columns() -> [TableColumnSpec; 4] {
        [
            TableColumnSpec {
                key: "id",
                title: "ID",
                width: 14,
                align: TextAlign::Left,
            },
            TableColumnSpec {
                key: "status",
                title: "Status",
                width: 12,
                align: TextAlign::Center,
            },
            TableColumnSpec {
                key: "target",
                title: "Target",
                width: 24,
                align: TextAlign::Left,
            },
            TableColumnSpec {
                key: "attempts",
                title: "Attempts",
                width: 10,
                align: TextAlign::Right,
            },
        ]
    }

    /// Mailbox columns consumed by fmail TUI crate.
    #[must_use]
    pub fn fmail_inbox_columns() -> [TableColumnSpec; 4] {
        [
            TableColumnSpec {
                key: "from",
                title: "From",
                width: 18,
                align: TextAlign::Left,
            },
            TableColumnSpec {
                key: "subject",
                title: "Subject",
                width: 32,
                align: TextAlign::Left,
            },
            TableColumnSpec {
                key: "age",
                title: "Age",
                width: 8,
                align: TextAlign::Right,
            },
            TableColumnSpec {
                key: "status",
                title: "Status",
                width: 10,
                align: TextAlign::Center,
            },
        ]
    }
}

/// Snapshot helpers for adapter-based render abstractions.
pub mod snapshot;

/// Lightweight perf measurement helpers (no CI gating; intended for local regression checks).
pub mod perf;

/// Stable input/event abstraction shielding TUI crates from upstream key models.
pub mod input {
    /// Canonical key set exposed to Forge TUI crates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Key {
        Char(char),
        Enter,
        Escape,
        Tab,
        Backspace,
        Up,
        Down,
        Left,
        Right,
    }

    /// Canonical keyboard modifiers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Modifiers {
        pub shift: bool,
        pub ctrl: bool,
        pub alt: bool,
    }

    impl Modifiers {
        #[must_use]
        pub const fn none() -> Self {
            Self {
                shift: false,
                ctrl: false,
                alt: false,
            }
        }
    }

    /// Canonical key event.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeyEvent {
        pub key: Key,
        pub modifiers: Modifiers,
    }

    impl KeyEvent {
        #[must_use]
        pub const fn plain(key: Key) -> Self {
            Self {
                key,
                modifiers: Modifiers::none(),
            }
        }
    }

    /// Canonical mouse wheel direction.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MouseWheelDirection {
        Up,
        Down,
    }

    /// Canonical mouse button.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
    }

    /// Canonical mouse event kind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MouseEventKind {
        Wheel(MouseWheelDirection),
        Down(MouseButton),
        Up(MouseButton),
        Drag(MouseButton),
        Move,
    }

    /// Canonical mouse event.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MouseEvent {
        pub kind: MouseEventKind,
        pub column: usize,
        pub row: usize,
    }

    /// Canonical frame resize event.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ResizeEvent {
        pub width: usize,
        pub height: usize,
    }

    /// Stable input stream event consumed by Forge target TUI crates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum InputEvent {
        Key(KeyEvent),
        Mouse(MouseEvent),
        Resize(ResizeEvent),
        Tick,
    }

    /// Stable high-level actions produced by adapter input translation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum UiAction {
        Noop,
        MoveUp,
        MoveDown,
        MoveLeft,
        MoveRight,
        Confirm,
        Cancel,
        Refresh,
        Search,
        Compose,
        ScrollUp,
        ScrollDown,
    }

    /// Translator trait allowing alternate mappings without exposing upstream APIs.
    pub trait InputTranslator {
        fn translate(&self, event: &InputEvent) -> UiAction;
    }

    /// Default keymap used by current Forge/fmail TUI bootstrap crates.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct DefaultInputTranslator;

    impl InputTranslator for DefaultInputTranslator {
        fn translate(&self, event: &InputEvent) -> UiAction {
            match event {
                InputEvent::Key(KeyEvent { key: Key::Up, .. })
                | InputEvent::Key(KeyEvent {
                    key: Key::Char('k'),
                    ..
                }) => UiAction::MoveUp,
                InputEvent::Key(KeyEvent { key: Key::Down, .. })
                | InputEvent::Key(KeyEvent {
                    key: Key::Char('j'),
                    ..
                }) => UiAction::MoveDown,
                InputEvent::Key(KeyEvent { key: Key::Left, .. })
                | InputEvent::Key(KeyEvent {
                    key: Key::Char('h'),
                    ..
                }) => UiAction::MoveLeft,
                InputEvent::Key(KeyEvent {
                    key: Key::Right, ..
                })
                | InputEvent::Key(KeyEvent {
                    key: Key::Char('l'),
                    ..
                }) => UiAction::MoveRight,
                InputEvent::Key(KeyEvent {
                    key: Key::Enter, ..
                }) => UiAction::Confirm,
                InputEvent::Key(KeyEvent {
                    key: Key::Escape, ..
                }) => UiAction::Cancel,
                InputEvent::Key(KeyEvent {
                    key: Key::Char('/'),
                    ..
                }) => UiAction::Search,
                InputEvent::Key(KeyEvent {
                    key: Key::Char('c'),
                    modifiers,
                }) if modifiers.ctrl => UiAction::Compose,
                InputEvent::Key(KeyEvent {
                    key: Key::Char('r'),
                    modifiers,
                }) if modifiers.ctrl => UiAction::Refresh,
                InputEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Wheel(MouseWheelDirection::Up),
                    ..
                }) => UiAction::ScrollUp,
                InputEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Wheel(MouseWheelDirection::Down),
                    ..
                }) => UiAction::ScrollDown,
                InputEvent::Resize(_) | InputEvent::Tick => UiAction::Refresh,
                _ => UiAction::Noop,
            }
        }
    }

    /// Convenience function for adapter consumers that do not need custom mapping.
    #[must_use]
    pub fn translate_input(event: &InputEvent) -> UiAction {
        DefaultInputTranslator.translate(event)
    }
}

#[cfg(test)]
mod tests {
    use super::input::{
        translate_input, InputEvent, Key, KeyEvent, Modifiers, MouseEvent, MouseEventKind,
        MouseWheelDirection, ResizeEvent, UiAction,
    };
    use super::render::{
        FrameSize, OwnedStyledSpan, PlainSpanSource, RenderFrame, SpanSource, SpanStyle,
        StyledLine, StyledSpan, StyledText, TermColor, TextRole,
        LEGACY_RENDER_FRAME_API_DELETE_GATE,
    };
    use super::style::{StyleToken, ThemeKind, ThemeSpec};
    use super::widgets::{self, Padding, TextAlign, WidgetSpec};
    use super::{crate_label, FRANKENTUI_PIN};

    #[test]
    fn crate_label_is_stable() {
        assert_eq!(crate_label(), "forge-ftui-adapter");
    }

    #[test]
    fn frankentui_pin_is_stable() {
        assert_eq!(FRANKENTUI_PIN, "23429fac0e739635c7b8e0b995bde09401ff6ea0");
    }

    #[test]
    fn default_theme_is_dark() {
        let theme = ThemeSpec::default();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.color(StyleToken::Accent), 45);
    }

    #[test]
    fn high_contrast_theme_snapshot() {
        let theme = ThemeSpec::for_kind(ThemeKind::HighContrast);
        let snapshot = format!(
            "kind={:?} bg={} surface={} fg={} muted={} accent={} success={} danger={} warning={} info={} focus={}",
            theme.kind,
            theme.color(StyleToken::Background),
            theme.color(StyleToken::Surface),
            theme.color(StyleToken::Foreground),
            theme.color(StyleToken::Muted),
            theme.color(StyleToken::Accent),
            theme.color(StyleToken::Success),
            theme.color(StyleToken::Danger),
            theme.color(StyleToken::Warning),
            theme.color(StyleToken::Info),
            theme.color(StyleToken::Focus),
        );
        assert_eq!(
            snapshot,
            "kind=HighContrast bg=16 surface=232 fg=231 muted=250 accent=51 success=118 danger=203 warning=226 info=159 focus=229"
        );
    }

    #[test]
    fn render_frame_text_snapshot() {
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 12,
                height: 2,
            },
            ThemeSpec::default(),
        );
        frame.draw_text(0, 0, "forge", TextRole::Accent);
        frame.draw_text(0, 1, "ready", TextRole::Muted);
        assert_eq!(frame.snapshot(), "forge       \nready       ");
    }

    #[test]
    #[allow(deprecated)]
    fn render_frame_legacy_aliases_map_to_current_apis() {
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 12,
                height: 2,
            },
            ThemeSpec::default(),
        );
        frame.draw_text(0, 0, "forge", TextRole::Accent);
        frame.draw_text(0, 1, "ready", TextRole::Muted);

        assert_eq!(frame.width(), frame.size().width);
        assert_eq!(frame.height(), frame.size().height);
        assert_eq!(frame.to_text(), frame.snapshot());
        assert_eq!(LEGACY_RENDER_FRAME_API_DELETE_GATE, "forge-brp");
    }

    #[test]
    fn render_frame_uses_role_color_tokens() {
        use super::render::TermColor;
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 4,
                height: 1,
            },
            theme,
        );
        frame.draw_text(1, 0, "!", TextRole::Focus);
        let fg = frame.cell(1, 0).map(|cell| cell.style.fg);
        let underline = frame.cell(1, 0).map(|cell| cell.style.underline);
        assert_eq!(fg, Some(TermColor::Ansi256(theme.color(StyleToken::Focus))));
        assert_eq!(underline, Some(true));
    }

    #[test]
    fn muted_role_uses_dim_when_typography_enables_it() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 5,
                height: 1,
            },
            theme,
        );
        frame.draw_text(0, 0, "muted", TextRole::Muted);
        assert_eq!(frame.cell(0, 0).map(|cell| cell.style.dim), Some(true));
    }

    #[test]
    fn draw_spans_supports_mixed_role_and_cell_styles() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 16,
                height: 1,
            },
            theme,
        );
        let custom = super::render::CellStyle {
            fg: TermColor::Ansi256(196),
            bg: TermColor::Ansi256(theme.color(StyleToken::Background)),
            bold: true,
            dim: false,
            underline: false,
        };
        frame.draw_spans(
            0,
            0,
            &[
                StyledSpan::role("ok", TextRole::Success),
                StyledSpan {
                    text: " ",
                    style: SpanStyle::Role(TextRole::Muted),
                },
                StyledSpan::cell("ERR", custom),
            ],
        );

        assert_eq!(frame.row_text(0), "ok ERR          ");
        assert_eq!(
            frame.cell(0, 0).map(|cell| cell.style.fg),
            Some(TermColor::Ansi256(theme.color(StyleToken::Success)))
        );
        assert_eq!(
            frame.cell(2, 0).map(|cell| cell.style.fg),
            Some(TermColor::Ansi256(theme.color(StyleToken::Muted)))
        );
        assert_eq!(frame.cell(3, 0).map(|cell| cell.style.fg), Some(custom.fg));
        assert_eq!(frame.cell(3, 0).map(|cell| cell.style.bold), Some(true));
    }

    #[test]
    fn draw_spans_clips_to_frame_width() {
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 5,
                height: 1,
            },
            ThemeSpec::default(),
        );
        frame.draw_spans(
            3,
            0,
            &[
                StyledSpan::role("abc", TextRole::Accent),
                StyledSpan::role("zzz", TextRole::Danger),
            ],
        );
        assert_eq!(frame.row_text(0), "   ab");
    }

    #[test]
    fn draw_spans_supports_style_token_variant() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 4,
                height: 1,
            },
            theme,
        );
        frame.draw_spans(
            0,
            0,
            &[StyledSpan {
                text: "A",
                style: SpanStyle::Token(StyleToken::Accent),
            }],
        );
        let fg = frame.cell(0, 0).map(|cell| cell.style.fg);
        assert_eq!(
            fg,
            Some(TermColor::Ansi256(theme.color(StyleToken::Accent)))
        );
    }

    #[test]
    fn draw_spans_in_rect_clips_to_rect_bounds() {
        use super::render::Rect;

        let mut frame = RenderFrame::new(
            FrameSize {
                width: 7,
                height: 1,
            },
            ThemeSpec::default(),
        );
        frame.draw_spans_in_rect(
            Rect {
                x: 2,
                y: 0,
                width: 3,
                height: 1,
            },
            0,
            0,
            &[StyledSpan::role("abcdef", TextRole::Primary)],
        );
        assert_eq!(frame.row_text(0), "  abc  ");
    }

    #[test]
    fn draw_text_in_rect_uses_span_pipeline() {
        use super::render::Rect;

        let mut frame = RenderFrame::new(
            FrameSize {
                width: 8,
                height: 1,
            },
            ThemeSpec::default(),
        );
        frame.draw_text_in_rect(
            Rect {
                x: 1,
                y: 0,
                width: 4,
                height: 1,
            },
            0,
            0,
            "status=ok",
            TextRole::Primary,
        );
        assert_eq!(frame.row_text(0), " stat   ");
    }

    #[test]
    fn loop_widget_panel_snapshot() {
        let panels = [
            WidgetSpec::loop_status_panel(),
            WidgetSpec::loop_queue_panel(),
            WidgetSpec::loop_log_panel(),
        ];
        let snapshot = format!(
            "{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}\n{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}\n{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}",
            panels[0].id,
            panels[0].title,
            panels[0].border,
            panels[0].align,
            panels[0].emphasis,
            panels[0].padding.top,
            panels[0].padding.right,
            panels[0].padding.bottom,
            panels[0].padding.left,
            panels[1].id,
            panels[1].title,
            panels[1].border,
            panels[1].align,
            panels[1].emphasis,
            panels[1].padding.top,
            panels[1].padding.right,
            panels[1].padding.bottom,
            panels[1].padding.left,
            panels[2].id,
            panels[2].title,
            panels[2].border,
            panels[2].align,
            panels[2].emphasis,
            panels[2].padding.top,
            panels[2].padding.right,
            panels[2].padding.bottom,
            panels[2].padding.left,
        );
        assert_eq!(
            snapshot,
            "loop.status|Loop Status|Rounded|Left|Strong|1/2/1/2\nloop.queue|Queue|Plain|Left|Normal|0/1/0/1\nloop.logs|Recent Logs|Heavy|Left|Subtle|0/1/0/1"
        );
    }

    #[test]
    fn loop_queue_columns_snapshot() {
        let columns = widgets::loop_queue_columns();
        let snapshot = format!(
            "{}:{}:{}:{:?}\n{}:{}:{}:{:?}\n{}:{}:{}:{:?}\n{}:{}:{}:{:?}",
            columns[0].key,
            columns[0].title,
            columns[0].width,
            columns[0].align,
            columns[1].key,
            columns[1].title,
            columns[1].width,
            columns[1].align,
            columns[2].key,
            columns[2].title,
            columns[2].width,
            columns[2].align,
            columns[3].key,
            columns[3].title,
            columns[3].width,
            columns[3].align,
        );
        assert_eq!(
            snapshot,
            "id:ID:14:Left\nstatus:Status:12:Center\ntarget:Target:24:Left\nattempts:Attempts:10:Right"
        );
    }

    #[test]
    fn compact_padding_constant_is_stable() {
        assert_eq!(
            Padding::COMPACT,
            Padding {
                top: 0,
                right: 1,
                bottom: 0,
                left: 1,
            }
        );
    }

    #[test]
    fn right_alignment_variant_is_exposed() {
        let columns = widgets::loop_queue_columns();
        assert_eq!(columns[3].align, TextAlign::Right);
    }

    #[test]
    fn input_translation_keymap_snapshot() {
        let snapshot = format!(
            "{:?}|{:?}|{:?}|{:?}",
            translate_input(&InputEvent::Key(KeyEvent::plain(Key::Up))),
            translate_input(&InputEvent::Key(KeyEvent::plain(Key::Enter))),
            translate_input(&InputEvent::Key(KeyEvent {
                key: Key::Char('/'),
                modifiers: Modifiers::none(),
            })),
            translate_input(&InputEvent::Key(KeyEvent {
                key: Key::Char('c'),
                modifiers: Modifiers {
                    shift: false,
                    ctrl: true,
                    alt: false,
                },
            })),
        );
        assert_eq!(snapshot, "MoveUp|Confirm|Search|Compose");
    }

    #[test]
    fn input_translation_mouse_wheel() {
        assert_eq!(
            translate_input(&InputEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Wheel(MouseWheelDirection::Up),
                column: 0,
                row: 0,
            })),
            UiAction::ScrollUp
        );
        assert_eq!(
            translate_input(&InputEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Wheel(MouseWheelDirection::Down),
                column: 0,
                row: 0,
            })),
            UiAction::ScrollDown
        );
    }

    #[test]
    fn input_translation_resize_refreshes() {
        assert_eq!(
            translate_input(&InputEvent::Resize(ResizeEvent {
                width: 120,
                height: 40,
            })),
            UiAction::Refresh
        );
    }

    #[test]
    fn fmail_widget_panel_snapshot() {
        let panels = [
            WidgetSpec::fmail_inbox_panel(),
            WidgetSpec::fmail_message_panel(),
            WidgetSpec::fmail_compose_panel(),
        ];
        let snapshot = format!(
            "{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}\n{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}\n{}|{}|{:?}|{:?}|{:?}|{}/{}/{}/{}",
            panels[0].id,
            panels[0].title,
            panels[0].border,
            panels[0].align,
            panels[0].emphasis,
            panels[0].padding.top,
            panels[0].padding.right,
            panels[0].padding.bottom,
            panels[0].padding.left,
            panels[1].id,
            panels[1].title,
            panels[1].border,
            panels[1].align,
            panels[1].emphasis,
            panels[1].padding.top,
            panels[1].padding.right,
            panels[1].padding.bottom,
            panels[1].padding.left,
            panels[2].id,
            panels[2].title,
            panels[2].border,
            panels[2].align,
            panels[2].emphasis,
            panels[2].padding.top,
            panels[2].padding.right,
            panels[2].padding.bottom,
            panels[2].padding.left,
        );
        assert_eq!(
            snapshot,
            "fmail.inbox|Inbox|Rounded|Left|Strong|1/2/1/2\nfmail.message|Message|Plain|Left|Normal|0/1/0/1\nfmail.compose|Compose|Heavy|Left|Subtle|0/1/0/1"
        );
    }

    #[test]
    fn fmail_inbox_columns_snapshot() {
        let columns = widgets::fmail_inbox_columns();
        let snapshot = format!(
            "{}:{}:{}:{:?}\n{}:{}:{}:{:?}\n{}:{}:{}:{:?}\n{}:{}:{}:{:?}",
            columns[0].key,
            columns[0].title,
            columns[0].width,
            columns[0].align,
            columns[1].key,
            columns[1].title,
            columns[1].width,
            columns[1].align,
            columns[2].key,
            columns[2].title,
            columns[2].width,
            columns[2].align,
            columns[3].key,
            columns[3].title,
            columns[3].width,
            columns[3].align,
        );
        assert_eq!(
            snapshot,
            "from:From:18:Left\nsubject:Subject:32:Left\nage:Age:8:Right\nstatus:Status:10:Center"
        );
    }

    #[test]
    fn styled_span_convenience_builders() {
        assert_eq!(
            StyledSpan::accent("hi").style,
            SpanStyle::Role(TextRole::Accent)
        );
        assert_eq!(
            StyledSpan::danger("err").style,
            SpanStyle::Role(TextRole::Danger)
        );
        assert_eq!(
            StyledSpan::muted("dim").style,
            SpanStyle::Role(TextRole::Muted)
        );
        assert_eq!(
            StyledSpan::success("ok").style,
            SpanStyle::Role(TextRole::Success)
        );
        assert_eq!(
            StyledSpan::warning("warn").style,
            SpanStyle::Role(TextRole::Warning)
        );
        assert_eq!(
            StyledSpan::info("note").style,
            SpanStyle::Role(TextRole::Info)
        );
        assert_eq!(
            StyledSpan::focus("sel").style,
            SpanStyle::Role(TextRole::Focus)
        );
        assert_eq!(
            StyledSpan::primary("txt").style,
            SpanStyle::Role(TextRole::Primary)
        );
    }

    #[test]
    fn owned_styled_span_as_span_roundtrips() {
        let owned = OwnedStyledSpan::role("hello", TextRole::Accent);
        let borrowed = owned.as_span();
        assert_eq!(borrowed.text, "hello");
        assert_eq!(borrowed.style, SpanStyle::Role(TextRole::Accent));
    }

    #[test]
    fn styled_line_push_and_plain_text() {
        let mut line = StyledLine::new();
        line.push_role("ERR", TextRole::Danger);
        line.push_role(" ", TextRole::Muted);
        line.push_role("ok", TextRole::Success);
        assert_eq!(line.len(), 3);
        assert_eq!(line.plain_text(), "ERR ok");
        assert_eq!(line.char_count(), 6);
    }

    #[test]
    fn styled_line_from_role_shorthand() {
        let line = StyledLine::from_role("status: running", TextRole::Info);
        assert_eq!(line.len(), 1);
        assert_eq!(line.plain_text(), "status: running");
    }

    #[test]
    fn styled_line_plain_shorthand() {
        let line = StyledLine::plain("hello world");
        assert_eq!(line.spans[0].style, SpanStyle::Role(TextRole::Primary));
    }

    #[test]
    fn styled_text_push_and_count() {
        let mut text = StyledText::new();
        text.push(StyledLine::plain("line 1"));
        text.push(StyledLine::plain("line 2"));
        assert_eq!(text.line_count(), 2);
        assert!(!text.is_empty());
    }

    #[test]
    fn draw_styled_line_renders_to_frame() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 10,
                height: 1,
            },
            theme,
        );
        let mut line = StyledLine::new();
        line.push_role("AB", TextRole::Accent);
        line.push_role("cd", TextRole::Muted);
        frame.draw_styled_line(0, 0, &line);
        assert_eq!(frame.row_text(0), "ABcd      ");
        assert_eq!(
            frame.cell(0, 0).map(|c| c.style.fg),
            Some(TermColor::Ansi256(theme.color(StyleToken::Accent)))
        );
        assert_eq!(
            frame.cell(2, 0).map(|c| c.style.fg),
            Some(TermColor::Ansi256(theme.color(StyleToken::Muted)))
        );
    }

    #[test]
    fn draw_styled_text_block_renders_multiple_lines() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 6,
                height: 3,
            },
            theme,
        );
        let mut text = StyledText::new();
        text.push(StyledLine::plain("line1"));
        text.push(StyledLine::from_role("line2", TextRole::Danger));
        text.push(StyledLine::plain("line3"));
        frame.draw_styled_text_block(0, 0, &text);
        assert_eq!(frame.row_text(0), "line1 ");
        assert_eq!(frame.row_text(1), "line2 ");
        assert_eq!(frame.row_text(2), "line3 ");
    }

    #[test]
    fn draw_styled_text_block_clips_to_frame_height() {
        let theme = ThemeSpec::default();
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 6,
                height: 1,
            },
            theme,
        );
        let mut text = StyledText::new();
        text.push(StyledLine::plain("row-0"));
        text.push(StyledLine::plain("row-1")); // should be clipped
        frame.draw_styled_text_block(0, 0, &text);
        assert_eq!(frame.row_text(0), "row-0 ");
    }

    #[test]
    fn plain_span_source_wraps_as_primary() {
        let source = PlainSpanSource;
        let line = source.style_line("hello world");
        assert_eq!(line.len(), 1);
        assert_eq!(line.spans[0].text, "hello world");
        assert_eq!(line.spans[0].style, SpanStyle::Role(TextRole::Primary));
    }

    #[test]
    fn plain_span_source_handles_multiline() {
        let source = PlainSpanSource;
        let text = source.style_text("line1\nline2\nline3");
        assert_eq!(text.line_count(), 3);
        assert_eq!(text.lines[1].plain_text(), "line2");
    }
}

#[cfg(all(test, feature = "frankentui-upstream"))]
mod upstream_bridge_tests {
    #![allow(clippy::expect_used)]

    use super::render::{CellStyle, TermColor};
    use super::style::{StyleToken, ThemeKind, ThemeSpec};
    use super::upstream_bridge::{
        cell_style_to_ftui_style, term_color_to_ftui_color, term_color_to_packed_rgba, token_style,
    };
    use super::upstream_ftui as ftui;

    #[test]
    fn term_color_conversion_preserves_variants() {
        assert_eq!(
            term_color_to_ftui_color(TermColor::Ansi256(203)),
            ftui::Color::Ansi256(203)
        );
        assert_eq!(
            term_color_to_ftui_color(TermColor::Rgb(10, 20, 30)),
            ftui::Color::rgb(10, 20, 30)
        );
    }

    #[test]
    fn term_color_to_packed_rgba_handles_ansi_and_rgb() {
        let ansi_rgb = ftui::Color::Ansi256(196).to_rgb();
        assert_eq!(
            term_color_to_packed_rgba(TermColor::Ansi256(196)),
            ftui::render::cell::PackedRgba::rgb(ansi_rgb.r, ansi_rgb.g, ansi_rgb.b)
        );
        assert_eq!(
            term_color_to_packed_rgba(TermColor::Rgb(1, 2, 3)),
            ftui::render::cell::PackedRgba::rgb(1, 2, 3)
        );
    }

    #[test]
    fn cell_style_conversion_maps_colors_and_attrs() {
        let style = CellStyle {
            fg: TermColor::Ansi256(51),
            bg: TermColor::Rgb(1, 2, 3),
            bold: true,
            dim: true,
            underline: true,
        };
        let converted = cell_style_to_ftui_style(style);

        let fg_rgb = ftui::Color::Ansi256(51).to_rgb();
        assert_eq!(
            converted.fg,
            Some(ftui::render::cell::PackedRgba::rgb(
                fg_rgb.r, fg_rgb.g, fg_rgb.b
            ))
        );
        assert_eq!(
            converted.bg,
            Some(ftui::render::cell::PackedRgba::rgb(1, 2, 3))
        );
        assert!(converted.has_attr(ftui::StyleFlags::BOLD));
        assert!(converted.has_attr(ftui::StyleFlags::DIM));
        assert!(converted.has_attr(ftui::StyleFlags::UNDERLINE));
    }

    #[test]
    fn token_style_applies_typography_policy() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);

        let accent = token_style(theme, StyleToken::Accent);
        assert!(accent.has_attr(ftui::StyleFlags::BOLD));

        let muted = token_style(theme, StyleToken::Muted);
        assert!(muted.has_attr(ftui::StyleFlags::DIM));

        let focus = token_style(theme, StyleToken::Focus);
        assert!(focus.has_attr(ftui::StyleFlags::UNDERLINE));
    }

    #[test]
    fn span_style_to_ftui_style_resolves_all_variants() {
        use super::render::{SpanStyle, TextRole};
        use super::upstream_bridge::span_style_to_ftui_style;

        let theme = ThemeSpec::for_kind(ThemeKind::Dark);

        // Role variant
        let accent_ftui = span_style_to_ftui_style(theme, SpanStyle::Role(TextRole::Accent));
        let accent_expected = token_style(theme, StyleToken::Accent);
        assert_eq!(accent_ftui.fg, accent_expected.fg);
        assert_eq!(accent_ftui.attrs, accent_expected.attrs);

        // Token variant
        let danger_ftui = span_style_to_ftui_style(theme, SpanStyle::Token(StyleToken::Danger));
        let danger_expected = token_style(theme, StyleToken::Danger);
        assert_eq!(danger_ftui.fg, danger_expected.fg);

        // Cell variant
        let cell = CellStyle {
            fg: TermColor::Ansi256(196),
            bg: TermColor::Rgb(1, 2, 3),
            bold: true,
            dim: false,
            underline: false,
        };
        let cell_ftui = span_style_to_ftui_style(theme, SpanStyle::Cell(cell));
        let cell_expected = cell_style_to_ftui_style(cell);
        assert_eq!(cell_ftui.fg, cell_expected.fg);
        assert_eq!(cell_ftui.attrs, cell_expected.attrs);
    }

    #[test]
    fn styled_line_to_ftui_line_produces_correct_spans() {
        use super::render::{OwnedStyledSpan, StyledLine, TextRole};
        use super::upstream_bridge::styled_line_to_ftui_line;

        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut line = StyledLine::new();
        line.push(OwnedStyledSpan::role("ok", TextRole::Success));
        line.push(OwnedStyledSpan::role(" ", TextRole::Muted));
        line.push(OwnedStyledSpan::role("ERR", TextRole::Danger));

        let ftui_line = styled_line_to_ftui_line(theme, &line);
        assert_eq!(ftui_line.len(), 3);
        assert_eq!(ftui_line.width(), 6);
    }

    #[test]
    fn styled_text_to_ftui_text_produces_correct_lines() {
        use super::render::{StyledLine, StyledText};
        use super::upstream_bridge::styled_text_to_ftui_text;

        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut text = StyledText::new();
        text.push(StyledLine::plain("line 1"));
        text.push(StyledLine::plain("line 2"));

        let ftui_text = styled_text_to_ftui_text(theme, &text);
        assert_eq!(ftui_text.height(), 2);
    }
}

#[cfg(all(test, feature = "frankentui-upstream"))]
mod upstream_widgets_tests {
    use super::upstream_widgets::{
        list_state, log_viewer, log_viewer_state, modal_state, notification_queue, QueueConfig,
    };

    #[test]
    fn widget_constructor_helpers_build_expected_defaults() {
        let mut viewer = log_viewer(3);
        viewer.push("a");
        viewer.push("b");
        viewer.push("c");
        viewer.push("d");
        assert_eq!(viewer.line_count(), 3);

        let list = list_state();
        assert_eq!(list.selected(), None);

        let log_state = log_viewer_state();
        assert_eq!(log_state.last_visible_lines, 0);

        let queue = notification_queue(QueueConfig::default().max_visible(2));
        assert_eq!(queue.visible_count(), 0);

        let modal = modal_state();
        assert!(modal.is_open());
    }
}

#[cfg(all(test, feature = "frankentui-upstream"))]
mod upstream_primitives_tests {
    use super::upstream_primitives::{
        badge, forge_badge, forge_status_line, forge_table, horizontal_flex, status_line,
        table_state, vertical_flex, Constraint, TableRow,
    };
    use super::{style::ThemeSpec, upstream_ftui as ftui};
    use ftui::core::geometry::Rect;
    use ftui::render::frame::Frame;
    use ftui::widgets::{StatefulWidget, Widget};

    #[test]
    fn primitive_constructor_helpers_build_expected_shapes() {
        let table = table_state();
        assert_eq!(table.selected, None);

        let _status = status_line().left(super::upstream_ftui::widgets::StatusItem::text("ok"));
        let _badge = badge("running");

        let horizontal = horizontal_flex([Constraint::Fixed(10), Constraint::Fill]);
        assert_eq!(horizontal.constraint_count(), 2);

        let vertical = vertical_flex([Constraint::Fixed(1), Constraint::Fixed(2)]);
        assert_eq!(vertical.constraint_count(), 2);
    }

    #[test]
    fn forge_badge_applies_theme_token_style() {
        let theme = ThemeSpec::for_kind(super::style::ThemeKind::Dark);
        let badge = forge_badge("ERR", theme, super::style::StyleToken::Danger);

        let mut pool = ftui::render::grapheme_pool::GraphemePool::new();
        let mut frame = Frame::new(8, 1, &mut pool);
        badge.render(Rect::new(0, 0, 8, 1), &mut frame);

        let fg = match frame.buffer.get(1, 0) {
            Some(cell) => cell.fg,
            None => panic!("cell"),
        };
        let danger_rgb =
            ftui::Color::Ansi256(theme.color(super::style::StyleToken::Danger)).to_rgb();
        assert_eq!(
            fg,
            ftui::render::cell::PackedRgba::rgb(danger_rgb.r, danger_rgb.g, danger_rgb.b)
        );
    }

    #[test]
    fn forge_statusline_and_table_render_smoke() {
        let theme = ThemeSpec::for_kind(super::style::ThemeKind::Dark);

        let status = forge_status_line(theme)
            .left(ftui::widgets::StatusItem::text("forge"))
            .right(ftui::widgets::StatusItem::key_hint("q", "quit"));
        let mut pool = ftui::render::grapheme_pool::GraphemePool::new();
        let mut frame = Frame::new(40, 4, &mut pool);
        status.render(Rect::new(0, 0, 40, 1), &mut frame);
        let first_char = match frame.buffer.get(0, 0) {
            Some(cell) => cell.content.as_char(),
            None => panic!("status cell"),
        };
        assert_eq!(first_char, Some('f'));

        let rows = vec![TableRow::new(vec!["id-1", "running"])];
        let table = forge_table(rows, [Constraint::Fixed(10), Constraint::Fill], theme);
        let mut table_state = table_state();
        StatefulWidget::render(&table, Rect::new(0, 1, 40, 3), &mut frame, &mut table_state);
    }
}

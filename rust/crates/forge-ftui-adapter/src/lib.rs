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

/// Render and frame primitives consumed by Forge TUI crates.
pub mod render {
    use super::style::{StyleToken, ThemeSpec};

    /// Frame dimensions in terminal cells.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FrameSize {
        pub width: usize,
        pub height: usize,
    }

    /// Cell style represented as terminal color indexes and text attributes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellStyle {
        pub fg: u8,
        pub bg: u8,
        pub bold: bool,
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
                    fg: theme.color(StyleToken::Foreground),
                    bg: theme.color(StyleToken::Background),
                    bold: false,
                },
            };
            Self {
                size,
                cells: vec![default_cell; size.width.saturating_mul(size.height)],
                theme,
            }
        }

        #[must_use]
        pub fn size(&self) -> FrameSize {
            self.size
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
        pub fn draw_text(&mut self, x: usize, y: usize, text: &str, role: TextRole) {
            if y >= self.size.height || x >= self.size.width {
                return;
            }
            let fg = self.color_for_role(role);
            let bg = self.theme.color(StyleToken::Background);
            let bold = matches!(role, TextRole::Accent);
            for (offset, glyph) in text.chars().enumerate() {
                let col = x + offset;
                if col >= self.size.width {
                    break;
                }
                self.cells[y * self.size.width + col] = FrameCell {
                    glyph,
                    style: CellStyle { fg, bg, bold },
                };
            }
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

        fn color_for_role(&self, role: TextRole) -> u8 {
            match role {
                TextRole::Primary => self.theme.color(StyleToken::Foreground),
                TextRole::Muted => self.theme.color(StyleToken::Muted),
                TextRole::Accent => self.theme.color(StyleToken::Accent),
                TextRole::Success => self.theme.color(StyleToken::Success),
                TextRole::Danger => self.theme.color(StyleToken::Danger),
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

    /// Canonical mouse event.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MouseEvent {
        pub wheel: Option<MouseWheelDirection>,
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
                    wheel: Some(MouseWheelDirection::Up),
                }) => UiAction::ScrollUp,
                InputEvent::Mouse(MouseEvent {
                    wheel: Some(MouseWheelDirection::Down),
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
        translate_input, InputEvent, Key, KeyEvent, Modifiers, MouseEvent, MouseWheelDirection,
        ResizeEvent, UiAction,
    };
    use super::render::{FrameSize, RenderFrame, TextRole};
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
    fn render_frame_uses_role_color_tokens() {
        let theme = ThemeSpec::for_kind(ThemeKind::Dark);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 4,
                height: 1,
            },
            theme,
        );
        frame.draw_text(1, 0, "!", TextRole::Danger);
        let fg = frame.cell(1, 0).map(|cell| cell.style.fg);
        assert_eq!(fg, Some(theme.color(StyleToken::Danger)));
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
                wheel: Some(MouseWheelDirection::Up),
            })),
            UiAction::ScrollUp
        );
        assert_eq!(
            translate_input(&InputEvent::Mouse(MouseEvent {
                wheel: Some(MouseWheelDirection::Down),
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
}

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
            let bg = TermColor::Ansi256(self.theme.color(StyleToken::Background));
            let (bold, dim, underline) = self.style_for_role(role);
            for (offset, glyph) in text.chars().enumerate() {
                let col = x + offset;
                if col >= self.size.width {
                    break;
                }
                self.cells[y * self.size.width + col] = FrameCell {
                    glyph,
                    style: CellStyle {
                        fg,
                        bg,
                        bold,
                        dim,
                        underline,
                    },
                };
            }
        }

        /// Draw text with explicit foreground/background colors.
        pub fn draw_styled_text(
            &mut self,
            x: usize,
            y: usize,
            text: &str,
            fg: TermColor,
            bg: TermColor,
            bold: bool,
        ) {
            if y >= self.size.height || x >= self.size.width {
                return;
            }
            let style = CellStyle {
                fg,
                bg,
                bold,
                dim: false,
                underline: false,
            };
            for (offset, glyph) in text.chars().enumerate() {
                let col = x + offset;
                if col >= self.size.width {
                    break;
                }
                self.cells[y * self.size.width + col] = FrameCell { glyph, style };
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
        pub fn draw_horizontal_rule(
            &mut self,
            x: usize,
            y: usize,
            width: usize,
            role: TextRole,
        ) {
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
                        glyph: '─',
                        style,
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
        pub fn draw_text_in_rect(
            &mut self,
            rect: Rect,
            x_offset: usize,
            y_offset: usize,
            text: &str,
            role: TextRole,
        ) {
            let abs_x = rect.x + x_offset;
            let abs_y = rect.y + y_offset;
            if abs_y >= rect.y + rect.height {
                return;
            }
            let max_chars = (rect.x + rect.width).saturating_sub(abs_x);
            let fg = self.color_for_role(role);
            let bg = TermColor::Ansi256(self.theme.color(StyleToken::Background));
            let (bold, dim, underline) = self.style_for_role(role);
            for (offset, glyph) in text.chars().take(max_chars).enumerate() {
                let col = abs_x + offset;
                if col >= self.size.width || abs_y >= self.size.height {
                    break;
                }
                self.cells[abs_y * self.size.width + col] = FrameCell {
                    glyph,
                    style: CellStyle {
                        fg,
                        bg,
                        bold,
                        dim,
                        underline,
                    },
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

//! Highlighting specification: token taxonomy, precedence, and fallback rules.
//!
//! This module defines the deterministic token model used by `rforge logs`
//! (and eventually the TUI logs tab) for harness-grade syntax highlighting.
//!
//! Design principles:
//! - Every span of log text maps to exactly one [`TokenKind`].
//! - Precedence is a simple numeric ordering — lower number wins.
//! - Unknown content always falls through to [`TokenKind::Plain`].
//! - No-color mode replaces style with text-only signifiers (prefix markers).

use std::fmt;

// ---------------------------------------------------------------------------
// Token taxonomy
// ---------------------------------------------------------------------------

/// Semantic token types for harness log highlighting.
///
/// Ordered by intended visual distinctiveness (most distinctive first).
/// The discriminant values encode default precedence: lower wins in overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TokenKind {
    // ── Structural markers (highest precedence) ──────────────────────
    /// `[claude:init]`, `[claude:result]`, `[claude:error]`, `[claude:event]`
    /// or harness header lines like `OpenAI Codex v0.80.0`.
    SectionHeader = 0,

    /// Role transition markers: `user`, `assistant`, `codex`, `thinking`, etc.
    RoleMarker = 1,

    // ── Severity / status ────────────────────────────────────────────
    /// Error lines: `error:`, `FAILED`, `panicked`, exit-code nonzero.
    Error = 2,

    /// Warning lines: `⚠️`, `warning:`, `WARN`.
    Warning = 3,

    // ── Diff tokens ──────────────────────────────────────────────────
    /// Diff header: `diff --git a/... b/...`, `--- a/...`, `+++ b/...`.
    DiffHeader = 4,

    /// Hunk range: `@@ -10,7 +10,12 @@`.
    DiffHunk = 5,

    /// Added line: `+...`.
    DiffAdd = 6,

    /// Removed line: `-...`.
    DiffDel = 7,

    // ── Code structure ───────────────────────────────────────────────
    /// Code fence delimiter and language hint: `` ```rust ``, `` ``` ``.
    CodeFence = 8,

    /// Content inside a fenced code block (inherits fence context).
    CodeContent = 9,

    // ── Structured data ──────────────────────────────────────────────
    /// JSON/YAML/TOML key (left of `:` or `=`).
    DataKey = 10,

    /// JSON/YAML/TOML value (right of `:` or `=`).
    DataValue = 11,

    // ── Inline semantic spans ────────────────────────────────────────
    /// Timestamp: RFC 3339, ISO 8601, or `[2026-02-09T16:00:01Z]` wrapper.
    Timestamp = 12,

    /// Shell command prompt or invocation: `$`, `❯`, tool invocations.
    CommandPrompt = 13,

    /// Stack frame: function name + optional `at file:line`.
    StackFrame = 14,

    /// File path with optional line number: `src/auth.rs:45`.
    PathLine = 15,

    /// URL: `http://`, `https://`, `file://`.
    Url = 16,

    /// Numeric literal (token counts, durations, exit codes).
    Number = 17,

    /// String literal (quoted values in key=value pairs).
    StringLiteral = 18,

    // ── Catch-all ────────────────────────────────────────────────────
    /// Unclassified text — rendered as-is.
    Plain = 255,
}

/// Total number of classified token kinds (excludes `Plain`).
pub const TOKEN_KIND_COUNT: usize = 19;

/// All classified token kinds in precedence order (lowest precedence value first).
pub const TOKEN_KINDS_BY_PRECEDENCE: [TokenKind; TOKEN_KIND_COUNT] = [
    TokenKind::SectionHeader,
    TokenKind::RoleMarker,
    TokenKind::Error,
    TokenKind::Warning,
    TokenKind::DiffHeader,
    TokenKind::DiffHunk,
    TokenKind::DiffAdd,
    TokenKind::DiffDel,
    TokenKind::CodeFence,
    TokenKind::CodeContent,
    TokenKind::DataKey,
    TokenKind::DataValue,
    TokenKind::Timestamp,
    TokenKind::CommandPrompt,
    TokenKind::StackFrame,
    TokenKind::PathLine,
    TokenKind::Url,
    TokenKind::Number,
    TokenKind::StringLiteral,
];

impl TokenKind {
    /// Numeric precedence (lower wins in overlap resolution).
    #[must_use]
    pub fn precedence(self) -> u8 {
        self as u8
    }

    /// Stable slug for serialization and snapshot tests.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::SectionHeader => "section-header",
            Self::RoleMarker => "role-marker",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::DiffHeader => "diff-header",
            Self::DiffHunk => "diff-hunk",
            Self::DiffAdd => "diff-add",
            Self::DiffDel => "diff-del",
            Self::CodeFence => "code-fence",
            Self::CodeContent => "code-content",
            Self::DataKey => "data-key",
            Self::DataValue => "data-value",
            Self::Timestamp => "timestamp",
            Self::CommandPrompt => "command-prompt",
            Self::StackFrame => "stack-frame",
            Self::PathLine => "path-line",
            Self::Url => "url",
            Self::Number => "number",
            Self::StringLiteral => "string-literal",
            Self::Plain => "plain",
        }
    }

    /// Parse from slug. Returns `None` for unknown slugs.
    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "section-header" => Some(Self::SectionHeader),
            "role-marker" => Some(Self::RoleMarker),
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "diff-header" => Some(Self::DiffHeader),
            "diff-hunk" => Some(Self::DiffHunk),
            "diff-add" => Some(Self::DiffAdd),
            "diff-del" => Some(Self::DiffDel),
            "code-fence" => Some(Self::CodeFence),
            "code-content" => Some(Self::CodeContent),
            "data-key" => Some(Self::DataKey),
            "data-value" => Some(Self::DataValue),
            "timestamp" => Some(Self::Timestamp),
            "command-prompt" => Some(Self::CommandPrompt),
            "stack-frame" => Some(Self::StackFrame),
            "path-line" => Some(Self::PathLine),
            "url" => Some(Self::Url),
            "number" => Some(Self::Number),
            "string-literal" => Some(Self::StringLiteral),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }

    /// Whether this token kind represents diff content.
    #[must_use]
    pub fn is_diff(self) -> bool {
        matches!(
            self,
            Self::DiffHeader | Self::DiffHunk | Self::DiffAdd | Self::DiffDel
        )
    }

    /// Whether this token kind represents an error or warning.
    #[must_use]
    pub fn is_severity(self) -> bool {
        matches!(self, Self::Error | Self::Warning)
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

// ---------------------------------------------------------------------------
// Token span
// ---------------------------------------------------------------------------

/// A contiguous span of text classified as a single token kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    /// Byte offset from start of the line.
    pub start: usize,
    /// Byte length of the span.
    pub len: usize,
    /// Semantic classification.
    pub kind: TokenKind,
}

impl TokenSpan {
    /// Byte offset of one past the last byte.
    #[must_use]
    pub fn end(&self) -> usize {
        self.start + self.len
    }
}

// ---------------------------------------------------------------------------
// Precedence resolution
// ---------------------------------------------------------------------------

/// Given two overlapping token kinds, return the winner (lower precedence value).
///
/// Tie-break: when both have equal precedence, the first (leftmost / already-
/// assigned) token wins. This ensures deterministic output regardless of
/// classification order.
#[must_use]
pub fn resolve_precedence(existing: TokenKind, candidate: TokenKind) -> TokenKind {
    if candidate.precedence() < existing.precedence() {
        candidate
    } else {
        existing
    }
}

// ---------------------------------------------------------------------------
// ANSI style mapping
// ---------------------------------------------------------------------------

/// ANSI SGR codes for each token kind (16-color baseline).
///
/// These are the escape sequences applied when color is enabled.
/// The mapping intentionally uses only ANSI 16-color codes so that it works
/// on all terminals without capability detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiStyle {
    /// ANSI SGR parameter string (e.g. `"1;31"` for bold red).
    pub sgr: &'static str,
    /// Human-readable label for tests/docs.
    pub label: &'static str,
}

/// Supported terminal color capability tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorCapability {
    Ansi16,
    Ansi256,
    TrueColor,
}

impl TerminalColorCapability {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Ansi16 => "ansi16",
            Self::Ansi256 => "ansi256",
            Self::TrueColor => "truecolor",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ansi16" | "16" => Some(Self::Ansi16),
            "ansi256" | "256" => Some(Self::Ansi256),
            "truecolor" | "24bit" => Some(Self::TrueColor),
            _ => None,
        }
    }
}

/// Background tone hint used to keep contrast readable across light/dark terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalTone {
    Dark,
    Light,
}

impl TerminalTone {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

/// Full highlighting theme decision: color on/off, capability tier, and tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightTheme {
    pub use_color: bool,
    pub capability: TerminalColorCapability,
    pub tone: TerminalTone,
}

impl Default for HighlightTheme {
    fn default() -> Self {
        Self {
            use_color: true,
            capability: TerminalColorCapability::Ansi16,
            tone: TerminalTone::Dark,
        }
    }
}

/// Resolved environment hints used to derive [`HighlightTheme`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThemeEnvHints {
    pub term: Option<String>,
    pub colorterm: Option<String>,
    pub colorfgbg: Option<String>,
    pub no_color_env: bool,
    pub force_color_env: bool,
    pub capability_override: Option<String>,
    pub tone_override: Option<String>,
}

impl ThemeEnvHints {
    #[must_use]
    pub fn detect() -> Self {
        Self {
            term: std::env::var("TERM").ok(),
            colorterm: std::env::var("COLORTERM").ok(),
            colorfgbg: std::env::var("COLORFGBG").ok(),
            no_color_env: std::env::var_os("NO_COLOR").is_some(),
            force_color_env: std::env::var_os("FORCE_COLOR").is_some()
                || std::env::var_os("CLICOLOR_FORCE").is_some(),
            capability_override: std::env::var("FORGE_LOG_COLOR_CAPABILITY").ok(),
            tone_override: std::env::var("FORGE_LOG_COLOR_SCHEME").ok(),
        }
    }
}

/// Resolve theme policy from CLI/env inputs.
///
/// Explicit `--no-color` and `NO_COLOR` disable color even when force-color
/// env vars are present.
#[must_use]
pub fn resolve_theme(no_color_flag: bool, hints: &ThemeEnvHints) -> HighlightTheme {
    let no_color_requested = no_color_flag || hints.no_color_env;
    // Explicit --no-color/NO_COLOR always wins. FORCE_COLOR only matters when
    // color is otherwise enabled.
    let use_color = !no_color_requested;

    let capability = hints
        .capability_override
        .as_deref()
        .and_then(TerminalColorCapability::from_slug)
        .unwrap_or_else(|| {
            detect_terminal_capability(hints.term.as_deref(), hints.colorterm.as_deref())
        });

    let tone = hints
        .tone_override
        .as_deref()
        .and_then(TerminalTone::from_slug)
        .or_else(|| detect_terminal_tone(hints.colorfgbg.as_deref()))
        .unwrap_or(TerminalTone::Dark);

    HighlightTheme {
        use_color,
        capability,
        tone,
    }
}

/// Resolve theme policy from process environment + explicit `--no-color` flag.
#[must_use]
pub fn resolve_theme_from_env(no_color_flag: bool) -> HighlightTheme {
    let hints = ThemeEnvHints::detect();
    resolve_theme(no_color_flag, &hints)
}

fn detect_terminal_capability(
    term: Option<&str>,
    colorterm: Option<&str>,
) -> TerminalColorCapability {
    if colorterm.is_some_and(|value| contains_ci(value, "truecolor") || contains_ci(value, "24bit"))
    {
        return TerminalColorCapability::TrueColor;
    }
    if term.is_some_and(|value| contains_ci(value, "truecolor") || contains_ci(value, "24bit")) {
        return TerminalColorCapability::TrueColor;
    }
    if term.is_some_and(|value| contains_ci(value, "256color")) {
        return TerminalColorCapability::Ansi256;
    }
    if colorterm.is_some_and(|value| contains_ci(value, "256")) {
        return TerminalColorCapability::Ansi256;
    }
    TerminalColorCapability::Ansi16
}

fn detect_terminal_tone(colorfgbg: Option<&str>) -> Option<TerminalTone> {
    let bg_index = colorfgbg?
        .split(';')
        .next_back()
        .and_then(|value| value.trim().parse::<u8>().ok())?;
    if bg_index >= 7 {
        Some(TerminalTone::Light)
    } else {
        Some(TerminalTone::Dark)
    }
}

fn contains_ci(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

/// Map a token kind to its baseline ANSI 16-color style.
#[must_use]
pub fn ansi_style_for(kind: TokenKind) -> AnsiStyle {
    ansi_style_for_theme(kind, TerminalColorCapability::Ansi16, TerminalTone::Dark)
}

/// Capability/tone-aware style map.
#[must_use]
pub fn ansi_style_for_theme(
    kind: TokenKind,
    capability: TerminalColorCapability,
    tone: TerminalTone,
) -> AnsiStyle {
    match capability {
        TerminalColorCapability::Ansi16 => ansi16_style_for(kind, tone),
        TerminalColorCapability::Ansi256 => ansi256_style_for(kind, tone),
        TerminalColorCapability::TrueColor => truecolor_style_for(kind, tone),
    }
}

fn ansi16_style_for(kind: TokenKind, tone: TerminalTone) -> AnsiStyle {
    match tone {
        TerminalTone::Dark => ansi16_dark_style_for(kind),
        TerminalTone::Light => ansi16_light_style_for(kind),
    }
}

fn ansi16_dark_style_for(kind: TokenKind) -> AnsiStyle {
    match kind {
        TokenKind::SectionHeader => AnsiStyle {
            sgr: "1;36",
            label: "bold cyan",
        },
        TokenKind::RoleMarker => AnsiStyle {
            sgr: "1;35",
            label: "bold magenta",
        },
        TokenKind::Error => AnsiStyle {
            sgr: "1;31",
            label: "bold red",
        },
        TokenKind::Warning => AnsiStyle {
            sgr: "33",
            label: "yellow",
        },
        TokenKind::DiffHeader => AnsiStyle {
            sgr: "1",
            label: "bold",
        },
        TokenKind::DiffHunk => AnsiStyle {
            sgr: "36",
            label: "cyan",
        },
        TokenKind::DiffAdd => AnsiStyle {
            sgr: "32",
            label: "green",
        },
        TokenKind::DiffDel => AnsiStyle {
            sgr: "31",
            label: "red",
        },
        TokenKind::CodeFence => AnsiStyle {
            sgr: "2;33",
            label: "dim yellow",
        },
        TokenKind::CodeContent => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
        TokenKind::DataKey => AnsiStyle {
            sgr: "34",
            label: "blue",
        },
        TokenKind::DataValue => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
        TokenKind::Timestamp => AnsiStyle {
            sgr: "2",
            label: "dim",
        },
        TokenKind::CommandPrompt => AnsiStyle {
            sgr: "1;33",
            label: "bold yellow",
        },
        TokenKind::StackFrame => AnsiStyle {
            sgr: "2;31",
            label: "dim red",
        },
        TokenKind::PathLine => AnsiStyle {
            sgr: "4;36",
            label: "underline cyan",
        },
        TokenKind::Url => AnsiStyle {
            sgr: "4;34",
            label: "underline blue",
        },
        TokenKind::Number => AnsiStyle {
            sgr: "33",
            label: "yellow",
        },
        TokenKind::StringLiteral => AnsiStyle {
            sgr: "32",
            label: "green",
        },
        TokenKind::Plain => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
    }
}

fn ansi16_light_style_for(kind: TokenKind) -> AnsiStyle {
    match kind {
        TokenKind::SectionHeader => AnsiStyle {
            sgr: "1;34",
            label: "bold blue",
        },
        TokenKind::RoleMarker => AnsiStyle {
            sgr: "1;35",
            label: "bold magenta",
        },
        TokenKind::Error => AnsiStyle {
            sgr: "1;31",
            label: "bold red",
        },
        TokenKind::Warning => AnsiStyle {
            sgr: "1;35",
            label: "bold magenta",
        },
        TokenKind::DiffHeader => AnsiStyle {
            sgr: "1",
            label: "bold",
        },
        TokenKind::DiffHunk => AnsiStyle {
            sgr: "34",
            label: "blue",
        },
        TokenKind::DiffAdd => AnsiStyle {
            sgr: "32",
            label: "green",
        },
        TokenKind::DiffDel => AnsiStyle {
            sgr: "31",
            label: "red",
        },
        TokenKind::CodeFence => AnsiStyle {
            sgr: "2;35",
            label: "dim magenta",
        },
        TokenKind::CodeContent => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
        TokenKind::DataKey => AnsiStyle {
            sgr: "35",
            label: "magenta",
        },
        TokenKind::DataValue => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
        TokenKind::Timestamp => AnsiStyle {
            sgr: "2",
            label: "dim",
        },
        TokenKind::CommandPrompt => AnsiStyle {
            sgr: "1;34",
            label: "bold blue",
        },
        TokenKind::StackFrame => AnsiStyle {
            sgr: "2;31",
            label: "dim red",
        },
        TokenKind::PathLine => AnsiStyle {
            sgr: "4;34",
            label: "underline blue",
        },
        TokenKind::Url => AnsiStyle {
            sgr: "4;34",
            label: "underline blue",
        },
        TokenKind::Number => AnsiStyle {
            sgr: "35",
            label: "magenta",
        },
        TokenKind::StringLiteral => AnsiStyle {
            sgr: "32",
            label: "green",
        },
        TokenKind::Plain => AnsiStyle {
            sgr: "0",
            label: "reset",
        },
    }
}

fn ansi256_style_for(kind: TokenKind, tone: TerminalTone) -> AnsiStyle {
    match tone {
        TerminalTone::Dark => match kind {
            TokenKind::SectionHeader => AnsiStyle {
                sgr: "1;38;5;45",
                label: "bold cyan-45",
            },
            TokenKind::RoleMarker => AnsiStyle {
                sgr: "1;38;5;141",
                label: "bold purple-141",
            },
            TokenKind::Error => AnsiStyle {
                sgr: "1;38;5;203",
                label: "bold red-203",
            },
            TokenKind::Warning => AnsiStyle {
                sgr: "38;5;214",
                label: "orange-214",
            },
            TokenKind::DiffHeader => AnsiStyle {
                sgr: "1",
                label: "bold",
            },
            TokenKind::DiffHunk => AnsiStyle {
                sgr: "38;5;81",
                label: "cyan-81",
            },
            TokenKind::DiffAdd => AnsiStyle {
                sgr: "38;5;78",
                label: "green-78",
            },
            TokenKind::DiffDel => AnsiStyle {
                sgr: "38;5;167",
                label: "red-167",
            },
            TokenKind::CodeFence => AnsiStyle {
                sgr: "2;38;5;179",
                label: "dim amber-179",
            },
            TokenKind::CodeContent => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::DataKey => AnsiStyle {
                sgr: "38;5;75",
                label: "blue-75",
            },
            TokenKind::DataValue => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::Timestamp => AnsiStyle {
                sgr: "2",
                label: "dim",
            },
            TokenKind::CommandPrompt => AnsiStyle {
                sgr: "1;38;5;220",
                label: "bold yellow-220",
            },
            TokenKind::StackFrame => AnsiStyle {
                sgr: "2;38;5;167",
                label: "dim red-167",
            },
            TokenKind::PathLine => AnsiStyle {
                sgr: "4;38;5;81",
                label: "underline cyan-81",
            },
            TokenKind::Url => AnsiStyle {
                sgr: "4;38;5;75",
                label: "underline blue-75",
            },
            TokenKind::Number => AnsiStyle {
                sgr: "38;5;221",
                label: "yellow-221",
            },
            TokenKind::StringLiteral => AnsiStyle {
                sgr: "38;5;114",
                label: "green-114",
            },
            TokenKind::Plain => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
        },
        TerminalTone::Light => match kind {
            TokenKind::SectionHeader => AnsiStyle {
                sgr: "1;38;5;25",
                label: "bold blue-25",
            },
            TokenKind::RoleMarker => AnsiStyle {
                sgr: "1;38;5;90",
                label: "bold purple-90",
            },
            TokenKind::Error => AnsiStyle {
                sgr: "1;38;5;124",
                label: "bold red-124",
            },
            TokenKind::Warning => AnsiStyle {
                sgr: "38;5;130",
                label: "brown-130",
            },
            TokenKind::DiffHeader => AnsiStyle {
                sgr: "1",
                label: "bold",
            },
            TokenKind::DiffHunk => AnsiStyle {
                sgr: "38;5;31",
                label: "blue-31",
            },
            TokenKind::DiffAdd => AnsiStyle {
                sgr: "38;5;28",
                label: "green-28",
            },
            TokenKind::DiffDel => AnsiStyle {
                sgr: "38;5;124",
                label: "red-124",
            },
            TokenKind::CodeFence => AnsiStyle {
                sgr: "2;38;5;130",
                label: "dim brown-130",
            },
            TokenKind::CodeContent => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::DataKey => AnsiStyle {
                sgr: "38;5;24",
                label: "blue-24",
            },
            TokenKind::DataValue => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::Timestamp => AnsiStyle {
                sgr: "2",
                label: "dim",
            },
            TokenKind::CommandPrompt => AnsiStyle {
                sgr: "1;38;5;25",
                label: "bold blue-25",
            },
            TokenKind::StackFrame => AnsiStyle {
                sgr: "2;38;5;124",
                label: "dim red-124",
            },
            TokenKind::PathLine => AnsiStyle {
                sgr: "4;38;5;25",
                label: "underline blue-25",
            },
            TokenKind::Url => AnsiStyle {
                sgr: "4;38;5;25",
                label: "underline blue-25",
            },
            TokenKind::Number => AnsiStyle {
                sgr: "38;5;130",
                label: "brown-130",
            },
            TokenKind::StringLiteral => AnsiStyle {
                sgr: "38;5;28",
                label: "green-28",
            },
            TokenKind::Plain => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
        },
    }
}

fn truecolor_style_for(kind: TokenKind, tone: TerminalTone) -> AnsiStyle {
    match tone {
        TerminalTone::Dark => match kind {
            TokenKind::SectionHeader => AnsiStyle {
                sgr: "1;38;2;80;200;255",
                label: "bold cyan-rgb",
            },
            TokenKind::RoleMarker => AnsiStyle {
                sgr: "1;38;2;190;135;255",
                label: "bold purple-rgb",
            },
            TokenKind::Error => AnsiStyle {
                sgr: "1;38;2;255;95;95",
                label: "bold red-rgb",
            },
            TokenKind::Warning => AnsiStyle {
                sgr: "38;2;255;196;107",
                label: "amber-rgb",
            },
            TokenKind::DiffHeader => AnsiStyle {
                sgr: "1",
                label: "bold",
            },
            TokenKind::DiffHunk => AnsiStyle {
                sgr: "38;2;110;190;255",
                label: "cyan-rgb",
            },
            TokenKind::DiffAdd => AnsiStyle {
                sgr: "38;2;111;214;146",
                label: "green-rgb",
            },
            TokenKind::DiffDel => AnsiStyle {
                sgr: "38;2;240;110;110",
                label: "red-rgb",
            },
            TokenKind::CodeFence => AnsiStyle {
                sgr: "2;38;2;224;184;88",
                label: "dim amber-rgb",
            },
            TokenKind::CodeContent => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::DataKey => AnsiStyle {
                sgr: "38;2;120;170;255",
                label: "blue-rgb",
            },
            TokenKind::DataValue => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::Timestamp => AnsiStyle {
                sgr: "2",
                label: "dim",
            },
            TokenKind::CommandPrompt => AnsiStyle {
                sgr: "1;38;2;255;215;120",
                label: "bold amber-rgb",
            },
            TokenKind::StackFrame => AnsiStyle {
                sgr: "2;38;2;230;118;118",
                label: "dim red-rgb",
            },
            TokenKind::PathLine => AnsiStyle {
                sgr: "4;38;2;110;190;255",
                label: "underline cyan-rgb",
            },
            TokenKind::Url => AnsiStyle {
                sgr: "4;38;2;120;170;255",
                label: "underline blue-rgb",
            },
            TokenKind::Number => AnsiStyle {
                sgr: "38;2;255;196;107",
                label: "amber-rgb",
            },
            TokenKind::StringLiteral => AnsiStyle {
                sgr: "38;2;111;214;146",
                label: "green-rgb",
            },
            TokenKind::Plain => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
        },
        TerminalTone::Light => match kind {
            TokenKind::SectionHeader => AnsiStyle {
                sgr: "1;38;2;0;95;175",
                label: "bold blue-rgb",
            },
            TokenKind::RoleMarker => AnsiStyle {
                sgr: "1;38;2;108;0;168",
                label: "bold purple-rgb",
            },
            TokenKind::Error => AnsiStyle {
                sgr: "1;38;2;170;0;0",
                label: "bold red-rgb",
            },
            TokenKind::Warning => AnsiStyle {
                sgr: "38;2;140;90;0",
                label: "brown-rgb",
            },
            TokenKind::DiffHeader => AnsiStyle {
                sgr: "1",
                label: "bold",
            },
            TokenKind::DiffHunk => AnsiStyle {
                sgr: "38;2;0;110;184",
                label: "blue-rgb",
            },
            TokenKind::DiffAdd => AnsiStyle {
                sgr: "38;2;0;125;45",
                label: "green-rgb",
            },
            TokenKind::DiffDel => AnsiStyle {
                sgr: "38;2;170;0;0",
                label: "red-rgb",
            },
            TokenKind::CodeFence => AnsiStyle {
                sgr: "2;38;2;140;90;0",
                label: "dim brown-rgb",
            },
            TokenKind::CodeContent => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::DataKey => AnsiStyle {
                sgr: "38;2;0;95;175",
                label: "blue-rgb",
            },
            TokenKind::DataValue => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
            TokenKind::Timestamp => AnsiStyle {
                sgr: "2",
                label: "dim",
            },
            TokenKind::CommandPrompt => AnsiStyle {
                sgr: "1;38;2;0;95;175",
                label: "bold blue-rgb",
            },
            TokenKind::StackFrame => AnsiStyle {
                sgr: "2;38;2;170;0;0",
                label: "dim red-rgb",
            },
            TokenKind::PathLine => AnsiStyle {
                sgr: "4;38;2;0;95;175",
                label: "underline blue-rgb",
            },
            TokenKind::Url => AnsiStyle {
                sgr: "4;38;2;0;95;175",
                label: "underline blue-rgb",
            },
            TokenKind::Number => AnsiStyle {
                sgr: "38;2;140;90;0",
                label: "brown-rgb",
            },
            TokenKind::StringLiteral => AnsiStyle {
                sgr: "38;2;0;125;45",
                label: "green-rgb",
            },
            TokenKind::Plain => AnsiStyle {
                sgr: "0",
                label: "reset",
            },
        },
    }
}

// ---------------------------------------------------------------------------
// No-color fallback
// ---------------------------------------------------------------------------

/// Text-only signifier prepended/used when color is disabled (NO_COLOR / --no-color).
///
/// Returns `None` when no textual signifier is needed (the content is
/// self-describing). Returns `Some(prefix)` for tokens that lose meaning
/// without color (e.g. diff add/del lines already have `+`/`-` prefixes,
/// so they return `None`).
#[must_use]
pub fn no_color_signifier(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Error => Some("[ERROR] "),
        TokenKind::Warning => Some("[WARN] "),
        TokenKind::SectionHeader => Some("== "),
        TokenKind::RoleMarker => Some(">> "),
        TokenKind::CommandPrompt => Some("$ "),
        // Diff lines are self-describing (+/-/@@/diff).
        TokenKind::DiffHeader | TokenKind::DiffHunk | TokenKind::DiffAdd | TokenKind::DiffDel => {
            None
        }
        // Code fences are self-describing (```).
        TokenKind::CodeFence | TokenKind::CodeContent => None,
        // Timestamps, paths, URLs are self-describing.
        TokenKind::Timestamp
        | TokenKind::PathLine
        | TokenKind::Url
        | TokenKind::Number
        | TokenKind::StringLiteral => None,
        // Data key/value rendered as-is.
        TokenKind::DataKey | TokenKind::DataValue => None,
        // Stack frames rendered as-is.
        TokenKind::StackFrame => None,
        // Plain text never needs a signifier.
        TokenKind::Plain => None,
    }
}

// ---------------------------------------------------------------------------
// Style application helpers
// ---------------------------------------------------------------------------

/// Apply ANSI styling to a text span.
///
/// When `use_color` is false, applies the no-color signifier instead.
#[must_use]
pub fn style_span(text: &str, kind: TokenKind, use_color: bool) -> String {
    let theme = HighlightTheme {
        use_color,
        ..HighlightTheme::default()
    };
    style_span_with_theme(text, kind, theme)
}

/// Apply style with explicit capability/tone-aware theme policy.
#[must_use]
pub fn style_span_with_theme(text: &str, kind: TokenKind, theme: HighlightTheme) -> String {
    if text.is_empty() {
        return String::new();
    }

    if theme.use_color {
        let style = ansi_style_for_theme(kind, theme.capability, theme.tone);
        if style.sgr == "0" {
            return text.to_string();
        }
        format!("\x1b[{}m{}\x1b[0m", style.sgr, text)
    } else {
        match no_color_signifier(kind) {
            Some(prefix) => format!("{prefix}{text}"),
            None => text.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kind_count_matches_array() {
        assert_eq!(TOKEN_KINDS_BY_PRECEDENCE.len(), TOKEN_KIND_COUNT);
    }

    #[test]
    fn precedence_array_is_sorted() {
        for window in TOKEN_KINDS_BY_PRECEDENCE.windows(2) {
            assert!(
                window[0].precedence() < window[1].precedence(),
                "{:?} (prec {}) should be before {:?} (prec {})",
                window[0],
                window[0].precedence(),
                window[1],
                window[1].precedence(),
            );
        }
    }

    #[test]
    fn plain_has_highest_precedence_value() {
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            assert!(
                kind.precedence() < TokenKind::Plain.precedence(),
                "{kind:?} should have lower precedence value than Plain"
            );
        }
    }

    #[test]
    fn slug_roundtrip() {
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            let slug = kind.slug();
            let parsed = TokenKind::from_slug(slug);
            assert_eq!(parsed, Some(*kind), "roundtrip failed for {slug}");
        }
        assert_eq!(TokenKind::from_slug("plain"), Some(TokenKind::Plain));
        assert_eq!(TokenKind::from_slug("nonexistent"), None);
    }

    #[test]
    fn slug_snapshot() {
        let slugs: Vec<&str> = TOKEN_KINDS_BY_PRECEDENCE.iter().map(|k| k.slug()).collect();
        assert_eq!(
            slugs,
            vec![
                "section-header",
                "role-marker",
                "error",
                "warning",
                "diff-header",
                "diff-hunk",
                "diff-add",
                "diff-del",
                "code-fence",
                "code-content",
                "data-key",
                "data-value",
                "timestamp",
                "command-prompt",
                "stack-frame",
                "path-line",
                "url",
                "number",
                "string-literal",
            ]
        );
    }

    #[test]
    fn display_matches_slug() {
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            assert_eq!(format!("{kind}"), kind.slug());
        }
    }

    #[test]
    fn resolve_precedence_lower_wins() {
        assert_eq!(
            resolve_precedence(TokenKind::Plain, TokenKind::Error),
            TokenKind::Error,
        );
        assert_eq!(
            resolve_precedence(TokenKind::Warning, TokenKind::SectionHeader),
            TokenKind::SectionHeader,
        );
    }

    #[test]
    fn resolve_precedence_equal_keeps_existing() {
        assert_eq!(
            resolve_precedence(TokenKind::Error, TokenKind::Error),
            TokenKind::Error,
        );
    }

    #[test]
    fn resolve_precedence_higher_loses() {
        assert_eq!(
            resolve_precedence(TokenKind::SectionHeader, TokenKind::Warning),
            TokenKind::SectionHeader,
        );
    }

    #[test]
    fn token_span_end() {
        let span = TokenSpan {
            start: 5,
            len: 10,
            kind: TokenKind::Error,
        };
        assert_eq!(span.end(), 15);
    }

    #[test]
    fn ansi_style_snapshot() {
        let snapshot: Vec<String> = TOKEN_KINDS_BY_PRECEDENCE
            .iter()
            .map(|k| {
                let style = ansi_style_for(*k);
                format!("{}:sgr={}:{}", k.slug(), style.sgr, style.label)
            })
            .collect();
        assert_eq!(
            snapshot,
            vec![
                "section-header:sgr=1;36:bold cyan",
                "role-marker:sgr=1;35:bold magenta",
                "error:sgr=1;31:bold red",
                "warning:sgr=33:yellow",
                "diff-header:sgr=1:bold",
                "diff-hunk:sgr=36:cyan",
                "diff-add:sgr=32:green",
                "diff-del:sgr=31:red",
                "code-fence:sgr=2;33:dim yellow",
                "code-content:sgr=0:reset",
                "data-key:sgr=34:blue",
                "data-value:sgr=0:reset",
                "timestamp:sgr=2:dim",
                "command-prompt:sgr=1;33:bold yellow",
                "stack-frame:sgr=2;31:dim red",
                "path-line:sgr=4;36:underline cyan",
                "url:sgr=4;34:underline blue",
                "number:sgr=33:yellow",
                "string-literal:sgr=32:green",
            ]
        );
    }

    #[test]
    fn no_color_signifier_snapshot() {
        let snapshot: Vec<String> = TOKEN_KINDS_BY_PRECEDENCE
            .iter()
            .map(|k| {
                let sig = no_color_signifier(*k);
                format!(
                    "{}:{}",
                    k.slug(),
                    sig.map_or("none".to_string(), |s| format!("{s:?}"))
                )
            })
            .collect();
        assert_eq!(
            snapshot,
            vec![
                "section-header:\"== \"",
                "role-marker:\">> \"",
                "error:\"[ERROR] \"",
                "warning:\"[WARN] \"",
                "diff-header:none",
                "diff-hunk:none",
                "diff-add:none",
                "diff-del:none",
                "code-fence:none",
                "code-content:none",
                "data-key:none",
                "data-value:none",
                "timestamp:none",
                "command-prompt:\"$ \"",
                "stack-frame:none",
                "path-line:none",
                "url:none",
                "number:none",
                "string-literal:none",
            ]
        );
    }

    #[test]
    fn style_span_with_color() {
        let styled = style_span("error: test failed", TokenKind::Error, true);
        assert_eq!(styled, "\x1b[1;31merror: test failed\x1b[0m");
    }

    #[test]
    fn style_span_no_color() {
        let styled = style_span("test failed", TokenKind::Error, false);
        assert_eq!(styled, "[ERROR] test failed");
    }

    #[test]
    fn style_span_no_color_self_describing() {
        let styled = style_span("+added line", TokenKind::DiffAdd, false);
        assert_eq!(styled, "+added line");
    }

    #[test]
    fn style_span_plain_passthrough() {
        let styled_color = style_span("hello", TokenKind::Plain, true);
        assert_eq!(styled_color, "hello");
        let styled_no_color = style_span("hello", TokenKind::Plain, false);
        assert_eq!(styled_no_color, "hello");
    }

    #[test]
    fn style_span_empty_returns_empty() {
        assert_eq!(style_span("", TokenKind::Error, true), "");
        assert_eq!(style_span("", TokenKind::Error, false), "");
    }

    #[test]
    fn is_diff_classification() {
        assert!(TokenKind::DiffHeader.is_diff());
        assert!(TokenKind::DiffHunk.is_diff());
        assert!(TokenKind::DiffAdd.is_diff());
        assert!(TokenKind::DiffDel.is_diff());
        assert!(!TokenKind::Error.is_diff());
        assert!(!TokenKind::Plain.is_diff());
    }

    #[test]
    fn is_severity_classification() {
        assert!(TokenKind::Error.is_severity());
        assert!(TokenKind::Warning.is_severity());
        assert!(!TokenKind::DiffAdd.is_severity());
        assert!(!TokenKind::Plain.is_severity());
    }

    #[test]
    fn all_classified_kinds_have_unique_precedence() {
        let mut seen = std::collections::HashSet::new();
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            assert!(
                seen.insert(kind.precedence()),
                "duplicate precedence {} for {:?}",
                kind.precedence(),
                kind,
            );
        }
    }

    #[test]
    fn all_classified_kinds_have_unique_slugs() {
        let mut seen = std::collections::HashSet::new();
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            assert!(
                seen.insert(kind.slug()),
                "duplicate slug {:?} for {:?}",
                kind.slug(),
                kind,
            );
        }
    }

    #[test]
    fn every_kind_has_an_ansi_style() {
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            let style = ansi_style_for(*kind);
            assert!(!style.sgr.is_empty(), "empty sgr for {kind:?}");
            assert!(!style.label.is_empty(), "empty label for {kind:?}");
        }
        // Plain too.
        let plain = ansi_style_for(TokenKind::Plain);
        assert_eq!(plain.sgr, "0");
    }

    #[test]
    fn capability_slug_roundtrip() {
        assert_eq!(
            TerminalColorCapability::from_slug("ANSI16"),
            Some(TerminalColorCapability::Ansi16)
        );
        assert_eq!(
            TerminalColorCapability::from_slug("256"),
            Some(TerminalColorCapability::Ansi256)
        );
        assert_eq!(
            TerminalColorCapability::from_slug("24bit"),
            Some(TerminalColorCapability::TrueColor)
        );
        assert_eq!(TerminalColorCapability::from_slug("weird"), None);
        assert_eq!(TerminalColorCapability::Ansi256.slug(), "ansi256");
    }

    #[test]
    fn terminal_tone_slug_roundtrip() {
        assert_eq!(TerminalTone::from_slug("dark"), Some(TerminalTone::Dark));
        assert_eq!(TerminalTone::from_slug("LIGHT"), Some(TerminalTone::Light));
        assert_eq!(TerminalTone::from_slug("other"), None);
        assert_eq!(TerminalTone::Dark.slug(), "dark");
    }

    #[test]
    fn resolve_theme_prefers_no_color_over_force() {
        let hints = ThemeEnvHints {
            no_color_env: true,
            force_color_env: true,
            term: Some("xterm-256color".to_string()),
            ..ThemeEnvHints::default()
        };
        let theme = resolve_theme(false, &hints);
        assert!(!theme.use_color);
        assert_eq!(theme.capability, TerminalColorCapability::Ansi256);
    }

    #[test]
    fn resolve_theme_supports_truecolor_override_and_light_tone() {
        let hints = ThemeEnvHints {
            capability_override: Some("truecolor".to_string()),
            tone_override: Some("light".to_string()),
            ..ThemeEnvHints::default()
        };
        let theme = resolve_theme(false, &hints);
        assert!(theme.use_color);
        assert_eq!(theme.capability, TerminalColorCapability::TrueColor);
        assert_eq!(theme.tone, TerminalTone::Light);
    }

    #[test]
    fn resolve_theme_detects_truecolor_then_ansi256_then_ansi16() {
        let truecolor = ThemeEnvHints {
            colorterm: Some("truecolor".to_string()),
            term: Some("xterm-256color".to_string()),
            ..ThemeEnvHints::default()
        };
        assert_eq!(
            resolve_theme(false, &truecolor).capability,
            TerminalColorCapability::TrueColor
        );

        let ansi256 = ThemeEnvHints {
            term: Some("screen-256color".to_string()),
            ..ThemeEnvHints::default()
        };
        assert_eq!(
            resolve_theme(false, &ansi256).capability,
            TerminalColorCapability::Ansi256
        );

        let ansi16 = ThemeEnvHints {
            term: Some("xterm".to_string()),
            ..ThemeEnvHints::default()
        };
        assert_eq!(
            resolve_theme(false, &ansi16).capability,
            TerminalColorCapability::Ansi16
        );
    }

    #[test]
    fn resolve_theme_detects_light_tone_from_colorfgbg() {
        let light = ThemeEnvHints {
            colorfgbg: Some("0;15".to_string()),
            ..ThemeEnvHints::default()
        };
        assert_eq!(resolve_theme(false, &light).tone, TerminalTone::Light);

        let dark = ThemeEnvHints {
            colorfgbg: Some("15;0".to_string()),
            ..ThemeEnvHints::default()
        };
        assert_eq!(resolve_theme(false, &dark).tone, TerminalTone::Dark);
    }

    #[test]
    fn light_warning_palette_avoids_low_contrast_yellow() {
        let ansi16_warning = ansi_style_for_theme(
            TokenKind::Warning,
            TerminalColorCapability::Ansi16,
            TerminalTone::Light,
        );
        assert_eq!(ansi16_warning.sgr, "1;35");

        let truecolor_warning = ansi_style_for_theme(
            TokenKind::Warning,
            TerminalColorCapability::TrueColor,
            TerminalTone::Light,
        );
        assert_eq!(truecolor_warning.sgr, "38;2;140;90;0");
    }

    #[test]
    fn style_span_with_theme_uses_capability_and_tone() {
        let themed = style_span_with_theme(
            "warn",
            TokenKind::Warning,
            HighlightTheme {
                use_color: true,
                capability: TerminalColorCapability::TrueColor,
                tone: TerminalTone::Light,
            },
        );
        assert_eq!(themed, "\x1b[38;2;140;90;0mwarn\x1b[0m");
    }

    #[test]
    fn style_span_with_theme_no_color_uses_signifier() {
        let themed = style_span_with_theme(
            "failed",
            TokenKind::Error,
            HighlightTheme {
                use_color: false,
                capability: TerminalColorCapability::TrueColor,
                tone: TerminalTone::Dark,
            },
        );
        assert_eq!(themed, "[ERROR] failed");
    }
}

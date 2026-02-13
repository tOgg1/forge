//! Harness-aware section parser state machine.
//!
//! Classifies contiguous runs of log lines into structural *sections* that
//! correspond to the block types emitted by AI-coding harnesses (Codex, Claude,
//! OpenCode, Pi, etc.).
//!
//! The parser is **streaming-safe**: you feed it one line at a time via
//! [`SectionParser::feed`] and it emits zero or more [`SectionEvent`]s per
//! line.  Internal state tracks which block is currently "open" so that
//! multi-line structures (code fences, diff hunks, thinking blocks, etc.) are
//! kept together.
//!
//! Design constraints (from PAR-103):
//! - Classify lines without regex-backtracking blowups — all matchers are
//!   simple prefix / `starts_with` / fixed-byte checks.
//! - Unknown-block fallback preserves raw content as [`SectionKind::Unknown`].

use std::fmt;

// ---------------------------------------------------------------------------
// Section kinds
// ---------------------------------------------------------------------------

/// The structural block type for a contiguous section of log output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    /// Harness header + config block (e.g. `OpenAI Codex v0.80.0` + separator
    /// + key-value config lines).
    HarnessHeader,

    /// Role transition marker line (`user`, `codex`, `exec`, `thinking`,
    /// `assistant`, `system`).
    RoleMarker,

    /// Thinking / planning block — the content between a `thinking` role
    /// marker and the next role change (includes `**bold**` planning lines).
    Thinking,

    /// Tool invocation or action line (`tool: ...`, `● ...`, `action: ...`).
    ToolCall,

    /// Code fence block (`` ``` `` open through `` ``` `` close, inclusive).
    CodeFence,

    /// Diff block — contiguous diff lines starting with a diff header or hunk
    /// marker, through trailing context / add / del lines.
    Diff,

    /// Claude stream-JSON event line (`{"type":"..."}`).
    JsonEvent,

    /// Summary / token-count block (`tokens used` + numeric count).
    Summary,

    /// Approval request block (`APPROVAL REQUIRED` … `approved by operator`).
    Approval,

    /// Error + optional recovery block.
    ErrorBlock,

    /// Status / timestamp metadata line.
    StatusLine,

    /// Catch-all for lines that do not match any known structural pattern.
    /// Raw content is preserved as-is.
    Unknown,
}

impl SectionKind {
    /// Stable slug for serialization and snapshot tests.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::HarnessHeader => "harness-header",
            Self::RoleMarker => "role-marker",
            Self::Thinking => "thinking",
            Self::ToolCall => "tool-call",
            Self::CodeFence => "code-fence",
            Self::Diff => "diff",
            Self::JsonEvent => "json-event",
            Self::Summary => "summary",
            Self::Approval => "approval",
            Self::ErrorBlock => "error-block",
            Self::StatusLine => "status-line",
            Self::Unknown => "unknown",
        }
    }

    /// Parse from slug.
    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "harness-header" => Some(Self::HarnessHeader),
            "role-marker" => Some(Self::RoleMarker),
            "thinking" => Some(Self::Thinking),
            "tool-call" => Some(Self::ToolCall),
            "code-fence" => Some(Self::CodeFence),
            "diff" => Some(Self::Diff),
            "json-event" => Some(Self::JsonEvent),
            "summary" => Some(Self::Summary),
            "approval" => Some(Self::Approval),
            "error-block" => Some(Self::ErrorBlock),
            "status-line" => Some(Self::StatusLine),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for SectionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

// ---------------------------------------------------------------------------
// Section events
// ---------------------------------------------------------------------------

/// An event emitted by [`SectionParser::feed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionEvent {
    /// A new section has started. The line that triggered it is included.
    Start {
        kind: SectionKind,
        line: String,
        line_number: usize,
    },
    /// A line that continues the currently-open section.
    Continue {
        kind: SectionKind,
        line: String,
        line_number: usize,
    },
    /// The currently-open section has ended. This event is emitted *before*
    /// the new section's `Start` (if any) in the same `feed` call.
    End {
        kind: SectionKind,
        line_number: usize,
    },
}

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

/// Internal state of the currently-open block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenBlock {
    None,
    HarnessHeader,
    Thinking,
    CodeFence {
        /// The fence marker used to open the block (e.g. "```").
        /// We match the same marker (same or more backticks) to close.
        backtick_count: usize,
    },
    Diff,
    Summary,
    Approval,
    ErrorBlock,
}

/// Streaming section parser for harness log output.
///
/// Feed lines one at a time; the parser maintains internal state to track
/// multi-line blocks.
pub struct SectionParser {
    state: OpenBlock,
    current_line: usize,
}

impl Default for SectionParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SectionParser {
    /// Create a new parser starting at line 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: OpenBlock::None,
            current_line: 0,
        }
    }

    /// Feed a single line to the parser. Returns events (typically 1-2).
    ///
    /// The line should NOT include a trailing newline.
    pub fn feed(&mut self, line: &str) -> Vec<SectionEvent> {
        self.current_line += 1;
        let ln = self.current_line;
        let mut events = Vec::new();

        match &self.state {
            OpenBlock::CodeFence { backtick_count } => {
                let bc = *backtick_count;
                if is_code_fence_close(line, bc) {
                    // Closing fence line is part of the code-fence section.
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::CodeFence,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    events.push(SectionEvent::End {
                        kind: SectionKind::CodeFence,
                        line_number: ln,
                    });
                    self.state = OpenBlock::None;
                } else {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::CodeFence,
                        line: line.to_string(),
                        line_number: ln,
                    });
                }
                return events;
            }
            OpenBlock::Thinking => {
                // Thinking ends when we see a role marker, tool call, code
                // fence, or any other structural indicator.
                if is_role_marker(line)
                    || is_tool_call(line)
                    || is_code_fence_open(line).is_some()
                    || is_harness_header(line)
                    || is_json_event(line)
                    || is_diff_start(line)
                    || is_summary_start(line)
                    || is_approval_start(line)
                {
                    events.push(SectionEvent::End {
                        kind: SectionKind::Thinking,
                        line_number: ln.saturating_sub(1),
                    });
                    self.state = OpenBlock::None;
                    // Fall through to classify this line below.
                } else {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::Thinking,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    return events;
                }
            }
            OpenBlock::HarnessHeader => {
                if is_harness_config_line(line) || is_separator_line(line) {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::HarnessHeader,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    return events;
                }
                // Header block ends.
                events.push(SectionEvent::End {
                    kind: SectionKind::HarnessHeader,
                    line_number: ln.saturating_sub(1),
                });
                self.state = OpenBlock::None;
                // Fall through.
            }
            OpenBlock::Diff => {
                if is_diff_continuation(line) {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::Diff,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    return events;
                }
                events.push(SectionEvent::End {
                    kind: SectionKind::Diff,
                    line_number: ln.saturating_sub(1),
                });
                self.state = OpenBlock::None;
                // Fall through.
            }
            OpenBlock::Summary => {
                if is_summary_continuation(line) {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::Summary,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    return events;
                }
                events.push(SectionEvent::End {
                    kind: SectionKind::Summary,
                    line_number: ln.saturating_sub(1),
                });
                self.state = OpenBlock::None;
                // Fall through.
            }
            OpenBlock::Approval => {
                let is_end = is_approval_end(line);
                events.push(SectionEvent::Continue {
                    kind: SectionKind::Approval,
                    line: line.to_string(),
                    line_number: ln,
                });
                if is_end {
                    events.push(SectionEvent::End {
                        kind: SectionKind::Approval,
                        line_number: ln,
                    });
                    self.state = OpenBlock::None;
                }
                return events;
            }
            OpenBlock::ErrorBlock => {
                if is_error_continuation(line) {
                    events.push(SectionEvent::Continue {
                        kind: SectionKind::ErrorBlock,
                        line: line.to_string(),
                        line_number: ln,
                    });
                    return events;
                }
                events.push(SectionEvent::End {
                    kind: SectionKind::ErrorBlock,
                    line_number: ln.saturating_sub(1),
                });
                self.state = OpenBlock::None;
                // Fall through.
            }
            OpenBlock::None => {}
        }

        // ── Classify the current line ────────────────────────────────────

        // Code fence (must check before diff because ```diff is a fence).
        if let Some(backtick_count) = is_code_fence_open(line) {
            self.state = OpenBlock::CodeFence { backtick_count };
            events.push(SectionEvent::Start {
                kind: SectionKind::CodeFence,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // JSON event.
        if is_json_event(line) {
            events.push(SectionEvent::Start {
                kind: SectionKind::JsonEvent,
                line: line.to_string(),
                line_number: ln,
            });
            events.push(SectionEvent::End {
                kind: SectionKind::JsonEvent,
                line_number: ln,
            });
            return events;
        }

        // Harness header.
        if is_harness_header(line) {
            self.state = OpenBlock::HarnessHeader;
            events.push(SectionEvent::Start {
                kind: SectionKind::HarnessHeader,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Box-drawing header (OpenCode style).
        if is_box_header(line) {
            self.state = OpenBlock::HarnessHeader;
            events.push(SectionEvent::Start {
                kind: SectionKind::HarnessHeader,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Approval block.
        if is_approval_start(line) {
            self.state = OpenBlock::Approval;
            events.push(SectionEvent::Start {
                kind: SectionKind::Approval,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Role marker — check for `thinking` which opens a multi-line block.
        if is_role_marker(line) {
            let trimmed = line.trim();
            if trimmed == "thinking" {
                self.state = OpenBlock::Thinking;
                events.push(SectionEvent::Start {
                    kind: SectionKind::Thinking,
                    line: line.to_string(),
                    line_number: ln,
                });
            } else {
                events.push(SectionEvent::Start {
                    kind: SectionKind::RoleMarker,
                    line: line.to_string(),
                    line_number: ln,
                });
                events.push(SectionEvent::End {
                    kind: SectionKind::RoleMarker,
                    line_number: ln,
                });
            }
            return events;
        }

        // Tool call.
        if is_tool_call(line) {
            events.push(SectionEvent::Start {
                kind: SectionKind::ToolCall,
                line: line.to_string(),
                line_number: ln,
            });
            events.push(SectionEvent::End {
                kind: SectionKind::ToolCall,
                line_number: ln,
            });
            return events;
        }

        // Diff start.
        if is_diff_start(line) {
            self.state = OpenBlock::Diff;
            events.push(SectionEvent::Start {
                kind: SectionKind::Diff,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Summary.
        if is_summary_start(line) {
            self.state = OpenBlock::Summary;
            events.push(SectionEvent::Start {
                kind: SectionKind::Summary,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Error block.
        if is_error_start(line) {
            self.state = OpenBlock::ErrorBlock;
            events.push(SectionEvent::Start {
                kind: SectionKind::ErrorBlock,
                line: line.to_string(),
                line_number: ln,
            });
            return events;
        }

        // Status / timestamp line.
        if is_status_line(line) {
            events.push(SectionEvent::Start {
                kind: SectionKind::StatusLine,
                line: line.to_string(),
                line_number: ln,
            });
            events.push(SectionEvent::End {
                kind: SectionKind::StatusLine,
                line_number: ln,
            });
            return events;
        }

        // Unknown fallback.
        events.push(SectionEvent::Start {
            kind: SectionKind::Unknown,
            line: line.to_string(),
            line_number: ln,
        });
        events.push(SectionEvent::End {
            kind: SectionKind::Unknown,
            line_number: ln,
        });
        events
    }

    /// Flush any open block at end-of-input. Returns an `End` event if a
    /// block was open, otherwise empty.
    pub fn flush(&mut self) -> Vec<SectionEvent> {
        let mut events = Vec::new();
        let kind = match &self.state {
            OpenBlock::None => return events,
            OpenBlock::HarnessHeader => SectionKind::HarnessHeader,
            OpenBlock::Thinking => SectionKind::Thinking,
            OpenBlock::CodeFence { .. } => SectionKind::CodeFence,
            OpenBlock::Diff => SectionKind::Diff,
            OpenBlock::Summary => SectionKind::Summary,
            OpenBlock::Approval => SectionKind::Approval,
            OpenBlock::ErrorBlock => SectionKind::ErrorBlock,
        };
        events.push(SectionEvent::End {
            kind,
            line_number: self.current_line,
        });
        self.state = OpenBlock::None;
        events
    }

    /// Parse all lines of a complete input at once. Convenience wrapper
    /// around repeated [`feed`](Self::feed) + [`flush`](Self::flush).
    pub fn parse_all(input: &str) -> Vec<SectionEvent> {
        let mut parser = Self::new();
        let mut events = Vec::new();
        for line in input.lines() {
            events.extend(parser.feed(line));
        }
        events.extend(parser.flush());
        events
    }
}

// ---------------------------------------------------------------------------
// Line classifiers — simple prefix / byte checks, no regex.
// ---------------------------------------------------------------------------

/// Returns Some(backtick_count) if line opens a code fence.
fn is_code_fence_open(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        let count = trimmed.bytes().take_while(|&b| b == b'`').count();
        if count >= 3 {
            // It's an opening fence if there is either a language hint or
            // nothing after the backticks (but NOT a closing-only fence which
            // we handle separately).
            let after = &trimmed[count..];
            // A line that is ONLY backticks with nothing else could be either
            // open or close. We treat it as open when there's no current fence
            // (handled by caller context). Lines with a language hint are
            // always opens.
            if after.is_empty() || !after.starts_with('`') {
                return Some(count);
            }
        }
    }
    None
}

/// Returns true if line closes a code fence opened with `open_count` backticks.
fn is_code_fence_close(line: &str, open_count: usize) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("```") {
        return false;
    }
    let count = trimmed.bytes().take_while(|&b| b == b'`').count();
    // Closing fence: at least as many backticks, nothing else after.
    count >= open_count && trimmed.len() == count
}

fn is_role_marker(line: &str) -> bool {
    let trimmed = line.trim();
    matches!(
        trimmed,
        "user" | "codex" | "exec" | "thinking" | "assistant" | "system"
    )
}

fn is_tool_call(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("tool: ")
        || trimmed.starts_with("tool:")
        || trimmed.starts_with("action: ")
        || trimmed.starts_with("action:")
        || trimmed.starts_with("● ")
        || trimmed.starts_with("mcp: ")
        || trimmed.starts_with("mcp:")
}

fn is_harness_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("OpenAI Codex v")
        || trimmed.starts_with("OpenCode v")
        || trimmed.starts_with("Pi Coding Agent v")
}

fn is_box_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('\u{256D}') // ╭
        || trimmed.starts_with('\u{2570}') // ╰
}

fn is_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // All dashes.
    if trimmed.bytes().all(|b| b == b'-') && trimmed.len() >= 3 {
        return true;
    }
    // Box-drawing lines.
    trimmed.starts_with('\u{2502}') // │
        || trimmed.starts_with('\u{256D}') // ╭
        || trimmed.starts_with('\u{2570}') // ╰
}

fn is_harness_config_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Key: value patterns commonly found in harness config blocks.
    if is_separator_line(line) {
        return true;
    }
    // Box-drawing content lines.
    if trimmed.starts_with('\u{2502}') {
        return true;
    }
    // Common config keys.
    for prefix in &[
        "workdir:",
        "model:",
        "provider:",
        "approval:",
        "sandbox:",
        "reasoning effort:",
        "session id:",
        "session_id:",
        "profile:",
        "harness:",
        "Configuration loaded from",
        "Status:",
        "Task:",
    ] {
        if trimmed.starts_with(prefix) {
            return true;
        }
    }
    false
}

fn is_json_event(line: &str) -> bool {
    let trimmed = line.trim();
    // Claude stream JSON always starts with {"type":
    trimmed.starts_with("{\"type\":")
}

fn is_diff_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("diff --git ")
        || trimmed.starts_with("--- a/")
        || trimmed.starts_with("+++ b/")
        || trimmed.starts_with("--- /dev/null")
        || trimmed.starts_with("+++ /dev/null")
}

fn is_diff_continuation(line: &str) -> bool {
    if is_diff_start(line) {
        return true;
    }
    let trimmed = line.trim_start();
    // Hunk header.
    if trimmed.starts_with("@@ ") {
        return true;
    }
    // Index line.
    if trimmed.starts_with("index ") || trimmed.starts_with("new file mode") {
        return true;
    }
    // Add/del/context lines — single +/- prefix (not +++/---).
    if let Some(first) = trimmed.bytes().next() {
        match first {
            b'+' => {
                // Not +++ (already handled above as diff start).
                return !trimmed.starts_with("+++");
            }
            b'-' => {
                return !trimmed.starts_with("---");
            }
            _ => {}
        }
    }
    if line.starts_with(' ') {
        // Context line in unified diff must preserve leading space.
        return true;
    }
    false
}

fn is_summary_start(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "tokens used"
}

fn is_summary_continuation(line: &str) -> bool {
    let trimmed = line.trim();
    // Numeric lines (token counts): all digits, commas, dots, spaces.
    if trimmed.is_empty() {
        return false;
    }
    trimmed
        .bytes()
        .all(|b| b.is_ascii_digit() || b == b',' || b == b'.' || b == b' ')
}

fn is_approval_start(line: &str) -> bool {
    let upper = line.to_ascii_uppercase();
    upper.contains("APPROVAL REQUIRED") || upper.contains("AWAITING APPROVAL")
}

fn is_approval_end(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("approved by operator")
        || lower.contains("rejected")
        || lower.contains("press [y]")
}

fn is_error_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("error:")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("error[")
        || trimmed.starts_with("panic:")
        || trimmed.starts_with("panicked at")
        || trimmed.starts_with("FAIL")
        || trimmed.starts_with("Traceback (most recent call last):")
        || (trimmed.starts_with("unexpected concurrent change") && !trimmed.is_empty())
        || is_exception_class_start(trimmed)
}

/// Check if a line looks like an exception class name starting an error block
/// (e.g. `TypeError: ...`, `AssertionError: ...`, `NullPointerException`).
fn is_exception_class_start(trimmed: &str) -> bool {
    let first = match trimmed.as_bytes().first() {
        Some(&b) if b.is_ascii_uppercase() => b,
        _ => return false,
    };
    // Must start uppercase and not be a known non-error prefix.
    if first == b'F' && trimmed.starts_with("FAIL") {
        return false; // Already matched above.
    }
    let word_end = trimmed.find([':', ' ']).unwrap_or(trimmed.len());
    let word = &trimmed[..word_end];
    // Must contain Error/Exception/Panic and be followed by `:`.
    (word.contains("Error") || word.contains("Exception") || word.contains("Panic"))
        && trimmed.len() > word_end
        && trimmed.as_bytes()[word_end] == b':'
}

fn is_error_continuation(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Recovery lines, caused-by, indented continuation, stack frames, signals.
    trimmed.starts_with("recovery:")
        || starts_with_ci_local(trimmed, "caused by:")
        || trimmed.starts_with("note:")
        || trimmed.starts_with("help:")
        || line.starts_with("  at ")
        || line.starts_with("    ")
        || trimmed.starts_with("--> ")
        || line.starts_with(" --> ")
        || trimmed.starts_with("goroutine ")
        || trimmed.starts_with("thread '")
        || trimmed.starts_with("[signal ")
        || trimmed.starts_with("File \"")
}

/// Case-insensitive starts_with for error continuation matching.
fn starts_with_ci_local(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn is_status_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Bracketed RFC 3339 timestamp prefix.
    if trimmed.starts_with('[') && trimmed.len() > 22 {
        // Check for ISO timestamp pattern [2026-...Z]
        if let Some(close) = trimmed.find(']') {
            let inside = &trimmed[1..close];
            if inside.len() >= 20 && inside.as_bytes()[4] == b'-' && inside.as_bytes()[7] == b'-' {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Slug roundtrip ────────────────────────────────────────────────

    #[test]
    fn section_kind_slug_roundtrip() {
        let all = [
            SectionKind::HarnessHeader,
            SectionKind::RoleMarker,
            SectionKind::Thinking,
            SectionKind::ToolCall,
            SectionKind::CodeFence,
            SectionKind::Diff,
            SectionKind::JsonEvent,
            SectionKind::Summary,
            SectionKind::Approval,
            SectionKind::ErrorBlock,
            SectionKind::StatusLine,
            SectionKind::Unknown,
        ];
        for kind in &all {
            let slug = kind.slug();
            assert_eq!(
                SectionKind::from_slug(slug),
                Some(*kind),
                "roundtrip failed for {slug}"
            );
        }
        assert_eq!(SectionKind::from_slug("nonexistent"), None);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", SectionKind::CodeFence), "code-fence");
        assert_eq!(format!("{}", SectionKind::Unknown), "unknown");
    }

    // ── Line classifiers ──────────────────────────────────────────────

    #[test]
    fn code_fence_open_detection() {
        assert_eq!(is_code_fence_open("```rust"), Some(3));
        assert_eq!(is_code_fence_open("```"), Some(3));
        assert_eq!(is_code_fence_open("````"), Some(4));
        assert_eq!(is_code_fence_open("  ```python"), Some(3));
        assert_eq!(is_code_fence_open("no fence"), None);
        assert_eq!(is_code_fence_open("``not enough"), None);
    }

    #[test]
    fn code_fence_close_detection() {
        assert!(is_code_fence_close("```", 3));
        assert!(is_code_fence_close("````", 3));
        assert!(!is_code_fence_close("``", 3));
        assert!(!is_code_fence_close("```rust", 3));
    }

    #[test]
    fn role_marker_detection() {
        assert!(is_role_marker("user"));
        assert!(is_role_marker("codex"));
        assert!(is_role_marker("exec"));
        assert!(is_role_marker("thinking"));
        assert!(is_role_marker("assistant"));
        assert!(is_role_marker("system"));
        assert!(is_role_marker("  user  "));
        assert!(!is_role_marker("username"));
        assert!(!is_role_marker("the user said"));
    }

    #[test]
    fn tool_call_detection() {
        assert!(is_tool_call("tool: read file `foo.rs`"));
        assert!(is_tool_call("tool: run command `cargo test`"));
        assert!(is_tool_call("action: approved by operator"));
        assert!(is_tool_call("● Reading file: foo.rs"));
        assert!(is_tool_call("mcp: playwright starting"));
        assert!(!is_tool_call("This is not a tool call"));
    }

    #[test]
    fn harness_header_detection() {
        assert!(is_harness_header("OpenAI Codex v0.80.0 (research preview)"));
        assert!(is_harness_header("OpenCode v0.1.0"));
        assert!(is_harness_header("Pi Coding Agent v2.1.0"));
        assert!(!is_harness_header("Some other header"));
    }

    #[test]
    fn json_event_detection() {
        assert!(is_json_event(r#"{"type":"system","subtype":"init"}"#));
        assert!(is_json_event(r#"{"type":"stream_event","event":{}}"#));
        assert!(!is_json_event("not json"));
        assert!(!is_json_event(r#"{"data":"no type key"}"#));
    }

    #[test]
    fn diff_start_detection() {
        assert!(is_diff_start("diff --git a/foo.rs b/foo.rs"));
        assert!(is_diff_start("--- a/foo.rs"));
        assert!(is_diff_start("+++ b/foo.rs"));
        assert!(is_diff_start("--- /dev/null"));
        assert!(!is_diff_start("+added line"));
        assert!(!is_diff_start("-removed line"));
    }

    #[test]
    fn diff_continuation_detection() {
        assert!(is_diff_continuation("@@ -10,7 +10,12 @@"));
        assert!(is_diff_continuation("+added line"));
        assert!(is_diff_continuation("-removed line"));
        assert!(is_diff_continuation(" context line"));
        assert!(is_diff_continuation("index 0000000..a1b2c3d"));
        assert!(is_diff_continuation("new file mode 100644"));
        assert!(is_diff_continuation("diff --git a/x b/x"));
    }

    #[test]
    fn summary_detection() {
        assert!(is_summary_start("tokens used"));
        assert!(!is_summary_start("15,892"));
        assert!(is_summary_continuation("15,892"));
        assert!(is_summary_continuation("28410"));
        assert!(!is_summary_continuation("tokens used"));
        assert!(!is_summary_continuation(""));
    }

    #[test]
    fn approval_detection() {
        assert!(is_approval_start("⚠️  APPROVAL REQUIRED"));
        assert!(is_approval_start("Awaiting Approval"));
        assert!(is_approval_end("approved by operator"));
        assert!(is_approval_end("Press [y] to approve all"));
    }

    #[test]
    fn error_start_detection() {
        assert!(is_error_start("error: test failed"));
        assert!(is_error_start("error[E0277]: trait bound not satisfied"));
        assert!(is_error_start("panic: runtime error"));
        assert!(is_error_start("FAIL    internal/integration"));
        assert!(is_error_start(
            "unexpected concurrent change detected; I stopped."
        ));
        // Python traceback.
        assert!(is_error_start("Traceback (most recent call last):"));
        // Exception class names.
        assert!(is_error_start("TypeError: cannot read property"));
        assert!(is_error_start("AssertionError: expected True"));
        assert!(is_error_start("NullPointerException: null"));
        assert!(is_error_start(
            "ProviderModelNotFoundError: model not found"
        ));
        // Negative: non-exception class.
        assert!(!is_error_start("SomeClass: not an error"));
        assert!(!is_error_start("lowercase_error: nope"));
    }

    #[test]
    fn error_continuation_extended() {
        // Existing patterns.
        assert!(is_error_continuation("recovery: fix the test"));
        assert!(is_error_continuation("caused by: connection refused"));
        assert!(is_error_continuation("note: required by trait"));
        assert!(is_error_continuation("  at Object.foo (file.js:10:5)"));
        assert!(is_error_continuation("    indented continuation"));
        // New patterns.
        assert!(is_error_continuation("Caused by: uppercase variant"));
        assert!(is_error_continuation("help: try this instead"));
        assert!(is_error_continuation("[signal SIGSEGV: code=0x1]"));
        assert!(is_error_continuation(
            "  File \"test.py\", line 42, in test_foo"
        ));
    }

    #[test]
    fn python_traceback_error_block() {
        let input = "\
Traceback (most recent call last):
  File \"test_main.py\", line 10, in test_foo
    assert result == 42
AssertionError: expected 42 but got 0
plain line after";

        let mut parser = SectionParser::new();
        let starts: Vec<SectionKind> = input
            .lines()
            .flat_map(|line| parser.feed(line))
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(kind),
                _ => None,
            })
            .collect();

        // First section: ErrorBlock (Traceback header).
        assert_eq!(starts[0], SectionKind::ErrorBlock);
    }

    #[test]
    fn status_line_detection() {
        assert!(is_status_line("[2026-02-09T16:00:01Z] status: idle"));
        assert!(is_status_line("[2026-01-10T20:10:00Z] status: starting"));
        assert!(!is_status_line("no timestamp here"));
        assert!(!is_status_line("[short]"));
    }

    // ── Integration: full transcript parsing ──────────────────────────

    #[test]
    fn codex_transcript_sections() {
        let input = "\
OpenAI Codex v0.80.0 (research preview)
--------
workdir: /repo/forge
model: gpt-5.2-codex
--------
user
Implement the database connection pool
thinking
**Analyzing requirements for connection pool**
The task requires implementing a connection pool.
codex
I'll implement the connection pool.

```rust
pub struct ConnectionPool {}
```

tokens used
15,892
exec
$ cargo test -p forge-db --lib pool::tests
running 8 tests
test pool::tests::acquire_returns_connection ... ok

test result: ok. 8 passed; 0 failed;";

        let events = SectionParser::parse_all(input);
        let kinds: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert!(
            kinds.contains(&SectionKind::HarnessHeader),
            "should detect harness header"
        );
        assert!(
            kinds.contains(&SectionKind::RoleMarker),
            "should detect role markers"
        );
        assert!(
            kinds.contains(&SectionKind::Thinking),
            "should detect thinking block"
        );
        assert!(
            kinds.contains(&SectionKind::CodeFence),
            "should detect code fence"
        );
        assert!(
            kinds.contains(&SectionKind::Summary),
            "should detect summary"
        );
    }

    #[test]
    fn claude_json_events_parsed() {
        let input = r#"{"type":"system","subtype":"init","model":"claude-opus-4-6"}
{"type":"stream_event","event":{"type":"content_block_start"}}
[2026-02-09T16:00:01Z] status: idle"#;

        let events = SectionParser::parse_all(input);
        let kinds: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(kinds[0], SectionKind::JsonEvent);
        assert_eq!(kinds[1], SectionKind::JsonEvent);
        assert_eq!(kinds[2], SectionKind::StatusLine);
    }

    #[test]
    fn diff_block_continuity() {
        let input = "\
diff --git a/foo.rs b/foo.rs
index 0000000..a1b2c3d
--- a/foo.rs
+++ b/foo.rs
@@ -10,7 +10,12 @@
-old line
+new line
 context line
next section here";

        let events = SectionParser::parse_all(input);
        let starts: Vec<(SectionKind, usize)> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start {
                    kind, line_number, ..
                } => Some((*kind, *line_number)),
                _ => None,
            })
            .collect();

        // Should be one diff start, then eventually a non-diff line.
        assert_eq!(starts[0], (SectionKind::Diff, 1));
        // The diff should end before "next section here".
        let ends: Vec<(SectionKind, usize)> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::End { kind, line_number } => Some((*kind, *line_number)),
                _ => None,
            })
            .collect();

        let diff_end_line = ends
            .iter()
            .find(|(kind, _)| *kind == SectionKind::Diff)
            .map(|(_, line_number)| *line_number);
        assert_eq!(
            diff_end_line,
            Some(8),
            "diff should end at line 8 (context line)"
        );
    }

    #[test]
    fn code_fence_with_language() {
        let input = "\
```rust
fn main() {}
```
plain text";

        let events = SectionParser::parse_all(input);
        let starts: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(starts[0], SectionKind::CodeFence);
        // After fence closes, "plain text" should be Unknown.
        assert_eq!(starts[1], SectionKind::Unknown);
    }

    #[test]
    fn thinking_block_ends_at_role_change() {
        let input = "\
thinking
**Planning the approach**
Let me analyze the code.
codex
Here's my implementation.";

        let events = SectionParser::parse_all(input);
        let starts: Vec<(SectionKind, usize)> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start {
                    kind, line_number, ..
                } => Some((*kind, *line_number)),
                _ => None,
            })
            .collect();

        assert_eq!(starts[0], (SectionKind::Thinking, 1));
        assert_eq!(starts[1], (SectionKind::RoleMarker, 4));
    }

    #[test]
    fn approval_block_spans() {
        let input = "\
⚠️  APPROVAL REQUIRED
Delete file: old_db_pool.go
Run command: go mod tidy
Press [y] to approve all
action: approved by operator";

        let events = SectionParser::parse_all(input);
        let starts: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();
        let ends: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::End { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(starts[0], SectionKind::Approval);
        // Approval ends at "Press [y]" line.
        assert!(ends.contains(&SectionKind::Approval));
    }

    #[test]
    fn error_block_with_recovery() {
        let input = "\
error: integration tests failed with 1 failure
recovery: fix rate limiter test fixture
plain line after";

        let events = SectionParser::parse_all(input);
        let starts: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(starts[0], SectionKind::ErrorBlock);
        // "plain line after" should be Unknown.
        assert_eq!(starts[1], SectionKind::Unknown);
    }

    #[test]
    fn unknown_fallback_preserves_content() {
        let input = "\
This is just some random text.
Another line of prose.";

        let events = SectionParser::parse_all(input);
        let starts: Vec<(SectionKind, String)> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, line, .. } => Some((*kind, line.clone())),
                _ => None,
            })
            .collect();

        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0].0, SectionKind::Unknown);
        assert_eq!(starts[0].1, "This is just some random text.");
        assert_eq!(starts[1].0, SectionKind::Unknown);
        assert_eq!(starts[1].1, "Another line of prose.");
    }

    #[test]
    fn flush_closes_open_block() {
        let mut parser = SectionParser::new();
        let _ = parser.feed("```rust");
        let _ = parser.feed("fn main() {}");
        // No closing fence — flush should emit End.
        let flush_events = parser.flush();
        assert_eq!(flush_events.len(), 1);
        match &flush_events[0] {
            SectionEvent::End { kind, .. } => assert_eq!(*kind, SectionKind::CodeFence),
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_no_events() {
        let events = SectionParser::parse_all("");
        assert!(events.is_empty());
    }

    #[test]
    fn opencode_box_drawing_header() {
        let input = "\
╭─────────────────────────╮
│ OpenCode v0.1.0         │
╰─────────────────────────╯";

        let events = SectionParser::parse_all(input);
        let starts: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(starts[0], SectionKind::HarnessHeader);
    }

    #[test]
    fn pi_transcript_sections() {
        let input = "\
Pi Coding Agent v2.1.0
Configuration loaded from /home/user/.pi/agent/config.toml
[2026-01-10T20:10:00Z] status: starting
[2026-01-10T20:10:01Z] profile: pi-default
[2026-01-10T20:10:02Z] status: ready
tool: read /repo/src/api/client.rs

```rust
pub struct RetryPolicy {}
```

[2026-01-10T20:11:30Z] status: success";

        let events = SectionParser::parse_all(input);
        let kinds: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert!(kinds.contains(&SectionKind::HarnessHeader));
        assert!(kinds.contains(&SectionKind::StatusLine));
        assert!(kinds.contains(&SectionKind::ToolCall));
        assert!(kinds.contains(&SectionKind::CodeFence));
    }

    #[test]
    fn malformed_fence_handled() {
        // Fence with only 2 backticks — not a real fence.
        let input = "\
``not a fence
regular text";

        let events = SectionParser::parse_all(input);
        let kinds: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        assert_eq!(kinds[0], SectionKind::Unknown);
        assert_eq!(kinds[1], SectionKind::Unknown);
    }

    #[test]
    fn nested_diff_inside_code_fence() {
        // Diff lines inside a code fence should stay as CodeFence, not
        // break out into a Diff section.
        let input = "\
```diff
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
-old
+new
```";

        let events = SectionParser::parse_all(input);
        let starts: Vec<SectionKind> = events
            .iter()
            .filter_map(|e| match e {
                SectionEvent::Start { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();

        // Only one section: CodeFence. No Diff section should appear.
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0], SectionKind::CodeFence);
    }
}

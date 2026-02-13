//! Log pipeline v2: lane parsing + semantic syntax theme + folding.
//!
//! Extends the lane model with sub-line token spans for syntax highlighting
//! and block-level grouping with interactive fold/unfold state.
//!
//! # Architecture
//!
//! ```text
//! raw lines
//!   -> classify_line (lane_model.rs)  => LanedLogLine
//!   -> highlight_spans                => Vec<LogSpan> per line
//!   -> group_blocks                   => Vec<LogBlock> with fold state
//! ```

use crate::lane_model::{classify_line, LogLane};
use crate::theme::ThemeSemanticSlot;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// LogSpan — sub-line semantic token
// ---------------------------------------------------------------------------

/// Semantic role for a sub-line token span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanKind {
    /// Plain text, uses lane's default color.
    Plain,
    /// Keyword-like tokens (tool names, section headers, status labels).
    Keyword,
    /// String literals (quoted values).
    StringLiteral,
    /// Numeric literals (counts, durations, exit codes).
    Number,
    /// Shell commands or executable names.
    Command,
    /// File paths.
    Path,
    /// Error emphasis (error messages, panic text).
    Error,
    /// Muted / dimmed text (timestamps, brackets, separators).
    Muted,
    /// Punctuation and structural characters.
    Punctuation,
}

impl SpanKind {
    /// Map span kind to the closest theme semantic slot for coloring.
    #[must_use]
    pub fn theme_slot(self) -> ThemeSemanticSlot {
        match self {
            Self::Plain => ThemeSemanticSlot::UiTextPrimary,
            Self::Keyword => ThemeSemanticSlot::TokenKeyword,
            Self::StringLiteral => ThemeSemanticSlot::TokenString,
            Self::Number => ThemeSemanticSlot::TokenNumber,
            Self::Command => ThemeSemanticSlot::TokenCommand,
            Self::Path => ThemeSemanticSlot::TokenPath,
            Self::Error => ThemeSemanticSlot::StatusError,
            Self::Muted => ThemeSemanticSlot::UiTextMuted,
            Self::Punctuation => ThemeSemanticSlot::UiTextMuted,
        }
    }
}

/// A sub-line span with byte offsets and semantic kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSpan {
    /// Byte offset into the line text (inclusive).
    pub start: usize,
    /// Byte offset into the line text (exclusive).
    pub end: usize,
    /// Semantic classification for this span.
    pub kind: SpanKind,
}

impl LogSpan {
    /// Extract the text slice from the original line.
    #[must_use]
    pub fn text<'a>(&self, line: &'a str) -> &'a str {
        &line[self.start..self.end]
    }
}

// ---------------------------------------------------------------------------
// HighlightedLine — a line with its spans
// ---------------------------------------------------------------------------

/// A log line enriched with sub-line semantic spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedLine {
    /// The raw line text.
    pub text: String,
    /// Semantic lane classification.
    pub lane: LogLane,
    /// Zero-based index in the original stream.
    pub index: usize,
    /// Ordered, non-overlapping spans covering the full line.
    pub spans: Vec<LogSpan>,
}

// ---------------------------------------------------------------------------
// SectionKind — block-level section classification
// ---------------------------------------------------------------------------

/// Section kind for grouping consecutive lines into foldable blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    /// Tool invocation block (tool call + result).
    ToolCall,
    /// Thinking / reasoning block.
    Thinking,
    /// Error block (error messages, stack traces).
    ErrorBlock,
    /// Stack trace frames and trace headers.
    StackTrace,
    /// Command transcript (shell commands + output).
    CommandBlock,
    /// Diff output block.
    DiffBlock,
    /// Event / lifecycle markers.
    Event,
    /// Generic content (not grouped into a special section).
    Content,
}

impl SectionKind {
    /// Human-readable label for the section.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ToolCall => "tool",
            Self::Thinking => "thinking",
            Self::ErrorBlock => "error",
            Self::StackTrace => "stacktrace",
            Self::CommandBlock => "command",
            Self::DiffBlock => "diff",
            Self::Event => "event",
            Self::Content => "content",
        }
    }

    /// Whether this section kind supports folding by default.
    #[must_use]
    pub fn is_foldable(self) -> bool {
        matches!(
            self,
            Self::ToolCall
                | Self::Thinking
                | Self::StackTrace
                | Self::CommandBlock
                | Self::DiffBlock
        )
    }

    /// Glyph shown when a block is folded, representing hidden content.
    #[must_use]
    pub fn fold_glyph(self) -> &'static str {
        match self {
            Self::ToolCall => "\u{25b6} tool",
            Self::Thinking => "\u{25b6} thinking",
            Self::StackTrace => "\u{25b6} trace",
            Self::CommandBlock => "\u{25b6} command",
            Self::DiffBlock => "\u{25b6} diff",
            Self::ErrorBlock => "\u{25b6} error",
            Self::Event => "\u{25b6} event",
            Self::Content => "\u{25b6} ...",
        }
    }
}

// ---------------------------------------------------------------------------
// LogBlock — a group of consecutive lines forming a foldable section
// ---------------------------------------------------------------------------

/// A block of consecutive log lines that can be folded/unfolded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogBlock {
    /// Section classification.
    pub kind: SectionKind,
    /// Lines belonging to this block (indices into the pipeline's line array).
    pub line_start: usize,
    /// Number of lines in this block.
    pub line_count: usize,
    /// Whether the block is currently folded (collapsed).
    pub folded: bool,
    /// Summary text shown when folded (e.g. "tool: Bash — 14 lines").
    pub fold_summary: String,
}

impl LogBlock {
    /// The visible line count: 1 if folded (summary line), all lines if unfolded.
    #[must_use]
    pub fn visible_lines(&self) -> usize {
        if self.folded {
            1
        } else {
            self.line_count
        }
    }
}

// ---------------------------------------------------------------------------
// LogPipelineV2 — the full pipeline
// ---------------------------------------------------------------------------

/// The log pipeline v2: transforms raw lines into highlighted, block-grouped output.
#[derive(Debug, Clone)]
pub struct LogPipelineV2 {
    /// All highlighted lines.
    lines: Vec<HighlightedLine>,
    /// Block groupings over the lines.
    blocks: Vec<LogBlock>,
}

impl LogPipelineV2 {
    /// Create an empty pipeline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// Process raw lines through the full pipeline.
    #[must_use]
    pub fn from_raw_lines(raw_lines: &[String]) -> Self {
        let highlighted: Vec<HighlightedLine> = raw_lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let lane = classify_line(line);
                let spans = highlight_spans(line, lane);
                HighlightedLine {
                    text: line.clone(),
                    lane,
                    index: i,
                    spans,
                }
            })
            .collect();

        let blocks = group_blocks(&highlighted);

        Self {
            lines: highlighted,
            blocks,
        }
    }

    /// Total number of lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// All highlighted lines.
    #[must_use]
    pub fn lines(&self) -> &[HighlightedLine] {
        &self.lines
    }

    /// All blocks.
    #[must_use]
    pub fn blocks(&self) -> &[LogBlock] {
        &self.blocks
    }

    /// Total visible line count (accounting for folded blocks).
    #[must_use]
    pub fn visible_line_count(&self) -> usize {
        self.blocks.iter().map(LogBlock::visible_lines).sum()
    }

    /// Toggle fold state for the block at the given block index.
    /// Returns true if the block was found and toggled.
    pub fn toggle_fold(&mut self, block_index: usize) -> bool {
        if let Some(block) = self.blocks.get_mut(block_index) {
            if block.kind.is_foldable() {
                block.folded = !block.folded;
                return true;
            }
        }
        false
    }

    /// Fold all foldable blocks.
    pub fn fold_all(&mut self) {
        for block in &mut self.blocks {
            if block.kind.is_foldable() && block.line_count > 1 {
                block.folded = true;
            }
        }
    }

    /// Unfold all blocks.
    pub fn unfold_all(&mut self) {
        for block in &mut self.blocks {
            block.folded = false;
        }
    }

    /// Resolve a visible line index to the actual line, accounting for folds.
    /// Returns `None` if the index is out of range.
    /// For folded blocks, the visible line maps to the fold summary.
    #[must_use]
    pub fn resolve_visible_line(&self, visible_idx: usize) -> Option<VisibleLine<'_>> {
        let mut remaining = visible_idx;
        for block in &self.blocks {
            let visible = block.visible_lines();
            if remaining < visible {
                if block.folded {
                    return Some(VisibleLine::FoldSummary {
                        block_kind: block.kind,
                        summary: &block.fold_summary,
                        hidden_count: block.line_count,
                    });
                } else {
                    let line_idx = block.line_start + remaining;
                    return self.lines.get(line_idx).map(|line| VisibleLine::Line {
                        line,
                        block_kind: block.kind,
                    });
                }
            }
            remaining -= visible;
        }
        None
    }

    /// Find the block index that contains the given raw line index.
    #[must_use]
    pub fn block_for_line(&self, line_index: usize) -> Option<usize> {
        self.blocks.iter().position(|block| {
            line_index >= block.line_start && line_index < block.line_start + block.line_count
        })
    }

    /// Append a single raw line and update the pipeline incrementally.
    pub fn push_line(&mut self, line: String) {
        let index = self.lines.len();
        let lane = classify_line(&line);
        let spans = highlight_spans(&line, lane);
        let highlighted = HighlightedLine {
            text: line,
            lane,
            index,
            spans,
        };

        let section = classify_section(&highlighted);

        // Try to extend the last block if it matches.
        if let Some(last) = self.blocks.last_mut() {
            if last.kind == section && last.line_start + last.line_count == index {
                last.line_count += 1;
                update_fold_summary(last, &self.lines, Some(&highlighted));
                self.lines.push(highlighted);
                return;
            }
        }

        // Create a new block.
        let summary = make_fold_summary(section, &highlighted.text, 1);
        self.lines.push(highlighted);
        self.blocks.push(LogBlock {
            kind: section,
            line_start: index,
            line_count: 1,
            folded: false,
            fold_summary: summary,
        });
    }
}

impl Default for LogPipelineV2 {
    fn default() -> Self {
        Self::new()
    }
}

/// What a visible line resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibleLine<'a> {
    /// A normal line with its highlight data.
    Line {
        line: &'a HighlightedLine,
        block_kind: SectionKind,
    },
    /// A folded block's summary line.
    FoldSummary {
        block_kind: SectionKind,
        summary: &'a str,
        hidden_count: usize,
    },
}

// ---------------------------------------------------------------------------
// highlight_spans — sub-line token extraction
// ---------------------------------------------------------------------------

/// Extract semantic spans from a single log line.
///
/// The span list is guaranteed to:
/// - Cover the entire line text `[0..line.len()]`
/// - Be sorted by `start`
/// - Have no overlaps
#[must_use]
pub fn highlight_spans(line: &str, lane: LogLane) -> Vec<LogSpan> {
    if line.is_empty() {
        return vec![];
    }

    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let len = line.len();
    let mut pos = 0;

    // Fast path for empty/whitespace lines.
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![LogSpan {
            start: 0,
            end: len,
            kind: SpanKind::Plain,
        }];
    }

    // Parse leading bracket markers as muted [EVENT], [ERROR], etc.
    if bytes[0] == b'[' {
        if let Some(close) = line[..len.min(32)].find(']') {
            spans.push(LogSpan {
                start: 0,
                end: close + 1,
                kind: SpanKind::Muted,
            });
            pos = close + 1;
        }
    }

    // Parse prefix labels like "Tool:", "Error:", "Thinking:", etc.
    if pos == 0 {
        if let Some(colon_pos) = find_prefix_label(trimmed) {
            let leading_ws = line.len() - trimmed.len();
            if leading_ws > 0 {
                spans.push(LogSpan {
                    start: 0,
                    end: leading_ws,
                    kind: SpanKind::Plain,
                });
            }
            let label_end = leading_ws + colon_pos + 1;
            let kind = label_span_kind(trimmed, lane);
            spans.push(LogSpan {
                start: leading_ws,
                end: label_end,
                kind,
            });
            pos = label_end;
        }
    }

    // Parse "$ command" lines.
    if pos == 0 && trimmed.starts_with("$ ") {
        let leading_ws = line.len() - trimmed.len();
        if leading_ws > 0 {
            spans.push(LogSpan {
                start: 0,
                end: leading_ws,
                kind: SpanKind::Plain,
            });
        }
        spans.push(LogSpan {
            start: leading_ws,
            end: leading_ws + 2,
            kind: SpanKind::Punctuation,
        });
        pos = leading_ws + 2;
        // Rest of the line is a command.
        if pos < len {
            spans.push(LogSpan {
                start: pos,
                end: len,
                kind: SpanKind::Command,
            });
            return spans;
        }
    }

    // Parse ">>> marker" and "<<< marker" as keyword.
    if pos == 0 && (trimmed.starts_with(">>> ") || trimmed.starts_with("<<< ")) {
        let leading_ws = line.len() - trimmed.len();
        if leading_ws > 0 {
            spans.push(LogSpan {
                start: 0,
                end: leading_ws,
                kind: SpanKind::Plain,
            });
        }
        spans.push(LogSpan {
            start: leading_ws,
            end: leading_ws + 3,
            kind: SpanKind::Punctuation,
        });
        spans.push(LogSpan {
            start: leading_ws + 3,
            end: len,
            kind: SpanKind::Keyword,
        });
        return spans;
    }

    // Parse "> " quote prefix as muted (thinking lane).
    if pos == 0 && trimmed.starts_with("> ") && lane == LogLane::Thinking {
        let leading_ws = line.len() - trimmed.len();
        if leading_ws > 0 {
            spans.push(LogSpan {
                start: 0,
                end: leading_ws,
                kind: SpanKind::Plain,
            });
        }
        spans.push(LogSpan {
            start: leading_ws,
            end: leading_ws + 2,
            kind: SpanKind::Muted,
        });
        pos = leading_ws + 2;
    }

    // Scan the remaining text for inline tokens.
    let scan_start = pos;
    while pos < len {
        // Quoted strings.
        if bytes[pos] == b'"' || bytes[pos] == b'\'' {
            let quote = bytes[pos];
            if let Some(end) = find_closing_quote(line, pos, quote) {
                if pos > scan_start {
                    emit_plain_or_inline(&mut spans, line, scan_start, pos);
                }
                // Don't backtrack — just re-emit from pos.
                spans.push(LogSpan {
                    start: pos,
                    end: end + 1,
                    kind: SpanKind::StringLiteral,
                });
                pos = end + 1;
                continue;
            }
        }

        // Numbers: standalone numeric tokens (not inside words).
        if is_number_start(bytes, pos) {
            let num_end = scan_number(bytes, pos);
            if num_end > pos && is_number_boundary(bytes, num_end) {
                if pos > scan_start {
                    emit_plain_or_inline(&mut spans, line, scan_start, pos);
                }
                spans.push(LogSpan {
                    start: pos,
                    end: num_end,
                    kind: SpanKind::Number,
                });
                pos = num_end;
                continue;
            }
        }

        // File paths: sequences starting with / or ./ containing path separators.
        if (bytes[pos] == b'/' || (bytes[pos] == b'.' && pos + 1 < len && bytes[pos + 1] == b'/'))
            && is_word_boundary_before(bytes, pos)
        {
            let path_end = scan_path(bytes, pos);
            if path_end > pos + 1 {
                if pos > scan_start {
                    emit_plain_or_inline(&mut spans, line, scan_start, pos);
                }
                spans.push(LogSpan {
                    start: pos,
                    end: path_end,
                    kind: SpanKind::Path,
                });
                pos = path_end;
                continue;
            }
        }

        pos += 1;
    }

    // Emit remaining text.
    if scan_start < len && scan_start < pos {
        let remaining_start = spans.last().map(|s| s.end).unwrap_or(scan_start);
        if remaining_start < len {
            emit_plain_or_inline(&mut spans, line, remaining_start, len);
        }
    }

    // Ensure full coverage: fill any gaps.
    fill_gaps(&mut spans, len);

    spans
}

// ---------------------------------------------------------------------------
// Span helper functions
// ---------------------------------------------------------------------------

fn find_prefix_label(trimmed: &str) -> Option<usize> {
    // Match "Label:" at the start (max 20 chars before colon).
    let search = &trimmed[..trimmed.len().min(24)];
    let colon_pos = search.find(':')?;
    if colon_pos == 0 || colon_pos > 20 {
        return None;
    }
    let label = &trimmed[..colon_pos];
    // Must be a single word (no spaces) or known multi-word labels.
    if label.contains(' ') && !label.eq_ignore_ascii_case("exit code") {
        return None;
    }
    // Must start with a letter.
    if !label.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    Some(colon_pos)
}

fn label_span_kind(trimmed: &str, lane: LogLane) -> SpanKind {
    match lane {
        LogLane::Stderr => SpanKind::Error,
        LogLane::Event => SpanKind::Muted,
        LogLane::Tool => SpanKind::Keyword,
        LogLane::Thinking => SpanKind::Keyword,
        _ => {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("error:")
                || lower.starts_with("panic:")
                || lower.starts_with("err:")
                || lower.starts_with("warn:")
                || lower.starts_with("warning:")
            {
                SpanKind::Error
            } else {
                SpanKind::Keyword
            }
        }
    }
}

fn find_closing_quote(line: &str, open_pos: usize, quote: u8) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = open_pos + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_number_start(bytes: &[u8], pos: usize) -> bool {
    let b = bytes[pos];
    if b.is_ascii_digit() {
        return is_word_boundary_before(bytes, pos);
    }
    false
}

fn scan_number(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    // Integer part.
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // Decimal part.
    if i < bytes.len() && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    // Duration suffixes: ms, s, m, h, ns, us.
    if i < bytes.len() {
        let rest = &bytes[i..];
        if rest.starts_with(b"ms")
            || rest.starts_with(b"ns")
            || rest.starts_with(b"us")
            || rest.starts_with(b"\xc2\xb5s")
        {
            // µs is 2 bytes in UTF-8 + s.
            if rest.starts_with(b"\xc2\xb5s") {
                i += 3;
            } else {
                i += 2;
            }
        } else if i < bytes.len()
            && (bytes[i] == b's' || bytes[i] == b'm' || bytes[i] == b'h')
            && is_number_boundary(bytes, i + 1)
        {
            i += 1;
        }
    }
    i
}

fn is_number_boundary(bytes: &[u8], pos: usize) -> bool {
    if pos >= bytes.len() {
        return true;
    }
    let b = bytes[pos];
    b.is_ascii_whitespace() || b == b')' || b == b']' || b == b'}' || b == b',' || b == b';'
}

fn is_word_boundary_before(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let b = bytes[pos - 1];
    b.is_ascii_whitespace()
        || b == b'('
        || b == b'['
        || b == b'{'
        || b == b'='
        || b == b','
        || b == b':'
        || b == b';'
}

fn scan_path(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    let mut has_sep = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' {
            has_sep = true;
            i += 1;
        } else if b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_' || b == b':' {
            i += 1;
        } else {
            break;
        }
    }
    // Trim trailing dots/colons which are likely punctuation.
    while i > pos && (bytes[i - 1] == b'.' || bytes[i - 1] == b':') {
        i -= 1;
    }
    if has_sep {
        i
    } else {
        pos
    }
}

fn emit_plain_or_inline(spans: &mut Vec<LogSpan>, line: &str, start: usize, end: usize) {
    if start >= end || start >= line.len() {
        return;
    }
    let end = end.min(line.len());
    // Check if the text contains error-like keywords.
    let text = &line[start..end];
    let lower = text.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
        spans.push(LogSpan {
            start,
            end,
            kind: SpanKind::Error,
        });
    } else {
        spans.push(LogSpan {
            start,
            end,
            kind: SpanKind::Plain,
        });
    }
}

fn fill_gaps(spans: &mut Vec<LogSpan>, total_len: usize) {
    if total_len == 0 {
        return;
    }
    if spans.is_empty() {
        spans.push(LogSpan {
            start: 0,
            end: total_len,
            kind: SpanKind::Plain,
        });
        return;
    }

    // Sort spans by start position.
    spans.sort_by_key(|s| s.start);

    // Remove overlaps by trimming later spans.
    let mut i = 1;
    while i < spans.len() {
        if spans[i].start < spans[i - 1].end {
            spans[i].start = spans[i - 1].end;
            if spans[i].start >= spans[i].end {
                spans.remove(i);
                continue;
            }
        }
        i += 1;
    }

    // Fill leading gap.
    let mut filled = Vec::new();
    let mut cursor = 0;
    for span in spans.iter() {
        if span.start > cursor {
            filled.push(LogSpan {
                start: cursor,
                end: span.start,
                kind: SpanKind::Plain,
            });
        }
        cursor = span.end;
        filled.push(span.clone());
    }
    if cursor < total_len {
        filled.push(LogSpan {
            start: cursor,
            end: total_len,
            kind: SpanKind::Plain,
        });
    }

    *spans = filled;
}

// ---------------------------------------------------------------------------
// classify_section — block-level section detection
// ---------------------------------------------------------------------------

/// Classify a highlighted line into a section kind for block grouping.
#[must_use]
fn classify_section(line: &HighlightedLine) -> SectionKind {
    if is_stacktrace_line(&line.text) {
        return SectionKind::StackTrace;
    }

    match line.lane {
        LogLane::Tool => {
            let trimmed = line.text.trim();
            if trimmed.starts_with("$ ")
                || trimmed.starts_with("Running:")
                || trimmed.starts_with("Executing:")
            {
                SectionKind::CommandBlock
            } else {
                SectionKind::ToolCall
            }
        }
        LogLane::Thinking => SectionKind::Thinking,
        LogLane::Stderr => SectionKind::ErrorBlock,
        LogLane::Event => SectionKind::Event,
        LogLane::Stdout => {
            let trimmed = line.text.trim();
            if trimmed.starts_with("diff ")
                || trimmed.starts_with("--- ")
                || trimmed.starts_with("+++ ")
            {
                SectionKind::DiffBlock
            } else {
                SectionKind::Content
            }
        }
        LogLane::Unknown => SectionKind::Content,
    }
}

fn is_stacktrace_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("traceback (most recent call last):")
        || lower.starts_with("stack backtrace:")
        || lower.starts_with("stack trace:")
        || lower.starts_with("stacktrace:")
        || lower.starts_with("caused by:")
    {
        return true;
    }

    if lower.starts_with("file \"") && lower.contains(", line ") {
        return true;
    }

    if lower.starts_with("at ")
        && (lower.contains("::")
            || lower.contains(".rs:")
            || lower.contains(".go:")
            || lower.contains(".py:")
            || lower.contains(".js:")
            || lower.contains(".ts:")
            || lower.contains(".java:")
            || lower.contains(".kt:")
            || lower.contains(".swift:")
            || lower.contains('('))
    {
        return true;
    }

    parse_numbered_frame_prefix(&lower).is_some()
}

fn parse_numbered_frame_prefix(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    let digit_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digit_start || idx >= bytes.len() || bytes[idx] != b':' {
        return None;
    }
    idx += 1;
    if idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        Some(idx + 1)
    } else {
        Some(idx)
    }
}

// ---------------------------------------------------------------------------
// group_blocks — group consecutive lines into foldable blocks
// ---------------------------------------------------------------------------

/// Group highlighted lines into blocks. Consecutive lines of the same section
/// kind are merged into a single block. Each block can be independently folded.
#[must_use]
fn group_blocks(lines: &[HighlightedLine]) -> Vec<LogBlock> {
    if lines.is_empty() {
        return vec![];
    }

    let mut blocks = Vec::new();
    let mut current_kind = classify_section(&lines[0]);
    let mut current_start = 0;
    let mut first_text = lines[0].text.clone();

    for (i, line) in lines.iter().enumerate().skip(1) {
        let kind = classify_section(line);
        if kind != current_kind {
            let count = i - current_start;
            blocks.push(LogBlock {
                kind: current_kind,
                line_start: current_start,
                line_count: count,
                folded: false,
                fold_summary: make_fold_summary(current_kind, &first_text, count),
            });
            current_kind = kind;
            current_start = i;
            first_text = line.text.clone();
        }
    }

    // Final block.
    let count = lines.len() - current_start;
    blocks.push(LogBlock {
        kind: current_kind,
        line_start: current_start,
        line_count: count,
        folded: false,
        fold_summary: make_fold_summary(current_kind, &first_text, count),
    });

    blocks
}

fn make_fold_summary(kind: SectionKind, first_line: &str, line_count: usize) -> String {
    let label = kind.fold_glyph();
    let excerpt = first_line.trim();
    let excerpt = if excerpt.len() > 60 {
        &excerpt[..60]
    } else {
        excerpt
    };
    if line_count == 1 {
        format!("{label}: {excerpt}")
    } else {
        format!("{label}: {excerpt} ({line_count} lines)")
    }
}

fn update_fold_summary(
    block: &mut LogBlock,
    existing_lines: &[HighlightedLine],
    _new_line: Option<&HighlightedLine>,
) {
    let first_text = existing_lines
        .get(block.line_start)
        .map(|l| l.text.as_str())
        .unwrap_or("");
    block.fold_summary = make_fold_summary(block.kind, first_text, block.line_count);
}

// ---------------------------------------------------------------------------
// Rule-based anomaly detection
// ---------------------------------------------------------------------------

/// Detected anomaly kind for a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogAnomalyKind {
    /// Panic/fatal crash signatures.
    PanicLike,
    /// Resource exhaustion signatures (OOM/disk full/memory allocation).
    ResourceExhaustion,
    /// Timeout/network failure signatures.
    TimeoutLike,
    /// Explicit non-zero exit code signatures.
    NonZeroExitCode,
    /// A repeated error-like signature in the current window.
    RepeatedSignature,
}

impl LogAnomalyKind {
    #[must_use]
    fn marker_label(self) -> &'static str {
        match self {
            Self::PanicLike => "PANIC",
            Self::ResourceExhaustion => "OOM",
            Self::TimeoutLike => "TIMEOUT",
            Self::NonZeroExitCode => "EXIT",
            Self::RepeatedSignature => "REPEAT",
        }
    }
}

/// A single anomaly match attached to one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogAnomaly {
    /// Zero-based index into the line stream.
    pub line_index: usize,
    /// Matched rule kind.
    pub kind: LogAnomalyKind,
    /// Normalized signature used for grouping similar lines.
    pub signature: String,
    /// Number of times `signature` appeared in this stream.
    pub repeat_count: usize,
}

/// Detect anomalies from a stream of rendered/raw lines using simple rules.
#[must_use]
pub fn detect_rule_based_anomalies(lines: &[String]) -> Vec<LogAnomaly> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut signature_counts: HashMap<String, usize> = HashMap::new();
    let mut signatures_by_line: Vec<Option<String>> = Vec::with_capacity(lines.len());
    for line in lines {
        let signature = anomaly_signature(line);
        if let Some(sig) = &signature {
            *signature_counts.entry(sig.clone()).or_insert(0) += 1;
        }
        signatures_by_line.push(signature);
    }

    let mut anomalies = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let lower = line.trim().to_ascii_lowercase();
        if lower.is_empty() {
            continue;
        }

        if is_panic_like(&lower) {
            anomalies.push(LogAnomaly {
                line_index,
                kind: LogAnomalyKind::PanicLike,
                signature: anomaly_signature(line).unwrap_or_else(|| lower.clone()),
                repeat_count: 1,
            });
        }

        if is_resource_exhaustion(&lower) {
            anomalies.push(LogAnomaly {
                line_index,
                kind: LogAnomalyKind::ResourceExhaustion,
                signature: anomaly_signature(line).unwrap_or_else(|| lower.clone()),
                repeat_count: 1,
            });
        }

        if is_timeout_like(&lower) {
            anomalies.push(LogAnomaly {
                line_index,
                kind: LogAnomalyKind::TimeoutLike,
                signature: anomaly_signature(line).unwrap_or_else(|| lower.clone()),
                repeat_count: 1,
            });
        }

        if has_non_zero_exit_code(&lower) {
            anomalies.push(LogAnomaly {
                line_index,
                kind: LogAnomalyKind::NonZeroExitCode,
                signature: anomaly_signature(line).unwrap_or_else(|| lower.clone()),
                repeat_count: 1,
            });
        }

        if let Some(signature) = signatures_by_line.get(line_index).and_then(Option::as_ref) {
            let repeat_count = signature_counts.get(signature).copied().unwrap_or(0);
            if repeat_count >= 3 {
                anomalies.push(LogAnomaly {
                    line_index,
                    kind: LogAnomalyKind::RepeatedSignature,
                    signature: signature.clone(),
                    repeat_count,
                });
            }
        }
    }

    anomalies
}

/// Prefix detected anomalies into log lines so they stand out in-stream.
#[must_use]
pub fn annotate_lines_with_anomaly_markers(
    lines: &[String],
    anomalies: &[LogAnomaly],
) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }
    if anomalies.is_empty() {
        return lines.to_vec();
    }

    let mut labels_by_line: HashMap<usize, Vec<String>> = HashMap::new();
    for anomaly in anomalies {
        let label = if anomaly.kind == LogAnomalyKind::RepeatedSignature {
            format!(
                "{}x{}",
                anomaly.kind.marker_label(),
                anomaly.repeat_count.max(2)
            )
        } else {
            anomaly.kind.marker_label().to_owned()
        };
        let entry = labels_by_line.entry(anomaly.line_index).or_default();
        if !entry.iter().any(|existing| existing == &label) {
            entry.push(label);
        }
    }

    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let Some(labels) = labels_by_line.get(&index) else {
                return line.clone();
            };
            if line.starts_with("! [ANOM:") {
                return line.clone();
            }
            format!("! [ANOM:{}] {line}", labels.join(","))
        })
        .collect()
}

fn anomaly_signature(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let lane = classify_line(trimmed);
    if lane != LogLane::Stderr && !looks_error_like(&lower) {
        return None;
    }

    let mut normalized = String::with_capacity(lower.len().min(160));
    let mut previous_space = false;
    for ch in lower.chars() {
        let mapped = if ch.is_ascii_digit() { '#' } else { ch };
        if mapped.is_whitespace() {
            if previous_space {
                continue;
            }
            previous_space = true;
            normalized.push(' ');
        } else {
            previous_space = false;
            normalized.push(mapped);
        }
        if normalized.len() >= 160 {
            break;
        }
    }

    let normalized = normalized.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_owned())
    }
}

fn looks_error_like(lower: &str) -> bool {
    lower.contains("error")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("exception")
        || lower.contains("exit code")
        || lower.contains("exit status")
        || lower.contains("refused")
        || lower.contains("denied")
}

fn is_panic_like(lower: &str) -> bool {
    lower.contains("panic")
        || lower.contains("fatal")
        || lower.contains("segmentation fault")
        || lower.contains("assertion failed")
}

fn is_resource_exhaustion(lower: &str) -> bool {
    lower.contains("out of memory")
        || contains_token(lower, "oom")
        || lower.contains("cannot allocate memory")
        || lower.contains("no space left on device")
        || lower.contains("killed process")
}

fn contains_token(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == needle)
}

fn is_timeout_like(lower: &str) -> bool {
    lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("deadline exceeded")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
}

fn has_non_zero_exit_code(lower: &str) -> bool {
    parse_code_after_marker(lower, "exit code")
        .or_else(|| parse_code_after_marker(lower, "exit status"))
        .map(|code| code != 0)
        .unwrap_or(false)
}

fn parse_code_after_marker(lower: &str, marker: &str) -> Option<i32> {
    let marker_index = lower.find(marker)?;
    let tail = &lower[marker_index + marker.len()..];
    let mut number = String::new();
    let mut started = false;

    for ch in tail.chars() {
        if !started {
            if ch == '-' {
                number.push(ch);
                started = true;
                continue;
            }
            if ch.is_ascii_digit() {
                number.push(ch);
                started = true;
                continue;
            }
            continue;
        }

        if ch.is_ascii_digit() {
            number.push(ch);
        } else {
            break;
        }
    }

    if number.is_empty() || number == "-" {
        return None;
    }
    number.parse::<i32>().ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // -- SpanKind tests --

    #[test]
    fn span_kind_theme_slots_are_valid() {
        // Just verify the mapping doesn't panic.
        for kind in [
            SpanKind::Plain,
            SpanKind::Keyword,
            SpanKind::StringLiteral,
            SpanKind::Number,
            SpanKind::Command,
            SpanKind::Path,
            SpanKind::Error,
            SpanKind::Muted,
            SpanKind::Punctuation,
        ] {
            let _slot = kind.theme_slot();
        }
    }

    // -- LogSpan::text --

    #[test]
    fn span_text_extracts_slice() {
        let line = "Tool: read_file";
        let span = LogSpan {
            start: 0,
            end: 5,
            kind: SpanKind::Keyword,
        };
        assert_eq!(span.text(line), "Tool:");
    }

    // -- highlight_spans --

    #[test]
    fn highlight_empty_line() {
        let spans = highlight_spans("", LogLane::Stdout);
        assert!(spans.is_empty());
    }

    #[test]
    fn highlight_whitespace_only() {
        let spans = highlight_spans("   ", LogLane::Stdout);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, SpanKind::Plain);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 3);
    }

    #[test]
    fn highlight_bracket_marker() {
        let spans = highlight_spans("[EVENT] loop started", LogLane::Event);
        assert_eq!(spans[0].kind, SpanKind::Muted);
        assert_eq!(spans[0].text("[EVENT] loop started"), "[EVENT]");
    }

    #[test]
    fn highlight_tool_prefix() {
        let spans = highlight_spans("Tool: read_file", LogLane::Tool);
        assert!(spans.iter().any(|s| s.kind == SpanKind::Keyword));
        let kw = match spans.iter().find(|s| s.kind == SpanKind::Keyword) {
            Some(span) => span,
            None => panic!("expected keyword span"),
        };
        assert_eq!(kw.text("Tool: read_file"), "Tool:");
    }

    #[test]
    fn highlight_error_prefix() {
        let spans = highlight_spans("Error: file not found", LogLane::Stderr);
        let err = match spans.iter().find(|s| s.kind == SpanKind::Error) {
            Some(span) => span,
            None => panic!("expected error span"),
        };
        assert_eq!(err.text("Error: file not found"), "Error:");
    }

    #[test]
    fn highlight_dollar_command() {
        let spans = highlight_spans("$ cargo test", LogLane::Tool);
        let punct = match spans.iter().find(|s| s.kind == SpanKind::Punctuation) {
            Some(span) => span,
            None => panic!("expected punctuation span"),
        };
        assert_eq!(punct.text("$ cargo test"), "$ ");
        let cmd = match spans.iter().find(|s| s.kind == SpanKind::Command) {
            Some(span) => span,
            None => panic!("expected command span"),
        };
        assert_eq!(cmd.text("$ cargo test"), "cargo test");
    }

    #[test]
    fn highlight_arrow_markers() {
        let spans = highlight_spans(">>> tool call: grep", LogLane::Tool);
        assert!(spans.iter().any(|s| s.kind == SpanKind::Punctuation));
        assert!(spans.iter().any(|s| s.kind == SpanKind::Keyword));
    }

    #[test]
    fn highlight_quoted_string() {
        let line = r#"tool: Bash(command="ls -la")"#;
        let spans = highlight_spans(line, LogLane::Tool);
        let string_span = match spans.iter().find(|s| s.kind == SpanKind::StringLiteral) {
            Some(span) => span,
            None => panic!("expected string literal span"),
        };
        assert_eq!(string_span.text(line), r#""ls -la""#);
    }

    #[test]
    fn highlight_number() {
        let line = "exit code: 1";
        let spans = highlight_spans(line, LogLane::Stdout);
        let num = match spans.iter().find(|s| s.kind == SpanKind::Number) {
            Some(span) => span,
            None => panic!("expected number span"),
        };
        assert_eq!(num.text(line), "1");
    }

    #[test]
    fn highlight_number_with_duration_suffix() {
        let line = "took 42ms";
        let spans = highlight_spans(line, LogLane::Stdout);
        let num = match spans.iter().find(|s| s.kind == SpanKind::Number) {
            Some(span) => span,
            None => panic!("expected number span"),
        };
        assert_eq!(num.text(line), "42ms");
    }

    #[test]
    fn highlight_file_path() {
        let line = "at /src/main.rs:10:5";
        let spans = highlight_spans(line, LogLane::Stderr);
        let path = match spans.iter().find(|s| s.kind == SpanKind::Path) {
            Some(span) => span,
            None => panic!("expected path span"),
        };
        assert_eq!(path.text(line), "/src/main.rs:10:5");
    }

    #[test]
    fn highlight_relative_path() {
        let line = "reading ./config/settings.toml";
        let spans = highlight_spans(line, LogLane::Stdout);
        let path = match spans.iter().find(|s| s.kind == SpanKind::Path) {
            Some(span) => span,
            None => panic!("expected path span"),
        };
        assert_eq!(path.text(line), "./config/settings.toml");
    }

    #[test]
    fn highlight_thinking_quote() {
        let spans = highlight_spans("> let me think about this", LogLane::Thinking);
        assert_eq!(spans[0].kind, SpanKind::Muted);
        assert_eq!(spans[0].text("> let me think about this"), "> ");
    }

    #[test]
    fn spans_cover_full_line() {
        let cases = vec![
            ("Tool: read_file /src/main.rs", LogLane::Tool),
            ("Error: compile failed at 42ms", LogLane::Stderr),
            ("[EVENT] started at 12:00", LogLane::Event),
            ("$ cargo test --all", LogLane::Tool),
            (">>> thinking about \"hello\"", LogLane::Thinking),
            ("plain undecorated line", LogLane::Stdout),
            ("running 3 tests... /tmp/out", LogLane::Stdout),
        ];
        for (line, lane) in cases {
            let spans = highlight_spans(line, lane);
            // Check full coverage.
            assert!(!spans.is_empty(), "no spans for: {line}");
            assert_eq!(spans[0].start, 0, "gap at start of: {line}");
            let end = match spans.last() {
                Some(last) => last.end,
                None => panic!("expected at least one span for {line}"),
            };
            assert_eq!(end, line.len(), "gap at end of: {line}");
            // Check no overlaps.
            for pair in spans.windows(2) {
                assert!(
                    pair[0].end <= pair[1].start,
                    "overlap in spans for: {line} — {:?} overlaps {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    // -- SectionKind tests --

    #[test]
    fn section_foldable() {
        assert!(SectionKind::ToolCall.is_foldable());
        assert!(SectionKind::Thinking.is_foldable());
        assert!(SectionKind::StackTrace.is_foldable());
        assert!(SectionKind::CommandBlock.is_foldable());
        assert!(SectionKind::DiffBlock.is_foldable());
        assert!(!SectionKind::Content.is_foldable());
        assert!(!SectionKind::ErrorBlock.is_foldable());
    }

    #[test]
    fn section_labels_are_stable() {
        assert_eq!(SectionKind::ToolCall.label(), "tool");
        assert_eq!(SectionKind::ErrorBlock.label(), "error");
        assert_eq!(SectionKind::StackTrace.label(), "stacktrace");
        assert_eq!(SectionKind::Content.label(), "content");
    }

    // -- LogBlock tests --

    #[test]
    fn block_visible_lines_folded_vs_unfolded() {
        let block = LogBlock {
            kind: SectionKind::ToolCall,
            line_start: 0,
            line_count: 10,
            folded: false,
            fold_summary: String::new(),
        };
        assert_eq!(block.visible_lines(), 10);

        let folded = LogBlock {
            folded: true,
            ..block
        };
        assert_eq!(folded.visible_lines(), 1);
    }

    // -- LogPipelineV2 tests --

    #[test]
    fn pipeline_empty() {
        let pipeline = LogPipelineV2::new();
        assert_eq!(pipeline.line_count(), 0);
        assert_eq!(pipeline.visible_line_count(), 0);
        assert!(pipeline.blocks().is_empty());
    }

    #[test]
    fn pipeline_from_raw_lines() {
        let lines = vec![
            "[EVENT] start".to_owned(),
            "Tool: read_file".to_owned(),
            "Tool: write_file".to_owned(),
            "hello world".to_owned(),
            "Error: bad input".to_owned(),
        ];
        let pipeline = LogPipelineV2::from_raw_lines(&lines);
        assert_eq!(pipeline.line_count(), 5);
        assert_eq!(pipeline.lines()[0].lane, LogLane::Event);
        assert_eq!(pipeline.lines()[1].lane, LogLane::Tool);
        assert_eq!(pipeline.lines()[3].lane, LogLane::Stdout);
        assert_eq!(pipeline.lines()[4].lane, LogLane::Stderr);

        // Should have blocks grouped.
        assert!(!pipeline.blocks().is_empty());
        // First block: event.
        assert_eq!(pipeline.blocks()[0].kind, SectionKind::Event);
        assert_eq!(pipeline.blocks()[0].line_count, 1);
        // Second block: tool (2 lines).
        assert_eq!(pipeline.blocks()[1].kind, SectionKind::ToolCall);
        assert_eq!(pipeline.blocks()[1].line_count, 2);
    }

    #[test]
    fn pipeline_fold_unfold() {
        let lines = vec![
            "Tool: read_file".to_owned(),
            "Tool: write_file".to_owned(),
            "Tool: grep".to_owned(),
            "hello world".to_owned(),
        ];
        let mut pipeline = LogPipelineV2::from_raw_lines(&lines);
        let initial_visible = pipeline.visible_line_count();
        assert_eq!(initial_visible, 4);

        // Fold the tool block (block 0).
        assert!(pipeline.toggle_fold(0));
        assert_eq!(pipeline.visible_line_count(), 2); // 1 (folded) + 1 (content)

        // Unfold.
        assert!(pipeline.toggle_fold(0));
        assert_eq!(pipeline.visible_line_count(), 4);
    }

    #[test]
    fn pipeline_fold_all() {
        let lines = vec![
            "Thinking: let me consider".to_owned(),
            "Thinking: step 1".to_owned(),
            "hello world".to_owned(),
            "Tool: bash".to_owned(),
            "Tool: read".to_owned(),
        ];
        let mut pipeline = LogPipelineV2::from_raw_lines(&lines);
        pipeline.fold_all();

        // Thinking block (2 lines -> 1) + content (1 line) + tool block (2 lines -> 1) = 3
        assert_eq!(pipeline.visible_line_count(), 3);

        pipeline.unfold_all();
        assert_eq!(pipeline.visible_line_count(), 5);
    }

    #[test]
    fn pipeline_resolve_visible_line() {
        let lines = vec![
            "Tool: read_file".to_owned(),
            "Tool: write_file".to_owned(),
            "hello world".to_owned(),
        ];
        let mut pipeline = LogPipelineV2::from_raw_lines(&lines);

        // Unfold: visible lines map 1:1.
        match pipeline.resolve_visible_line(0) {
            Some(VisibleLine::Line { line, .. }) => assert_eq!(line.text, "Tool: read_file"),
            other => panic!("expected Line, got {other:?}"),
        }

        // Fold tool block.
        pipeline.toggle_fold(0);
        match pipeline.resolve_visible_line(0) {
            Some(VisibleLine::FoldSummary {
                block_kind,
                hidden_count,
                ..
            }) => {
                assert_eq!(block_kind, SectionKind::ToolCall);
                assert_eq!(hidden_count, 2);
            }
            other => panic!("expected FoldSummary, got {other:?}"),
        }
        // Second visible line is "hello world".
        match pipeline.resolve_visible_line(1) {
            Some(VisibleLine::Line { line, .. }) => assert_eq!(line.text, "hello world"),
            other => panic!("expected Line, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_push_line_incremental() {
        let mut pipeline = LogPipelineV2::new();
        pipeline.push_line("Tool: first".to_owned());
        pipeline.push_line("Tool: second".to_owned());
        pipeline.push_line("hello".to_owned());

        assert_eq!(pipeline.line_count(), 3);
        // Tool lines should be in the same block.
        assert_eq!(pipeline.blocks().len(), 2);
        assert_eq!(pipeline.blocks()[0].kind, SectionKind::ToolCall);
        assert_eq!(pipeline.blocks()[0].line_count, 2);
    }

    #[test]
    fn pipeline_block_for_line() {
        let lines = vec![
            "[EVENT] start".to_owned(),
            "Tool: bash".to_owned(),
            "Tool: read".to_owned(),
            "output here".to_owned(),
        ];
        let pipeline = LogPipelineV2::from_raw_lines(&lines);
        assert_eq!(pipeline.block_for_line(0), Some(0)); // event block
        assert_eq!(pipeline.block_for_line(1), Some(1)); // tool block
        assert_eq!(pipeline.block_for_line(2), Some(1)); // tool block
        assert_eq!(pipeline.block_for_line(3), Some(2)); // content block
        assert_eq!(pipeline.block_for_line(99), None);
    }

    // -- group_blocks tests --

    #[test]
    fn group_blocks_merges_consecutive_same_kind() {
        let lines: Vec<HighlightedLine> = ["Tool: a", "Tool: b", "Tool: c"]
            .iter()
            .enumerate()
            .map(|(i, text)| HighlightedLine {
                text: text.to_string(),
                lane: LogLane::Tool,
                index: i,
                spans: vec![],
            })
            .collect();
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, SectionKind::ToolCall);
        assert_eq!(blocks[0].line_count, 3);
    }

    #[test]
    fn group_blocks_splits_different_kinds() {
        let lines = vec![
            HighlightedLine {
                text: "Tool: a".to_owned(),
                lane: LogLane::Tool,
                index: 0,
                spans: vec![],
            },
            HighlightedLine {
                text: "hello".to_owned(),
                lane: LogLane::Stdout,
                index: 1,
                spans: vec![],
            },
            HighlightedLine {
                text: "Error: bad".to_owned(),
                lane: LogLane::Stderr,
                index: 2,
                spans: vec![],
            },
        ];
        let blocks = group_blocks(&lines);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, SectionKind::ToolCall);
        assert_eq!(blocks[1].kind, SectionKind::Content);
        assert_eq!(blocks[2].kind, SectionKind::ErrorBlock);
    }

    // -- classify_section tests --

    #[test]
    fn classify_section_tool_line() {
        let line = HighlightedLine {
            text: "Tool: read_file".to_owned(),
            lane: LogLane::Tool,
            index: 0,
            spans: vec![],
        };
        assert_eq!(classify_section(&line), SectionKind::ToolCall);
    }

    #[test]
    fn classify_section_command_line() {
        let line = HighlightedLine {
            text: "$ cargo test".to_owned(),
            lane: LogLane::Tool,
            index: 0,
            spans: vec![],
        };
        assert_eq!(classify_section(&line), SectionKind::CommandBlock);
    }

    #[test]
    fn classify_section_diff_line() {
        let line = HighlightedLine {
            text: "diff --git a/file b/file".to_owned(),
            lane: LogLane::Stdout,
            index: 0,
            spans: vec![],
        };
        assert_eq!(classify_section(&line), SectionKind::DiffBlock);
    }

    #[test]
    fn classify_section_stacktrace_rust_frame() {
        let line = HighlightedLine {
            text: "at forge::runner::execute (src/runner.rs:41)".to_owned(),
            lane: LogLane::Stderr,
            index: 0,
            spans: vec![],
        };
        assert_eq!(classify_section(&line), SectionKind::StackTrace);
    }

    #[test]
    fn classify_section_stacktrace_python_frame() {
        let line = HighlightedLine {
            text: "  File \"/srv/app/main.py\", line 42, in run".to_owned(),
            lane: LogLane::Stdout,
            index: 0,
            spans: vec![],
        };
        assert_eq!(classify_section(&line), SectionKind::StackTrace);
    }

    #[test]
    fn pipeline_fold_all_folds_stacktrace_blocks() {
        let lines = vec![
            "error: task failed".to_owned(),
            "stack backtrace:".to_owned(),
            "   0: std::panicking::begin_panic".to_owned(),
            "   1: forge::runner::execute at src/runner.rs:41".to_owned(),
            "help: rerun with RUST_BACKTRACE=1".to_owned(),
        ];

        let mut pipeline = LogPipelineV2::from_raw_lines(&lines);
        assert_eq!(pipeline.blocks().len(), 3);
        assert_eq!(pipeline.blocks()[1].kind, SectionKind::StackTrace);
        pipeline.fold_all();
        assert!(pipeline.blocks()[1].folded);
        assert_eq!(pipeline.visible_line_count(), 3);
    }

    // -- anomaly detection tests --

    #[test]
    fn detect_rule_based_anomalies_flags_repeat_and_signatures() {
        let lines = vec![
            "info: boot complete".to_owned(),
            "Error: request timed out after 30s".to_owned(),
            "Error: request timed out after 31s".to_owned(),
            "Error: request timed out after 32s".to_owned(),
            "panic: unreachable state".to_owned(),
            "exit code: 2".to_owned(),
            "fatal: out of memory".to_owned(),
        ];

        let anomalies = detect_rule_based_anomalies(&lines);
        assert!(anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::TimeoutLike && a.line_index == 1));
        assert!(anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::RepeatedSignature && a.repeat_count == 3));
        assert!(anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::PanicLike && a.line_index == 4));
        assert!(anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::NonZeroExitCode && a.line_index == 5));
        assert!(anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::ResourceExhaustion && a.line_index == 6));
    }

    #[test]
    fn detect_rule_based_anomalies_ignores_zero_exit_code() {
        let lines = vec!["exit code: 0".to_owned(), "exit status=0".to_owned()];
        let anomalies = detect_rule_based_anomalies(&lines);
        assert!(anomalies
            .iter()
            .all(|a| a.kind != LogAnomalyKind::NonZeroExitCode));
    }

    #[test]
    fn detect_rule_based_anomalies_does_not_flag_boom_as_oom() {
        let lines = vec!["panic: boom".to_owned()];
        let anomalies = detect_rule_based_anomalies(&lines);
        assert!(anomalies.iter().any(|a| a.kind == LogAnomalyKind::PanicLike));
        assert!(!anomalies
            .iter()
            .any(|a| a.kind == LogAnomalyKind::ResourceExhaustion));
    }

    #[test]
    fn annotate_lines_with_anomaly_markers_prefixes_target_lines() {
        let lines = vec![
            "ok".to_owned(),
            "panic: boom".to_owned(),
            "Error: request timed out".to_owned(),
            "Error: request timed out".to_owned(),
            "Error: request timed out".to_owned(),
        ];
        let anomalies = detect_rule_based_anomalies(&lines);
        let annotated = annotate_lines_with_anomaly_markers(&lines, &anomalies);
        assert_eq!(annotated[0], "ok");
        assert!(annotated[1].starts_with("! [ANOM:PANIC]"));
        assert!(annotated[2].contains("TIMEOUT"));
        assert!(annotated[2].contains("REPEATx3"));
    }
}

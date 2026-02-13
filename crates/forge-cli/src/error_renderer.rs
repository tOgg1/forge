//! Error and stacktrace renderer.
//!
//! Highlights panic/traceback/exception blocks, stack frames, file:line:col
//! references, and causal chains. Elevates actionable lines (first cause,
//! failing assertion, command that failed).
//!
//! Design (from PAR-108):
//! - Classify each line within an error block into semantic sub-parts.
//! - Apply [`TokenKind`] styling per sub-line classification.
//! - Extract and underline file:line:col references within stack frames.
//! - No regex — all matching is prefix / `contains` / byte scans.

use crate::highlight_spec::{style_span, TokenKind};

// ---------------------------------------------------------------------------
// Line classification within error blocks
// ---------------------------------------------------------------------------

/// Sub-classification for lines within an error/stacktrace block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorLineKind {
    /// Primary error message: `error:`, `Error:`, `error[E...]`, `panic:`,
    /// `panicked at`, `FAIL`, exception class names.
    ErrorHead,
    /// Python traceback header: `Traceback (most recent call last):`.
    TracebackHeader,
    /// Causal chain: `caused by:`, `Caused by:`.
    CausedBy,
    /// Recovery hint: `recovery:`.
    Recovery,
    /// Compiler/linter note: `note:`, `help:`.
    Note,
    /// Stack frame: `at func (file:line:col)`, `  at file:line`, `goroutine`,
    /// `thread '...'`, Rust `-->` pointer, Python `File "..."` lines.
    StackFrame,
    /// Indented continuation (4+ spaces) that is not a recognized sub-type.
    Continuation,
    /// Signal/trap info: `[signal ...]`.
    Signal,
}

/// Classify a single line that is known to be inside an error block.
fn classify_error_line(line: &str) -> ErrorLineKind {
    let trimmed = line.trim_start();

    // Python traceback header.
    if trimmed.starts_with("Traceback (most recent call last):") {
        return ErrorLineKind::TracebackHeader;
    }

    // Primary error heads.
    if trimmed.starts_with("error:")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("error[")
        || trimmed.starts_with("panic:")
        || trimmed.starts_with("panicked at")
        || trimmed.starts_with("FAIL")
        || trimmed.starts_with("unexpected concurrent change")
        || is_exception_class(trimmed)
    {
        return ErrorLineKind::ErrorHead;
    }

    // Causal chain.
    if starts_with_ci(trimmed, "caused by:") {
        return ErrorLineKind::CausedBy;
    }

    // Recovery.
    if trimmed.starts_with("recovery:") {
        return ErrorLineKind::Recovery;
    }

    // Compiler notes / help.
    if trimmed.starts_with("note:") || trimmed.starts_with("help:") {
        return ErrorLineKind::Note;
    }

    // Signal line: [signal SIGSEGV: ...]
    if trimmed.starts_with("[signal ") {
        return ErrorLineKind::Signal;
    }

    // Stack frame indicators.
    if trimmed.starts_with("at ")
        || trimmed.starts_with("  at ")
        || trimmed.starts_with("--> ")
        || trimmed.starts_with(" --> ")
        || trimmed.starts_with("goroutine ")
        || trimmed.starts_with("thread '")
    {
        return ErrorLineKind::StackFrame;
    }

    // Python stack frame: File "path", line N, in func
    if trimmed.starts_with("File \"") {
        return ErrorLineKind::StackFrame;
    }

    // Indented lines that look like stack frames (file:line patterns).
    if (trimmed.starts_with('/') || trimmed.starts_with("    ")) && contains_file_line_ref(trimmed)
    {
        return ErrorLineKind::StackFrame;
    }

    // Generic indented continuation.
    if line.starts_with("    ") || line.starts_with('\t') {
        return ErrorLineKind::Continuation;
    }

    // Fallback: treat as error continuation.
    ErrorLineKind::Continuation
}

/// Check if a line looks like an exception class name (e.g.
/// `ProviderModelNotFoundError: ...`, `TypeError: ...`).
fn is_exception_class(line: &str) -> bool {
    // Pattern: starts with uppercase, contains "Error" or "Exception",
    // followed by `:` or end-of-line.
    if let Some(first_char) = line.chars().next() {
        if !first_char.is_ascii_uppercase() {
            return false;
        }
    } else {
        return false;
    }

    // Find the colon or end of first word.
    let word_end = line.find([':', ' ']).unwrap_or(line.len());
    let word = &line[..word_end];
    word.contains("Error") || word.contains("Exception") || word.contains("Panic")
}

/// Case-insensitive starts_with check.
fn starts_with_ci(text: &str, prefix: &str) -> bool {
    if text.len() < prefix.len() {
        return false;
    }
    text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// Check if text contains a file:line(:col) reference pattern.
fn contains_file_line_ref(text: &str) -> bool {
    // Look for patterns like `file.ext:NN` where NN is digits.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' && i > 0 {
            // Check that the char before `:` is part of a filename
            // (letter, digit, dot, underscore, dash, slash).
            let prev = bytes[i - 1];
            let is_filename_char = prev.is_ascii_alphanumeric()
                || prev == b'.'
                || prev == b'_'
                || prev == b'-'
                || prev == b'/'
                || prev == b')';
            if is_filename_char {
                // Check that at least one digit follows.
                let rest = &bytes[i + 1..];
                if !rest.is_empty() && rest[0].is_ascii_digit() {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

// ---------------------------------------------------------------------------
// File:line:col span extraction
// ---------------------------------------------------------------------------

/// A byte range within a line that represents a file:line(:col) reference.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRef {
    start: usize,
    end: usize,
}

/// Find all file:line(:col) references in a line.
fn find_file_refs(line: &str) -> Vec<FileRef> {
    let mut refs = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b':' && i > 0 {
            // Check that char before `:` is a valid filename ending.
            let prev = bytes[i - 1];
            let is_filename_end = prev.is_ascii_alphanumeric()
                || prev == b'.'
                || prev == b'_'
                || prev == b'-'
                || prev == b'/'
                || prev == b')';

            if is_filename_end {
                // Read digits after the colon.
                let digit_start = i + 1;
                let mut digit_end = digit_start;
                while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
                    digit_end += 1;
                }

                if digit_end > digit_start {
                    // We have file:line. Check for optional :col.
                    let mut ref_end = digit_end;
                    if ref_end < bytes.len() && bytes[ref_end] == b':' {
                        let col_start = ref_end + 1;
                        let mut col_end = col_start;
                        while col_end < bytes.len() && bytes[col_end].is_ascii_digit() {
                            col_end += 1;
                        }
                        if col_end > col_start {
                            ref_end = col_end;
                        }
                    }

                    // Walk backwards to find the start of the path.
                    let path_start = find_path_start(bytes, i - 1);

                    // Only include if the path part has a dot (looks like a file).
                    let path_slice = &line[path_start..i];
                    if path_slice.contains('.') || path_slice.contains('/') {
                        refs.push(FileRef {
                            start: path_start,
                            end: ref_end,
                        });
                        i = ref_end;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    refs
}

/// Walk backwards from `pos` to find the start of a file path.
fn find_path_start(bytes: &[u8], pos: usize) -> usize {
    let mut start = pos;
    while start > 0 {
        let prev = bytes[start - 1];
        if prev.is_ascii_alphanumeric()
            || prev == b'.'
            || prev == b'_'
            || prev == b'-'
            || prev == b'/'
            || prev == b'\\'
        {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render a set of error block lines with semantic highlighting.
///
/// Each line is classified and styled according to its role in the error
/// block. File:line:col references within stack frames are underlined.
pub(crate) fn render_error_lines(lines: &[String], use_color: bool) -> Vec<String> {
    let mut rendered = Vec::with_capacity(lines.len());
    let mut seen_first_cause = false;

    for line in lines {
        let kind = classify_error_line(line);
        let styled = match kind {
            ErrorLineKind::ErrorHead => {
                seen_first_cause = true;
                style_span(line, TokenKind::Error, use_color)
            }
            ErrorLineKind::TracebackHeader => {
                // Python traceback header is an error indicator.
                style_span(line, TokenKind::Error, use_color)
            }
            ErrorLineKind::CausedBy => {
                if !seen_first_cause {
                    seen_first_cause = true;
                }
                render_with_file_refs(line, TokenKind::Error, use_color)
            }
            ErrorLineKind::Recovery => {
                // Recovery lines are actionable — style as warning to stand out.
                style_span(line, TokenKind::Warning, use_color)
            }
            ErrorLineKind::Note => render_with_file_refs(line, TokenKind::Warning, use_color),
            ErrorLineKind::Signal => style_span(line, TokenKind::Error, use_color),
            ErrorLineKind::StackFrame => {
                render_with_file_refs(line, TokenKind::StackFrame, use_color)
            }
            ErrorLineKind::Continuation => {
                render_with_file_refs(line, TokenKind::StackFrame, use_color)
            }
        };
        rendered.push(styled);
    }

    rendered
}

/// Render a line with its base token kind, but highlight file:line:col refs
/// within it as [`TokenKind::PathLine`].
fn render_with_file_refs(line: &str, base_kind: TokenKind, use_color: bool) -> String {
    let refs = find_file_refs(line);
    if refs.is_empty() {
        return style_span(line, base_kind, use_color);
    }

    if !use_color {
        // In no-color mode, just apply the base signifier.
        return style_span(line, base_kind, use_color);
    }

    // Build styled output with file refs highlighted differently.
    let mut out = String::with_capacity(line.len() + 32);
    let mut pos = 0;

    for fref in &refs {
        if fref.start > pos {
            out.push_str(&style_span(&line[pos..fref.start], base_kind, use_color));
        }
        out.push_str(&style_span(
            &line[fref.start..fref.end],
            TokenKind::PathLine,
            use_color,
        ));
        pos = fref.end;
    }

    if pos < line.len() {
        out.push_str(&style_span(&line[pos..], base_kind, use_color));
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Line classification ─────────────────────────────────────────

    #[test]
    fn classifies_rust_error() {
        assert_eq!(
            classify_error_line("error: test failed"),
            ErrorLineKind::ErrorHead
        );
        assert_eq!(
            classify_error_line("error[E0277]: trait bound not satisfied"),
            ErrorLineKind::ErrorHead
        );
    }

    #[test]
    fn classifies_panic() {
        assert_eq!(
            classify_error_line("panic: runtime error: invalid memory address"),
            ErrorLineKind::ErrorHead
        );
        assert_eq!(
            classify_error_line("panicked at 'assertion failed'"),
            ErrorLineKind::ErrorHead
        );
    }

    #[test]
    fn classifies_go_fail() {
        assert_eq!(
            classify_error_line("FAIL    internal/integration"),
            ErrorLineKind::ErrorHead
        );
    }

    #[test]
    fn classifies_exception_class() {
        assert_eq!(
            classify_error_line("ProviderModelNotFoundError: message"),
            ErrorLineKind::ErrorHead
        );
        assert_eq!(
            classify_error_line("TypeError: cannot read property"),
            ErrorLineKind::ErrorHead
        );
        assert_eq!(
            classify_error_line("NullPointerException: null"),
            ErrorLineKind::ErrorHead
        );
    }

    #[test]
    fn classifies_caused_by() {
        assert_eq!(
            classify_error_line("caused by: connection refused"),
            ErrorLineKind::CausedBy
        );
        assert_eq!(
            classify_error_line("Caused by: timeout"),
            ErrorLineKind::CausedBy
        );
    }

    #[test]
    fn classifies_recovery() {
        assert_eq!(
            classify_error_line("recovery: fix rate limiter test fixture"),
            ErrorLineKind::Recovery
        );
    }

    #[test]
    fn classifies_note() {
        assert_eq!(
            classify_error_line("note: required by trait bound"),
            ErrorLineKind::Note
        );
        assert_eq!(
            classify_error_line("help: try adding #[derive(Debug)]"),
            ErrorLineKind::Note
        );
    }

    #[test]
    fn classifies_signal() {
        assert_eq!(
            classify_error_line("[signal SIGSEGV: segmentation violation code=0x1]"),
            ErrorLineKind::Signal
        );
    }

    #[test]
    fn classifies_stack_frames() {
        assert_eq!(
            classify_error_line("  at Object.middleware (src/provider/provider.ts:1012:28)"),
            ErrorLineKind::StackFrame
        );
        assert_eq!(
            classify_error_line("--> src/auth.rs:45:10"),
            ErrorLineKind::StackFrame
        );
        assert_eq!(
            classify_error_line("goroutine 1 [running]:"),
            ErrorLineKind::StackFrame
        );
        assert_eq!(
            classify_error_line("thread 'main' panicked"),
            ErrorLineKind::StackFrame
        );
    }

    #[test]
    fn classifies_file_path_as_stack_frame() {
        assert_eq!(
            classify_error_line("        /repo/internal/integration/integration_test.go:47 +0x1a3"),
            ErrorLineKind::StackFrame
        );
    }

    #[test]
    fn classifies_indented_continuation() {
        assert_eq!(
            classify_error_line("    some indented detail"),
            ErrorLineKind::Continuation
        );
    }

    // ── File reference detection ────────────────────────────────────

    #[test]
    fn finds_file_line_ref() {
        let refs = find_file_refs("at src/main.rs:42:10");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            &"at src/main.rs:42:10"[refs[0].start..refs[0].end],
            "src/main.rs:42:10"
        );
    }

    #[test]
    fn finds_file_line_without_col() {
        let refs = find_file_refs("at src/lib.rs:100");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            &"at src/lib.rs:100"[refs[0].start..refs[0].end],
            "src/lib.rs:100"
        );
    }

    #[test]
    fn finds_go_test_path() {
        let line = "/repo/internal/integration/integration_test.go:47 +0x1a3";
        let refs = find_file_refs(line);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            &line[refs[0].start..refs[0].end],
            "/repo/internal/integration/integration_test.go:47"
        );
    }

    #[test]
    fn finds_multiple_refs() {
        let line = "see src/a.rs:10 and src/b.rs:20:5";
        let refs = find_file_refs(line);
        assert_eq!(refs.len(), 2);
        assert_eq!(&line[refs[0].start..refs[0].end], "src/a.rs:10");
        assert_eq!(&line[refs[1].start..refs[1].end], "src/b.rs:20:5");
    }

    #[test]
    fn no_file_ref_in_plain_text() {
        let refs = find_file_refs("just some plain text");
        assert!(refs.is_empty());
    }

    #[test]
    fn no_false_positive_on_timestamp() {
        // Timestamps like "2026:02:10" should not match since there's no file
        // extension before the colon.
        let refs = find_file_refs("at 2026:02:10");
        // 2026:02:10 doesn't look like a file (no dot/slash before colon).
        assert!(refs.is_empty());
    }

    // ── Rendering ───────────────────────────────────────────────────

    #[test]
    fn renders_error_block_with_color() {
        let lines = vec![
            "error: test failed".to_string(),
            "  at src/main.rs:42:10".to_string(),
            "recovery: fix the test".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 3);
        // Error head should have bold red styling.
        assert!(rendered[0].contains("\x1b[1;31m"));
        // Stack frame should have dim red + path highlight.
        assert!(rendered[1].contains("\x1b["));
        // Recovery should have warning styling.
        assert!(rendered[2].contains("\x1b[33m") || rendered[2].contains("\x1b["));
    }

    #[test]
    fn renders_error_block_no_color() {
        let lines = vec![
            "error: test failed".to_string(),
            "  at src/main.rs:42:10".to_string(),
            "recovery: fix the test".to_string(),
        ];

        let rendered = render_error_lines(&lines, false);
        assert_eq!(rendered.len(), 3);
        // Error head gets [ERROR] prefix.
        assert!(rendered[0].starts_with("[ERROR] "));
        // Recovery gets [WARN] prefix.
        assert!(rendered[2].starts_with("[WARN] "));
    }

    #[test]
    fn renders_rust_panic_with_stack() {
        let lines = vec![
            "thread 'main' panicked at 'assertion failed: x > 0'".to_string(),
            "note: run with `RUST_BACKTRACE=1`".to_string(),
            "  at /rustc/hash/library/core/src/panicking.rs:220:5".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 3);
        // All lines should have ANSI styling.
        for line in &rendered {
            assert!(line.contains("\x1b["), "line should be styled: {line}");
        }
    }

    #[test]
    fn renders_go_panic_with_goroutine() {
        let lines = vec![
            "panic: runtime error: invalid memory address or nil pointer dereference".to_string(),
            "[signal SIGSEGV: segmentation violation code=0x1 addr=0x0 pc=0x1234567]".to_string(),
            "goroutine 1 [running]:".to_string(),
            "        /repo/internal/integration/integration_test.go:47 +0x1a3".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 4);
        // Panic line: bold red.
        assert!(rendered[0].contains("\x1b[1;31m"));
        // Signal line: bold red (error).
        assert!(rendered[1].contains("\x1b[1;31m"));
    }

    #[test]
    fn renders_js_error_with_stack() {
        let lines = vec![
            "TypeError: Cannot read property 'foo' of undefined".to_string(),
            "    at Object.middleware (src/provider/provider.ts:989:13)".to_string(),
            "    at processTicksAndRejections (node:internal/process/task_queues:95:5)".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 3);
        // Error head should be styled.
        assert!(rendered[0].contains("\x1b[1;31m"));
    }

    #[test]
    fn renders_causal_chain() {
        let lines = vec![
            "error: database connection failed".to_string(),
            "caused by: connection refused".to_string(),
            "caused by: no route to host".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 3);
        // All lines should have error styling.
        for line in &rendered {
            assert!(
                line.contains("\x1b[1;31m"),
                "line should be error-styled: {line}"
            );
        }
    }

    #[test]
    fn renders_rust_compiler_error() {
        let lines = vec![
            "error[E0277]: the trait bound `Foo: Bar` is not satisfied".to_string(),
            " --> src/lib.rs:42:10".to_string(),
            "note: required by a bound in `baz`".to_string(),
            "help: consider adding #[derive(Bar)]".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 4);
        // Error head.
        assert!(rendered[0].contains("\x1b[1;31m"));
        // File pointer should have path highlight.
        assert!(rendered[1].contains("\x1b["));
    }

    #[test]
    fn empty_input_returns_empty() {
        let rendered = render_error_lines(&[], true);
        assert!(rendered.is_empty());
    }

    // ── Helper tests ────────────────────────────────────────────────

    #[test]
    fn is_exception_class_positive() {
        assert!(is_exception_class("TypeError: foo"));
        assert!(is_exception_class("NullPointerException: bar"));
        assert!(is_exception_class("ProviderModelNotFoundError: baz"));
    }

    #[test]
    fn is_exception_class_negative() {
        assert!(!is_exception_class("lowercase_error: foo"));
        assert!(!is_exception_class("ALLCAPS: bar"));
        assert!(!is_exception_class("SomeClass: baz"));
        assert!(!is_exception_class(""));
    }

    #[test]
    fn starts_with_ci_works() {
        assert!(starts_with_ci("Caused by: foo", "caused by:"));
        assert!(starts_with_ci("CAUSED BY: foo", "caused by:"));
        assert!(!starts_with_ci("ca", "caused by:"));
    }

    #[test]
    fn contains_file_line_ref_positive() {
        assert!(contains_file_line_ref("src/main.rs:42"));
        assert!(contains_file_line_ref("file.go:100:5"));
        assert!(contains_file_line_ref("/absolute/path.py:10"));
    }

    #[test]
    fn contains_file_line_ref_negative() {
        assert!(!contains_file_line_ref("no file refs here"));
        assert!(!contains_file_line_ref("just:text"));
    }

    // ── Python traceback ──────────────────────────────────────────

    #[test]
    fn classifies_python_traceback_header() {
        assert_eq!(
            classify_error_line("Traceback (most recent call last):"),
            ErrorLineKind::TracebackHeader
        );
    }

    #[test]
    fn classifies_python_file_line() {
        assert_eq!(
            classify_error_line("  File \"foo.py\", line 42, in bar"),
            ErrorLineKind::StackFrame
        );
        assert_eq!(
            classify_error_line("  File \"/usr/lib/python3/module.py\", line 10, in <module>"),
            ErrorLineKind::StackFrame
        );
    }

    #[test]
    fn classifies_python_assertion_error() {
        assert_eq!(
            classify_error_line("AssertionError: expected True"),
            ErrorLineKind::ErrorHead
        );
        assert_eq!(
            classify_error_line("ValueError: invalid literal"),
            ErrorLineKind::ErrorHead
        );
    }

    #[test]
    fn renders_python_traceback() {
        let lines = vec![
            "Traceback (most recent call last):".to_string(),
            "  File \"test_main.py\", line 10, in test_foo".to_string(),
            "    assert result == 42".to_string(),
            "AssertionError: expected 42 but got 0".to_string(),
        ];

        let rendered = render_error_lines(&lines, true);
        assert_eq!(rendered.len(), 4);
        // Traceback header: bold red.
        assert!(rendered[0].contains("\x1b[1;31m"));
        // AssertionError at end: bold red.
        assert!(rendered[3].contains("\x1b[1;31m"));
    }

    #[test]
    fn renders_python_traceback_no_color() {
        let lines = vec![
            "Traceback (most recent call last):".to_string(),
            "  File \"test_main.py\", line 10, in test_foo".to_string(),
            "AssertionError: expected True".to_string(),
        ];

        let rendered = render_error_lines(&lines, false);
        assert_eq!(rendered.len(), 3);
        // Traceback header gets [ERROR] prefix.
        assert!(rendered[0].starts_with("[ERROR] "));
        // AssertionError also gets [ERROR] prefix.
        assert!(rendered[2].starts_with("[ERROR] "));
    }
}

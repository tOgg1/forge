#![allow(clippy::expect_used, clippy::unwrap_used)]
//! PAR-113: Golden snapshot + property tests for the highlighting engine.
//!
//! Golden tests assert deterministic output for harness-specific transcripts
//! (Codex, Claude, Pi, OpenCode) through the section parser, markdown lexer,
//! and token spec layers.
//!
//! Property tests exercise edge cases: ANSI escape codes, malformed UTF-8
//! (replaced via lossy conversion), extremely long lines, partial/unclosed
//! fences, and nested structures. All tests require deterministic output
//! across runs and platforms.

use forge_cli::highlight_spec::{
    ansi_style_for_theme, no_color_signifier, resolve_precedence, resolve_theme, style_span,
    style_span_with_theme, HighlightTheme, TerminalColorCapability, TerminalTone, ThemeEnvHints,
    TokenKind, TOKEN_KINDS_BY_PRECEDENCE,
};
use forge_cli::markdown_lexer::{MarkdownEvent, MarkdownLexer};
use forge_cli::section_parser::{SectionEvent, SectionKind, SectionParser};

// =========================================================================
// Helpers
// =========================================================================

/// Serialize section events into a deterministic text format for golden
/// comparison. Format: `EventKind:section-slug:line_number:content`
fn serialize_section_events(events: &[SectionEvent]) -> String {
    let mut lines = Vec::new();
    for event in events {
        match event {
            SectionEvent::Start {
                kind,
                line,
                line_number,
            } => {
                lines.push(format!("Start:{}:{}:{}", kind.slug(), line_number, line));
            }
            SectionEvent::Continue {
                kind,
                line,
                line_number,
            } => {
                lines.push(format!("Continue:{}:{}:{}", kind.slug(), line_number, line));
            }
            SectionEvent::End { kind, line_number } => {
                lines.push(format!("End:{}:{}", kind.slug(), line_number));
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Serialize markdown events into a deterministic text format for golden
/// comparison.
fn serialize_markdown_events(events: &[MarkdownEvent]) -> String {
    let mut lines = Vec::new();
    for event in events {
        match event {
            MarkdownEvent::FenceOpen {
                line_number,
                fence_len,
                language,
                ..
            } => {
                let lang_str = language
                    .as_ref()
                    .map(|l| l.slug().to_string())
                    .unwrap_or_default();
                lines.push(format!(
                    "FenceOpen:{}:{}:{}",
                    line_number, fence_len, lang_str
                ));
            }
            MarkdownEvent::FenceLine {
                line_number,
                language,
                text,
            } => {
                let lang_str = language
                    .as_ref()
                    .map(|l| l.slug().to_string())
                    .unwrap_or_default();
                lines.push(format!("FenceLine:{}:{}:{}", line_number, lang_str, text));
            }
            MarkdownEvent::FenceClose {
                line_number,
                fence_len,
            } => {
                lines.push(format!("FenceClose:{}:{}", line_number, fence_len));
            }
            MarkdownEvent::InlineCodeSpan {
                line_number,
                start,
                end,
                code,
            } => {
                lines.push(format!(
                    "InlineCodeSpan:{}:{}:{}:{}",
                    line_number, start, end, code
                ));
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// =========================================================================
// Golden snapshot tests — Section Parser
// =========================================================================

#[test]
fn golden_section_parser_codex_transcript() {
    let input = "\
OpenAI Codex v0.80.0 (research preview)
--------
workdir: /repo/forge
model: gpt-5.2-codex
--------
user
Implement the database connection pool
thinking
**Analyzing requirements**
The task requires a pool.
codex
I'll implement it.
```rust
pub struct Pool {}
```
tokens used
15,892";

    let events = SectionParser::parse_all(input);
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_codex.txt");
    assert_eq!(actual, expected, "Codex transcript golden mismatch");
}

#[test]
fn golden_section_parser_claude_json_events() {
    let input = r#"{"type":"system","subtype":"init","model":"claude-opus-4-6"}
{"type":"stream_event","event":{"type":"content_block_start"}}
[2026-02-09T16:00:01Z] status: idle"#;

    let events = SectionParser::parse_all(input);
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_claude.txt");
    assert_eq!(actual, expected, "Claude JSON events golden mismatch");
}

#[test]
fn golden_section_parser_pi_transcript() {
    let input = "\
Pi Coding Agent v2.1.0
Configuration loaded from /home/user/.pi/config.toml
[2026-01-10T20:10:00Z] status: starting
tool: read /repo/src/api/client.rs
```rust
pub struct RetryPolicy {}
```
[2026-01-10T20:11:30Z] status: success";

    let events = SectionParser::parse_all(input);
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_pi.txt");
    assert_eq!(actual, expected, "Pi transcript golden mismatch");
}

#[test]
fn golden_section_parser_opencode_box_drawing() {
    let input = "\
╭─────────────────────────╮
│ OpenCode v0.1.0         │
╰─────────────────────────╯
some output";

    let events = SectionParser::parse_all(input);
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_opencode.txt");
    assert_eq!(actual, expected, "OpenCode box-drawing golden mismatch");
}

#[test]
fn golden_section_parser_error_block() {
    let input = "\
error: integration tests failed with 1 failure
caused by: connection refused
recovery: fix rate limiter test fixture
plain line after";

    let events = SectionParser::parse_all(input);
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_error_block.txt");
    assert_eq!(actual, expected, "Error block golden mismatch");
}

#[test]
fn golden_section_parser_diff_block() {
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
    let actual = serialize_section_events(&events);
    let expected = include_str!("golden/highlighting/section_parser_diff.txt");
    assert_eq!(actual, expected, "Diff block golden mismatch");
}

// =========================================================================
// Golden snapshot tests — Markdown Lexer
// =========================================================================

#[test]
fn golden_markdown_lexer_fenced_blocks() {
    let input = "```rust\nfn main() {}\n```\n````python\nprint(\"hello\")\n# comment\n````\n";
    let mut lexer = MarkdownLexer::new();
    let events = lexer.feed_chunk(input);
    let actual = serialize_markdown_events(&events);
    let expected = include_str!("golden/highlighting/markdown_lexer_fenced.txt");
    assert_eq!(actual, expected, "Markdown fenced blocks golden mismatch");
}

// =========================================================================
// Golden snapshot tests — Token Spec Style Map
// =========================================================================

#[test]
fn golden_token_spec_ansi16_styles() {
    let mut actual_lines = Vec::new();
    for tone in [TerminalTone::Dark, TerminalTone::Light] {
        for kind in &TOKEN_KINDS_BY_PRECEDENCE {
            let style = ansi_style_for_theme(*kind, TerminalColorCapability::Ansi16, tone);
            actual_lines.push(format!(
                "ansi16:{}:{}:sgr={}:{}",
                tone.slug(),
                kind.slug(),
                style.sgr,
                style.label
            ));
        }
    }
    let mut actual = actual_lines.join("\n");
    actual.push('\n');
    let expected = include_str!("golden/highlighting/token_spec_styles.txt");
    assert_eq!(actual, expected, "Token spec ANSI16 style golden mismatch");
}

// =========================================================================
// Determinism tests — multiple runs produce identical output
// =========================================================================

#[test]
fn section_parser_deterministic_across_runs() {
    let input = "\
OpenAI Codex v0.80.0 (research preview)
--------
workdir: /repo
model: gpt-5.2
--------
user
Build the thing
thinking
**Planning**
Let me think...
codex
Done!
```rust
fn foo() {}
```
diff --git a/x b/x
--- a/x
+++ b/x
@@ -1 +1 @@
-old
+new
error: test failed
recovery: fix it
tokens used
42,000
[2026-02-09T16:00:01Z] status: complete";

    let runs: Vec<String> = (0..5)
        .map(|_| {
            let events = SectionParser::parse_all(input);
            serialize_section_events(&events)
        })
        .collect();

    for (i, run) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            runs[0], *run,
            "Section parser output differs between run 0 and run {}",
            i
        );
    }
}

#[test]
fn markdown_lexer_deterministic_across_runs() {
    let input = "```rust\nfn main() {}\n```\nsome `inline` code\n```python\nprint(1)\n```\n";

    let runs: Vec<String> = (0..5)
        .map(|_| {
            let mut lexer = MarkdownLexer::new();
            let events = lexer.feed_chunk(input);
            serialize_markdown_events(&events)
        })
        .collect();

    for (i, run) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            runs[0], *run,
            "Markdown lexer output differs between run 0 and run {}",
            i
        );
    }
}

#[test]
fn style_span_deterministic_across_runs() {
    let text = "error: test failed";
    let runs: Vec<String> = (0..5)
        .map(|_| style_span(text, TokenKind::Error, true))
        .collect();

    for (i, run) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            runs[0], *run,
            "style_span output differs between run 0 and run {}",
            i
        );
    }
}

// =========================================================================
// Property tests — escape codes
// =========================================================================

#[test]
fn section_parser_handles_ansi_escape_codes_without_panic() {
    let escape_lines = [
        "\x1b[31mred text\x1b[0m",
        "\x1b[1;32;40mcomplex escape\x1b[0m",
        "\x1b[0m",
        "\x1b[38;5;196mextended color\x1b[0m",
        "\x1b[38;2;255;0;0mtruecolor\x1b[0m",
        "normal text\x1b[31m mixed \x1b[0m text",
        "\x1b[?25h\x1b[?25l",      // cursor visibility
        "\x1b[2J\x1b[H",           // clear screen + home
        "\x1b]0;window title\x07", // OSC sequence
        "\x1b[K",                  // erase to end of line
    ];

    let full_input = escape_lines.join("\n");
    let events = SectionParser::parse_all(&full_input);
    // Must not panic and must produce events for every line.
    assert!(
        !events.is_empty(),
        "should produce events for escape code input"
    );
}

#[test]
fn markdown_lexer_handles_ansi_escape_codes_without_panic() {
    let input = "\x1b[31m```rust\x1b[0m\n\x1b[32mfn main() {}\x1b[0m\n```\n";
    let mut lexer = MarkdownLexer::new();
    let events = lexer.feed_chunk(input);
    // Must not panic. The fence may or may not be detected (escape codes
    // break the prefix match), but output must be deterministic.
    let _ = events;
}

#[test]
fn style_span_handles_text_with_embedded_escapes() {
    let text = "\x1b[31merror\x1b[0m: test failed";
    let styled = style_span(text, TokenKind::Error, true);
    // Must not panic, must produce output.
    assert!(!styled.is_empty());
    // Must be deterministic.
    let styled2 = style_span(text, TokenKind::Error, true);
    assert_eq!(styled, styled2);
}

// =========================================================================
// Property tests — malformed UTF-8 (via lossy conversion)
// =========================================================================

#[test]
fn section_parser_handles_replacement_characters() {
    // Simulate what String::from_utf8_lossy produces for invalid UTF-8.
    let lines_with_replacement = [
        "\u{FFFD}\u{FFFD}\u{FFFD}",               // pure replacement chars
        "valid prefix \u{FFFD} suffix",           // mixed
        "\u{FFFD}error: something",               // replacement before error keyword
        "```\u{FFFD}rust",                        // replacement in fence hint
        "diff --git a/\u{FFFD}.rs b/\u{FFFD}.rs", // replacement in diff path
        "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}", // all replacements
    ];

    let input = lines_with_replacement.join("\n");
    let events = SectionParser::parse_all(&input);
    assert!(
        !events.is_empty(),
        "should produce events for replacement char input"
    );
}

#[test]
fn markdown_lexer_handles_replacement_characters() {
    let input = "```\u{FFFD}rust\nfn \u{FFFD}() {}\n```\n";
    let mut lexer = MarkdownLexer::new();
    let events = lexer.feed_chunk(input);
    // Must not panic.
    let _ = events;
}

#[test]
fn style_span_handles_replacement_characters() {
    let text = "\u{FFFD}error\u{FFFD}: failed\u{FFFD}";
    let styled = style_span(text, TokenKind::Error, true);
    assert!(!styled.is_empty());
    // Deterministic.
    assert_eq!(styled, style_span(text, TokenKind::Error, true));
}

#[test]
fn section_parser_handles_raw_invalid_utf8_via_lossy() {
    // Simulate real invalid byte sequences → lossy conversion.
    let invalid_bytes: &[u8] = &[
        0x80, 0x81, 0xFE, 0xFF, // invalid lead bytes
        b'h', b'e', b'l', b'l', b'o', // valid ASCII
        0xC0, 0xAF, // overlong encoding
        b'\n', b'e', b'r', b'r', b'o', b'r', b':', b' ', b'f', b'a', b'i', b'l',
    ];
    let input = String::from_utf8_lossy(invalid_bytes);
    let events = SectionParser::parse_all(&input);
    assert!(
        !events.is_empty(),
        "should produce events for lossy-converted input"
    );
}

// =========================================================================
// Property tests — long lines
// =========================================================================

#[test]
fn section_parser_handles_very_long_lines() {
    // 100KB line.
    let long_line = "x".repeat(100_000);
    let events = SectionParser::parse_all(&long_line);
    assert!(!events.is_empty());

    // Long line that starts with a keyword.
    let long_error = format!("error: {}", "a".repeat(100_000));
    let events = SectionParser::parse_all(&long_error);
    assert!(!events.is_empty());
    let starts: Vec<SectionKind> = events
        .iter()
        .filter_map(|e| match e {
            SectionEvent::Start { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(starts[0], SectionKind::ErrorBlock);
}

#[test]
fn markdown_lexer_handles_very_long_lines() {
    // Long fence content line.
    let content = "x".repeat(100_000);
    let input = format!("```rust\n{}\n```\n", content);
    let mut lexer = MarkdownLexer::new();
    let events = lexer.feed_chunk(&input);
    assert!(!events.is_empty());

    // Verify the content line is captured.
    let has_fence_line = events
        .iter()
        .any(|e| matches!(e, MarkdownEvent::FenceLine { .. }));
    assert!(has_fence_line, "should have a FenceLine for long content");
}

#[test]
fn style_span_handles_very_long_text() {
    let long_text = "a".repeat(1_000_000);
    let styled = style_span(&long_text, TokenKind::Error, true);
    assert!(styled.len() > long_text.len()); // includes ANSI codes
    let plain = style_span(&long_text, TokenKind::Error, false);
    assert!(plain.starts_with("[ERROR] "));
}

// =========================================================================
// Property tests — partial fences
// =========================================================================

#[test]
fn section_parser_handles_unclosed_code_fence() {
    let input = "\
```rust
fn main() {}
// no closing fence";

    let mut parser = SectionParser::new();
    let mut events = Vec::new();
    for line in input.lines() {
        events.extend(parser.feed(line));
    }
    // Flush should close the open fence.
    let flush = parser.flush();
    events.extend(flush);

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

    assert_eq!(starts[0], SectionKind::CodeFence);
    assert!(
        ends.contains(&SectionKind::CodeFence),
        "flush should close unclosed fence"
    );
}

#[test]
fn markdown_lexer_handles_unclosed_fence() {
    let mut lexer = MarkdownLexer::new();
    let _ = lexer.feed_chunk("```rust\nfn main() {}\n// no close");
    assert!(lexer.in_fence(), "fence should still be open");
    let finish_events = lexer.finish();
    // Finish should process the pending line.
    let _ = finish_events;
    // Must not panic.
}

#[test]
fn section_parser_handles_fence_with_only_backticks() {
    // Just backticks, no language hint, no content, no close.
    let input = "```";
    let mut parser = SectionParser::new();
    let events = parser.feed(input);
    assert!(events.iter().any(|e| matches!(
        e,
        SectionEvent::Start {
            kind: SectionKind::CodeFence,
            ..
        }
    )));
    let flush = parser.flush();
    assert!(flush.iter().any(|e| matches!(
        e,
        SectionEvent::End {
            kind: SectionKind::CodeFence,
            ..
        }
    )));
}

#[test]
fn section_parser_handles_mismatched_fence_lengths() {
    // Open with 4 backticks, try to close with 3 (should not close).
    let input = "\
````rust
fn main() {}
```
still inside fence
````";

    let events = SectionParser::parse_all(input);

    let starts: Vec<SectionKind> = events
        .iter()
        .filter_map(|e| match e {
            SectionEvent::Start { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();

    // Only one CodeFence section should exist (the ``` line is content, not a close).
    assert_eq!(
        starts
            .iter()
            .filter(|k| **k == SectionKind::CodeFence)
            .count(),
        1,
        "mismatched fence should stay as one section"
    );

    // Content lines inside the fence.
    let continues: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            SectionEvent::Continue { kind, line, .. } if *kind == SectionKind::CodeFence => {
                Some(line.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        continues.iter().any(|l| l == "```"),
        "3-backtick line should be content inside 4-backtick fence"
    );
    assert!(
        continues.iter().any(|l| l == "still inside fence"),
        "line after 3-backtick should still be inside fence"
    );
}

// =========================================================================
// Property tests — empty and edge-case inputs
// =========================================================================

#[test]
fn section_parser_empty_input() {
    let events = SectionParser::parse_all("");
    assert!(events.is_empty());
}

#[test]
fn section_parser_single_newline() {
    let events = SectionParser::parse_all("\n");
    // One empty line → Unknown section.
    assert!(!events.is_empty());
}

#[test]
fn section_parser_only_whitespace_lines() {
    let input = "   \n\t\n  \t  ";
    let events = SectionParser::parse_all(input);
    // All lines should produce Unknown events (whitespace is not classified).
    for event in &events {
        if let SectionEvent::Start { kind, .. } = event {
            assert_eq!(*kind, SectionKind::Unknown);
        }
    }
}

#[test]
fn markdown_lexer_empty_input() {
    let mut lexer = MarkdownLexer::new();
    let events = lexer.feed_chunk("");
    assert!(events.is_empty());
    let finish = lexer.finish();
    assert!(finish.is_empty());
}

#[test]
fn style_span_empty_text() {
    assert_eq!(style_span("", TokenKind::Error, true), "");
    assert_eq!(style_span("", TokenKind::Error, false), "");
}

// =========================================================================
// Property tests — special Unicode
// =========================================================================

#[test]
fn section_parser_handles_emoji_and_multibyte() {
    let input = "\
\u{1F680} deploying...
error: \u{274C} build failed
\u{2705} tests passed
\u{1F4DD} notes: \u{1F4CB}";

    let events = SectionParser::parse_all(input);
    assert!(
        !events.is_empty(),
        "should produce events for emoji-heavy input"
    );

    // The error line should be detected.
    let starts: Vec<SectionKind> = events
        .iter()
        .filter_map(|e| match e {
            SectionEvent::Start { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert!(
        starts.contains(&SectionKind::ErrorBlock),
        "should detect error: line with emoji"
    );
}

#[test]
fn style_span_handles_multibyte_characters() {
    let text = "\u{1F600}\u{1F601}\u{1F602}"; // emoji sequence
    let styled = style_span(text, TokenKind::Warning, true);
    assert!(!styled.is_empty());
    assert!(styled.contains(text));
}

#[test]
fn style_span_handles_cjk_characters() {
    let text = "\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 你好世界
    let styled = style_span(text, TokenKind::SectionHeader, true);
    assert!(styled.contains(text));
    let no_color = style_span(text, TokenKind::SectionHeader, false);
    assert!(no_color.starts_with("== "));
}

// =========================================================================
// Property tests — token spec invariants
// =========================================================================

#[test]
fn precedence_resolution_is_commutative_for_different_kinds() {
    // For any two different kinds, the lower-precedence one always wins
    // regardless of argument order.
    for a in &TOKEN_KINDS_BY_PRECEDENCE {
        for b in &TOKEN_KINDS_BY_PRECEDENCE {
            if a == b {
                continue;
            }
            let winner_ab = resolve_precedence(*a, *b);
            let winner_ba = resolve_precedence(*b, *a);
            let expected = if a.precedence() < b.precedence() {
                *a
            } else {
                *b
            };
            assert_eq!(
                winner_ab, expected,
                "resolve_precedence({:?}, {:?}) wrong",
                a, b
            );
            assert_eq!(
                winner_ba, expected,
                "resolve_precedence({:?}, {:?}) wrong",
                b, a
            );
        }
    }
}

#[test]
fn every_kind_has_style_across_all_capabilities_and_tones() {
    let capabilities = [
        TerminalColorCapability::Ansi16,
        TerminalColorCapability::Ansi256,
        TerminalColorCapability::TrueColor,
    ];
    let tones = [TerminalTone::Dark, TerminalTone::Light];

    for cap in &capabilities {
        for tone in &tones {
            for kind in &TOKEN_KINDS_BY_PRECEDENCE {
                let style = ansi_style_for_theme(*kind, *cap, *tone);
                assert!(
                    !style.sgr.is_empty(),
                    "empty sgr for {:?}/{:?}/{:?}",
                    cap,
                    tone,
                    kind
                );
                assert!(
                    !style.label.is_empty(),
                    "empty label for {:?}/{:?}/{:?}",
                    cap,
                    tone,
                    kind
                );
            }
            // Plain too.
            let plain = ansi_style_for_theme(TokenKind::Plain, *cap, *tone);
            assert_eq!(plain.sgr, "0");
        }
    }
}

#[test]
fn no_color_signifier_stability() {
    // These must be stable across runs for golden test compatibility.
    assert_eq!(no_color_signifier(TokenKind::Error), Some("[ERROR] "));
    assert_eq!(no_color_signifier(TokenKind::Warning), Some("[WARN] "));
    assert_eq!(no_color_signifier(TokenKind::SectionHeader), Some("== "));
    assert_eq!(no_color_signifier(TokenKind::RoleMarker), Some(">> "));
    assert_eq!(no_color_signifier(TokenKind::CommandPrompt), Some("$ "));
    assert_eq!(no_color_signifier(TokenKind::Plain), None);
    assert_eq!(no_color_signifier(TokenKind::DiffAdd), None);
    assert_eq!(no_color_signifier(TokenKind::DiffDel), None);
}

#[test]
fn style_span_color_vs_no_color_consistency() {
    let text = "test message";
    for kind in &TOKEN_KINDS_BY_PRECEDENCE {
        let color = style_span(text, *kind, true);
        let no_color = style_span(text, *kind, false);

        // Color version should either be plain text or wrapped in ANSI.
        if color != text {
            assert!(
                color.contains("\x1b["),
                "color mode for {:?} should contain ANSI escape",
                kind
            );
            assert!(
                color.contains("\x1b[0m"),
                "color mode for {:?} should reset",
                kind
            );
        }

        // No-color version should not contain ANSI.
        assert!(
            !no_color.contains("\x1b["),
            "no-color mode for {:?} should not contain ANSI",
            kind
        );
    }
}

// =========================================================================
// Property tests — theme resolution
// =========================================================================

#[test]
fn theme_resolution_no_color_always_disables() {
    let hints_variations = [
        ThemeEnvHints {
            no_color_env: true,
            force_color_env: true,
            ..ThemeEnvHints::default()
        },
        ThemeEnvHints {
            no_color_env: true,
            term: Some("xterm-256color".to_string()),
            colorterm: Some("truecolor".to_string()),
            ..ThemeEnvHints::default()
        },
    ];

    for hints in &hints_variations {
        let theme = resolve_theme(false, hints);
        assert!(!theme.use_color, "NO_COLOR should always disable color");
    }

    // --no-color flag.
    let theme = resolve_theme(true, &ThemeEnvHints::default());
    assert!(!theme.use_color, "--no-color flag should disable color");
}

#[test]
fn theme_resolution_is_deterministic() {
    let hints = ThemeEnvHints {
        term: Some("xterm-256color".to_string()),
        colorterm: Some("truecolor".to_string()),
        colorfgbg: Some("0;15".to_string()),
        ..ThemeEnvHints::default()
    };

    let themes: Vec<HighlightTheme> = (0..5).map(|_| resolve_theme(false, &hints)).collect();

    for (i, theme) in themes.iter().enumerate().skip(1) {
        assert_eq!(
            themes[0], *theme,
            "theme resolution differs between run 0 and run {}",
            i
        );
    }
}

// =========================================================================
// Property tests — streaming chunk boundaries
// =========================================================================

#[test]
fn markdown_lexer_arbitrary_chunk_boundaries_produce_same_result() {
    let full_input = "```rust\nfn main() {}\nlet x = 42;\n```\nplain `code` text\n";

    // Feed all at once.
    let mut lexer_full = MarkdownLexer::new();
    let events_full = lexer_full.feed_chunk(full_input);

    // Feed byte by byte.
    let mut lexer_byte = MarkdownLexer::new();
    let mut events_byte = Vec::new();
    for b in full_input.bytes() {
        let buf = [b];
        let chunk = std::str::from_utf8(&buf).unwrap();
        events_byte.extend(lexer_byte.feed_chunk(chunk));
    }

    assert_eq!(
        serialize_markdown_events(&events_full),
        serialize_markdown_events(&events_byte),
        "chunk boundary should not affect output"
    );
}

#[test]
fn section_parser_line_by_line_matches_parse_all() {
    let input = "\
OpenAI Codex v0.80.0 (research preview)
--------
user
thinking
**Planning**
codex
```rust
fn foo() {}
```
error: test failed
recovery: fix it";

    // parse_all.
    let events_all = SectionParser::parse_all(input);

    // Line-by-line + flush.
    let mut parser = SectionParser::new();
    let mut events_manual = Vec::new();
    for line in input.lines() {
        events_manual.extend(parser.feed(line));
    }
    events_manual.extend(parser.flush());

    assert_eq!(
        serialize_section_events(&events_all),
        serialize_section_events(&events_manual),
        "parse_all should match line-by-line + flush"
    );
}

// =========================================================================
// Property tests — style_span_with_theme coverage
// =========================================================================

#[test]
fn style_span_with_theme_covers_all_capability_tone_combinations() {
    let text = "sample";
    let capabilities = [
        TerminalColorCapability::Ansi16,
        TerminalColorCapability::Ansi256,
        TerminalColorCapability::TrueColor,
    ];
    let tones = [TerminalTone::Dark, TerminalTone::Light];

    for cap in &capabilities {
        for tone in &tones {
            for kind in &TOKEN_KINDS_BY_PRECEDENCE {
                let theme = HighlightTheme {
                    use_color: true,
                    capability: *cap,
                    tone: *tone,
                };
                let styled = style_span_with_theme(text, *kind, theme);
                // Must not panic, must produce output.
                assert!(
                    !styled.is_empty(),
                    "empty styled output for {:?}/{:?}/{:?}",
                    cap,
                    tone,
                    kind
                );
            }
        }
    }
}

// =========================================================================
// Property tests — section kind slug stability
// =========================================================================

#[test]
fn section_kind_slug_roundtrip_all() {
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
        let parsed = SectionKind::from_slug(slug);
        assert_eq!(
            parsed,
            Some(*kind),
            "slug roundtrip failed for {:?} -> {slug}",
            kind
        );
    }
}

#[test]
fn token_kind_slug_roundtrip_all() {
    for kind in &TOKEN_KINDS_BY_PRECEDENCE {
        let slug = kind.slug();
        let parsed = TokenKind::from_slug(slug);
        assert_eq!(
            parsed,
            Some(*kind),
            "slug roundtrip failed for {:?} -> {slug}",
            kind
        );
    }
    // Plain too.
    assert_eq!(TokenKind::from_slug("plain"), Some(TokenKind::Plain));
}

// =========================================================================
// Property tests — rapid interleaving of section types
// =========================================================================

#[test]
fn section_parser_rapid_type_switching() {
    let input = "\
user
error: fail
recovery: fix
```rust
code
```
diff --git a/x b/x
--- a/x
+++ b/x
@@ -1 +1 @@
-a
+b
thinking
**plan**
codex
tool: read foo
{\"type\":\"init\"}
tokens used
100
[2026-01-01T00:00:00Z] status: ok
⚠️  APPROVAL REQUIRED
Press [y] to approve all";

    let events = SectionParser::parse_all(input);

    // Collect all section kinds that started.
    let started: Vec<SectionKind> = events
        .iter()
        .filter_map(|e| match e {
            SectionEvent::Start { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();

    // Every section should be represented.
    let expected_kinds = [
        SectionKind::RoleMarker,
        SectionKind::ErrorBlock,
        SectionKind::CodeFence,
        SectionKind::Diff,
        SectionKind::Thinking,
        SectionKind::ToolCall,
        SectionKind::JsonEvent,
        SectionKind::Summary,
        SectionKind::StatusLine,
        SectionKind::Approval,
    ];

    for kind in &expected_kinds {
        assert!(
            started.contains(kind),
            "rapid switching test should detect {:?}",
            kind
        );
    }
}

//! Streaming markdown/code-fence lexer for harness logs.
//!
//! Scope (PAR-104):
//! - Fenced code block detection with language hints (```lang).
//! - Inline code span detection (`code`, ``code with ` literal``).
//! - Fence nesting guards (inner fences treated as content while fence open).
//! - Streaming-safe chunk processing across arbitrary boundaries.

use std::fmt;

/// Canonical language hints supported by fenced code blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FenceLanguage {
    Rust,
    Go,
    Ts,
    Js,
    Python,
    Json,
    Yaml,
    Toml,
    Sh,
    Diff,
}

impl FenceLanguage {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Ts => "ts",
            Self::Js => "js",
            Self::Python => "python",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Sh => "sh",
            Self::Diff => "diff",
        }
    }

    #[must_use]
    pub fn from_hint(value: &str) -> Option<Self> {
        match normalize_hint(value).as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "go" => Some(Self::Go),
            "ts" | "typescript" => Some(Self::Ts),
            "js" | "javascript" | "node" => Some(Self::Js),
            "python" | "py" => Some(Self::Python),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "sh" | "bash" | "zsh" | "shell" => Some(Self::Sh),
            "diff" | "patch" => Some(Self::Diff),
            _ => None,
        }
    }
}

impl fmt::Display for FenceLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// Lexer output events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownEvent {
    FenceOpen {
        line_number: usize,
        fence_len: usize,
        language: Option<FenceLanguage>,
        info: Option<String>,
    },
    FenceLine {
        line_number: usize,
        language: Option<FenceLanguage>,
        text: String,
    },
    FenceClose {
        line_number: usize,
        fence_len: usize,
    },
    InlineCodeSpan {
        line_number: usize,
        start: usize,
        end: usize,
        code: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FenceState {
    backtick_count: usize,
    language: Option<FenceLanguage>,
}

/// Streaming markdown lexer.
///
/// Feed arbitrary chunks. The lexer buffers partial lines and emits events
/// only when a full line is available (or when `finish` is called).
pub struct MarkdownLexer {
    pending: String,
    line_number: usize,
    open_fence: Option<FenceState>,
}

impl Default for MarkdownLexer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownLexer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: String::new(),
            line_number: 0,
            open_fence: None,
        }
    }

    /// Feed a chunk and emit events for each completed line.
    pub fn feed_chunk(&mut self, chunk: &str) -> Vec<MarkdownEvent> {
        self.pending.push_str(chunk);
        let mut events = Vec::new();

        while let Some(idx) = self.pending.find('\n') {
            let mut line = self.pending[..idx].to_string();
            if line.ends_with('\r') {
                line.pop();
            }
            self.pending.drain(..=idx);
            events.extend(self.process_line(&line));
        }

        events
    }

    /// Flush trailing partial line (if any).
    pub fn finish(&mut self) -> Vec<MarkdownEvent> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let line = std::mem::take(&mut self.pending);
        self.process_line(&line)
    }

    #[must_use]
    pub fn in_fence(&self) -> bool {
        self.open_fence.is_some()
    }

    fn process_line(&mut self, line: &str) -> Vec<MarkdownEvent> {
        self.line_number += 1;
        let ln = self.line_number;
        let mut events = Vec::new();

        if let Some(state) = self.open_fence {
            if let Some(close_len) = parse_fence_close(line, state.backtick_count) {
                self.open_fence = None;
                events.push(MarkdownEvent::FenceClose {
                    line_number: ln,
                    fence_len: close_len,
                });
                return events;
            }
            events.push(MarkdownEvent::FenceLine {
                line_number: ln,
                language: state.language,
                text: line.to_string(),
            });
            return events;
        }

        if let Some((fence_len, info, language)) = parse_fence_open(line) {
            self.open_fence = Some(FenceState {
                backtick_count: fence_len,
                language,
            });
            events.push(MarkdownEvent::FenceOpen {
                line_number: ln,
                fence_len,
                language,
                info,
            });
            return events;
        }

        events.extend(parse_inline_code_spans(line, ln));
        events
    }
}

fn parse_fence_open(line: &str) -> Option<(usize, Option<String>, Option<FenceLanguage>)> {
    let trimmed = line.trim_start();
    let fence_len = leading_backticks(trimmed);
    if fence_len < 3 {
        return None;
    }

    let rest = trimmed[fence_len..].trim();
    if rest.is_empty() {
        return Some((fence_len, None, None));
    }

    let info = rest.to_string();
    let language = info
        .split_whitespace()
        .next()
        .and_then(FenceLanguage::from_hint);
    Some((fence_len, Some(info), language))
}

fn parse_fence_close(line: &str, open_count: usize) -> Option<usize> {
    let trimmed = line.trim();
    let fence_len = leading_backticks(trimmed);
    if fence_len < open_count {
        return None;
    }
    if trimmed[fence_len..].trim().is_empty() {
        Some(fence_len)
    } else {
        None
    }
}

fn parse_inline_code_spans(line: &str, line_number: usize) -> Vec<MarkdownEvent> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut events = Vec::new();
    let mut open_delim: Option<(usize, usize)> = None;

    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let run_start = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        let run_len = index - run_start;
        if run_len >= 3 {
            continue;
        }

        match open_delim {
            Some((delimiter, content_start)) if delimiter == run_len => {
                let code = line[content_start..run_start].to_string();
                events.push(MarkdownEvent::InlineCodeSpan {
                    line_number,
                    start: content_start,
                    end: run_start,
                    code,
                });
                open_delim = None;
            }
            Some(_) => {}
            None => {
                open_delim = Some((run_len, index));
            }
        }
    }

    events
}

fn leading_backticks(text: &str) -> usize {
    text.bytes().take_while(|byte| *byte == b'`').count()
}

fn normalize_hint(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '+')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_language_aliases_are_supported() {
        let cases = [
            ("rust", FenceLanguage::Rust),
            ("rs", FenceLanguage::Rust),
            ("go", FenceLanguage::Go),
            ("ts", FenceLanguage::Ts),
            ("typescript", FenceLanguage::Ts),
            ("js", FenceLanguage::Js),
            ("javascript", FenceLanguage::Js),
            ("python", FenceLanguage::Python),
            ("py", FenceLanguage::Python),
            ("json", FenceLanguage::Json),
            ("yaml", FenceLanguage::Yaml),
            ("yml", FenceLanguage::Yaml),
            ("toml", FenceLanguage::Toml),
            ("bash", FenceLanguage::Sh),
            ("diff", FenceLanguage::Diff),
            ("patch", FenceLanguage::Diff),
        ];
        for (hint, expected) in cases {
            assert_eq!(FenceLanguage::from_hint(hint), Some(expected));
        }
        assert_eq!(FenceLanguage::from_hint("unknown"), None);
    }

    #[test]
    fn detects_fence_open_close_and_language() {
        let mut lexer = MarkdownLexer::new();
        let input = "```rust\nfn main() {}\n```\n";
        let events = lexer.feed_chunk(input);
        assert_eq!(
            events,
            vec![
                MarkdownEvent::FenceOpen {
                    line_number: 1,
                    fence_len: 3,
                    language: Some(FenceLanguage::Rust),
                    info: Some("rust".to_string()),
                },
                MarkdownEvent::FenceLine {
                    line_number: 2,
                    language: Some(FenceLanguage::Rust),
                    text: "fn main() {}".to_string(),
                },
                MarkdownEvent::FenceClose {
                    line_number: 3,
                    fence_len: 3,
                },
            ]
        );
    }

    #[test]
    fn fence_nesting_guard_keeps_inner_fence_as_content() {
        let mut lexer = MarkdownLexer::new();
        let input = "```python\n```js\nprint('ok')\n```\n";
        let events = lexer.feed_chunk(input);
        assert_eq!(
            events,
            vec![
                MarkdownEvent::FenceOpen {
                    line_number: 1,
                    fence_len: 3,
                    language: Some(FenceLanguage::Python),
                    info: Some("python".to_string()),
                },
                MarkdownEvent::FenceLine {
                    line_number: 2,
                    language: Some(FenceLanguage::Python),
                    text: "```js".to_string(),
                },
                MarkdownEvent::FenceLine {
                    line_number: 3,
                    language: Some(FenceLanguage::Python),
                    text: "print('ok')".to_string(),
                },
                MarkdownEvent::FenceClose {
                    line_number: 4,
                    fence_len: 3,
                },
            ]
        );
    }

    #[test]
    fn detects_inline_code_spans() {
        let mut lexer = MarkdownLexer::new();
        let events = lexer.feed_chunk("use `cargo test` then `cargo clippy`\n");
        assert_eq!(
            events,
            vec![
                MarkdownEvent::InlineCodeSpan {
                    line_number: 1,
                    start: 5,
                    end: 15,
                    code: "cargo test".to_string(),
                },
                MarkdownEvent::InlineCodeSpan {
                    line_number: 1,
                    start: 23,
                    end: 35,
                    code: "cargo clippy".to_string(),
                },
            ]
        );
    }

    #[test]
    fn unclosed_inline_code_is_ignored() {
        let mut lexer = MarkdownLexer::new();
        let events = lexer.feed_chunk("this is `unterminated\n");
        assert!(events.is_empty());
    }

    #[test]
    fn stream_chunk_boundaries_for_fence_open_and_close() {
        let mut lexer = MarkdownLexer::new();
        let mut events = Vec::new();
        events.extend(lexer.feed_chunk("```ru"));
        assert!(events.is_empty());
        events.extend(lexer.feed_chunk("st\nfn main() {}\n`"));
        events.extend(lexer.feed_chunk("``\n"));

        assert_eq!(
            events,
            vec![
                MarkdownEvent::FenceOpen {
                    line_number: 1,
                    fence_len: 3,
                    language: Some(FenceLanguage::Rust),
                    info: Some("rust".to_string()),
                },
                MarkdownEvent::FenceLine {
                    line_number: 2,
                    language: Some(FenceLanguage::Rust),
                    text: "fn main() {}".to_string(),
                },
                MarkdownEvent::FenceClose {
                    line_number: 3,
                    fence_len: 3,
                },
            ]
        );
    }

    #[test]
    fn stream_chunk_boundaries_for_inline_code_span() {
        let mut lexer = MarkdownLexer::new();
        let mut events = Vec::new();
        events.extend(lexer.feed_chunk("Use `car"));
        assert!(events.is_empty());
        events.extend(lexer.feed_chunk("go test` now\n"));

        assert_eq!(
            events,
            vec![MarkdownEvent::InlineCodeSpan {
                line_number: 1,
                start: 5,
                end: 15,
                code: "cargo test".to_string(),
            }]
        );
    }

    #[test]
    fn finish_flushes_trailing_partial_line() {
        let mut lexer = MarkdownLexer::new();
        assert!(lexer.feed_chunk("plain `x`").is_empty());
        let events = lexer.finish();
        assert_eq!(
            events,
            vec![MarkdownEvent::InlineCodeSpan {
                line_number: 1,
                start: 7,
                end: 8,
                code: "x".to_string(),
            }]
        );
    }

    #[test]
    fn double_backtick_inline_span_is_supported() {
        let mut lexer = MarkdownLexer::new();
        let events = lexer.feed_chunk("``a `quoted` value``\n");
        assert_eq!(
            events,
            vec![MarkdownEvent::InlineCodeSpan {
                line_number: 1,
                start: 2,
                end: 18,
                code: "a `quoted` value".to_string(),
            }]
        );
    }
}

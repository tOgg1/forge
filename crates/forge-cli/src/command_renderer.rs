//! Command transcript renderer.
//!
//! Detects shell prompt + command lines, annotates stdout/stderr streams,
//! surfaces exit code markers, and highlights common command patterns
//! (cargo/go/git/sv/forge/fmail). Preserves plain mode when ambiguous.
//!
//! Design (from PAR-107):
//! - Classify each line within a command transcript block into semantic parts.
//! - Apply [`TokenKind`] styling per sub-line classification.
//! - Highlight known command names within prompt lines.
//! - No regex — all matching is prefix / `contains` / byte scans.

use crate::highlight_spec::{style_span, TokenKind};

// ---------------------------------------------------------------------------
// Line classification within command transcript blocks
// ---------------------------------------------------------------------------

/// Sub-classification for lines within a command transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandLineKind {
    /// Prompt + command: `$ cargo test`, `❯ git status`, `➜ go build`.
    PromptCommand,
    /// Exit code indicator: `exit code: N`, `exit status: N`, `Process exited with code N`.
    ExitCode,
    /// Stderr indicator: lines starting with known stderr prefixes.
    Stderr,
    /// Plain stdout output.
    Stdout,
}

/// Classify a single line that is known to be inside a command transcript block.
fn classify_command_line(line: &str) -> CommandLineKind {
    let trimmed = line.trim_start();

    // Prompt + command lines.
    if is_prompt_line(trimmed) {
        return CommandLineKind::PromptCommand;
    }

    // Exit code indicators.
    if is_exit_code_line(trimmed) {
        return CommandLineKind::ExitCode;
    }

    // Stderr indicators.
    if is_stderr_line(trimmed) {
        return CommandLineKind::Stderr;
    }

    CommandLineKind::Stdout
}

/// Check if a line starts with a shell prompt character.
fn is_prompt_line(trimmed: &str) -> bool {
    // $ command
    if trimmed.starts_with("$ ") {
        return true;
    }
    // ❯ command (starship/fish)
    if trimmed.starts_with('\u{276F}') {
        let after = &trimmed['\u{276F}'.len_utf8()..];
        if after.starts_with(' ') || after.is_empty() {
            return true;
        }
    }
    // ➜ command (oh-my-zsh)
    if trimmed.starts_with('\u{279C}') {
        let after = &trimmed['\u{279C}'.len_utf8()..];
        if after.starts_with(' ') || after.is_empty() {
            return true;
        }
    }
    // % command (csh/tcsh/zsh default)
    if trimmed.starts_with("% ") {
        return true;
    }
    // > command (Windows/PowerShell style in transcripts)
    if trimmed.starts_with("> ") && !trimmed.starts_with(">>") {
        return true;
    }
    false
}

/// Check if line indicates an exit code.
fn is_exit_code_line(trimmed: &str) -> bool {
    // Common exit code patterns from harness transcripts.
    starts_with_ci(trimmed, "exit code:")
        || starts_with_ci(trimmed, "exit status:")
        || starts_with_ci(trimmed, "process exited with code")
        || starts_with_ci(trimmed, "exited with code")
        || starts_with_ci(trimmed, "command exited with")
        || starts_with_ci(trimmed, "return code:")
}

/// Check if line looks like stderr output.
fn is_stderr_line(trimmed: &str) -> bool {
    // Explicit stderr markers from harness output.
    trimmed.starts_with("stderr:")
        || trimmed.starts_with("STDERR:")
        || trimmed.starts_with("2>")
        || trimmed.starts_with("[stderr]")
        || trimmed.starts_with("(stderr)")
}

/// Case-insensitive starts_with check.
fn starts_with_ci(text: &str, prefix: &str) -> bool {
    if text.len() < prefix.len() {
        return false;
    }
    text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

// ---------------------------------------------------------------------------
// Command name detection
// ---------------------------------------------------------------------------

/// Known command names to highlight within prompt lines.
const KNOWN_COMMANDS: &[&str] = &[
    "cargo", "go", "git", "sv", "forge", "fmail", "npm", "npx", "yarn", "pnpm", "make", "cmake",
    "python", "python3", "pip", "pip3", "rustc", "rustup", "docker", "kubectl", "helm", "curl",
    "wget", "ssh", "scp", "rsync", "grep", "rg", "find", "sed", "awk", "jq", "node", "deno", "bun",
    "ruby", "perl", "javac", "java", "mvn", "gradle",
];

/// Find the byte range of the command name after the prompt character, if it
/// matches a known command. Returns (start, end) byte offsets within the line.
fn find_command_span(line: &str) -> Option<(usize, usize)> {
    // Find the prompt character and skip past it + space.
    let trimmed = line.trim_start();
    let trim_offset = line.len() - trimmed.len();

    let after_prompt = if trimmed.starts_with("$ ")
        || trimmed.starts_with("% ")
        || (trimmed.starts_with("> ") && !trimmed.starts_with(">>"))
    {
        trim_offset + 2
    } else if trimmed.starts_with('\u{276F}') {
        let skip = trim_offset + '\u{276F}'.len_utf8();
        if line.as_bytes().get(skip) == Some(&b' ') {
            skip + 1
        } else {
            return None;
        }
    } else if trimmed.starts_with('\u{279C}') {
        let skip = trim_offset + '\u{279C}'.len_utf8();
        if line.as_bytes().get(skip) == Some(&b' ') {
            skip + 1
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Extract the first word after the prompt.
    let rest = &line[after_prompt..];
    let word_end = rest.find([' ', '\t']).unwrap_or(rest.len());
    if word_end == 0 {
        return None;
    }

    let command = &rest[..word_end];
    if KNOWN_COMMANDS.contains(&command) {
        Some((after_prompt, after_prompt + word_end))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Exit code extraction
// ---------------------------------------------------------------------------

/// Extract the numeric exit code from an exit code line. Returns the value
/// and whether it indicates failure (non-zero).
fn parse_exit_code(line: &str) -> Option<(i32, bool)> {
    let trimmed = line.trim();
    // Find the last sequence of digits in the line.
    let mut last_num_end = 0;
    let mut last_num_start = 0;
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            last_num_start = start;
            last_num_end = i;
        } else {
            i += 1;
        }
    }
    if last_num_end > last_num_start {
        let num_str = &trimmed[last_num_start..last_num_end];
        if let Ok(code) = num_str.parse::<i32>() {
            return Some((code, code != 0));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render a set of command transcript lines with semantic highlighting.
///
/// Each line is classified and styled according to its role in the command
/// transcript. Prompt lines get the command prompt highlighted, known
/// commands get additional emphasis, and exit codes are color-coded by
/// success/failure.
pub(crate) fn render_command_lines(lines: &[String], use_color: bool) -> Vec<String> {
    let mut rendered = Vec::with_capacity(lines.len());

    for line in lines {
        let kind = classify_command_line(line);
        let styled = match kind {
            CommandLineKind::PromptCommand => render_prompt_line(line, use_color),
            CommandLineKind::ExitCode => render_exit_code_line(line, use_color),
            CommandLineKind::Stderr => style_span(line, TokenKind::Warning, use_color),
            CommandLineKind::Stdout => line.to_string(),
        };
        rendered.push(styled);
    }

    rendered
}

/// Render a prompt+command line with the prompt styled as CommandPrompt and
/// the command name highlighted if it matches a known command.
fn render_prompt_line(line: &str, use_color: bool) -> String {
    if let Some((cmd_start, cmd_end)) = find_command_span(line) {
        if use_color {
            let mut out = String::with_capacity(line.len() + 32);
            // Prompt portion (before command).
            out.push_str(&style_span(
                &line[..cmd_start],
                TokenKind::CommandPrompt,
                true,
            ));
            // Command name — bold yellow (same as CommandPrompt for emphasis).
            out.push_str(&style_span(
                &line[cmd_start..cmd_end],
                TokenKind::SectionHeader,
                true,
            ));
            // Rest of the line.
            if cmd_end < line.len() {
                out.push_str(&line[cmd_end..]);
            }
            out
        } else {
            style_span(line, TokenKind::CommandPrompt, false)
        }
    } else {
        style_span(line, TokenKind::CommandPrompt, use_color)
    }
}

/// Render an exit code line, using Error styling for non-zero and plain for zero.
fn render_exit_code_line(line: &str, use_color: bool) -> String {
    match parse_exit_code(line) {
        Some((_, true)) => style_span(line, TokenKind::Error, use_color),
        Some((_, false)) => {
            // Success exit code — keep it plain (not an error).
            line.to_string()
        }
        None => style_span(line, TokenKind::CommandPrompt, use_color),
    }
}

// ---------------------------------------------------------------------------
// Public detection helper for integration
// ---------------------------------------------------------------------------

/// Check if a line looks like a command prompt line. Used by the section
/// parser or logs pipeline to detect command transcript boundaries.
pub(crate) fn looks_like_command_prompt(line: &str) -> bool {
    is_prompt_line(line.trim_start())
}

/// Check if a line looks like an exit code line.
pub(crate) fn looks_like_exit_code(line: &str) -> bool {
    is_exit_code_line(line.trim_start())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── Line classification ─────────────────────────────────────────

    #[test]
    fn classifies_dollar_prompt() {
        assert_eq!(
            classify_command_line("$ cargo test"),
            CommandLineKind::PromptCommand
        );
        assert_eq!(
            classify_command_line("  $ git status"),
            CommandLineKind::PromptCommand
        );
    }

    #[test]
    fn classifies_arrow_prompts() {
        assert_eq!(
            classify_command_line("\u{276F} npm install"),
            CommandLineKind::PromptCommand
        );
        assert_eq!(
            classify_command_line("\u{279C} go build ./..."),
            CommandLineKind::PromptCommand
        );
    }

    #[test]
    fn classifies_percent_prompt() {
        assert_eq!(
            classify_command_line("% make build"),
            CommandLineKind::PromptCommand
        );
    }

    #[test]
    fn classifies_gt_prompt() {
        assert_eq!(
            classify_command_line("> forge logs alpha"),
            CommandLineKind::PromptCommand
        );
    }

    #[test]
    fn does_not_classify_double_gt_as_prompt() {
        // >> is not a prompt (could be heredoc or append redirect)
        assert_ne!(
            classify_command_line(">> not a prompt"),
            CommandLineKind::PromptCommand
        );
    }

    #[test]
    fn classifies_exit_code_lines() {
        assert_eq!(
            classify_command_line("exit code: 0"),
            CommandLineKind::ExitCode
        );
        assert_eq!(
            classify_command_line("exit code: 1"),
            CommandLineKind::ExitCode
        );
        assert_eq!(
            classify_command_line("Exit Status: 127"),
            CommandLineKind::ExitCode
        );
        assert_eq!(
            classify_command_line("Process exited with code 1"),
            CommandLineKind::ExitCode
        );
        assert_eq!(
            classify_command_line("exited with code 0"),
            CommandLineKind::ExitCode
        );
        assert_eq!(
            classify_command_line("return code: 2"),
            CommandLineKind::ExitCode
        );
    }

    #[test]
    fn classifies_stderr_lines() {
        assert_eq!(
            classify_command_line("stderr: something went wrong"),
            CommandLineKind::Stderr
        );
        assert_eq!(
            classify_command_line("STDERR: error output"),
            CommandLineKind::Stderr
        );
        assert_eq!(
            classify_command_line("[stderr] warning message"),
            CommandLineKind::Stderr
        );
        assert_eq!(
            classify_command_line("(stderr) error output"),
            CommandLineKind::Stderr
        );
    }

    #[test]
    fn classifies_stdout() {
        assert_eq!(
            classify_command_line("running 8 tests"),
            CommandLineKind::Stdout
        );
        assert_eq!(
            classify_command_line("test pool::tests::acquire ... ok"),
            CommandLineKind::Stdout
        );
    }

    // ── Command name detection ──────────────────────────────────────

    #[test]
    fn finds_known_command_after_dollar() {
        let span = find_command_span("$ cargo test --lib");
        assert_eq!(span, Some((2, 7)));
        assert_eq!(&"$ cargo test --lib"[2..7], "cargo");
    }

    #[test]
    fn finds_known_command_after_arrow() {
        let line = "\u{276F} git status";
        let span = find_command_span(line);
        match span {
            Some((start, end)) => assert_eq!(&line[start..end], "git"),
            None => panic!("expected Some span for arrow prompt"),
        }
    }

    #[test]
    fn returns_none_for_unknown_command() {
        let span = find_command_span("$ my_custom_script.sh");
        assert_eq!(span, None);
    }

    #[test]
    fn finds_go_command() {
        let span = find_command_span("$ go test ./...");
        assert_eq!(span, Some((2, 4)));
    }

    #[test]
    fn finds_sv_command() {
        let span = find_command_span("$ sv task list --status open");
        assert_eq!(span, Some((2, 4)));
    }

    #[test]
    fn finds_forge_command() {
        let span = find_command_span("$ forge logs alpha --follow");
        assert_eq!(span, Some((2, 7)));
    }

    #[test]
    fn finds_fmail_command() {
        let span = find_command_span("$ fmail send task 'claim: forge-7m9'");
        assert_eq!(span, Some((2, 7)));
    }

    // ── Exit code parsing ───────────────────────────────────────────

    #[test]
    fn parses_exit_code_zero() {
        assert_eq!(parse_exit_code("exit code: 0"), Some((0, false)));
    }

    #[test]
    fn parses_exit_code_nonzero() {
        assert_eq!(parse_exit_code("exit code: 1"), Some((1, true)));
        assert_eq!(parse_exit_code("exit code: 127"), Some((127, true)));
    }

    #[test]
    fn parses_exit_status_format() {
        assert_eq!(parse_exit_code("Exit Status: 2"), Some((2, true)));
    }

    #[test]
    fn parses_process_exited_format() {
        assert_eq!(
            parse_exit_code("Process exited with code 0"),
            Some((0, false))
        );
    }

    // ── Rendering ───────────────────────────────────────────────────

    #[test]
    fn renders_prompt_line_with_color() {
        let lines = vec!["$ cargo test --lib".to_string()];
        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 1);
        // Should contain ANSI escape sequences.
        assert!(
            rendered[0].contains("\x1b["),
            "prompt line should be styled: {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_prompt_line_no_color() {
        let lines = vec!["$ cargo test --lib".to_string()];
        let rendered = render_command_lines(&lines, false);
        assert_eq!(rendered.len(), 1);
        // No-color mode: CommandPrompt signifier is "$ ".
        assert!(
            rendered[0].starts_with("$ "),
            "no-color prompt should start with '$ ': {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_exit_code_zero_as_plain() {
        let lines = vec!["exit code: 0".to_string()];
        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 1);
        // Success: no error styling.
        assert!(
            !rendered[0].contains("\x1b[1;31m"),
            "exit code 0 should not be bold red: {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_exit_code_nonzero_as_error() {
        let lines = vec!["exit code: 1".to_string()];
        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 1);
        // Failure: bold red.
        assert!(
            rendered[0].contains("\x1b[1;31m"),
            "exit code 1 should be bold red: {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_exit_code_nonzero_no_color() {
        let lines = vec!["exit code: 1".to_string()];
        let rendered = render_command_lines(&lines, false);
        assert_eq!(rendered.len(), 1);
        assert!(
            rendered[0].starts_with("[ERROR] "),
            "exit code 1 no-color should get [ERROR]: {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_stderr_as_warning() {
        let lines = vec!["stderr: something went wrong".to_string()];
        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 1);
        assert!(
            rendered[0].contains("\x1b[33m") || rendered[0].contains("\x1b["),
            "stderr should have warning styling: {}",
            rendered[0]
        );
    }

    #[test]
    fn renders_stdout_as_plain() {
        let lines = vec!["running 8 tests".to_string()];
        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0], "running 8 tests");
    }

    #[test]
    fn renders_full_transcript() {
        let lines = vec![
            "$ cargo test -p forge-db --lib pool::tests".to_string(),
            "running 8 tests".to_string(),
            "test pool::tests::acquire_returns_connection ... ok".to_string(),
            "test result: ok. 8 passed; 0 failed;".to_string(),
            "exit code: 0".to_string(),
        ];

        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 5);
        // Prompt line styled.
        assert!(rendered[0].contains("\x1b["));
        // Stdout lines plain.
        assert_eq!(rendered[1], "running 8 tests");
        // Exit code 0 is plain.
        assert!(!rendered[4].contains("\x1b[1;31m"));
    }

    #[test]
    fn renders_failing_transcript() {
        let lines = vec![
            "$ cargo test -p forge-db --lib".to_string(),
            "running 1 test".to_string(),
            "stderr: thread 'pool::tests::timeout' panicked".to_string(),
            "exit code: 101".to_string(),
        ];

        let rendered = render_command_lines(&lines, true);
        assert_eq!(rendered.len(), 4);
        // Stderr line has warning styling.
        assert!(rendered[2].contains("\x1b["));
        // Exit code 101 has error styling.
        assert!(rendered[3].contains("\x1b[1;31m"));
    }

    #[test]
    fn empty_input_returns_empty() {
        let rendered = render_command_lines(&[], true);
        assert!(rendered.is_empty());
    }

    // ── Detection helpers ───────────────────────────────────────────

    #[test]
    fn looks_like_command_prompt_positive() {
        assert!(looks_like_command_prompt("$ cargo test"));
        assert!(looks_like_command_prompt("% make"));
        assert!(looks_like_command_prompt("> forge logs"));
        assert!(looks_like_command_prompt("\u{276F} git status"));
        assert!(looks_like_command_prompt("\u{279C} npm install"));
    }

    #[test]
    fn looks_like_command_prompt_negative() {
        assert!(!looks_like_command_prompt("running 8 tests"));
        assert!(!looks_like_command_prompt(">> heredoc content"));
        assert!(!looks_like_command_prompt("plain text"));
    }

    #[test]
    fn looks_like_exit_code_positive() {
        assert!(looks_like_exit_code("exit code: 0"));
        assert!(looks_like_exit_code("Exit Status: 1"));
        assert!(looks_like_exit_code("Process exited with code 127"));
    }

    #[test]
    fn looks_like_exit_code_negative() {
        assert!(!looks_like_exit_code("exit the program"));
        assert!(!looks_like_exit_code("running 8 tests"));
    }

    // ── starts_with_ci ──────────────────────────────────────────────

    #[test]
    fn starts_with_ci_works() {
        assert!(starts_with_ci("Exit Code: 1", "exit code:"));
        assert!(starts_with_ci("EXIT CODE: 0", "exit code:"));
        assert!(!starts_with_ci("ex", "exit code:"));
    }
}

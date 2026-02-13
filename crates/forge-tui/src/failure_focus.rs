//! Failure jump and root-cause focus helpers for TUI logs.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightRole {
    Failure,
    RootCause,
    RootFrame,
    CommandContext,
    CauseContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedLine {
    pub line_index: usize,
    pub role: HighlightRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseLink {
    pub line_index: usize,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureFocus {
    pub failure_line: usize,
    pub root_cause_line: usize,
    pub root_frame_line: Option<usize>,
    pub command_context_line: Option<usize>,
    pub highlights: Vec<HighlightedLine>,
    pub links: Vec<CauseLink>,
}

impl FailureFocus {
    #[must_use]
    pub fn chain_lines(&self) -> Vec<usize> {
        let mut lines: Vec<usize> = self.links.iter().map(|link| link.line_index).collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    #[must_use]
    pub fn next_chain_line(&self, current_line: usize) -> Option<usize> {
        self.chain_lines()
            .into_iter()
            .find(|line| *line > current_line)
    }

    #[must_use]
    pub fn previous_chain_line(&self, current_line: usize) -> Option<usize> {
        self.chain_lines()
            .into_iter()
            .filter(|line| *line < current_line)
            .max()
    }
}

#[must_use]
pub fn jump_to_first_failure(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| is_failure_line(line))
}

#[must_use]
pub fn jump_to_root_cause(lines: &[String], failure_line: Option<usize>) -> Option<usize> {
    build_failure_focus(lines, failure_line).map(|focus| focus.root_cause_line)
}

#[must_use]
pub fn jump_to_probable_root_frame(lines: &[String], failure_line: Option<usize>) -> Option<usize> {
    build_failure_focus(lines, failure_line)
        .map(|focus| focus.root_frame_line.unwrap_or(focus.root_cause_line))
}

#[must_use]
pub fn build_failure_focus(lines: &[String], failure_line: Option<usize>) -> Option<FailureFocus> {
    if lines.is_empty() {
        return None;
    }
    let failure_line = match failure_line {
        Some(idx) if idx < lines.len() => idx,
        Some(_) => return None,
        None => jump_to_first_failure(lines)?,
    };

    let root_cause_line = find_root_cause_line(lines, failure_line);
    let root_frame_line = find_root_frame_line(lines, root_cause_line, failure_line);
    let command_context_line = find_command_context(lines, root_cause_line, 120);
    let chain_lines = build_chain_lines(
        lines,
        root_cause_line,
        failure_line,
        root_frame_line,
        command_context_line,
    );
    let highlights = chain_lines
        .iter()
        .map(|line_index| HighlightedLine {
            line_index: *line_index,
            role: if *line_index == failure_line {
                HighlightRole::Failure
            } else if *line_index == root_cause_line {
                HighlightRole::RootCause
            } else if Some(*line_index) == root_frame_line {
                HighlightRole::RootFrame
            } else if Some(*line_index) == command_context_line {
                HighlightRole::CommandContext
            } else {
                HighlightRole::CauseContext
            },
        })
        .collect();
    let links = chain_lines
        .iter()
        .map(|line_index| CauseLink {
            line_index: *line_index,
            label: if *line_index == failure_line {
                "failure".to_owned()
            } else if *line_index == root_cause_line {
                "root-cause".to_owned()
            } else if Some(*line_index) == root_frame_line {
                "root-frame".to_owned()
            } else if Some(*line_index) == command_context_line {
                "command".to_owned()
            } else {
                "cause-context".to_owned()
            },
            text: lines[*line_index].clone(),
        })
        .collect();

    Some(FailureFocus {
        failure_line,
        root_cause_line,
        root_frame_line,
        command_context_line,
        highlights,
        links,
    })
}

fn build_chain_lines(
    lines: &[String],
    root_cause_line: usize,
    failure_line: usize,
    root_frame_line: Option<usize>,
    command_context_line: Option<usize>,
) -> Vec<usize> {
    let mut chain = Vec::new();
    if let Some(command_line) = command_context_line {
        chain.push(command_line);
    }
    if let Some(root_frame_line) = root_frame_line {
        chain.push(root_frame_line);
    }

    let context_start = root_cause_line.saturating_sub(2);
    let context_end = (failure_line.saturating_add(2)).min(lines.len().saturating_sub(1));
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(context_end + 1)
        .skip(context_start)
    {
        if idx == root_cause_line
            || idx == failure_line
            || is_cause_context_line(line)
            || is_failure_line(line)
        {
            chain.push(idx);
        }
    }

    chain.sort_unstable();
    chain.dedup();
    chain
}

fn find_root_frame_line(
    lines: &[String],
    root_cause_line: usize,
    failure_line: usize,
) -> Option<usize> {
    let search_start = root_cause_line.saturating_sub(3);
    let search_end = failure_line
        .saturating_add(80)
        .min(lines.len().saturating_sub(1));

    let mut stack_frames = Vec::new();
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(search_end + 1)
        .skip(search_start)
    {
        if is_stack_frame_line(line) {
            stack_frames.push(idx);
        }
    }

    if stack_frames.is_empty() {
        return None;
    }

    for idx in stack_frames.iter().rev() {
        if is_application_frame_line(&lines[*idx]) {
            return Some(*idx);
        }
    }

    stack_frames.last().copied()
}

fn find_root_cause_line(lines: &[String], failure_line: usize) -> usize {
    let forward_end = failure_line
        .saturating_add(12)
        .min(lines.len().saturating_sub(1));
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(forward_end + 1)
        .skip(failure_line)
    {
        if is_root_cause_marker(line) {
            return idx;
        }
    }

    let lookback = failure_line.min(200);
    let start = failure_line.saturating_sub(lookback);
    let mut candidate = failure_line;
    for idx in (start..=failure_line).rev() {
        let line = &lines[idx];
        if is_root_cause_marker(line) {
            return idx;
        }
        if idx < failure_line && is_failure_line(line) {
            candidate = idx;
        }
    }
    candidate
}

fn find_command_context(lines: &[String], from_line: usize, max_lookback: usize) -> Option<usize> {
    let lookback = from_line.min(max_lookback);
    let start = from_line.saturating_sub(lookback);
    (start..from_line)
        .rev()
        .find(|idx| is_command_context_line(&lines[*idx]))
}

fn is_failure_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    let success_shapes = [
        "0 failed",
        "0 failures",
        "no failures",
        "all tests passed",
        "completed successfully",
        "build succeeded",
    ];
    if success_shapes.iter().any(|token| lower.contains(token)) {
        return false;
    }
    let failure_shapes = [
        "[error]",
        " error:",
        "fatal:",
        "panic:",
        "panicked at",
        "exception",
        "traceback",
        "failed",
        "failure",
        "timed out",
    ];
    failure_shapes.iter().any(|token| lower.contains(token))
}

fn is_root_cause_marker(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    let tokens = [
        "caused by",
        "because",
        "failed to",
        "no such file or directory",
        "permission denied",
        "connection refused",
        "timed out waiting",
        "unable to",
        "could not",
        "panic:",
        "exception:",
    ];
    tokens.iter().any(|token| lower.contains(token))
}

fn is_command_context_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    trimmed.starts_with('$')
        || trimmed.starts_with("> ")
        || lower.starts_with("running:")
        || lower.starts_with("executing:")
        || lower.starts_with("tool:")
        || lower.starts_with("command:")
        || lower.starts_with("cmd:")
}

fn is_cause_context_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("at ")
        || lower.starts_with("--> ")
        || lower.starts_with("stack:")
        || lower.starts_with("note:")
        || lower.starts_with("help:")
        || lower.starts_with("hint:")
        || lower.starts_with("caused by:")
        || lower.starts_with("error:")
        || lower.starts_with("stderr:")
}

fn is_stack_frame_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("traceback (most recent call last):")
        || lower.starts_with("stack backtrace:")
        || lower.starts_with("stack trace:")
        || lower.starts_with("stacktrace:")
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
    let digits_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == digits_start || idx >= bytes.len() || bytes[idx] != b':' {
        return None;
    }
    Some(idx + 1)
}

fn is_application_frame_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let app_markers = [
        "forge::", "src/", "/src/", ".rs:", ".go:", ".py:", ".js:", ".ts:", ".java:", ".kt:",
        ".swift:",
    ];
    if !app_markers.iter().any(|marker| lower.contains(marker)) {
        return false;
    }

    let library_markers = [
        "std::",
        "core::",
        "tokio::",
        "futures::",
        "backtrace::",
        "__rust_begin_short_backtrace",
        "python/lib",
        "site-packages",
        "node_modules",
    ];
    !library_markers.iter().any(|marker| lower.contains(marker))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{
        build_failure_focus, jump_to_first_failure, jump_to_probable_root_frame,
        jump_to_root_cause, HighlightRole,
    };

    fn sample_log() -> Vec<String> {
        vec![
            "[INFO] starting run".to_owned(),
            "$ cargo test --workspace".to_owned(),
            "running 10 tests".to_owned(),
            "error: test failed, to rerun pass '--bin app'".to_owned(),
            "caused by: missing fixture file".to_owned(),
            "note: fixtures expected under ./fixtures".to_owned(),
            "fatal: run failed".to_owned(),
        ]
    }

    #[test]
    fn jump_to_first_failure_finds_first_error_shape() {
        let lines = sample_log();
        assert_eq!(jump_to_first_failure(&lines), Some(3));
    }

    #[test]
    fn jump_to_first_failure_ignores_success_shapes() {
        let lines = vec![
            "build succeeded".to_owned(),
            "0 failed".to_owned(),
            "all tests passed".to_owned(),
        ];
        assert_eq!(jump_to_first_failure(&lines), None);
    }

    #[test]
    fn jump_to_root_cause_prefers_explicit_marker() {
        let lines = sample_log();
        assert_eq!(jump_to_root_cause(&lines, None), Some(4));
    }

    #[test]
    fn build_focus_links_failure_root_and_command_context() {
        let lines = sample_log();
        let Some(focus) = build_failure_focus(&lines, None) else {
            panic!("expected focus");
        };
        assert_eq!(focus.failure_line, 3);
        assert_eq!(focus.root_cause_line, 4);
        assert_eq!(focus.root_frame_line, None);
        assert_eq!(focus.command_context_line, Some(1));
        assert!(focus
            .links
            .iter()
            .any(|link| link.label == "command" && link.line_index == 1));
        assert!(focus
            .links
            .iter()
            .any(|link| link.label == "root-cause" && link.line_index == 4));
    }

    #[test]
    fn highlights_assign_expected_roles() {
        let lines = sample_log();
        let Some(focus) = build_failure_focus(&lines, None) else {
            panic!("expected focus");
        };
        assert!(focus
            .highlights
            .iter()
            .any(|item| item.role == HighlightRole::Failure && item.line_index == 3));
        assert!(focus
            .highlights
            .iter()
            .any(|item| item.role == HighlightRole::RootCause && item.line_index == 4));
        assert!(!focus
            .highlights
            .iter()
            .any(|item| item.role == HighlightRole::RootFrame));
        assert!(focus
            .highlights
            .iter()
            .any(|item| item.role == HighlightRole::CommandContext && item.line_index == 1));
    }

    #[test]
    fn chain_navigation_moves_between_cause_lines() {
        let lines = sample_log();
        let Some(focus) = build_failure_focus(&lines, None) else {
            panic!("expected focus");
        };
        assert_eq!(focus.next_chain_line(1), Some(3));
        assert_eq!(focus.next_chain_line(3), Some(4));
        assert_eq!(focus.previous_chain_line(4), Some(3));
        assert_eq!(focus.previous_chain_line(1), None);
    }

    #[test]
    fn fallback_root_cause_is_first_failure_when_no_markers() {
        let lines = vec![
            "$ make build".to_owned(),
            "fatal: build failed".to_owned(),
            "error: command exited with code 2".to_owned(),
        ];
        let Some(focus) = build_failure_focus(&lines, None) else {
            panic!("expected focus");
        };
        assert_eq!(focus.failure_line, 1);
        assert_eq!(focus.root_cause_line, 1);
        assert_eq!(focus.root_frame_line, None);
    }

    #[test]
    fn jump_to_probable_root_frame_prefers_last_application_frame() {
        let lines = vec![
            "$ cargo test --workspace".to_owned(),
            "thread 'main' panicked at 'boom', src/main.rs:12:5".to_owned(),
            "stack backtrace:".to_owned(),
            "   0: std::panicking::begin_panic".to_owned(),
            "   1: forge::runtime::run at src/runtime.rs:44".to_owned(),
            "   2: forge::main at src/main.rs:12".to_owned(),
            "error: process failed".to_owned(),
        ];
        let Some(focus) = build_failure_focus(&lines, Some(6)) else {
            panic!("expected focus");
        };
        assert_eq!(focus.root_frame_line, Some(5));
        assert!(focus
            .links
            .iter()
            .any(|link| link.label == "root-frame" && link.line_index == 5));
        assert!(focus
            .highlights
            .iter()
            .any(|item| item.role == HighlightRole::RootFrame && item.line_index == 5));
        assert_eq!(jump_to_probable_root_frame(&lines, Some(6)), Some(5));
    }

    #[test]
    fn jump_to_probable_root_frame_falls_back_to_root_cause() {
        let lines = sample_log();
        assert_eq!(jump_to_probable_root_frame(&lines, None), Some(4));
    }

    #[test]
    fn respects_failure_override_when_provided() {
        let lines = sample_log();
        let Some(focus) = build_failure_focus(&lines, Some(6)) else {
            panic!("expected focus");
        };
        assert_eq!(focus.failure_line, 6);
        assert_eq!(focus.root_cause_line, 4);
    }

    #[test]
    fn out_of_range_failure_override_returns_none() {
        let lines = sample_log();
        assert!(build_failure_focus(&lines, Some(99)).is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(build_failure_focus(&[], None).is_none());
        assert_eq!(jump_to_root_cause(&[], None), None);
    }
}

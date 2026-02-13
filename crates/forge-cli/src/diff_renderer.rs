use std::ops::Range;

use crate::highlight_spec::{style_span, TokenKind};

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_ADD: &str = "\x1b[32m";
const COLOR_DEL: &str = "\x1b[31m";
const COLOR_ADD_INTRALINE: &str = "\x1b[1;92m";
const COLOR_DEL_INTRALINE: &str = "\x1b[1;91m";
const DEFAULT_PENDING_LIMIT: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct DiffRenderState {
    in_hunk: bool,
    pending_changes: Vec<String>,
    pending_limit: usize,
}

impl Default for DiffRenderState {
    fn default() -> Self {
        Self {
            in_hunk: false,
            pending_changes: Vec::new(),
            pending_limit: DEFAULT_PENDING_LIMIT,
        }
    }
}

impl DiffRenderState {
    #[cfg(test)]
    fn with_pending_limit(pending_limit: usize) -> Self {
        Self {
            pending_limit: pending_limit.max(1),
            ..Self::default()
        }
    }

    pub(crate) fn reset(&mut self) {
        self.in_hunk = false;
        self.pending_changes.clear();
    }
}

pub(crate) fn render_diff_lines(lines: &[String], use_color: bool) -> Vec<String> {
    let mut state = DiffRenderState::default();
    render_diff_lines_with_state(lines, use_color, &mut state, false)
}

pub(crate) fn render_diff_lines_incremental(
    lines: &[String],
    use_color: bool,
    state: &mut DiffRenderState,
) -> Vec<String> {
    render_diff_lines_with_state(lines, use_color, state, true)
}

pub(crate) fn flush_diff_lines(use_color: bool, state: &mut DiffRenderState) -> Vec<String> {
    let mut rendered = Vec::new();
    flush_pending_changes(state, use_color, &mut rendered);
    rendered
}

fn render_diff_lines_with_state(
    lines: &[String],
    use_color: bool,
    state: &mut DiffRenderState,
    incremental: bool,
) -> Vec<String> {
    let mut rendered = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        let line = &lines[index];

        if is_diff_header_or_metadata(line) || is_file_header(line) {
            flush_pending_changes(state, use_color, &mut rendered);
            state.in_hunk = false;
            rendered.push(style_span(line, TokenKind::DiffHeader, use_color));
            index += 1;
            continue;
        }

        if is_hunk_header(line) {
            flush_pending_changes(state, use_color, &mut rendered);
            state.in_hunk = true;
            rendered.push(style_span(line, TokenKind::DiffHunk, use_color));
            index += 1;
            continue;
        }

        if state.in_hunk && is_change_line(line) {
            let start = index;
            while index < lines.len() && is_change_line(&lines[index]) {
                index += 1;
            }
            let run = &lines[start..index];
            let at_chunk_end = index == lines.len();

            if incremental && at_chunk_end && state.pending_changes.is_empty() {
                queue_pending_changes(state, run, use_color, &mut rendered);
            } else {
                let mut combined = std::mem::take(&mut state.pending_changes);
                combined.extend(run.iter().cloned());
                rendered.extend(render_change_run(&combined, use_color));
            }
            continue;
        }

        flush_pending_changes(state, use_color, &mut rendered);

        if state.in_hunk && line.starts_with("\\ No newline at end of file") {
            rendered.push(style_span(line, TokenKind::DiffHeader, use_color));
            index += 1;
            continue;
        }

        rendered.push(line.clone());
        index += 1;
    }

    if !incremental {
        flush_pending_changes(state, use_color, &mut rendered);
    }

    rendered
}

fn queue_pending_changes(
    state: &mut DiffRenderState,
    run: &[String],
    use_color: bool,
    rendered: &mut Vec<String>,
) {
    state.pending_changes.extend(run.iter().cloned());
    if state.pending_changes.len() <= state.pending_limit {
        return;
    }

    let overflow = state
        .pending_changes
        .len()
        .saturating_sub(state.pending_limit);
    let overflow_lines: Vec<String> = state.pending_changes.drain(..overflow).collect();
    rendered.extend(render_change_run(&overflow_lines, use_color));
}

fn flush_pending_changes(state: &mut DiffRenderState, use_color: bool, rendered: &mut Vec<String>) {
    if state.pending_changes.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut state.pending_changes);
    rendered.extend(render_change_run(&pending, use_color));
}

fn render_change_run(lines: &[String], use_color: bool) -> Vec<String> {
    let mut del_positions = Vec::new();
    let mut add_positions = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if is_del_line(line) {
            del_positions.push(index);
        } else if is_add_line(line) {
            add_positions.push(index);
        }
    }

    let mut del_fragments: Vec<Option<Range<usize>>> = vec![None; lines.len()];
    let mut add_fragments: Vec<Option<Range<usize>>> = vec![None; lines.len()];

    for (del_pos, add_pos) in del_positions.iter().zip(add_positions.iter()) {
        let del_body = lines[*del_pos].strip_prefix('-').unwrap_or_default();
        let add_body = lines[*add_pos].strip_prefix('+').unwrap_or_default();
        if let Some((del_range, add_range)) = intraline_ranges(del_body, add_body) {
            del_fragments[*del_pos] = Some(del_range);
            add_fragments[*add_pos] = Some(add_range);
        }
    }

    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if is_del_line(line) {
            out.push(render_change_line(
                line,
                false,
                use_color,
                del_fragments[index].as_ref(),
            ));
            continue;
        }
        if is_add_line(line) {
            out.push(render_change_line(
                line,
                true,
                use_color,
                add_fragments[index].as_ref(),
            ));
            continue;
        }
        out.push(line.clone());
    }

    out
}

fn render_change_line(
    line: &str,
    is_add: bool,
    use_color: bool,
    fragment: Option<&Range<usize>>,
) -> String {
    let (prefix, kind) = if is_add {
        ('+', TokenKind::DiffAdd)
    } else {
        ('-', TokenKind::DiffDel)
    };

    let Some(content) = line.strip_prefix(prefix) else {
        return style_span(line, kind, use_color);
    };

    let Some(fragment) = fragment else {
        return style_span(line, kind, use_color);
    };

    if fragment.start >= fragment.end || fragment.end > content.len() {
        return style_span(line, kind, use_color);
    }

    let prefix_text = &content[..fragment.start];
    let changed_text = &content[fragment.start..fragment.end];
    let suffix_text = &content[fragment.end..];

    if use_color {
        let (base, accent) = if is_add {
            (COLOR_ADD, COLOR_ADD_INTRALINE)
        } else {
            (COLOR_DEL, COLOR_DEL_INTRALINE)
        };
        return format!(
            "{base}{prefix}{prefix_text}{accent}{changed_text}{base}{suffix_text}{COLOR_RESET}"
        );
    }

    let (open, close) = if is_add { ("{+", "+}") } else { ("[-", "-]") };
    format!("{prefix}{prefix_text}{open}{changed_text}{close}{suffix_text}")
}

fn intraline_ranges(left: &str, right: &str) -> Option<(Range<usize>, Range<usize>)> {
    if left == right {
        return None;
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let min_len = left_chars.len().min(right_chars.len());

    let mut prefix = 0usize;
    while prefix < min_len && left_chars[prefix] == right_chars[prefix] {
        prefix += 1;
    }

    let mut left_suffix = left_chars.len();
    let mut right_suffix = right_chars.len();
    while left_suffix > prefix
        && right_suffix > prefix
        && left_chars[left_suffix - 1] == right_chars[right_suffix - 1]
    {
        left_suffix -= 1;
        right_suffix -= 1;
    }

    let left_range = char_range_to_bytes(left, prefix, left_suffix);
    let right_range = char_range_to_bytes(right, prefix, right_suffix);

    if left_range.start == left_range.end && right_range.start == right_range.end {
        return None;
    }

    Some((left_range, right_range))
}

fn char_range_to_bytes(text: &str, start: usize, end: usize) -> Range<usize> {
    char_to_byte(text, start)..char_to_byte(text, end)
}

fn char_to_byte(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    match text.char_indices().nth(char_index) {
        Some((index, _)) => index,
        None => text.len(),
    }
}

fn is_diff_header_or_metadata(line: &str) -> bool {
    line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("old mode ")
        || line.starts_with("new mode ")
        || line.starts_with("similarity index ")
        || line.starts_with("dissimilarity index ")
        || line.starts_with("rename from ")
        || line.starts_with("rename to ")
        || line.starts_with("copy from ")
        || line.starts_with("copy to ")
        || line.starts_with("Binary files ")
        || line.starts_with("GIT binary patch")
}

fn is_file_header(line: &str) -> bool {
    line.starts_with("--- ") || line.starts_with("+++ ")
}

fn is_hunk_header(line: &str) -> bool {
    line.starts_with("@@")
}

fn is_change_line(line: &str) -> bool {
    is_add_line(line) || is_del_line(line)
}

fn is_add_line(line: &str) -> bool {
    line.starts_with('+') && !line.starts_with("+++ ")
}

fn is_del_line(line: &str) -> bool {
    line.starts_with('-') && !line.starts_with("--- ")
}

#[cfg(test)]
mod tests {
    use super::{render_diff_lines, render_diff_lines_incremental, DiffRenderState};

    #[test]
    fn renders_git_patch_headers_hunks_and_intraline_changes() {
        let lines = vec![
            "diff --git a/src/main.rs b/src/main.rs".to_string(),
            "index 1111111..2222222 100644".to_string(),
            "--- a/src/main.rs".to_string(),
            "+++ b/src/main.rs".to_string(),
            "@@ -1,3 +1,3 @@".to_string(),
            "-let answer = 41;".to_string(),
            "+let answer = 42;".to_string(),
        ];

        let rendered = render_diff_lines(&lines, true);
        assert!(rendered[0].contains("\x1b[1m"));
        assert!(rendered[4].contains("\x1b[36m"));
        assert!(rendered[5].contains("\x1b[1;91m1"));
        assert!(rendered[6].contains("\x1b[1;92m2"));
    }

    #[test]
    fn renders_unified_diff_without_git_metadata() {
        let lines = vec![
            "--- before.txt".to_string(),
            "+++ after.txt".to_string(),
            "@@ -1 +1 @@".to_string(),
            "-value=old".to_string(),
            "+value=new".to_string(),
        ];

        let rendered = render_diff_lines(&lines, false);
        assert_eq!(rendered[0], "--- before.txt");
        assert_eq!(rendered[2], "@@ -1 +1 @@");
        assert_eq!(rendered[3], "-value=[-old-]");
        assert_eq!(rendered[4], "+value={+new+}");
    }

    #[test]
    fn malformed_diff_does_not_panic_and_keeps_lines() {
        let lines = vec![
            "@@ malformed hunk".to_string(),
            "-".to_string(),
            "+".to_string(),
            "not-a-diff-line".to_string(),
            "--- still just text".to_string(),
        ];

        let rendered = render_diff_lines(&lines, false);
        assert_eq!(rendered.len(), lines.len());
        assert_eq!(rendered[1], "-");
        assert_eq!(rendered[2], "+");
        assert_eq!(rendered[3], "not-a-diff-line");
    }

    #[test]
    fn large_hunk_renders_without_dropping_lines() {
        let mut lines = vec![
            "diff --git a/src/lib.rs b/src/lib.rs".to_string(),
            "--- a/src/lib.rs".to_string(),
            "+++ b/src/lib.rs".to_string(),
            "@@ -1,512 +1,512 @@".to_string(),
        ];
        for idx in 0..512 {
            lines.push(format!("-old line {idx:04} value"));
            lines.push(format!("+new line {idx:04} value"));
        }

        let rendered = render_diff_lines(&lines, true);
        assert_eq!(rendered.len(), lines.len());
        assert!(rendered[4].contains("\x1b[31m"));
        assert!(rendered[5].contains("\x1b[32m"));
    }

    #[test]
    fn incremental_carries_change_run_across_chunk_boundary() {
        let mut state = DiffRenderState::default();
        let first = render_diff_lines_incremental(
            &["@@ -1 +1 @@".to_string(), "-value=old".to_string()],
            false,
            &mut state,
        );
        assert_eq!(first, vec!["@@ -1 +1 @@".to_string()]);

        let second = render_diff_lines_incremental(&["+value=new".to_string()], false, &mut state);
        assert_eq!(
            second,
            vec!["-value=[-old-]".to_string(), "+value={+new+}".to_string()]
        );
    }

    #[test]
    fn incremental_flushes_pending_when_context_line_arrives() {
        let mut state = DiffRenderState::default();
        let _ = render_diff_lines_incremental(
            &["@@ -1 +1 @@".to_string(), "-old".to_string()],
            false,
            &mut state,
        );

        let second = render_diff_lines_incremental(&[" context".to_string()], false, &mut state);
        assert_eq!(second, vec!["-old".to_string(), " context".to_string()]);
    }

    #[test]
    fn pending_buffer_is_bounded() {
        let mut state = DiffRenderState::with_pending_limit(2);

        let first = render_diff_lines_incremental(
            &[
                "@@ -1,3 +1,3 @@".to_string(),
                "-a".to_string(),
                "-b".to_string(),
                "-c".to_string(),
            ],
            false,
            &mut state,
        );
        assert_eq!(first, vec!["@@ -1,3 +1,3 @@".to_string(), "-a".to_string()]);

        let second = render_diff_lines_incremental(&[" context".to_string()], false, &mut state);
        assert_eq!(
            second,
            vec!["-b".to_string(), "-c".to_string(), " context".to_string()]
        );
    }
}

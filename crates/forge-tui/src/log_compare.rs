//! Cross-loop compare helpers for synchronized side-by-side log rendering.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparePaneWindow {
    pub start_line: usize,
    pub end_line: usize,
    pub anchor_line: usize,
    pub anchor_timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynchronizedCompareWindow {
    pub left: ComparePaneWindow,
    pub right: ComparePaneWindow,
    pub scroll_from_bottom: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffHint {
    Equal,
    Different,
    LeftOnly,
    RightOnly,
    Empty,
}

impl DiffHint {
    #[must_use]
    pub fn glyph(self) -> char {
        match self {
            Self::Equal => '=',
            Self::Different => '!',
            Self::LeftOnly => '<',
            Self::RightOnly => '>',
            Self::Empty => ' ',
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffHintSummary {
    pub equal: usize,
    pub different: usize,
    pub left_only: usize,
    pub right_only: usize,
}

#[must_use]
pub fn synchronized_windows(
    left_lines: &[String],
    right_lines: &[String],
    viewport_lines: usize,
    scroll_from_bottom: usize,
) -> SynchronizedCompareWindow {
    let viewport_lines = viewport_lines.max(1);
    let (left_start, left_end, clamped_scroll) =
        window_bounds(left_lines.len(), viewport_lines, scroll_from_bottom);
    let anchor_row = viewport_lines / 2;
    let left_anchor = resolve_anchor_line(left_start, left_end, anchor_row);
    let left_timestamp = left_lines
        .get(left_anchor)
        .and_then(|line| extract_timestamp_token(line));

    let right_anchor = left_timestamp
        .as_ref()
        .and_then(|token| find_matching_timestamp_anchor(right_lines, token))
        .unwrap_or_else(|| map_line_anchor(left_anchor, left_lines.len(), right_lines.len()));
    let (right_start, right_end, _) =
        window_around_anchor(right_lines.len(), viewport_lines, right_anchor, anchor_row);
    let right_timestamp = right_lines
        .get(right_anchor)
        .and_then(|line| extract_timestamp_token(line));

    SynchronizedCompareWindow {
        left: ComparePaneWindow {
            start_line: left_start,
            end_line: left_end,
            anchor_line: left_anchor,
            anchor_timestamp: left_timestamp,
        },
        right: ComparePaneWindow {
            start_line: right_start,
            end_line: right_end,
            anchor_line: right_anchor,
            anchor_timestamp: right_timestamp,
        },
        scroll_from_bottom: clamped_scroll,
    }
}

#[must_use]
pub fn diff_hint(left: Option<&str>, right: Option<&str>) -> DiffHint {
    let left = left.map(str::trim).filter(|value| !value.is_empty());
    let right = right.map(str::trim).filter(|value| !value.is_empty());
    match (left, right) {
        (Some(a), Some(b)) if a == b => DiffHint::Equal,
        (Some(_), Some(_)) => DiffHint::Different,
        (Some(_), None) => DiffHint::LeftOnly,
        (None, Some(_)) => DiffHint::RightOnly,
        (None, None) => DiffHint::Empty,
    }
}

#[must_use]
pub fn summarize_diff_hints(left: &[String], right: &[String]) -> DiffHintSummary {
    let rows = left.len().max(right.len());
    let mut summary = DiffHintSummary::default();
    for row in 0..rows {
        match diff_hint(
            left.get(row).map(String::as_str),
            right.get(row).map(String::as_str),
        ) {
            DiffHint::Equal => summary.equal = summary.equal.saturating_add(1),
            DiffHint::Different => summary.different = summary.different.saturating_add(1),
            DiffHint::LeftOnly => summary.left_only = summary.left_only.saturating_add(1),
            DiffHint::RightOnly => summary.right_only = summary.right_only.saturating_add(1),
            DiffHint::Empty => {}
        }
    }
    summary
}

#[must_use]
pub fn extract_timestamp_token(line: &str) -> Option<String> {
    let raw = line.split_whitespace().next()?;
    let token = raw.trim_matches(|c: char| matches!(c, '[' | ']' | '(' | ')' | ',' | ';'));
    if looks_like_timestamp(token) {
        Some(token.to_owned())
    } else {
        None
    }
}

fn looks_like_timestamp(token: &str) -> bool {
    let has_digit = token.bytes().any(|b| b.is_ascii_digit());
    let has_clock = token.contains(':');
    let has_date = token.contains('-') || token.contains('/');
    let has_t = token.contains('T') || token.contains('t');
    has_digit && has_clock && (has_date || has_t)
}

fn window_bounds(
    total: usize,
    viewport_lines: usize,
    scroll_from_bottom: usize,
) -> (usize, usize, usize) {
    if total == 0 {
        return (0, 0, 0);
    }
    let viewport_lines = viewport_lines.max(1);
    let max_scroll = total.saturating_sub(1);
    let clamped = scroll_from_bottom.min(max_scroll);
    let end = total.saturating_sub(clamped).min(total);
    let start = end.saturating_sub(viewport_lines);
    (start, end, clamped)
}

fn resolve_anchor_line(start: usize, end: usize, anchor_row: usize) -> usize {
    if end <= start {
        return start;
    }
    let visible = end.saturating_sub(start);
    let row = anchor_row.min(visible.saturating_sub(1));
    start.saturating_add(row)
}

fn window_around_anchor(
    total: usize,
    viewport_lines: usize,
    anchor_line: usize,
    anchor_row: usize,
) -> (usize, usize, usize) {
    if total == 0 {
        return (0, 0, 0);
    }
    let viewport_lines = viewport_lines.max(1);
    let anchor_line = anchor_line.min(total.saturating_sub(1));
    let anchor_row = anchor_row.min(viewport_lines.saturating_sub(1));
    let max_start = total.saturating_sub(viewport_lines);
    let start = anchor_line.saturating_sub(anchor_row).min(max_start);
    let end = start.saturating_add(viewport_lines).min(total);
    (start, end, anchor_line)
}

fn map_line_anchor(left_anchor: usize, left_total: usize, right_total: usize) -> usize {
    if right_total == 0 {
        return 0;
    }
    if left_total <= 1 {
        return 0;
    }
    let left_span = left_total.saturating_sub(1);
    let right_span = right_total.saturating_sub(1);
    let ratio = left_anchor as f64 / left_span as f64;
    ((ratio * right_span as f64).round() as usize).min(right_span)
}

fn find_matching_timestamp_anchor(lines: &[String], target: &str) -> Option<usize> {
    let target_norm = normalize_timestamp_precision(target);
    for (index, line) in lines.iter().enumerate() {
        if let Some(token) = extract_timestamp_token(line) {
            if token == target || normalize_timestamp_precision(&token) == target_norm {
                return Some(index);
            }
        } else if line.contains(target) {
            return Some(index);
        }
    }
    None
}

fn normalize_timestamp_precision(token: &str) -> String {
    if let Some((head, _)) = token.split_once('.') {
        head.to_owned()
    } else {
        token.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{diff_hint, summarize_diff_hints, synchronized_windows, DiffHint};

    #[test]
    fn synchronized_windows_prefers_matching_timestamp_anchor() {
        let left = vec![
            "2026-02-12T11:00:00Z start".to_owned(),
            "2026-02-12T11:00:01Z fetch".to_owned(),
            "2026-02-12T11:00:02Z compile".to_owned(),
            "2026-02-12T11:00:03Z test".to_owned(),
        ];
        let right = vec![
            "2026-02-12T11:00:00Z init".to_owned(),
            "2026-02-12T11:00:03Z test".to_owned(),
            "2026-02-12T11:00:04Z close".to_owned(),
        ];

        let synced = synchronized_windows(&left, &right, 3, 0);
        assert_eq!(
            synced.left.anchor_timestamp.as_deref(),
            Some("2026-02-12T11:00:02Z")
        );
        assert_eq!(synced.right.anchor_line, 1);
    }

    #[test]
    fn synchronized_windows_falls_back_to_ratio_anchor() {
        let left: Vec<String> = (0..20).map(|idx| format!("left {idx}")).collect();
        let right: Vec<String> = (0..40).map(|idx| format!("right {idx}")).collect();

        let synced = synchronized_windows(&left, &right, 5, 2);
        assert_eq!(synced.left.anchor_line, 15);
        assert_eq!(synced.right.anchor_line, 31);
    }

    #[test]
    fn diff_hint_variants_cover_common_cases() {
        assert_eq!(diff_hint(Some("same"), Some("same")), DiffHint::Equal);
        assert_eq!(diff_hint(Some("left"), Some("right")), DiffHint::Different);
        assert_eq!(diff_hint(Some("only-left"), None), DiffHint::LeftOnly);
        assert_eq!(diff_hint(None, Some("only-right")), DiffHint::RightOnly);
        assert_eq!(diff_hint(None, None), DiffHint::Empty);
    }

    #[test]
    fn summarize_diff_hints_counts_each_bucket() {
        let left = vec![
            "equal".to_owned(),
            "different-left".to_owned(),
            "left-only".to_owned(),
        ];
        let right = vec![
            "equal".to_owned(),
            "different-right".to_owned(),
            String::new(),
            "right-only".to_owned(),
        ];

        let summary = summarize_diff_hints(&left, &right);
        assert_eq!(summary.equal, 1);
        assert_eq!(summary.different, 1);
        assert_eq!(summary.left_only, 1);
        assert_eq!(summary.right_only, 1);
    }
}

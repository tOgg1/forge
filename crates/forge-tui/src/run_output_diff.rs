//! Run output semantic diff with noise/timestamp suppression.

use crate::log_compare::{diff_hint, DiffHint, DiffHintSummary};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutputDiffOptions {
    pub suppress_timestamps: bool,
    pub suppress_durations: bool,
    pub suppress_hex_addresses: bool,
    pub suppress_uuids: bool,
    pub suppress_ansi_sequences: bool,
}

impl Default for RunOutputDiffOptions {
    fn default() -> Self {
        Self {
            suppress_timestamps: true,
            suppress_durations: true,
            suppress_hex_addresses: true,
            suppress_uuids: true,
            suppress_ansi_sequences: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiffRow {
    pub line_index: usize,
    pub hint: DiffHint,
    pub left_raw: Option<String>,
    pub right_raw: Option<String>,
    pub left_normalized: Option<String>,
    pub right_normalized: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutputDiffReport {
    pub rows: Vec<SemanticDiffRow>,
    pub summary: DiffHintSummary,
    pub changed_ratio: f64,
}

#[must_use]
pub fn build_run_output_semantic_diff(
    left: &[String],
    right: &[String],
    options: &RunOutputDiffOptions,
) -> RunOutputDiffReport {
    let row_count = left.len().max(right.len());
    let mut rows = Vec::with_capacity(row_count);
    let mut summary = DiffHintSummary::default();

    for line_index in 0..row_count {
        let left_raw = left.get(line_index).cloned();
        let right_raw = right.get(line_index).cloned();
        let left_normalized = left_raw
            .as_deref()
            .map(|line| normalize_semantic_line(line, options));
        let right_normalized = right_raw
            .as_deref()
            .map(|line| normalize_semantic_line(line, options));
        let hint = diff_hint(left_normalized.as_deref(), right_normalized.as_deref());
        match hint {
            DiffHint::Equal => summary.equal = summary.equal.saturating_add(1),
            DiffHint::Different => summary.different = summary.different.saturating_add(1),
            DiffHint::LeftOnly => summary.left_only = summary.left_only.saturating_add(1),
            DiffHint::RightOnly => summary.right_only = summary.right_only.saturating_add(1),
            DiffHint::Empty => {}
        }
        rows.push(SemanticDiffRow {
            line_index,
            hint,
            left_raw,
            right_raw,
            left_normalized,
            right_normalized,
        });
    }

    let compared = summary.equal + summary.different;
    let changed_ratio = if compared == 0 {
        0.0
    } else {
        summary.different as f64 / compared as f64
    };

    RunOutputDiffReport {
        rows,
        summary,
        changed_ratio,
    }
}

#[must_use]
pub fn normalize_semantic_line(line: &str, options: &RunOutputDiffOptions) -> String {
    if line.trim().is_empty() {
        return String::new();
    }
    let mut normalized = line.to_owned();

    if options.suppress_ansi_sequences {
        normalized = replace_all(r"\x1b\[[0-9;]*[[:alpha:]]", &normalized, "");
    }
    if options.suppress_timestamps {
        normalized = replace_all(
            r"(?i)\b\d{4}-\d{2}-\d{2}[ t]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:z|[+-]\d{2}:?\d{2})?\b",
            &normalized,
            "<ts>",
        );
        normalized = replace_all(r"\b\d{2}:\d{2}:\d{2}(?:\.\d+)?\b", &normalized, "<ts>");
    }
    if options.suppress_durations {
        normalized = replace_all(r"\b\d+(?:\.\d+)?(?:ms|s|m|h|us|ns)\b", &normalized, "<dur>");
    }
    if options.suppress_hex_addresses {
        normalized = replace_all(r"\b0x[0-9a-fA-F]+\b", &normalized, "<hex>");
    }
    if options.suppress_uuids {
        normalized = replace_all(
            r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}\b",
            &normalized,
            "<id>",
        );
        normalized = replace_all(r"\b[0-9a-fA-F]{10,}\b", &normalized, "<id>");
    }

    collapse_whitespace(&normalized).to_ascii_lowercase()
}

fn replace_all(pattern: &str, source: &str, replacement: &str) -> String {
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(err) => panic!("regex pattern must compile: {err}"),
    };
    regex.replace_all(source, replacement).into_owned()
}

fn collapse_whitespace(source: &str) -> String {
    source
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{build_run_output_semantic_diff, normalize_semantic_line, RunOutputDiffOptions};
    use crate::log_compare::DiffHint;

    #[test]
    fn semantic_normalization_suppresses_timestamp_duration_and_ids() {
        let options = RunOutputDiffOptions::default();
        let left = "2026-02-13T20:01:02Z run 7f4c2d1a9b complete in 31ms id=2f8bd0b6-44ec-4fce-a404-4c4ae59f9fbf";
        let right = "2026-02-13T20:01:05Z run 1aaeeff443 complete in 45ms id=a12df0b6-44ec-4fce-a404-4c4ae59f9fbf";
        assert_eq!(
            normalize_semantic_line(left, &options),
            normalize_semantic_line(right, &options)
        );
    }

    #[test]
    fn semantic_diff_marks_equal_after_noise_suppression() {
        let left = vec![
            "2026-02-13T20:01:02Z step fetch completed in 31ms".to_owned(),
            "panic at 0x7ffeefbff5c0".to_owned(),
        ];
        let right = vec![
            "2026-02-13T20:01:05Z step fetch completed in 45ms".to_owned(),
            "panic at 0x7ffeefbff5f0".to_owned(),
        ];

        let report =
            build_run_output_semantic_diff(&left, &right, &RunOutputDiffOptions::default());
        assert_eq!(report.summary.equal, 2);
        assert_eq!(report.summary.different, 0);
        assert_eq!(report.changed_ratio, 0.0);
        assert!(report.rows.iter().all(|row| row.hint == DiffHint::Equal));
    }

    #[test]
    fn semantic_diff_reports_left_and_right_only_lines() {
        let left = vec!["line-a".to_owned(), "line-b".to_owned()];
        let right = vec![
            "line-a".to_owned(),
            "line-c".to_owned(),
            "line-d".to_owned(),
        ];
        let report =
            build_run_output_semantic_diff(&left, &right, &RunOutputDiffOptions::default());
        assert_eq!(report.summary.equal, 1);
        assert_eq!(report.summary.different, 1);
        assert_eq!(report.summary.right_only, 1);
        assert_eq!(report.rows[2].hint, DiffHint::RightOnly);
    }

    #[test]
    fn semantic_diff_can_keep_timestamps_when_disabled() {
        let options = RunOutputDiffOptions {
            suppress_timestamps: false,
            ..RunOutputDiffOptions::default()
        };
        let left = vec!["2026-02-13T20:01:02Z done".to_owned()];
        let right = vec!["2026-02-13T20:01:05Z done".to_owned()];
        let report = build_run_output_semantic_diff(&left, &right, &options);
        assert_eq!(report.summary.different, 1);
        assert_eq!(report.rows[0].hint, DiffHint::Different);
    }
}

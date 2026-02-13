//! Runs tab parity – view-model and rendering for the loop run history pane.
//!
//! Ports Go `internal/looptui/looptui.go` renderRunsPane + runs.go helpers.

use forge_ftui_adapter::render::{FrameSize, Rect, RenderFrame, TermColor, TextRole};
use forge_ftui_adapter::style::ThemeSpec;
use forge_ftui_adapter::widgets::BorderStyle;

use crate::theme::ResolvedPalette;

use crate::logs_tab::log_window_bounds;

// ---------------------------------------------------------------------------
// Extended RunView – enriched beyond app::RunView for rendering
// ---------------------------------------------------------------------------

/// A run entry enriched with display-ready fields.
#[derive(Debug, Clone, Default)]
pub struct RunEntry {
    pub id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub profile_name: String,
    pub profile_id: String,
    pub harness: String,
    pub started_at: String,
    pub duration_display: String,
    pub output_lines: Vec<String>,
}

// ---------------------------------------------------------------------------
// RunsTabState – view-model state for the runs pane
// ---------------------------------------------------------------------------

/// State for the runs tab rendering. The App owns selection/scroll/layer state;
/// this struct holds the display-ready data pushed from the refresh cycle.
#[derive(Debug, Clone, Default)]
pub struct RunsTabState {
    /// Display-ready run entries (newest first).
    pub runs: Vec<RunEntry>,
    /// Index of selected run (owned by App, mirrored here for rendering).
    pub selected_run: usize,
    /// Current log layer label (e.g. "raw", "events", ...).
    pub layer_label: String,
    /// Display ID of the selected loop.
    pub loop_display_id: String,
    /// Current log scroll offset.
    pub log_scroll: usize,
}

// ---------------------------------------------------------------------------
// Helper functions (matching Go helpers)
// ---------------------------------------------------------------------------

/// First 8 characters of a run ID, matching Go `shortRunID`.
#[must_use]
pub fn short_run_id(id: &str) -> &str {
    if id.len() <= 8 {
        id
    } else {
        &id[..8]
    }
}

/// Display name with fallback, matching Go `displayName`.
#[must_use]
pub fn display_name<'a>(name: &'a str, fallback: &'a str) -> &'a str {
    if !name.trim().is_empty() {
        name
    } else if !fallback.trim().is_empty() {
        fallback
    } else {
        "-"
    }
}

/// Truncate a line to `max_width` characters.
#[must_use]
pub fn truncate_line(line: &str, max_width: usize) -> String {
    if line.chars().count() <= max_width {
        line.to_owned()
    } else {
        line.chars().take(max_width).collect()
    }
}

/// Format a line-window indicator, matching Go `formatLineWindow`.
#[must_use]
pub fn format_line_window(start: i32, end: i32, total: i32, scroll: i32) -> String {
    if total <= 0 {
        return format!("lines 0/0 scroll={scroll}");
    }
    let visible_start = (start + 1).max(1);
    let end = end.clamp(0, total);
    format!("lines {visible_start}-{end}/{total} scroll={scroll}")
}

/// Extract output lines from raw output text, keeping last `max_lines`.
/// Matches Go `runOutputLines`.
#[must_use]
pub fn run_output_lines(output: &str, max_lines: usize) -> Vec<String> {
    let content = output.trim_end_matches('\n');
    if content.trim().is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    if max_lines > 0 && lines.len() > max_lines {
        lines[lines.len() - max_lines..]
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    } else {
        lines.iter().map(|s| (*s).to_owned()).collect()
    }
}

// ---------------------------------------------------------------------------
// Rendering – matches Go renderRunsPane
// ---------------------------------------------------------------------------

/// Render the runs tab pane content into a `RenderFrame`.
///
/// Layout:
/// - Line 0: timeline header + source context
/// - Line 1: key hints (including quick jump to logs)
/// - Lines 2..N: timeline rows with status/exit/duration chips
/// - Lower section: selected run context + output window
#[must_use]
pub fn render_runs_pane(state: &RunsTabState, size: FrameSize, theme: ThemeSpec) -> RenderFrame {
    let width = size.width.max(1);
    let height = size.height.max(1);
    let mut frame = RenderFrame::new(size, theme);
    let content_width = width.saturating_sub(1).max(1);

    // -- header lines -------------------------------------------------------
    let header = truncate_line(
        &format!(
            "Run timeline  loop:{}  layer:{}  source:run-selection",
            state.loop_display_id, state.layer_label
        ),
        width,
    );
    frame.draw_text(0, 0, &header, TextRole::Accent);

    let hints = truncate_line(
        ",/. select run | enter jump logs | x layer | u/d scroll output | l expanded",
        width,
    );
    frame.draw_text(0, 1, &hints, TextRole::Muted);

    if state.runs.is_empty() {
        if height > 3 {
            frame.draw_text(0, 3, "No runs captured yet for this loop.", TextRole::Muted);
        }
        if height > 4 {
            frame.draw_text(
                0,
                4,
                "Wait for next execution or jump to Logs for live stream.",
                TextRole::Muted,
            );
        }
        return frame;
    }

    // -- run timeline list --------------------------------------------------
    let selected_idx = state.selected_run.min(state.runs.len().saturating_sub(1));
    let min_output_rows = 6usize;
    let mut list_height = height.saturating_sub(min_output_rows);
    list_height = list_height.clamp(4, height.saturating_sub(2));
    let (start_idx, end_idx) = list_window(state.runs.len(), selected_idx, list_height);

    let mut y = 2;
    for i in start_idx..end_idx {
        if y >= height {
            break;
        }
        let run = &state.runs[i];
        let selected = i == selected_idx;
        let pointer = if selected { ">" } else { " " };
        let lane = timeline_lane(i, state.runs.len());
        let status = status_badge(&run.status);
        let exit = exit_badge(run.exit_code, &run.status);
        let duration = duration_chip(&run.duration_display);
        let identity = display_name(&run.profile_name, &run.profile_id);
        let line = format!(
            "{pointer}{lane} {} {status} {exit} {duration} {identity}",
            short_run_id(&run.id),
        );
        let role = if selected {
            TextRole::Accent
        } else {
            status_role(&run.status)
        };
        frame.draw_text(0, y, &truncate_line(&line, content_width), role);
        y += 1;
    }

    if start_idx > 0 && y < height {
        let more = format!("... {} newer runs", start_idx);
        frame.draw_text(0, y, &truncate_line(&more, content_width), TextRole::Muted);
        y += 1;
    }
    if end_idx < state.runs.len() && y < height {
        let more = format!("... {} older runs", state.runs.len() - end_idx);
        frame.draw_text(0, y, &truncate_line(&more, content_width), TextRole::Muted);
        y += 1;
    }

    // -- selected run output ------------------------------------------------
    let (title, output_lines, empty_msg) = selected_run_display(state);
    if y < height {
        frame.draw_text(0, y, &truncate_line("---", content_width), TextRole::Muted);
        y += 1;
    }

    if y < height {
        frame.draw_text(
            0,
            y,
            &truncate_line(&title, content_width),
            TextRole::Primary,
        );
        y += 1;
    }

    // Compute available space for output.
    let available = height.saturating_sub(y + 1).max(1) as i32;
    let total = output_lines.len() as i32;
    let (start, end, clamped) = log_window_bounds(total, available, state.log_scroll as i32);

    if y < height {
        let window_label = format!("output {}", format_line_window(start, end, total, clamped));
        frame.draw_text(
            0,
            y,
            &truncate_line(&window_label, content_width),
            TextRole::Muted,
        );
        y += 1;
    }

    if output_lines.is_empty() {
        if y < height {
            frame.draw_text(
                0,
                y,
                &truncate_line(&empty_msg, content_width),
                TextRole::Muted,
            );
        }
    } else {
        let start = start.max(0) as usize;
        let end = end.max(0) as usize;
        for i in start..end {
            if y >= height || i >= output_lines.len() {
                break;
            }
            frame.draw_text(
                0,
                y,
                &truncate_line(&output_lines[i], content_width),
                TextRole::Primary,
            );
            y += 1;
        }
    }

    frame
}

/// Render the runs tab with bordered panels into a pre-existing frame region.
///
/// Layout:
/// ```text
/// ╭─ Run Timeline ──────────────────────────────────────────────╮
/// │ > |- run-0172  [ERR ] [exit:1] [4m12s] prod-sre             │
/// │   |- run-0171  [OK  ] [exit:0] [3m09s] prod-sre             │
/// │   `- run-0170  [STOP] [exit:137] [45s] prod-sre             │
/// ╰─────────────────────────────────────────────────────────────╯
/// ╭─ Selected: run-0172 ────────────────────────────────────────╮
/// │ output lines ...                                             │
/// ╰─────────────────────────────────────────────────────────────╯
/// ```
#[must_use]
pub fn render_runs_paneled(state: &RunsTabState, size: FrameSize, theme: ThemeSpec, pal: &ResolvedPalette, focus_bottom: bool) -> RenderFrame {
    let width = size.width.max(1);
    let height = size.height.max(1);
    let mut frame = RenderFrame::new(size, theme);

    // Fill background
    frame.fill_bg(Rect { x: 0, y: 0, width, height }, pal.background);

    if state.runs.is_empty() {
        // Empty state panel
        let panel_h = 5usize.min(height);
        let inner = frame.draw_panel(
            Rect { x: 0, y: 0, width, height: panel_h },
            &format!("Run Timeline  loop:{}", state.loop_display_id),
            BorderStyle::Rounded,
            pal.border,
            pal.panel,
        );
        if inner.height >= 1 {
            frame.draw_styled_text(inner.x, inner.y, "No runs captured yet for this loop.", pal.text_muted, pal.panel, false);
        }
        if inner.height >= 2 {
            frame.draw_styled_text(
                inner.x,
                inner.y + 1,
                "Wait for next execution or jump to Logs for live stream.",
                pal.text_muted,
                pal.panel,
                false,
            );
        }
        return frame;
    }

    // -- Run timeline panel --
    let selected_idx = state.selected_run.min(state.runs.len().saturating_sub(1));
    let min_output_rows = 6usize;
    let mut list_height = height.saturating_sub(min_output_rows + 2); // 2 for panel borders
    list_height = list_height.clamp(4, height.saturating_sub(4));
    let timeline_panel_h = (list_height + 2).min(height); // +2 for borders
    let (timeline_rect, rest) = Rect { x: 0, y: 0, width, height }.split_vertical(timeline_panel_h);

    let timeline_title = format!("Run Timeline  loop:{}  layer:{}", state.loop_display_id, state.layer_label);
    let tl_border = if !focus_bottom { pal.accent } else { pal.border };
    let inner = frame.draw_panel(timeline_rect, &timeline_title, BorderStyle::Rounded, tl_border, pal.panel);

    let (start_idx, end_idx) = list_window(state.runs.len(), selected_idx, inner.height);
    let mut row = 0;
    for i in start_idx..end_idx {
        if row >= inner.height {
            break;
        }
        let run = &state.runs[i];
        let selected = i == selected_idx;
        let pointer = if selected { "\u{25B8}" } else { " " }; // ▸ or space
        let lane = timeline_lane(i, state.runs.len());
        let status = status_badge(&run.status);
        let exit = exit_badge(run.exit_code, &run.status);
        let duration = duration_chip(&run.duration_display);
        let identity = display_name(&run.profile_name, &run.profile_id);
        let line = format!(
            "{pointer}{lane} {} {status} {exit} {duration} {identity}",
            short_run_id(&run.id),
        );
        let fg = if selected {
            pal.accent
        } else {
            status_color(&run.status, pal)
        };
        let trunc = truncate_line(&line, inner.width);
        frame.draw_styled_text(inner.x, inner.y + row, &trunc, fg, pal.panel, selected);
        row += 1;
    }

    // -- Selected run output panel --
    if rest.height >= 4 {
        let (title, output_lines, empty_msg) = selected_run_display(state);
        let out_border = if focus_bottom { pal.accent } else { pal.border };
        let output_inner = frame.draw_panel(rest, &truncate_line(&title, rest.width.saturating_sub(4)), BorderStyle::Rounded, out_border, pal.panel);

        let available = output_inner.height;
        let total = output_lines.len();
        let (start, end, _clamped) = crate::logs_tab::log_window_bounds(total as i32, available as i32, state.log_scroll as i32);
        let start = start.max(0) as usize;
        let end = end.max(0) as usize;

        if output_lines.is_empty() {
            if available >= 1 {
                frame.draw_styled_text(output_inner.x, output_inner.y, &truncate_line(&empty_msg, output_inner.width), pal.text_muted, pal.panel, false);
            }
        } else {
            let mut row = 0;
            for i in start..end {
                if row >= available || i >= output_lines.len() {
                    break;
                }
                frame.draw_styled_text(
                    output_inner.x,
                    output_inner.y + row,
                    &truncate_line(&output_lines[i], output_inner.width),
                    pal.text,
                    pal.panel,
                    false,
                );
                row += 1;
            }
        }
    }

    frame
}

/// Map run status to a palette color.
fn status_color(status: &str, pal: &ResolvedPalette) -> TermColor {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "running" => pal.accent,
        "success" => pal.success,
        "error" | "failed" => pal.error,
        "killed" | "cancelled" | "canceled" | "stopped" => pal.text_muted,
        _ => pal.text,
    }
}

/// Build display data for the selected run, matching Go `currentRunDisplay`.
fn selected_run_display(state: &RunsTabState) -> (String, Vec<String>, String) {
    if state.runs.is_empty() || state.selected_run >= state.runs.len() {
        return (
            "No run selected.".to_owned(),
            Vec::new(),
            "No run output available.".to_owned(),
        );
    }
    let run = &state.runs[state.selected_run];
    let title = format!(
        "Selected {} {} {} profile={} started={}",
        short_run_id(&run.id),
        status_badge(&run.status),
        exit_badge(run.exit_code, &run.status),
        display_name(&run.profile_name, &run.profile_id),
        run.started_at,
    );
    let empty_msg = "Run output is empty.".to_owned();
    (title, run.output_lines.clone(), empty_msg)
}

fn status_badge(status: &str) -> &'static str {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "running" => "[RUN ]",
        "success" => "[OK  ]",
        "error" | "failed" => "[ERR ]",
        "killed" | "cancelled" | "canceled" | "stopped" => "[STOP]",
        "queued" | "pending" => "[WAIT]",
        _ => "[UNKN]",
    }
}

fn status_role(status: &str) -> TextRole {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "running" => TextRole::Accent,
        "success" => TextRole::Success,
        "error" | "failed" => TextRole::Danger,
        "killed" | "cancelled" | "canceled" | "stopped" => TextRole::Muted,
        _ => TextRole::Primary,
    }
}

fn exit_badge(exit_code: Option<i32>, status: &str) -> String {
    if let Some(code) = exit_code {
        return format!("[exit:{code}]");
    }
    if status.trim().eq_ignore_ascii_case("running") {
        "[live]".to_owned()
    } else {
        "[exit:-]".to_owned()
    }
}

fn duration_chip(duration: &str) -> String {
    let value = if duration.trim().is_empty() {
        "-"
    } else {
        duration.trim()
    };
    format!("[{value}]")
}

fn timeline_lane(index: usize, total: usize) -> &'static str {
    if index + 1 >= total {
        "`-"
    } else {
        "|-"
    }
}

fn list_window(total: usize, selected: usize, max_rows: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let rows = max_rows.max(1).min(total);
    if total <= rows {
        return (0, total);
    }
    let half = rows / 2;
    let mut start = selected.saturating_sub(half);
    let max_start = total.saturating_sub(rows);
    if start > max_start {
        start = max_start;
    }
    let end = (start + rows).min(total);
    (start, end)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

    fn test_theme() -> ThemeSpec {
        ThemeSpec::for_kind(ThemeKind::Dark)
    }

    fn sample_runs(n: usize) -> Vec<RunEntry> {
        (0..n)
            .map(|i| RunEntry {
                id: format!("abcdefghij-{i}"),
                status: if i == 0 {
                    "running".to_owned()
                } else {
                    "success".to_owned()
                },
                exit_code: if i == 0 { None } else { Some(0) },
                profile_name: format!("profile-{i}"),
                profile_id: format!("prof-{i}"),
                harness: "claude-code".to_owned(),
                started_at: format!("2026-02-09T1{i}:00:00Z"),
                duration_display: if i == 0 {
                    "running".to_owned()
                } else {
                    format!("{i}m30s")
                },
                output_lines: vec![format!("line1 from run {i}"), format!("line2 from run {i}")],
            })
            .collect()
    }

    // -- helper function tests --

    #[test]
    fn short_run_id_truncates() {
        assert_eq!(short_run_id("abcdefghijkl"), "abcdefgh");
        assert_eq!(short_run_id("abc"), "abc");
        assert_eq!(short_run_id(""), "");
        assert_eq!(short_run_id("12345678"), "12345678");
    }

    #[test]
    fn display_name_prefers_name() {
        assert_eq!(display_name("alice", "fallback"), "alice");
        assert_eq!(display_name("", "fallback"), "fallback");
        assert_eq!(display_name("  ", "  "), "-");
        assert_eq!(display_name("", ""), "-");
    }

    #[test]
    fn truncate_line_works() {
        assert_eq!(truncate_line("hello world", 5), "hello");
        assert_eq!(truncate_line("hi", 10), "hi");
        assert_eq!(truncate_line("", 5), "");
    }

    #[test]
    fn format_line_window_basic() {
        assert_eq!(format_line_window(0, 10, 100, 0), "lines 1-10/100 scroll=0");
        assert_eq!(format_line_window(0, 0, 0, 5), "lines 0/0 scroll=5");
    }

    #[test]
    fn run_output_lines_trims_trailing_newlines() {
        let lines = run_output_lines("hello\nworld\n\n", 100);
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn run_output_lines_limits() {
        let lines = run_output_lines("a\nb\nc\nd\ne", 3);
        assert_eq!(lines, vec!["c", "d", "e"]);
    }

    #[test]
    fn run_output_lines_empty() {
        assert!(run_output_lines("", 10).is_empty());
        assert!(run_output_lines("   ", 10).is_empty());
        assert!(run_output_lines("\n\n", 10).is_empty());
    }

    // -- render tests --

    #[test]
    fn render_empty_runs() {
        let state = RunsTabState {
            loop_display_id: "loop-abc".to_owned(),
            layer_label: "raw".to_owned(),
            ..Default::default()
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 60,
                height: 10,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        assert!(
            snap.contains("Run timeline  loop:loop-abc"),
            "snap:\n{snap}"
        );
        assert!(
            snap.contains("No runs captured yet for this loop."),
            "snap:\n{snap}"
        );
    }

    #[test]
    fn render_with_runs_shows_list() {
        let state = RunsTabState {
            runs: sample_runs(3),
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "my-loop".to_owned(),
            log_scroll: 0,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 80,
                height: 20,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        // Header present.
        assert!(
            snap.contains("Run timeline  loop:my-loop  layer:raw"),
            "snap:\n{snap}"
        );
        // Selected run includes timeline marker + status badge.
        assert!(snap.contains(">|- abcdefgh [RUN ]"), "snap:\n{snap}");
        // Non-selected row includes success badge.
        assert!(snap.contains(" |- abcdefgh [OK  ]"), "snap:\n{snap}");
        // Output lines visible.
        assert!(snap.contains("line1 from run 0"), "snap:\n{snap}");
    }

    #[test]
    fn render_selected_run_title() {
        let state = RunsTabState {
            runs: sample_runs(2),
            selected_run: 1,
            layer_label: "events".to_owned(),
            loop_display_id: "test-lp".to_owned(),
            log_scroll: 0,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 80,
                height: 20,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        // Selected run title should reference run 1.
        assert!(
            snap.contains("Selected abcdefgh [OK  ] [exit:0] profile=profile-1"),
            "snap:\n{snap}"
        );
    }

    #[test]
    fn render_many_runs_shows_more() {
        let state = RunsTabState {
            runs: sample_runs(20),
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "lp".to_owned(),
            log_scroll: 0,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 80,
                height: 15,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        assert!(snap.contains("... "), "snap:\n{snap}");
        assert!(snap.contains("older runs"), "snap:\n{snap}");
    }

    #[test]
    fn render_narrow_width_truncates() {
        let state = RunsTabState {
            runs: sample_runs(2),
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "loop-id".to_owned(),
            log_scroll: 0,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 30,
                height: 15,
            },
            test_theme(),
        );
        // Should not panic; lines are truncated.
        let snap = frame.snapshot();
        assert!(!snap.is_empty());
    }

    #[test]
    fn render_scroll_offset() {
        let mut runs = sample_runs(1);
        runs[0].output_lines = (0..50).map(|i| format!("output line {i}")).collect();
        let state = RunsTabState {
            runs,
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "lp".to_owned(),
            log_scroll: 10,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 60,
                height: 20,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        assert!(snap.contains("scroll=10"), "snap:\n{snap}");
    }

    #[test]
    fn selected_run_display_no_runs() {
        let state = RunsTabState::default();
        let (title, lines, msg) = selected_run_display(&state);
        assert_eq!(title, "No run selected.");
        assert!(lines.is_empty());
        assert_eq!(msg, "No run output available.");
    }

    #[test]
    fn selected_run_display_with_run() {
        let state = RunsTabState {
            runs: sample_runs(2),
            selected_run: 0,
            ..Default::default()
        };
        let (title, lines, _) = selected_run_display(&state);
        assert!(title.contains("Selected abcdefgh [RUN ] [live]"));
        assert!(title.contains("profile=profile-0"));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn render_snapshot_baseline() {
        let state = RunsTabState {
            runs: vec![RunEntry {
                id: "run-aaaa1111bbbb".to_owned(),
                status: "success".to_owned(),
                exit_code: Some(0),
                profile_name: "default".to_owned(),
                profile_id: "prof-1".to_owned(),
                harness: "claude-code".to_owned(),
                started_at: "2026-02-09T10:00:00Z".to_owned(),
                duration_display: "5m30s".to_owned(),
                output_lines: vec!["hello from run".to_owned()],
            }],
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "my-loop".to_owned(),
            log_scroll: 0,
        };
        let frame = render_runs_pane(
            &state,
            FrameSize {
                width: 70,
                height: 12,
            },
            test_theme(),
        );
        let snap = frame.snapshot();
        // Verify key content lines are present.
        assert!(frame
            .row_text(0)
            .contains("Run timeline  loop:my-loop  layer:raw"));
        assert!(frame.row_text(1).contains(",/. select run"));
        assert!(snap.contains("run-aaaa [OK  ]"), "snap:\n{snap}");
        assert!(snap.contains("[exit:0]"), "snap:\n{snap}");
        assert!(snap.contains("[5m30s]"), "snap:\n{snap}");
    }
}

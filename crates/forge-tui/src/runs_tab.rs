//! Runs tab parity – view-model and rendering for the loop run history pane.
//!
//! Ports Go `internal/looptui/looptui.go` renderRunsPane + runs.go helpers.

use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

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
    if line.len() <= max_width {
        line.to_owned()
    } else {
        line[..max_width].to_owned()
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
/// Layout (matching Go):
/// - Line 0: "Run history: <loop_display_id>  layer=<label>"
/// - Line 1: hint keys
/// - Line 2: blank
/// - Lines 3..3+listLimit: run list with ">" on selected
/// - Optional "... N more runs"
/// - Blank line
/// - Run output title (muted)
/// - "output lines X-Y/Z scroll=S"
/// - Log output lines (or empty message)
#[must_use]
pub fn render_runs_pane(state: &RunsTabState, size: FrameSize, theme: ThemeSpec) -> RenderFrame {
    let width = size.width.max(1);
    let height = size.height.max(1);
    let mut frame = RenderFrame::new(size, theme);
    let content_width = width.saturating_sub(2).max(1);

    // -- header lines --
    let header = truncate_line(
        &format!(
            "Run history: {}  layer={}",
            state.loop_display_id, state.layer_label
        ),
        width,
    );
    frame.draw_text(0, 0, &header, TextRole::Primary);

    let hints = truncate_line(
        ",/. select run | x layer | pgup/pgdn scroll output | l expanded",
        width,
    );
    frame.draw_text(0, 1, &hints, TextRole::Muted);
    // Line 2 is blank.

    if state.runs.is_empty() {
        frame.draw_text(0, 3, "No recorded runs yet.", TextRole::Muted);
        return frame;
    }

    // -- run list --
    let list_limit = state.runs.len().min(3_usize.max(height / 3));
    let mut y = 3;
    for i in 0..list_limit {
        if y >= height {
            break;
        }
        let run = &state.runs[i];
        let prefix = if i == state.selected_run { "> " } else { "  " };
        let exit = match run.exit_code {
            Some(code) => code.to_string(),
            None => "-".to_owned(),
        };
        let label = format!(
            "{} {:<7} exit={} dur={} {}",
            short_run_id(&run.id),
            run.status.to_uppercase(),
            exit,
            run.duration_display,
            display_name(&run.profile_name, &run.profile_id),
        );
        let line = format!(
            "{prefix}{}",
            truncate_line(&label, content_width.saturating_sub(2))
        );
        let role = if i == state.selected_run {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        frame.draw_text(0, y, &truncate_line(&line, width), role);
        y += 1;
    }

    if state.runs.len() > list_limit && y < height {
        let more = format!("... {} more runs", state.runs.len() - list_limit);
        frame.draw_text(0, y, &truncate_line(&more, width), TextRole::Muted);
        y += 1;
    }

    // Blank separator.
    y += 1;

    // -- selected run output --
    let (title, output_lines, empty_msg) = selected_run_display(state);

    if y < height {
        frame.draw_text(0, y, &truncate_line(&title, width), TextRole::Muted);
        y += 1;
    }

    // Compute available space for output.
    let available = height.saturating_sub(y + 1).max(1) as i32;
    let total = output_lines.len() as i32;
    let (start, end, clamped) = log_window_bounds(total, available, state.log_scroll as i32);

    if y < height {
        let window_label = format!("output {}", format_line_window(start, end, total, clamped));
        frame.draw_text(0, y, &truncate_line(&window_label, width), TextRole::Muted);
        y += 1;
    }

    if output_lines.is_empty() {
        if y < height {
            frame.draw_text(0, y, &truncate_line(&empty_msg, width), TextRole::Muted);
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
                &truncate_line(&output_lines[i], width),
                TextRole::Primary,
            );
            y += 1;
        }
    }

    frame
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
        "Run {} | profile={} | started={}",
        short_run_id(&run.id),
        display_name(&run.profile_name, &run.profile_id),
        run.started_at,
    );
    let empty_msg = "Run output is empty.".to_owned();
    (title, run.output_lines.clone(), empty_msg)
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
        assert!(snap.contains("Run history: loop-abc"), "snap:\n{snap}");
        assert!(snap.contains("No recorded runs yet."), "snap:\n{snap}");
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
            snap.contains("Run history: my-loop  layer=raw"),
            "snap:\n{snap}"
        );
        // Selected run has ">" prefix.
        assert!(snap.contains("> abcdefgh RUNNING"), "snap:\n{snap}");
        // Non-selected run has "  " prefix.
        assert!(snap.contains("  abcdefgh SUCCESS"), "snap:\n{snap}");
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
            snap.contains("Run abcdefgh | profile=profile-1"),
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
        assert!(snap.contains("more runs"), "snap:\n{snap}");
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
        assert!(title.contains("Run abcdefgh"));
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
        // Verify key content lines are present.
        assert!(frame
            .row_text(0)
            .contains("Run history: my-loop  layer=raw"));
        assert!(frame.row_text(1).contains(",/. select run"));
        assert!(frame.row_text(3).contains("> run-aaaa SUCCESS"));
        assert!(frame.row_text(3).contains("exit=0"));
        assert!(frame.row_text(3).contains("dur=5m30s"));
    }
}

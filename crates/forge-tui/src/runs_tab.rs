//! Runs tab parity – view-model and rendering for the loop run history pane.
//!
//! Ports Go `internal/looptui/looptui.go` renderRunsPane + runs.go helpers.

use forge_ftui_adapter::render::{
    CellStyle, FrameSize, Rect, RenderFrame, StyledSpan, TermColor, TextRole,
};
use forge_ftui_adapter::style::ThemeSpec;
use forge_ftui_adapter::widgets::BorderStyle;

use crate::lane_model::classify_line;
use crate::log_pipeline::{highlight_spans, SpanKind};
use crate::theme::ResolvedPalette;

use crate::logs_tab::log_window_bounds;

// ---------------------------------------------------------------------------
// Column layout helper for tabular run list
// ---------------------------------------------------------------------------

/// Fixed column widths for the run table.
struct ColumnLayout {
    id_w: usize,
    status_w: usize,
    exit_w: usize,
    duration_w: usize,
    profile_w: usize,
}

/// Pointer/selector column width (▸ + space).
const POINTER_W: usize = 2;

impl ColumnLayout {
    /// Compute column widths for the given inner width.
    fn for_width(width: usize) -> Self {
        // Fixed widths: pointer(2) + id(10) + status(7) + exit(10) + duration(10) + gaps(5) = 44
        let id_w = 10;
        let status_w = 7;
        let exit_w = 10;
        let duration_w = 10;
        let fixed = POINTER_W + id_w + status_w + exit_w + duration_w + 5; // 5 spaces between cols
        let profile_w = if width > fixed {
            width - fixed
        } else {
            4 // minimum
        };
        Self {
            id_w,
            status_w,
            exit_w,
            duration_w,
            profile_w,
        }
    }
}

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

/// Render the runs tab with bordered panels and columnar table layout.
///
/// Layout:
/// ```text
/// ╭─ Run Timeline  loop:abc  layer:raw ─────────────────────╮
/// │  ID        Status  Exit     Duration  Profile            │ ← header (muted)
/// │ ▸ abcdefgh [RUN ] [live]   [running] prod-sre           │ ← selected
/// │   abcdefgh [OK  ] [exit:0] [3m09s]   prod-sre           │
/// │   abcdefgh [STOP] [exit:137] [45s]   prod-sre           │
/// ╰─────────────────────────────────────────────────────────╯
/// ╭─ Output: abcdefgh [RUN ] ───────────────────────────────╮
/// │ output line 1                                            │
/// │ output line 2                                            │
/// │                                  lines 1-8/50  scroll=0  │
/// ╰─────────────────────────────────────────────────────────╯
/// ```
#[must_use]
pub fn render_runs_paneled(
    state: &RunsTabState,
    size: FrameSize,
    theme: ThemeSpec,
    pal: &ResolvedPalette,
    focus_bottom: bool,
) -> RenderFrame {
    let width = size.width.max(1);
    let height = size.height.max(1);
    let mut frame = RenderFrame::new(size, theme);

    // Fill background
    frame.fill_bg(
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
        pal.background,
    );

    let muted_style = CellStyle {
        fg: pal.text_muted,
        bg: pal.panel,
        bold: false,
        dim: false,
        underline: false,
    };

    if state.runs.is_empty() {
        // Empty state panel
        let panel_h = 5usize.min(height);
        let inner = frame.draw_panel(
            Rect {
                x: 0,
                y: 0,
                width,
                height: panel_h,
            },
            &format!("Run Timeline  loop:{}", state.loop_display_id),
            BorderStyle::Rounded,
            pal.border,
            pal.panel,
        );
        if inner.height >= 1 {
            frame.draw_spans_in_rect(
                inner,
                0,
                0,
                &[StyledSpan::cell(
                    "No runs captured yet for this loop.",
                    muted_style,
                )],
            );
        }
        if inner.height >= 2 {
            frame.draw_spans_in_rect(
                inner,
                0,
                1,
                &[StyledSpan::cell(
                    "Wait for next execution or jump to Logs for live stream.",
                    muted_style,
                )],
            );
        }
        return frame;
    }

    // -- Compute layout splits ------------------------------------------------
    let selected_idx = state.selected_run.min(state.runs.len().saturating_sub(1));
    let min_output_rows = 6usize;
    // inner rows = header(1) + data rows; reserve space for output panel + borders
    let mut list_inner = height.saturating_sub(min_output_rows + 2 + 2);
    list_inner = list_inner.clamp(3, height.saturating_sub(4));
    let timeline_panel_h = (list_inner + 2).min(height); // +2 for borders
    let (timeline_rect, rest) = Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
    .split_vertical(timeline_panel_h);

    // -- Timeline panel -------------------------------------------------------
    let timeline_title = format!(
        "Run Timeline  loop:{}  layer:{}",
        state.loop_display_id, state.layer_label
    );
    let tl_border = if !focus_bottom {
        pal.accent
    } else {
        pal.border
    };
    let inner = frame.draw_panel(
        timeline_rect,
        &timeline_title,
        BorderStyle::Rounded,
        tl_border,
        pal.panel,
    );

    let cols = ColumnLayout::for_width(inner.width);

    // -- Header row -----------------------------------------------------------
    let header_row: usize = if inner.height >= 1 {
        let hdr = format!(
            "{:ptr_w$}{:<id_w$} {:<sw$} {:<ew$} {:<dw$} {:<pw$}",
            "",
            "ID",
            "Status",
            "Exit",
            "Duration",
            "Profile",
            ptr_w = POINTER_W,
            id_w = cols.id_w,
            sw = cols.status_w,
            ew = cols.exit_w,
            dw = cols.duration_w,
            pw = cols.profile_w,
        );
        frame.draw_spans_in_rect(
            inner,
            0,
            0,
            &[StyledSpan::cell(
                &truncate_line(&hdr, inner.width),
                muted_style,
            )],
        );
        1
    } else {
        0
    };

    // -- Data rows ------------------------------------------------------------
    let data_rows = inner.height.saturating_sub(header_row);
    let (start_idx, end_idx) = list_window(state.runs.len(), selected_idx, data_rows);
    let mut row = header_row;
    for i in start_idx..end_idx {
        if row >= inner.height {
            break;
        }
        let run = &state.runs[i];
        let selected = i == selected_idx;

        let pointer = if selected { "\u{25B8} " } else { "  " };
        let id_col = format!("{:<w$}", short_run_id(&run.id), w = cols.id_w);
        let status_col = format!("{:<w$}", status_badge(&run.status), w = cols.status_w);
        let exit_str = exit_badge(run.exit_code, &run.status);
        let exit_col = format!("{:<w$}", exit_str, w = cols.exit_w);
        let dur_str = duration_chip(&run.duration_display);
        let dur_col = format!("{:<w$}", dur_str, w = cols.duration_w);
        let identity = display_name(&run.profile_name, &run.profile_id);
        let profile_col = truncate_line(identity, cols.profile_w);

        let line = format!("{pointer}{id_col} {status_col} {exit_col} {dur_col} {profile_col}");

        if selected {
            // Fill row background with panel_alt for selection band
            let row_rect = Rect {
                x: inner.x,
                y: inner.y + row,
                width: inner.width,
                height: 1,
            };
            frame.fill_bg(row_rect, pal.panel_alt);

            let cell_style = CellStyle {
                fg: pal.accent,
                bg: pal.panel_alt,
                bold: true,
                dim: false,
                underline: false,
            };
            frame.draw_spans_in_rect(
                inner,
                0,
                row,
                &[StyledSpan::cell(
                    &truncate_line(&line, inner.width),
                    cell_style,
                )],
            );
        } else {
            let fg = status_color(&run.status, pal);
            frame.draw_spans_in_rect(
                inner,
                0,
                row,
                &[StyledSpan::cell(
                    &truncate_line(&line, inner.width),
                    CellStyle {
                        fg,
                        bg: pal.panel,
                        bold: false,
                        dim: false,
                        underline: false,
                    },
                )],
            );
        }
        row += 1;
    }

    // -- Output panel ---------------------------------------------------------
    if rest.height >= 4 {
        let sel_run = &state.runs[selected_idx];
        let output_title = format!(
            "Output: {} {}",
            short_run_id(&sel_run.id),
            status_badge(&sel_run.status),
        );
        let output_lines = &sel_run.output_lines;

        let out_border = if focus_bottom { pal.accent } else { pal.border };
        let output_inner = frame.draw_panel(
            rest,
            &truncate_line(&output_title, rest.width.saturating_sub(4)),
            BorderStyle::Rounded,
            out_border,
            pal.panel,
        );

        // Reserve bottom row for scroll indicator when there are lines
        let has_lines = !output_lines.is_empty();
        let content_rows = if has_lines {
            output_inner.height.saturating_sub(1)
        } else {
            output_inner.height
        };
        let total = output_lines.len() as i32;
        let (start, end, clamped) =
            log_window_bounds(total, content_rows as i32, state.log_scroll as i32);
        let start_u = start.max(0) as usize;
        let end_u = end.max(0) as usize;

        if output_lines.is_empty() {
            if content_rows >= 1 {
                frame.draw_spans_in_rect(
                    output_inner,
                    0,
                    0,
                    &[StyledSpan::cell("Run output is empty.", muted_style)],
                );
            }
        } else {
            for (orow, i) in (start_u..end_u).enumerate() {
                if orow >= content_rows || i >= output_lines.len() {
                    break;
                }
                draw_syntax_highlighted_output_line(
                    &mut frame,
                    output_inner,
                    orow,
                    &output_lines[i],
                    pal,
                );
            }

            // Scroll indicator at bottom of output panel
            if output_inner.height >= 1 {
                let indicator = format_line_window(start, end, total, clamped);
                let indicator_len = indicator.chars().count();
                let x_off = output_inner.width.saturating_sub(indicator_len);
                frame.draw_spans_in_rect(
                    output_inner,
                    x_off,
                    output_inner.height.saturating_sub(1),
                    &[StyledSpan::cell(&indicator, muted_style)],
                );
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

fn draw_syntax_highlighted_output_line(
    frame: &mut RenderFrame,
    rect: Rect,
    y_offset: usize,
    line: &str,
    pal: &ResolvedPalette,
) {
    let clipped = truncate_line(line, rect.width);
    if clipped.is_empty() {
        return;
    }

    let lane = classify_line(&clipped);
    let spans = highlight_spans(&clipped, lane);
    if spans.is_empty() {
        frame.draw_spans_in_rect(
            rect,
            0,
            y_offset,
            &[StyledSpan::cell(
                &clipped,
                CellStyle {
                    fg: pal.text,
                    bg: pal.panel,
                    bold: false,
                    dim: false,
                    underline: false,
                },
            )],
        );
        return;
    }

    let mut styled = Vec::with_capacity(spans.len());
    for span in spans {
        if span.start >= span.end || span.end > clipped.len() {
            continue;
        }
        let token = &clipped[span.start..span.end];
        if token.is_empty() {
            continue;
        }
        styled.push(StyledSpan::cell(token, span_cell_style(span.kind, pal)));
    }

    if styled.is_empty() {
        frame.draw_spans_in_rect(
            rect,
            0,
            y_offset,
            &[StyledSpan::cell(
                &clipped,
                CellStyle {
                    fg: pal.text,
                    bg: pal.panel,
                    bold: false,
                    dim: false,
                    underline: false,
                },
            )],
        );
        return;
    }
    frame.draw_spans_in_rect(rect, 0, y_offset, &styled);
}

fn span_cell_style(kind: SpanKind, pal: &ResolvedPalette) -> CellStyle {
    let fg = match kind {
        SpanKind::Plain => pal.text,
        SpanKind::Keyword => pal.accent,
        SpanKind::StringLiteral => pal.success,
        SpanKind::Number => pal.warning,
        SpanKind::Command => pal.focus,
        SpanKind::Path => pal.info,
        SpanKind::Error => pal.error,
        SpanKind::Muted | SpanKind::Punctuation => pal.text_muted,
    };
    CellStyle {
        fg,
        bg: pal.panel,
        bold: matches!(kind, SpanKind::Keyword | SpanKind::Error),
        dim: false,
        underline: false,
    }
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

    // -- paneled render tests (new columnar table layout) --

    fn test_palette() -> ResolvedPalette {
        use crate::theme::{resolve_palette_colors, DEFAULT_PALETTE};
        resolve_palette_colors(&DEFAULT_PALETTE)
    }

    #[test]
    fn paneled_empty_runs_shows_panel_and_guidance() {
        let state = RunsTabState {
            loop_display_id: "loop-abc".to_owned(),
            layer_label: "raw".to_owned(),
            ..Default::default()
        };
        let pal = test_palette();
        let frame = render_runs_paneled(
            &state,
            FrameSize {
                width: 80,
                height: 20,
            },
            test_theme(),
            &pal,
            false,
        );
        let snap = frame.snapshot();
        assert!(
            snap.contains("Run Timeline  loop:loop-abc"),
            "snap:\n{snap}"
        );
        assert!(
            snap.contains("No runs captured yet for this loop."),
            "snap:\n{snap}"
        );
    }

    #[test]
    fn paneled_shows_column_header() {
        let state = RunsTabState {
            runs: sample_runs(2),
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "my-loop".to_owned(),
            log_scroll: 0,
        };
        let pal = test_palette();
        let frame = render_runs_paneled(
            &state,
            FrameSize {
                width: 80,
                height: 20,
            },
            test_theme(),
            &pal,
            false,
        );
        let snap = frame.snapshot();
        assert!(
            snap.contains("ID"),
            "header should contain ID column\nsnap:\n{snap}"
        );
        assert!(
            snap.contains("Status"),
            "header should contain Status column\nsnap:\n{snap}"
        );
        assert!(
            snap.contains("Duration"),
            "header should contain Duration column\nsnap:\n{snap}"
        );
        assert!(
            snap.contains("Profile"),
            "header should contain Profile column\nsnap:\n{snap}"
        );
    }

    #[test]
    fn paneled_selection_drives_output() {
        let state0 = RunsTabState {
            runs: sample_runs(3),
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "lp".to_owned(),
            log_scroll: 0,
        };
        let state2 = RunsTabState {
            selected_run: 2,
            ..state0.clone()
        };
        let pal = test_palette();
        let size = FrameSize {
            width: 80,
            height: 25,
        };
        let snap0 = render_runs_paneled(&state0, size, test_theme(), &pal, false).snapshot();
        let snap2 = render_runs_paneled(&state2, size, test_theme(), &pal, false).snapshot();

        // Run 0 output should appear when selected_run=0
        assert!(
            snap0.contains("line1 from run 0"),
            "snap0 should show run 0 output\nsnap:\n{snap0}"
        );
        // Run 2 output should appear when selected_run=2
        assert!(
            snap2.contains("line1 from run 2"),
            "snap2 should show run 2 output\nsnap:\n{snap2}"
        );
        // Run 0 output should NOT appear when selected_run=2
        assert!(
            !snap2.contains("line1 from run 0"),
            "snap2 should not show run 0 output\nsnap:\n{snap2}"
        );
    }

    #[test]
    fn paneled_output_applies_syntax_colors_to_tokens() {
        let mut runs = sample_runs(1);
        runs[0].output_lines =
            vec!["Tool: Bash(command=\"echo hi\", timeout=42s) path=/tmp/demo.txt".to_owned()];
        let state = RunsTabState {
            runs,
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "lp".to_owned(),
            log_scroll: 0,
        };
        let pal = test_palette();
        let frame = render_runs_paneled(
            &state,
            FrameSize {
                width: 110,
                height: 18,
            },
            test_theme(),
            &pal,
            false,
        );

        let mut output_row = None;
        for y in 0..18 {
            let row = frame.row_text(y);
            if let Some(x) = row.find("Tool: Bash(") {
                output_row = Some((x, y));
                break;
            }
        }
        let (start_x, row_y) = output_row.expect("output row should be rendered");

        let mut saw_non_primary_color = false;
        for x in start_x..(start_x + 56) {
            let Some(cell) = frame.cell(x, row_y) else {
                break;
            };
            if cell.style.fg != pal.text {
                saw_non_primary_color = true;
                break;
            }
        }
        assert!(
            saw_non_primary_color,
            "expected syntax-highlighted token colors on output row"
        );
    }

    #[test]
    fn paneled_scroll_offset_respected() {
        let mut runs = sample_runs(1);
        runs[0].output_lines = (0..50).map(|i| format!("output line {i}")).collect();
        let state = RunsTabState {
            runs,
            selected_run: 0,
            layer_label: "raw".to_owned(),
            loop_display_id: "lp".to_owned(),
            log_scroll: 10,
        };
        let pal = test_palette();
        let frame = render_runs_paneled(
            &state,
            FrameSize {
                width: 80,
                height: 25,
            },
            test_theme(),
            &pal,
            false,
        );
        let snap = frame.snapshot();
        assert!(
            snap.contains("scroll=10"),
            "scroll indicator should show scroll=10\nsnap:\n{snap}"
        );
    }

    #[test]
    fn paneled_narrow_width_no_panic() {
        let state = RunsTabState {
            runs: sample_runs(3),
            selected_run: 1,
            layer_label: "raw".to_owned(),
            loop_display_id: "loop-narrow".to_owned(),
            log_scroll: 0,
        };
        let pal = test_palette();
        // 30-wide render should not panic
        let frame = render_runs_paneled(
            &state,
            FrameSize {
                width: 30,
                height: 15,
            },
            test_theme(),
            &pal,
            false,
        );
        let snap = frame.snapshot();
        assert!(!snap.is_empty());
    }
}

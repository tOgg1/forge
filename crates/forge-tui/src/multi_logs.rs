//! Multi-logs tab rendering — grid of mini log panes.
//!
//! Parity port of Go `renderMultiLogsPane`, `renderMiniLogPane`,
//! `renderMiniLogEmptyPane`, and `renderLogBlock` from
//! `internal/looptui/looptui.go`.

use crate::app::{App, LogLayer, LogTailView, LoopView};
use crate::filter::loop_display_id;
use crate::layouts::{fit_pane_layout, layout_cell_size};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};

// ---------------------------------------------------------------------------
// Helper: truncate to width
// ---------------------------------------------------------------------------

fn truncate(s: &str, width: usize) -> String {
    if s.len() <= width {
        s.to_owned()
    } else {
        s[..width].to_owned()
    }
}

fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s[..width].to_owned()
    } else {
        format!("{s:<width$}")
    }
}

fn display_name(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.to_lowercase()
    }
}

// ---------------------------------------------------------------------------
// log_window_bounds — matching Go's logWindowBounds
// ---------------------------------------------------------------------------

/// Returns `(start, end, clamped_scroll)` for a window into `total_lines`.
pub fn log_window_bounds(
    total_lines: usize,
    available: usize,
    scroll: usize,
) -> (usize, usize, usize) {
    if total_lines == 0 {
        return (0, 0, 0);
    }
    let available = available.max(1);
    let max_scroll = total_lines.saturating_sub(1);
    let scroll = scroll.min(max_scroll);
    let end = total_lines.saturating_sub(scroll).min(total_lines);
    let start = end.saturating_sub(available);
    (start, end, scroll)
}

// ---------------------------------------------------------------------------
// render_log_block — simplified for mini panes (no layer filter/highlight)
// ---------------------------------------------------------------------------

fn render_log_block(
    lines: &[String],
    message: &str,
    width: usize,
    available: usize,
    _layer: LogLayer,
) -> Vec<String> {
    if available == 0 {
        return vec![];
    }
    if lines.is_empty() {
        let msg = if message.trim().is_empty() {
            "Log is empty."
        } else {
            message.trim()
        };
        return vec![truncate(msg, width)];
    }
    let (start, end, _) = log_window_bounds(lines.len(), available, 0);
    let window = &lines[start..end];
    if window.is_empty() {
        return vec![truncate("Log is empty.", width)];
    }
    window.iter().map(|line| truncate(line, width)).collect()
}

// ---------------------------------------------------------------------------
// App methods for multi-logs tab rendering
// ---------------------------------------------------------------------------

impl App {
    /// Get loop views for the current multi-logs page, matching Go's `multiPageTargets`.
    ///
    /// Returns `(targets, page, total_pages, start, end)`.
    #[must_use]
    pub fn multi_page_targets(&self) -> (Vec<&LoopView>, usize, usize, usize, usize) {
        let ordered = self.ordered_multi_target_views();
        let page_size = self.multi_page_size();
        let (page, total_pages, start, end) =
            crate::app::multi_page_bounds(ordered.len(), page_size, self.multi_page());
        if start >= ordered.len() {
            return (vec![], page, total_pages, start, end);
        }
        let targets = ordered[start..end].to_vec();
        (targets, page, total_pages, start, end)
    }

    /// Get the IDs of loops visible on the current multi-logs page.
    /// Used by the host to know which log tails to fetch.
    #[must_use]
    pub fn multi_target_ids(&self) -> Vec<String> {
        let (targets, _, _, _, _) = self.multi_page_targets();
        targets.iter().map(|v| v.id.clone()).collect()
    }

    /// Render the multi-logs pane into a `RenderFrame`.
    #[must_use]
    pub fn render_multi_logs_pane(&self, width: usize, height: usize) -> RenderFrame {
        let width = width.max(1);
        let height = height.max(1);
        let theme = crate::default_theme();
        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);

        let header_rows = self.multi_header_rows().max(1);
        let cell_gap = self.multi_cell_gap();
        let min_cell_width = self.multi_min_cell_width();
        let min_cell_height = self.multi_min_cell_height();
        let grid_height = ((height as i32) - header_rows).max(min_cell_height);
        let requested = self.current_layout();
        let layout = fit_pane_layout(
            requested,
            width as i32,
            grid_height,
            cell_gap,
            min_cell_width,
            min_cell_height,
        );
        let (cell_w, cell_h) = layout_cell_size(layout, width as i32, grid_height, cell_gap);
        let cell_w = cell_w.max(1) as usize;
        let cell_h = cell_h.max(1) as usize;

        let (targets, page, total_pages, start, end) = self.multi_page_targets();
        let total_targets = self.ordered_multi_target_views().len();

        if total_targets == 0 {
            frame.draw_text(
                0,
                0,
                "No loops selected. Pin with <space> or create loops.",
                TextRole::Muted,
            );
            return frame;
        }
        if targets.is_empty() {
            frame.draw_text(
                0,
                0,
                "No loops on this page. Use ,/. or g/G.",
                TextRole::Muted,
            );
            return frame;
        }

        // Header line.
        let header = truncate(
            &format!(
                "View 4 Matrix  requested={} effective={}  page={}/{}  showing={}-{}/{}",
                requested.label(),
                layout.label(),
                page + 1,
                total_pages,
                start + 1,
                end,
                total_targets,
            ),
            width,
        );
        frame.draw_text(0, 0, &header, TextRole::Accent);

        if header_rows > 1 {
            // Subheader line.
            let subheader = truncate(
                &format!(
                    "layer:{}  pin:<space> clear:c  layout:m  page:,/. g/G  order:pinned first",
                    self.log_layer().label(),
                ),
                width,
            );
            frame.draw_text(0, 1, &subheader, TextRole::Muted);
        }

        // Grid of mini panes.
        let header_rows = header_rows as usize;
        let gap = cell_gap.max(0) as usize;
        let mut index = 0;

        for row in 0..layout.rows as usize {
            let y_base = header_rows + row * (cell_h + gap);
            for col in 0..layout.cols as usize {
                let x_base = col * (cell_w + gap);
                let mini = if index < targets.len() {
                    self.render_mini_log_pane(targets[index], cell_w, cell_h)
                } else {
                    Self::render_mini_log_empty_pane(cell_w, cell_h)
                };
                // Blit mini frame into main frame.
                let mini_size = mini.size();
                for my in 0..mini_size.height {
                    for mx in 0..mini_size.width {
                        if let Some(cell) = mini.cell(mx, my) {
                            frame.set_cell(x_base + mx, y_base + my, cell);
                        }
                    }
                }
                index += 1;
            }
        }

        frame
    }

    /// Render a single mini log pane for one loop, matching Go's `renderMiniLogPane`.
    fn render_mini_log_pane(&self, view: &LoopView, width: usize, height: usize) -> RenderFrame {
        let theme = crate::default_theme();
        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);

        // Border color based on loop state — reflected in header role.
        let header_role = match view.state.as_str() {
            "running" => TextRole::Success,
            "error" => TextRole::Danger,
            "waiting" | "sleeping" => TextRole::Accent,
            _ => TextRole::Primary,
        };

        // Header: display ID + name + [PIN].
        let display_id = loop_display_id(&view.id, &view.short_id);
        let mut header_text = format!("{} {}", display_id, view.name);
        if self.is_pinned(&view.id) {
            header_text.push_str(" [PIN]");
        }
        let header_text = pad_right(&truncate(&header_text, width), width);
        frame.draw_text(0, 0, &header_text, header_role);

        // Meta line: STATUS harness=X runs=N.
        let status_upper = view.state.to_uppercase();
        let harness_display = display_name(&view.profile_harness, "-");
        let meta = truncate(
            &format!(
                "{:<8} harness={} runs={}",
                status_upper, harness_display, view.runs
            ),
            width,
        );
        frame.draw_text(0, 1, &meta, TextRole::Muted);

        // Separator line.
        let sep: String = "-".repeat(width);
        frame.draw_text(0, 2, &sep, TextRole::Muted);

        // Log block: fill remaining height.
        let log_start = 3;
        let log_available = height.saturating_sub(log_start).max(1);

        let tail = self.multi_logs().get(&view.id);
        let empty_tail = LogTailView::default();
        let tail = tail.unwrap_or(&empty_tail);

        let log_lines = render_log_block(
            &tail.lines,
            &tail.message,
            width,
            log_available,
            self.log_layer(),
        );

        for (i, line) in log_lines.iter().enumerate() {
            if i >= log_available {
                break;
            }
            frame.draw_text(0, log_start + i, line, TextRole::Primary);
        }

        frame
    }

    /// Render an empty mini pane (placeholder), matching Go's `renderMiniLogEmptyPane`.
    fn render_mini_log_empty_pane(width: usize, height: usize) -> RenderFrame {
        let theme = crate::default_theme();
        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);

        let header = pad_right("empty", width);
        frame.draw_text(0, 0, &header, TextRole::Muted);
        if height > 1 {
            frame.draw_text(
                0,
                1,
                &truncate("Pin loops with <space>.", width),
                TextRole::Muted,
            );
        }
        if height > 2 {
            frame.draw_text(
                0,
                2,
                &truncate("Change layout with m.", width),
                TextRole::Muted,
            );
        }
        frame
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::app::{App, LogTailView, LoopView, MainTab};
    use crate::layouts::layout_index_for;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use std::collections::HashMap;

    fn key(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent::plain(k))
    }

    fn sample_loops(n: usize) -> Vec<LoopView> {
        (0..n)
            .map(|i| LoopView {
                id: format!("loop-{i}"),
                name: format!("test-loop-{i}"),
                state: match i % 3 {
                    0 => "running".to_owned(),
                    1 => "stopped".to_owned(),
                    _ => "error".to_owned(),
                },
                repo_path: format!("/repo/{i}"),
                runs: i * 2,
                profile_harness: "claude-code".to_owned(),
                ..Default::default()
            })
            .collect()
    }

    fn multi_app(n: usize) -> App {
        let mut app = App::new("default", 12);
        app.set_loops(sample_loops(n));
        app.set_tab(MainTab::MultiLogs);
        app
    }

    // -- log_window_bounds --

    #[test]
    fn log_window_bounds_basic() {
        let (start, end, scroll) = log_window_bounds(100, 10, 0);
        assert_eq!(start, 90);
        assert_eq!(end, 100);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn log_window_bounds_with_scroll() {
        let (start, end, scroll) = log_window_bounds(100, 10, 15);
        assert_eq!(start, 75);
        assert_eq!(end, 85);
        assert_eq!(scroll, 15);
    }

    #[test]
    fn log_window_bounds_empty() {
        let (start, end, scroll) = log_window_bounds(0, 10, 0);
        assert_eq!(start, 0);
        assert_eq!(end, 0);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn log_window_bounds_clamps_scroll() {
        let (start, end, scroll) = log_window_bounds(5, 10, 999);
        assert_eq!(start, 0);
        assert_eq!(end, 1);
        assert_eq!(scroll, 4);
    }

    // -- render_log_block --

    #[test]
    fn render_log_block_empty_shows_message() {
        let result = render_log_block(&[], "", 40, 5, LogLayer::Raw);
        assert_eq!(result, vec!["Log is empty."]);
    }

    #[test]
    fn render_log_block_custom_message() {
        let result = render_log_block(&[], "  Loading...  ", 40, 5, LogLayer::Raw);
        assert_eq!(result, vec!["Loading..."]);
    }

    #[test]
    fn render_log_block_shows_lines() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {i}")).collect();
        let result = render_log_block(&lines, "", 40, 3, LogLayer::Raw);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "line 7");
        assert_eq!(result[2], "line 9");
    }

    #[test]
    fn render_log_block_truncates_width() {
        let lines = vec!["a".repeat(100)];
        let result = render_log_block(&lines, "", 20, 5, LogLayer::Raw);
        assert_eq!(result[0].len(), 20);
    }

    // -- multi_page_targets --

    #[test]
    fn multi_page_targets_basic() {
        let app = multi_app(6);
        let (targets, page, total_pages, start, end) = app.multi_page_targets();
        assert!(!targets.is_empty());
        assert_eq!(page, 0);
        assert!(total_pages >= 1);
        assert_eq!(start, 0);
        assert!(end <= 6);
    }

    #[test]
    fn multi_target_ids_returns_loop_ids() {
        let app = multi_app(3);
        let ids = app.multi_target_ids();
        assert!(ids.contains(&"loop-0".to_owned()));
        assert!(ids.contains(&"loop-1".to_owned()));
        assert!(ids.contains(&"loop-2".to_owned()));
    }

    // -- paging keys --

    #[test]
    fn multi_logs_paging_keys() {
        let mut app = App::new("default", 12);
        app.set_loops(sample_loops(3));
        app.set_tab(MainTab::MultiLogs);
        app.set_layout_idx(layout_index_for(1, 1));

        // Page forward.
        app.update(key(Key::Char('.')));
        assert_eq!(app.multi_page(), 1);

        app.update(key(Key::Char('.')));
        assert_eq!(app.multi_page(), 2);

        // Page back.
        app.update(key(Key::Char(',')));
        assert_eq!(app.multi_page(), 1);

        // First/last page (vim-style) for adapter-key compatibility.
        app.update(key(Key::Char('g')));
        assert_eq!(app.multi_page(), 0);

        app.update(key(Key::Char('G')));
        assert_eq!(app.multi_page(), 2);
    }

    #[test]
    fn set_tab_multi_enables_zen_by_default() {
        let mut app = App::new("default", 12);
        app.set_tab(MainTab::MultiLogs);
        assert!(app.focus_right());
        app.set_tab(MainTab::Overview);
        assert!(!app.focus_right());
    }

    // -- render_multi_logs_pane --

    #[test]
    fn render_multi_logs_pane_empty_loops() {
        let app = App::new("default", 12);
        let frame = app.render_multi_logs_pane(80, 30);
        let snapshot = frame.snapshot();
        assert!(
            snapshot.contains("No loops selected"),
            "expected empty message, got:\n{snapshot}"
        );
    }

    #[test]
    fn render_multi_logs_pane_has_header() {
        let app = multi_app(4);
        let frame = app.render_multi_logs_pane(120, 40);
        let header = frame.row_text(0);
        assert!(
            header.contains("View 4 Matrix"),
            "expected header, got: {header}"
        );
        assert!(header.contains("page="), "expected page info in header");
    }

    #[test]
    fn render_multi_logs_pane_has_subheader() {
        let app = multi_app(4);
        let frame = app.render_multi_logs_pane(120, 40);
        let subheader = frame.row_text(1);
        assert!(
            subheader.contains("layer:raw"),
            "expected subheader, got: {subheader}"
        );
        assert!(subheader.contains("pin:<space>"));
    }

    #[test]
    fn render_multi_logs_pane_shows_loop_names() {
        let app = multi_app(4);
        let frame = app.render_multi_logs_pane(120, 40);
        let snapshot = frame.snapshot();
        assert!(
            snapshot.contains("test-loop-0"),
            "expected loop name in grid:\n{snapshot}"
        );
    }

    #[test]
    fn render_multi_logs_pane_with_logs() {
        let mut app = multi_app(2);
        let mut logs = HashMap::new();
        logs.insert(
            "loop-0".to_owned(),
            LogTailView {
                lines: vec!["hello from loop-0".to_owned()],
                message: String::new(),
            },
        );
        app.set_multi_logs(logs);

        let frame = app.render_multi_logs_pane(120, 40);
        let snapshot = frame.snapshot();
        assert!(
            snapshot.contains("hello from loop-0"),
            "expected log content:\n{snapshot}"
        );
    }

    // -- render_mini_log_pane --

    #[test]
    fn mini_pane_shows_id_name_state() {
        let app = multi_app(1);
        let view = &app.filtered()[0].clone();
        let frame = app.render_mini_log_pane(view, 40, 10);
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("loop-0"));
        assert!(snapshot.contains("test-loop-0"));
        assert!(snapshot.contains("RUNNING"));
    }

    #[test]
    fn mini_pane_shows_pin_marker() {
        let mut app = multi_app(1);
        app.toggle_pinned("loop-0");
        let view = &app.filtered()[0].clone();
        let frame = app.render_mini_log_pane(view, 40, 10);
        let snapshot = frame.snapshot();
        assert!(
            snapshot.contains("[PIN]"),
            "expected [PIN] marker:\n{snapshot}"
        );
    }

    #[test]
    fn mini_pane_shows_separator() {
        let app = multi_app(1);
        let view = &app.filtered()[0].clone();
        let frame = app.render_mini_log_pane(view, 40, 10);
        let row2 = frame.row_text(2);
        assert!(row2.contains("---"), "expected separator line, got: {row2}");
    }

    // -- render_mini_log_empty_pane --

    #[test]
    fn empty_pane_shows_instructions() {
        let frame = App::render_mini_log_empty_pane(40, 5);
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("empty"));
        assert!(snapshot.contains("Pin loops with <space>"));
        assert!(snapshot.contains("Change layout with m"));
    }

    // -- integration: multi-logs tab renders in main frame --

    #[test]
    fn multi_logs_tab_renders_grid_in_main_frame() {
        let mut app = multi_app(4);
        // Set size large enough for multi-logs.
        app.update(InputEvent::Resize(forge_ftui_adapter::input::ResizeEvent {
            width: 160,
            height: 50,
        }));
        let frame = app.render();
        let snapshot = frame.snapshot();
        // Should contain the tab bar and multi-logs content.
        assert!(
            snapshot.contains("Multi Logs"),
            "expected Multi Logs tab active:\n{}",
            &snapshot[..snapshot.len().min(500)]
        );
    }

    // -- snapshot: full multi-logs pane --

    #[test]
    fn multi_logs_pane_snapshot_2x2() {
        let mut app = App::new("default", 12);
        app.set_loops(sample_loops(4));
        app.set_tab(MainTab::MultiLogs);
        // Default layout is 2x2.

        let mut logs = HashMap::new();
        for i in 0..4 {
            logs.insert(
                format!("loop-{i}"),
                LogTailView {
                    lines: vec![format!("output from loop-{i}")],
                    message: String::new(),
                },
            );
        }
        app.set_multi_logs(logs);

        let frame = app.render_multi_logs_pane(100, 30);
        let snapshot = frame.snapshot();

        // Verify header.
        assert!(snapshot.contains("View 4 Matrix"));
        // Verify all 4 loops appear.
        for i in 0..4 {
            assert!(
                snapshot.contains(&format!("test-loop-{i}")),
                "expected test-loop-{i} in snapshot:\n{snapshot}"
            );
            assert!(
                snapshot.contains(&format!("output from loop-{i}")),
                "expected output from loop-{i} in snapshot:\n{snapshot}"
            );
        }
    }
}

//! Overview tab parity helpers.
//!
//! Mirrors the Go `internal/looptui` overview pane content: loop metadata
//! lines + a small run-status snapshot.

use forge_ftui_adapter::render::TextRole;

use crate::app::{LoopView, RunView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewLine {
    pub text: String,
    pub role: TextRole,
}

fn truncate_line(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= width {
        return text.to_owned();
    }
    if width <= 3 {
        return text.chars().take(width).collect();
    }
    let mut out: String = text.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

fn display_name(name: &str, fallback: &str) -> String {
    let name = name.trim();
    if !name.is_empty() {
        return name.to_owned();
    }
    let fallback = fallback.trim();
    if !fallback.is_empty() {
        return fallback.to_owned();
    }
    "-".to_owned()
}

fn loop_display_id(view: &LoopView) -> String {
    let short = view.short_id.trim();
    if !short.is_empty() {
        return short.to_owned();
    }
    if view.id.chars().count() <= 8 {
        return view.id.clone();
    }
    view.id.chars().take(8).collect()
}

fn short_run_id(id: &str) -> String {
    if id.chars().count() <= 8 {
        return id.to_owned();
    }
    id.chars().take(8).collect()
}

fn format_iterations(max: i64) -> String {
    if max <= 0 {
        return "unlimited".to_owned();
    }
    max.to_string()
}

fn format_duration_seconds(seconds: i64) -> String {
    if seconds <= 0 {
        return "-".to_owned();
    }
    let mut remaining = seconds as u64;
    let hours = remaining / 3600;
    remaining %= 3600;
    let minutes = remaining / 60;
    let secs = remaining % 60;
    if hours > 0 {
        format!("{hours}h{minutes}m{secs}s")
    } else if minutes > 0 {
        format!("{minutes}m{secs}s")
    } else {
        format!("{secs}s")
    }
}

fn format_time(value: Option<&str>) -> String {
    value.unwrap_or("-").to_owned()
}

fn run_exit_code(run: &RunView) -> String {
    run.exit_code
        .map_or_else(|| "-".to_owned(), |code| code.to_string())
}

fn format_run_duration(run: &RunView) -> String {
    let trimmed = run.duration.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    if run.status.trim().eq_ignore_ascii_case("running") {
        return "running".to_owned();
    }
    "-".to_owned()
}

#[derive(Debug, Clone, Copy, Default)]
struct RunCounts {
    success: usize,
    error: usize,
    killed: usize,
    running: usize,
}

fn count_runs(run_history: &[RunView]) -> RunCounts {
    let mut counts = RunCounts::default();
    for run in run_history {
        let status = run.status.trim().to_ascii_lowercase();
        match status.as_str() {
            "success" => counts.success += 1,
            "error" => counts.error += 1,
            "killed" => counts.killed += 1,
            "running" => counts.running += 1,
            _ => {}
        }
    }
    counts
}

fn trim_to_height<T: Clone>(items: &[T], height: usize) -> Vec<T> {
    if height == 0 || items.len() <= height {
        return items.to_vec();
    }
    items[..height].to_vec()
}

#[must_use]
pub fn overview_pane_lines(
    selected_loop: Option<&LoopView>,
    run_history: &[RunView],
    selected_run: usize,
    width: usize,
    height: usize,
) -> Vec<OverviewLine> {
    let content_width = width.saturating_sub(2).max(1);

    let mut out: Vec<OverviewLine> = Vec::with_capacity(32);
    let mut push = |role: TextRole, text: String| {
        out.push(OverviewLine {
            text: truncate_line(&text, content_width),
            role,
        });
    };

    let Some(loop_view) = selected_loop else {
        push(TextRole::Primary, "No loop selected.".to_owned());
        push(
            TextRole::Primary,
            "Use j/k or arrow keys to choose a loop.".to_owned(),
        );
        push(
            TextRole::Primary,
            "Start one: forge up --count 1".to_owned(),
        );
        push(TextRole::Primary, String::new());
        push(
            TextRole::Muted,
            "Workflow: 2=Logs (deep scroll) | 3=Runs | 4=Multi Logs".to_owned(),
        );
        return trim_to_height(&out, height.max(1));
    };

    push(
        TextRole::Primary,
        format!("ID: {}", loop_display_id(loop_view)),
    );
    push(TextRole::Primary, format!("Name: {}", loop_view.name));
    push(
        TextRole::Primary,
        format!("Status: {}", loop_view.state.trim().to_ascii_uppercase()),
    );
    push(TextRole::Primary, format!("Runs: {}", loop_view.runs));
    push(TextRole::Primary, format!("Dir: {}", loop_view.repo_path));
    push(
        TextRole::Primary,
        format!(
            "Pool: {}",
            display_name(&loop_view.pool_name, &loop_view.pool_id)
        ),
    );
    push(
        TextRole::Primary,
        format!(
            "Profile: {}",
            display_name(&loop_view.profile_name, &loop_view.profile_id)
        ),
    );
    push(
        TextRole::Primary,
        format!(
            "Harness/Auth: {} / {}",
            display_name(&loop_view.profile_harness, "-"),
            display_name(&loop_view.profile_auth, "-")
        ),
    );
    push(
        TextRole::Primary,
        format!(
            "Last Run: {}",
            format_time(loop_view.last_run_at.as_deref())
        ),
    );
    push(
        TextRole::Primary,
        format!("Queue Depth: {}", loop_view.queue_depth),
    );
    push(
        TextRole::Primary,
        format!(
            "Interval: {}",
            format_duration_seconds(loop_view.interval_seconds)
        ),
    );
    push(
        TextRole::Primary,
        format!(
            "Max Runtime: {}",
            format_duration_seconds(loop_view.max_runtime_seconds)
        ),
    );
    push(
        TextRole::Primary,
        format!(
            "Max Iterations: {}",
            format_iterations(loop_view.max_iterations)
        ),
    );
    if !loop_view.last_error.trim().is_empty() {
        push(
            TextRole::Primary,
            format!("Last Error: {}", loop_view.last_error),
        );
    }

    let counts = count_runs(run_history);
    push(TextRole::Primary, String::new());
    push(TextRole::Muted, "Run snapshot:".to_owned());
    push(
        TextRole::Primary,
        format!(
            "  total={} success={} error={} killed={} running={}",
            run_history.len(),
            counts.success,
            counts.error,
            counts.killed,
            counts.running
        ),
    );
    if !run_history.is_empty() {
        let idx = selected_run.min(run_history.len().saturating_sub(1));
        let run = &run_history[idx];
        push(
            TextRole::Primary,
            format!(
                "  latest={} status={} exit={} duration={}",
                short_run_id(&run.id),
                run.status.trim().to_ascii_uppercase(),
                run_exit_code(run),
                format_run_duration(run)
            ),
        );
    }

    push(TextRole::Primary, String::new());
    push(
        TextRole::Muted,
        "Workflow: 2=Logs (deep scroll) | 3=Runs | 4=Multi Logs".to_owned(),
    );

    trim_to_height(&out, height.max(1))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use forge_ftui_adapter::render::{FrameSize, RenderFrame};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;

    fn frame_from_lines(lines: &[OverviewLine], width: usize, height: usize) -> RenderFrame {
        let theme = crate::default_theme();
        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
        for (i, line) in lines.iter().enumerate() {
            if i >= height {
                break;
            }
            frame.draw_text(0, i, &line.text, line.role);
        }
        frame
    }

    #[test]
    fn no_selection_guidance_matches_go_shape() {
        let lines = overview_pane_lines(None, &[], 0, 40, 8);
        assert_eq!(lines[0].text.trim(), "No loop selected.");
        assert!(lines[2].text.contains("forge up --count 1"));
    }

    #[test]
    fn snapshot_overview_pane_content() {
        let view = LoopView {
            id: "abcdef1234567890".to_owned(),
            short_id: "abc12345".to_owned(),
            name: "demo-loop".to_owned(),
            state: "running".to_owned(),
            repo_path: "/repo/demo".to_owned(),
            runs: 7,
            queue_depth: 3,
            last_run_at: Some("2026-02-09T20:00:00Z".to_owned()),
            interval_seconds: 60,
            max_runtime_seconds: 3600,
            max_iterations: 0,
            last_error: "boom".to_owned(),
            pool_name: "".to_owned(),
            pool_id: "pool-1".to_owned(),
            profile_name: "dev".to_owned(),
            profile_id: "profile-1".to_owned(),
            profile_harness: "".to_owned(),
            profile_auth: "ssh".to_owned(),
        };

        let history = vec![
            RunView {
                id: "run-000000001".to_owned(),
                status: "success".to_owned(),
                exit_code: Some(0),
                duration: "12s".to_owned(),
                ..Default::default()
            },
            RunView {
                id: "run-000000002".to_owned(),
                status: "error".to_owned(),
                exit_code: Some(1),
                duration: "3s".to_owned(),
                ..Default::default()
            },
            RunView {
                id: "run-000000003".to_owned(),
                status: "killed".to_owned(),
                exit_code: None,
                duration: "1s".to_owned(),
                ..Default::default()
            },
            RunView {
                id: "run-000000004".to_owned(),
                status: "running".to_owned(),
                exit_code: None,
                duration: "running".to_owned(),
                ..Default::default()
            },
        ];

        let lines = overview_pane_lines(Some(&view), &history, 1, 60, 20);
        let frame = frame_from_lines(&lines, 60, 20);
        assert_render_frame_snapshot(
            "forge_loop_overview_pane",
            &frame,
            "ID: abc12345                                                \nName: demo-loop                                             \nStatus: RUNNING                                             \nRuns: 7                                                     \nDir: /repo/demo                                             \nPool: pool-1                                                \nProfile: dev                                                \nHarness/Auth: - / ssh                                       \nLast Run: 2026-02-09T20:00:00Z                              \nQueue Depth: 3                                              \nInterval: 1m0s                                              \nMax Runtime: 1h0m0s                                         \nMax Iterations: unlimited                                   \nLast Error: boom                                            \n                                                            \nRun snapshot:                                               \n  total=4 success=1 error=1 killed=1 running=1              \n  latest=run-0000 status=ERROR exit=1 duration=3s           \n                                                            \nWorkflow: 2=Logs (deep scroll) | 3=Runs | 4=Multi Logs      ",
        );
    }
}

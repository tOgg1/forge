//! Dashboard hero widgets: throughput gauge, queue pressure bar, error radar.
//!
//! These render as compact, high-signal widget lines at the top of the
//! Overview pane, turning the first screen from a table dump into a
//! command-center feel.

use forge_ftui_adapter::render::TextRole;
use std::collections::BTreeMap;

use crate::app::{LoopView, RunView};

// ---------------------------------------------------------------------------
// Public output
// ---------------------------------------------------------------------------

/// A single styled line emitted by a hero widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroLine {
    pub text: String,
    pub role: TextRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    Up,
    Down,
    Flat,
}

impl TrendDirection {
    #[must_use]
    pub fn glyph(self) -> char {
        match self {
            Self::Up => '↑',
            Self::Down => '↓',
            Self::Flat => '→',
        }
    }
}

impl Default for TrendDirection {
    fn default() -> Self {
        Self::Flat
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThroughputMeter {
    pub runs_per_min: usize,
    pub messages_per_min: usize,
    pub errors_per_min: usize,
    pub runs_trend: TrendDirection,
    pub messages_trend: TrendDirection,
    pub errors_trend: TrendDirection,
    pub runs_sparkline: String,
}

/// Aggregate fleet snapshot powering the hero widgets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FleetSnapshot {
    pub total_loops: usize,
    pub running_loops: usize,
    pub sleeping_loops: usize,
    pub stopped_loops: usize,
    pub error_loops: usize,

    pub total_runs: usize,
    pub success_runs: usize,
    pub error_runs: usize,
    pub killed_runs: usize,
    pub running_runs: usize,

    pub total_queue_depth: usize,
    pub max_queue_depth: usize,
    pub loops_with_queue: usize,

    pub loops_with_errors: usize,
    pub total_error_lines: usize,

    pub throughput: ThroughputMeter,
}

// ---------------------------------------------------------------------------
// Computation
// ---------------------------------------------------------------------------

/// Build a fleet snapshot from all visible loops and the selected loop's run history.
#[must_use]
pub fn build_fleet_snapshot(loops: &[LoopView], run_history: &[RunView]) -> FleetSnapshot {
    let mut snap = FleetSnapshot {
        total_loops: loops.len(),
        ..Default::default()
    };

    for lv in loops {
        let state = lv.state.trim().to_ascii_lowercase();
        match state.as_str() {
            "running" => snap.running_loops += 1,
            "sleeping" | "waiting" => snap.sleeping_loops += 1,
            "stopped" => snap.stopped_loops += 1,
            "error" => snap.error_loops += 1,
            _ => {}
        }

        snap.total_queue_depth += lv.queue_depth;
        if lv.queue_depth > snap.max_queue_depth {
            snap.max_queue_depth = lv.queue_depth;
        }
        if lv.queue_depth > 0 {
            snap.loops_with_queue += 1;
        }

        if !lv.last_error.trim().is_empty() {
            snap.loops_with_errors += 1;
            snap.total_error_lines += 1;
        }
    }

    snap.total_runs = run_history.len();
    for run in run_history {
        let status = run.status.trim().to_ascii_lowercase();
        match status.as_str() {
            "success" => snap.success_runs += 1,
            "error" => snap.error_runs += 1,
            "killed" => snap.killed_runs += 1,
            "running" => snap.running_runs += 1,
            _ => {}
        }
    }

    snap.throughput = compute_throughput_meter(run_history);
    snap
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Unicode block characters for gauge bars (eighth-blocks, light to full).
const BAR_CHARS: [char; 8] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
];

/// Build a compact gauge bar of `width` characters from a ratio 0.0..=1.0.
fn gauge_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let filled_exact = clamped * width as f64;
    let full_blocks = filled_exact as usize;
    let remainder = filled_exact - full_blocks as f64;

    let mut out = String::with_capacity(width);
    for _ in 0..full_blocks.min(width) {
        out.push('\u{2588}'); // full block
    }
    if full_blocks < width {
        let frac_index = (remainder * 7.0) as usize;
        out.push(BAR_CHARS[frac_index.min(7)]);
    }
    while out.chars().count() < width {
        out.push(' ');
    }
    out
}

/// Render throughput hero widget lines.
fn render_throughput(snap: &FleetSnapshot, width: usize) -> Vec<HeroLine> {
    let content_w = width.saturating_sub(2).max(1);
    let mut lines = Vec::with_capacity(3);

    // Title line
    let title = format!(
        "\u{25B6} THROUGHPUT  {} runs | {} ok | {} err | {} killed",
        snap.total_runs, snap.success_runs, snap.error_runs, snap.killed_runs
    );
    lines.push(HeroLine {
        text: truncate(&title, content_w),
        role: TextRole::Accent,
    });

    // Success rate gauge
    let success_rate = if snap.total_runs > 0 {
        snap.success_runs as f64 / snap.total_runs as f64
    } else {
        0.0
    };
    let bar_width = 20.min(content_w.saturating_sub(16));
    let bar = gauge_bar(success_rate, bar_width);
    let pct = (success_rate * 100.0) as u32;
    let gauge_line = format!("  success [{bar}] {pct}%");
    let role = if pct >= 80 {
        TextRole::Success
    } else if pct >= 50 {
        TextRole::Accent
    } else {
        TextRole::Danger
    };
    lines.push(HeroLine {
        text: truncate(&gauge_line, content_w),
        role,
    });

    let meter = &snap.throughput;
    let meter_line = format!(
        "  rate r/m {}{}  m/m {}{}  e/m {}{}  {}",
        meter.runs_per_min,
        meter.runs_trend.glyph(),
        meter.messages_per_min,
        meter.messages_trend.glyph(),
        meter.errors_per_min,
        meter.errors_trend.glyph(),
        meter.runs_sparkline
    );
    let meter_role = if meter.errors_per_min >= 2 {
        TextRole::Danger
    } else if meter.errors_per_min == 1 || meter.runs_trend == TrendDirection::Down {
        TextRole::Warning
    } else {
        TextRole::Success
    };
    lines.push(HeroLine {
        text: truncate(&meter_line, content_w),
        role: meter_role,
    });

    lines
}

/// Render queue pressure hero widget lines.
fn render_queue_pressure(snap: &FleetSnapshot, width: usize) -> Vec<HeroLine> {
    let content_w = width.saturating_sub(2).max(1);
    let mut lines = Vec::with_capacity(3);

    let title = format!(
        "\u{25C6} QUEUE PRESSURE  depth:{} across {} loop(s) | peak:{}",
        snap.total_queue_depth, snap.loops_with_queue, snap.max_queue_depth
    );
    lines.push(HeroLine {
        text: truncate(&title, content_w),
        role: TextRole::Accent,
    });

    // Pressure indicator
    let pressure_level = if snap.total_queue_depth == 0 {
        "idle"
    } else if snap.total_queue_depth <= 3 {
        "low"
    } else if snap.total_queue_depth <= 10 {
        "moderate"
    } else if snap.total_queue_depth <= 25 {
        "high"
    } else {
        "critical"
    };
    let bar_width = 20.min(content_w.saturating_sub(20));
    let ratio = if snap.total_loops > 0 {
        (snap.total_queue_depth as f64 / (snap.total_loops as f64 * 5.0)).min(1.0)
    } else {
        0.0
    };
    let bar = gauge_bar(ratio, bar_width);
    let pressure_line = format!("  pressure [{bar}] {pressure_level}");
    let role = match pressure_level {
        "idle" | "low" => TextRole::Success,
        "moderate" => TextRole::Accent,
        _ => TextRole::Danger,
    };
    lines.push(HeroLine {
        text: truncate(&pressure_line, content_w),
        role,
    });

    lines
}

/// Render error radar hero widget lines.
fn render_error_radar(snap: &FleetSnapshot, width: usize) -> Vec<HeroLine> {
    let content_w = width.saturating_sub(2).max(1);
    let mut lines = Vec::with_capacity(3);

    let title = format!(
        "\u{26A0} ERROR RADAR  {} loop(s) w/ errors | {} error run(s) | {} in error state",
        snap.loops_with_errors, snap.error_runs, snap.error_loops
    );
    lines.push(HeroLine {
        text: truncate(&title, content_w),
        role: TextRole::Accent,
    });

    // Fleet health summary
    let healthy = snap
        .total_loops
        .saturating_sub(snap.error_loops)
        .saturating_sub(snap.stopped_loops);
    let health_pct = if snap.total_loops > 0 {
        (healthy as f64 / snap.total_loops as f64 * 100.0) as u32
    } else {
        100
    };
    let bar_width = 20.min(content_w.saturating_sub(22));
    let ratio = if snap.total_loops > 0 {
        healthy as f64 / snap.total_loops as f64
    } else {
        1.0
    };
    let bar = gauge_bar(ratio, bar_width);
    let health_line = format!("  fleet ok [{bar}] {health_pct}%");
    let role = if health_pct >= 80 {
        TextRole::Success
    } else if health_pct >= 50 {
        TextRole::Accent
    } else {
        TextRole::Danger
    };
    lines.push(HeroLine {
        text: truncate(&health_line, content_w),
        role,
    });

    lines
}

/// Render the fleet status bar (top-line loop state summary).
fn render_fleet_status(snap: &FleetSnapshot, width: usize) -> HeroLine {
    let content_w = width.saturating_sub(2).max(1);
    let text = format!(
        "\u{2593} FLEET  {} loops | \u{25CF}{} run \u{25CB}{} sleep \u{25A0}{} stop \u{2716}{} err",
        snap.total_loops,
        snap.running_loops,
        snap.sleeping_loops,
        snap.stopped_loops,
        snap.error_loops,
    );
    HeroLine {
        text: truncate(&text, content_w),
        role: TextRole::Muted,
    }
}

/// Build all hero widget lines for the overview dashboard header.
///
/// Returns styled lines that should be rendered above the selected-loop detail
/// in the overview pane. Designed to fit in a compact vertical space while
/// giving command-center at-a-glance visibility.
#[must_use]
pub fn hero_widget_lines(
    loops: &[LoopView],
    run_history: &[RunView],
    width: usize,
) -> Vec<HeroLine> {
    let snap = build_fleet_snapshot(loops, run_history);
    let mut lines = Vec::with_capacity(12);

    // Fleet status bar
    lines.push(render_fleet_status(&snap, width));
    lines.push(HeroLine {
        text: String::new(),
        role: TextRole::Primary,
    });

    // Three hero widgets side by side conceptually, stacked vertically
    lines.extend(render_throughput(&snap, width));
    lines.push(HeroLine {
        text: String::new(),
        role: TextRole::Primary,
    });
    lines.extend(render_queue_pressure(&snap, width));
    lines.push(HeroLine {
        text: String::new(),
        role: TextRole::Primary,
    });
    lines.extend(render_error_radar(&snap, width));
    lines.push(HeroLine {
        text: String::new(),
        role: TextRole::Primary,
    });

    // Separator
    let sep = "\u{2500}".repeat(width.saturating_sub(2).min(60));
    lines.push(HeroLine {
        text: sep,
        role: TextRole::Muted,
    });

    lines
}

fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    if max <= 3 {
        return text.chars().take(max).collect();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

#[derive(Debug, Clone, Copy, Default)]
struct MinuteBucket {
    runs: usize,
    messages: usize,
    errors: usize,
}

fn compute_throughput_meter(run_history: &[RunView]) -> ThroughputMeter {
    let mut buckets: BTreeMap<String, MinuteBucket> = BTreeMap::new();
    for run in run_history {
        let Some(minute_key) = minute_bucket_key(&run.started_at) else {
            continue;
        };
        let entry = buckets.entry(minute_key).or_default();
        entry.runs = entry.runs.saturating_add(1);
        entry.messages = entry.messages.saturating_add(run.output_lines.len());
        if is_error_status(&run.status) {
            entry.errors = entry.errors.saturating_add(1);
        }
    }

    if buckets.is_empty() {
        return ThroughputMeter::default();
    }

    let values = buckets.values().copied().collect::<Vec<_>>();
    let current = values.last().copied().unwrap_or_default();
    let previous = if values.len() >= 2 {
        values[values.len() - 2]
    } else {
        MinuteBucket::default()
    };
    let run_history_slice = values
        .iter()
        .rev()
        .take(8)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let run_sparkline = sparkline(
        &run_history_slice
            .iter()
            .map(|bucket| bucket.runs)
            .collect::<Vec<_>>(),
    );

    ThroughputMeter {
        runs_per_min: current.runs,
        messages_per_min: current.messages,
        errors_per_min: current.errors,
        runs_trend: trend(current.runs, previous.runs),
        messages_trend: trend(current.messages, previous.messages),
        errors_trend: trend(current.errors, previous.errors),
        runs_sparkline: run_sparkline,
    }
}

fn minute_bucket_key(started_at: &str) -> Option<String> {
    let trimmed = started_at.trim();
    if trimmed.chars().count() < 16 {
        return None;
    }
    let key = trimmed.chars().take(16).collect::<String>();
    if !key.contains('T') || !key.contains(':') {
        return None;
    }
    Some(key)
}

fn trend(current: usize, previous: usize) -> TrendDirection {
    if current > previous {
        TrendDirection::Up
    } else if current < previous {
        TrendDirection::Down
    } else {
        TrendDirection::Flat
    }
}

fn is_error_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "error" | "failed" | "killed"
    )
}

fn sparkline(values: &[usize]) -> String {
    if values.is_empty() {
        return "-".to_owned();
    }
    let max_value = values.iter().copied().max().unwrap_or(0);
    if max_value == 0 {
        return "·".repeat(values.len().max(1));
    }
    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut out = String::with_capacity(values.len());
    for value in values {
        let idx = ((*value * 7) / max_value).min(7);
        out.push(blocks[idx]);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_loops() -> Vec<LoopView> {
        vec![
            LoopView {
                id: "loop-a".to_owned(),
                state: "running".to_owned(),
                queue_depth: 2,
                last_error: String::new(),
                ..Default::default()
            },
            LoopView {
                id: "loop-b".to_owned(),
                state: "running".to_owned(),
                queue_depth: 5,
                last_error: "timeout".to_owned(),
                ..Default::default()
            },
            LoopView {
                id: "loop-c".to_owned(),
                state: "stopped".to_owned(),
                queue_depth: 0,
                last_error: String::new(),
                ..Default::default()
            },
            LoopView {
                id: "loop-d".to_owned(),
                state: "error".to_owned(),
                queue_depth: 8,
                last_error: "crash".to_owned(),
                ..Default::default()
            },
        ]
    }

    fn sample_runs() -> Vec<RunView> {
        vec![
            RunView {
                id: "run-1".to_owned(),
                status: "success".to_owned(),
                exit_code: Some(0),
                started_at: "2026-02-13T12:00:01Z".to_owned(),
                output_lines: vec!["ok".to_owned(), "done".to_owned()],
                ..Default::default()
            },
            RunView {
                id: "run-2".to_owned(),
                status: "success".to_owned(),
                exit_code: Some(0),
                started_at: "2026-02-13T12:00:35Z".to_owned(),
                output_lines: vec!["message".to_owned()],
                ..Default::default()
            },
            RunView {
                id: "run-3".to_owned(),
                status: "error".to_owned(),
                exit_code: Some(1),
                started_at: "2026-02-13T12:01:04Z".to_owned(),
                output_lines: vec![
                    "error".to_owned(),
                    "stack".to_owned(),
                    "line".to_owned(),
                    "tail".to_owned(),
                ],
                ..Default::default()
            },
            RunView {
                id: "run-4".to_owned(),
                status: "running".to_owned(),
                exit_code: None,
                started_at: "2026-02-13T12:01:41Z".to_owned(),
                output_lines: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
                ..Default::default()
            },
        ]
    }

    #[test]
    fn fleet_snapshot_computes_aggregates() {
        let snap = build_fleet_snapshot(&sample_loops(), &sample_runs());
        assert_eq!(snap.total_loops, 4);
        assert_eq!(snap.running_loops, 2);
        assert_eq!(snap.stopped_loops, 1);
        assert_eq!(snap.error_loops, 1);
        assert_eq!(snap.total_queue_depth, 15);
        assert_eq!(snap.max_queue_depth, 8);
        assert_eq!(snap.loops_with_queue, 3);
        assert_eq!(snap.loops_with_errors, 2);

        assert_eq!(snap.total_runs, 4);
        assert_eq!(snap.success_runs, 2);
        assert_eq!(snap.error_runs, 1);
        assert_eq!(snap.running_runs, 1);
        assert_eq!(snap.throughput.runs_per_min, 2);
        assert_eq!(snap.throughput.messages_per_min, 7);
        assert_eq!(snap.throughput.errors_per_min, 1);
        assert_eq!(snap.throughput.runs_trend, TrendDirection::Flat);
        assert_eq!(snap.throughput.messages_trend, TrendDirection::Up);
        assert_eq!(snap.throughput.errors_trend, TrendDirection::Up);
    }

    #[test]
    fn fleet_snapshot_empty_inputs() {
        let snap = build_fleet_snapshot(&[], &[]);
        assert_eq!(snap.total_loops, 0);
        assert_eq!(snap.total_runs, 0);
        assert_eq!(snap.max_queue_depth, 0);
    }

    #[test]
    fn hero_lines_contain_all_three_widgets() {
        let lines = hero_widget_lines(&sample_loops(), &sample_runs(), 80);
        let text: String = lines.iter().map(|l| format!("{}\n", l.text)).collect();

        assert!(text.contains("FLEET"), "missing fleet status");
        assert!(text.contains("THROUGHPUT"), "missing throughput widget");
        assert!(
            text.contains("QUEUE PRESSURE"),
            "missing queue pressure widget"
        );
        assert!(text.contains("ERROR RADAR"), "missing error radar widget");
    }

    #[test]
    fn hero_lines_have_separator_at_end() {
        let lines = hero_widget_lines(&sample_loops(), &sample_runs(), 80);
        let last = &lines[lines.len() - 1];
        assert!(last.text.contains('\u{2500}'), "missing separator");
        assert_eq!(last.role, TextRole::Muted);
    }

    #[test]
    fn gauge_bar_empty_for_zero_width() {
        assert_eq!(gauge_bar(0.5, 0), "");
    }

    #[test]
    fn gauge_bar_full_for_ratio_one() {
        let bar = gauge_bar(1.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == '\u{2588}'));
    }

    #[test]
    fn gauge_bar_empty_for_ratio_zero() {
        let bar = gauge_bar(0.0, 10);
        assert_eq!(bar.chars().count(), 10);
        assert!(bar.chars().all(|c| c == ' '));
    }

    #[test]
    fn gauge_bar_half_ratio() {
        let bar = gauge_bar(0.5, 10);
        assert_eq!(bar.chars().count(), 10);
        let full = bar.chars().filter(|c| *c == '\u{2588}').count();
        assert_eq!(full, 5);
    }

    #[test]
    fn truncate_respects_max() {
        assert_eq!(truncate("hello world", 5), "he...");
        assert_eq!(truncate("hi", 5), "hi");
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn throughput_role_reflects_success_rate() {
        // All success
        let snap = FleetSnapshot {
            total_runs: 10,
            success_runs: 10,
            ..Default::default()
        };
        let lines = render_throughput(&snap, 80);
        assert_eq!(lines[1].role, TextRole::Success);

        // Low success
        let snap = FleetSnapshot {
            total_runs: 10,
            success_runs: 2,
            error_runs: 8,
            ..Default::default()
        };
        let lines = render_throughput(&snap, 80);
        assert_eq!(lines[1].role, TextRole::Danger);
    }

    #[test]
    fn throughput_rate_line_includes_meter_arrows_and_sparkline() {
        let snap = build_fleet_snapshot(&sample_loops(), &sample_runs());
        let lines = render_throughput(&snap, 120);
        assert!(lines.len() >= 3);
        assert!(lines[2].text.contains("r/m 2→"));
        assert!(lines[2].text.contains("m/m 7↑"));
        assert!(lines[2].text.contains("e/m 1↑"));
        assert!(lines[2].text.contains('▁') || lines[2].text.contains('█'));
    }

    #[test]
    fn queue_pressure_labels() {
        // Idle
        let snap = FleetSnapshot {
            total_loops: 4,
            total_queue_depth: 0,
            ..Default::default()
        };
        let lines = render_queue_pressure(&snap, 80);
        assert!(lines[1].text.contains("idle"));

        // Critical
        let snap = FleetSnapshot {
            total_loops: 4,
            total_queue_depth: 30,
            loops_with_queue: 4,
            max_queue_depth: 12,
            ..Default::default()
        };
        let lines = render_queue_pressure(&snap, 80);
        assert!(lines[1].text.contains("critical"));
    }

    #[test]
    fn error_radar_shows_counts() {
        let snap = FleetSnapshot {
            total_loops: 5,
            error_loops: 2,
            loops_with_errors: 3,
            error_runs: 4,
            ..Default::default()
        };
        let lines = render_error_radar(&snap, 80);
        assert!(lines[0].text.contains("3 loop(s) w/ errors"));
        assert!(lines[0].text.contains("4 error run(s)"));
        assert!(lines[0].text.contains("2 in error state"));
    }

    #[test]
    fn narrow_width_does_not_panic() {
        let lines = hero_widget_lines(&sample_loops(), &sample_runs(), 10);
        for line in &lines {
            assert!(line.text.chars().count() <= 10);
        }
    }
}

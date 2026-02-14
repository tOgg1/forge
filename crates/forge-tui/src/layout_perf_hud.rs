//! Live layout inspector + frame budget HUD helpers.

use std::collections::VecDeque;

use crate::app::{DensityMode, FocusMode, MainTab, UiMode};
use crate::layouts::PaneLayout;

pub const DEFAULT_FRAME_BUDGET_FPS: u32 = 60;
pub const DEFAULT_FRAME_BUDGET_MS: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBudget {
    pub target_fps: u32,
    pub max_frame_ms: u64,
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self {
            target_fps: DEFAULT_FRAME_BUDGET_FPS,
            max_frame_ms: DEFAULT_FRAME_BUDGET_MS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePerfSample {
    pub frame_ms: u64,
    pub layout_ms: u64,
    pub render_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramePerfSummary {
    pub sample_count: usize,
    pub latest_frame_ms: u64,
    pub latest_layout_ms: u64,
    pub latest_render_ms: u64,
    pub avg_frame_ms: u64,
    pub p50_frame_ms: u64,
    pub p95_frame_ms: u64,
    pub worst_frame_ms: u64,
    pub estimated_fps: u64,
    pub dropped_frames: usize,
    pub over_budget_latest: bool,
    pub budget: FrameBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramePerfHud {
    budget: FrameBudget,
    max_samples: usize,
    samples: VecDeque<FramePerfSample>,
}

impl Default for FramePerfHud {
    fn default() -> Self {
        Self::new(FrameBudget::default(), 120)
    }
}

impl FramePerfHud {
    #[must_use]
    pub fn new(budget: FrameBudget, max_samples: usize) -> Self {
        Self {
            budget,
            max_samples: max_samples.max(1),
            samples: VecDeque::new(),
        }
    }

    pub fn push_sample(&mut self, sample: FramePerfSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    #[must_use]
    pub fn summary(&self) -> FramePerfSummary {
        let samples = self.samples.iter().copied().collect::<Vec<_>>();
        summarize_frame_perf(&samples, self.budget)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutInspectorSnapshot {
    pub tab: MainTab,
    pub mode: UiMode,
    pub frame_width: usize,
    pub frame_height: usize,
    pub content_start_row: usize,
    pub content_height: usize,
    pub requested_layout: PaneLayout,
    pub effective_layout: PaneLayout,
    pub density_mode: DensityMode,
    pub focus_mode: FocusMode,
    pub focus_right: bool,
    pub split_focus_supported: bool,
    pub focus_graph_nodes: Vec<String>,
    pub focused_node: String,
}

#[must_use]
pub fn summarize_frame_perf(samples: &[FramePerfSample], budget: FrameBudget) -> FramePerfSummary {
    if samples.is_empty() {
        return FramePerfSummary {
            sample_count: 0,
            latest_frame_ms: 0,
            latest_layout_ms: 0,
            latest_render_ms: 0,
            avg_frame_ms: 0,
            p50_frame_ms: 0,
            p95_frame_ms: 0,
            worst_frame_ms: 0,
            estimated_fps: 0,
            dropped_frames: 0,
            over_budget_latest: false,
            budget,
        };
    }

    let latest = samples[samples.len() - 1];
    let mut frame_times = samples
        .iter()
        .map(|sample| sample.frame_ms.max(1))
        .collect::<Vec<_>>();
    frame_times.sort_unstable();
    let total = frame_times.iter().copied().sum::<u64>();
    let avg_frame_ms = total / frame_times.len() as u64;
    let p50_frame_ms = percentile(&frame_times, 50);
    let p95_frame_ms = percentile(&frame_times, 95);
    let worst_frame_ms = frame_times[frame_times.len() - 1];
    let dropped_frames = frame_times
        .iter()
        .filter(|frame_ms| **frame_ms > budget.max_frame_ms)
        .count();
    let estimated_fps = if avg_frame_ms == 0 {
        0
    } else {
        1_000 / avg_frame_ms
    };

    FramePerfSummary {
        sample_count: samples.len(),
        latest_frame_ms: latest.frame_ms,
        latest_layout_ms: latest.layout_ms,
        latest_render_ms: latest.render_ms,
        avg_frame_ms,
        p50_frame_ms,
        p95_frame_ms,
        worst_frame_ms,
        estimated_fps,
        dropped_frames,
        over_budget_latest: latest.frame_ms > budget.max_frame_ms,
        budget,
    }
}

#[must_use]
pub fn render_layout_perf_hud_lines(
    snapshot: &LayoutInspectorSnapshot,
    summary: &FramePerfSummary,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mode = format!("{:?}", snapshot.mode);
    lines.push(fit_width(
        &format!(
            "Layout Inspector tab={} mode={} frame={}x{} content@y{} h{}",
            snapshot.tab.label(),
            mode,
            snapshot.frame_width,
            snapshot.frame_height,
            snapshot.content_start_row,
            snapshot.content_height
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    lines.push(fit_width(
        &format!(
            "layout req={} eff={} density={} focus={} active={}",
            snapshot.requested_layout.label(),
            snapshot.effective_layout.label(),
            snapshot.density_mode.label(),
            snapshot.focus_mode.label(),
            snapshot.focused_node
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    let graph = if snapshot.focus_graph_nodes.is_empty() {
        "-".to_owned()
    } else {
        snapshot.focus_graph_nodes.join(" -> ")
    };
    lines.push(fit_width(
        &format!(
            "focus-graph split={} right-pane={} path={}",
            if snapshot.split_focus_supported {
                "yes"
            } else {
                "no"
            },
            if snapshot.focus_right { "yes" } else { "no" },
            graph
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    let budget_state = if summary.over_budget_latest {
        format!(
            "BREACH +{}ms",
            summary
                .latest_frame_ms
                .saturating_sub(summary.budget.max_frame_ms)
        )
    } else {
        "ok".to_owned()
    };
    lines.push(fit_width(
        &format!(
            "perf budget={}ms@{}fps state={} samples={} latest={}ms (layout:{} render:{})",
            summary.budget.max_frame_ms,
            summary.budget.target_fps,
            budget_state,
            summary.sample_count,
            summary.latest_frame_ms,
            summary.latest_layout_ms,
            summary.latest_render_ms
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    lines.push(fit_width(
        &format!(
            "latency avg/p50/p95/worst={} / {} / {} / {}ms fps~{} dropped={}",
            summary.avg_frame_ms,
            summary.p50_frame_ms,
            summary.p95_frame_ms,
            summary.worst_frame_ms,
            summary.estimated_fps,
            summary.dropped_frames
        ),
        width,
    ));
    lines
}

fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    let rank = ((n - 1) * p) / 100;
    sorted[rank]
}

fn fit_width(value: &str, width: usize) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if out.chars().count() >= width {
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        render_layout_perf_hud_lines, summarize_frame_perf, FrameBudget, FramePerfHud,
        FramePerfSample, LayoutInspectorSnapshot,
    };
    use crate::app::{DensityMode, FocusMode, MainTab, UiMode};
    use crate::layouts::PaneLayout;

    #[test]
    fn perf_hud_ring_buffer_caps_samples() {
        let mut hud = FramePerfHud::new(FrameBudget::default(), 3);
        hud.push_sample(FramePerfSample {
            frame_ms: 12,
            layout_ms: 2,
            render_ms: 8,
        });
        hud.push_sample(FramePerfSample {
            frame_ms: 13,
            layout_ms: 2,
            render_ms: 9,
        });
        hud.push_sample(FramePerfSample {
            frame_ms: 14,
            layout_ms: 2,
            render_ms: 10,
        });
        hud.push_sample(FramePerfSample {
            frame_ms: 15,
            layout_ms: 2,
            render_ms: 11,
        });
        assert_eq!(hud.len(), 3);
        let summary = hud.summary();
        assert_eq!(summary.latest_frame_ms, 15);
        assert_eq!(summary.sample_count, 3);
    }

    #[test]
    fn summarize_frame_perf_reports_percentiles_and_budget_breaches() {
        let budget = FrameBudget {
            target_fps: 60,
            max_frame_ms: 16,
        };
        let samples = vec![
            FramePerfSample {
                frame_ms: 12,
                layout_ms: 2,
                render_ms: 8,
            },
            FramePerfSample {
                frame_ms: 17,
                layout_ms: 3,
                render_ms: 10,
            },
            FramePerfSample {
                frame_ms: 30,
                layout_ms: 4,
                render_ms: 20,
            },
            FramePerfSample {
                frame_ms: 14,
                layout_ms: 2,
                render_ms: 9,
            },
        ];
        let summary = summarize_frame_perf(&samples, budget);
        assert_eq!(summary.sample_count, 4);
        assert_eq!(summary.p50_frame_ms, 14);
        assert_eq!(summary.p95_frame_ms, 17);
        assert_eq!(summary.worst_frame_ms, 30);
        assert_eq!(summary.dropped_frames, 2);
        assert!(!summary.over_budget_latest);
        assert_eq!(summary.estimated_fps, 55);
    }

    #[test]
    fn render_lines_include_focus_graph_and_perf_state() {
        let snapshot = LayoutInspectorSnapshot {
            tab: MainTab::MultiLogs,
            mode: UiMode::Main,
            frame_width: 120,
            frame_height: 40,
            content_start_row: 2,
            content_height: 36,
            requested_layout: PaneLayout { rows: 3, cols: 4 },
            effective_layout: PaneLayout { rows: 2, cols: 4 },
            density_mode: DensityMode::Compact,
            focus_mode: FocusMode::DeepDebug,
            focus_right: true,
            split_focus_supported: true,
            focus_graph_nodes: vec!["left".to_owned(), "right".to_owned()],
            focused_node: "right".to_owned(),
        };
        let summary = summarize_frame_perf(
            &[
                FramePerfSample {
                    frame_ms: 18,
                    layout_ms: 3,
                    render_ms: 11,
                },
                FramePerfSample {
                    frame_ms: 22,
                    layout_ms: 4,
                    render_ms: 14,
                },
            ],
            FrameBudget {
                target_fps: 60,
                max_frame_ms: 16,
            },
        );
        let lines = render_layout_perf_hud_lines(&snapshot, &summary, 120, 8);
        let text = lines.join("\n");
        assert!(text.contains("Layout Inspector"));
        assert!(text.contains("path=left -> right"));
        assert!(text.contains("state=BREACH"));
        assert!(text.contains("latency avg/p50/p95/worst"));
    }
}

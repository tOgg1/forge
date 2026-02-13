//! Cross-loop health heatmap timeline for state and error-burst visibility.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopHealthBucket {
    pub timestamp_epoch_s: i64,
    pub state: String,
    pub run_count: u32,
    pub error_count: u32,
    pub queue_depth: usize,
    pub stalled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopHealthTimelineInput {
    pub loop_id: String,
    pub buckets: Vec<LoopHealthBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HeatCellSeverity {
    Healthy,
    Degraded,
    Warning,
    Critical,
    Offline,
}

impl HeatCellSeverity {
    #[must_use]
    pub fn glyph(self) -> char {
        match self {
            Self::Healthy => '.',
            Self::Degraded => ':',
            Self::Warning => '!',
            Self::Critical => 'X',
            Self::Offline => 'o',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopHealthHeatmapRow {
    pub loop_id: String,
    pub heatmap: String,
    pub healthy_cells: usize,
    pub degraded_cells: usize,
    pub warning_cells: usize,
    pub critical_cells: usize,
    pub offline_cells: usize,
    pub peak_queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HealthTimelineSummary {
    pub loop_count: usize,
    pub bucket_count: usize,
    pub healthy_cells: usize,
    pub degraded_cells: usize,
    pub warning_cells: usize,
    pub critical_cells: usize,
    pub offline_cells: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrossLoopHealthTimeline {
    pub rows: Vec<LoopHealthHeatmapRow>,
    pub summary: HealthTimelineSummary,
}

#[must_use]
pub fn build_cross_loop_health_timeline(
    loops: &[LoopHealthTimelineInput],
    max_buckets: usize,
) -> CrossLoopHealthTimeline {
    let mut rows = loops
        .iter()
        .filter_map(|loop_input| build_row(loop_input, max_buckets))
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        b.critical_cells
            .cmp(&a.critical_cells)
            .then(b.warning_cells.cmp(&a.warning_cells))
            .then(b.degraded_cells.cmp(&a.degraded_cells))
            .then(b.peak_queue_depth.cmp(&a.peak_queue_depth))
            .then(a.loop_id.cmp(&b.loop_id))
    });

    let mut summary = HealthTimelineSummary {
        loop_count: rows.len(),
        ..HealthTimelineSummary::default()
    };
    for row in &rows {
        summary.bucket_count = summary.bucket_count.max(row.heatmap.chars().count());
        summary.healthy_cells += row.healthy_cells;
        summary.degraded_cells += row.degraded_cells;
        summary.warning_cells += row.warning_cells;
        summary.critical_cells += row.critical_cells;
        summary.offline_cells += row.offline_cells;
    }

    CrossLoopHealthTimeline { rows, summary }
}

#[must_use]
pub fn render_cross_loop_heatmap_lines(
    timeline: &CrossLoopHealthTimeline,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let summary = &timeline.summary;
    lines.push(truncate(
        &format!(
            "Health Heatmap loops={} buckets={} X={} !={} :={} o={}",
            summary.loop_count,
            summary.bucket_count,
            summary.critical_cells,
            summary.warning_cells,
            summary.degraded_cells,
            summary.offline_cells,
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    if timeline.rows.is_empty() {
        lines.push(truncate("no loop health samples", width));
        return lines;
    }

    lines.push(truncate(
        "legend: . healthy : degraded ! warning X critical o offline",
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    let row_width = width.saturating_sub(2);
    for row in &timeline.rows {
        if lines.len() >= max_rows {
            break;
        }
        let line = format!(
            "{} {:<12} q{:>3} {}",
            trend_indicator(row),
            row.loop_id,
            row.peak_queue_depth,
            row.heatmap
        );
        lines.push(truncate(&line, row_width));
    }

    lines
}

fn build_row(input: &LoopHealthTimelineInput, max_buckets: usize) -> Option<LoopHealthHeatmapRow> {
    let loop_id = input.loop_id.trim().to_owned();
    if loop_id.is_empty() {
        return None;
    }
    if input.buckets.is_empty() {
        return None;
    }

    let mut buckets = input
        .buckets
        .iter()
        .filter(|bucket| bucket.timestamp_epoch_s >= 0)
        .cloned()
        .collect::<Vec<_>>();
    if buckets.is_empty() {
        return None;
    }
    buckets.sort_by(|a, b| a.timestamp_epoch_s.cmp(&b.timestamp_epoch_s));

    let max_buckets = if max_buckets == 0 { 24 } else { max_buckets };
    if buckets.len() > max_buckets {
        buckets.drain(0..(buckets.len() - max_buckets));
    }

    let mut row = LoopHealthHeatmapRow {
        loop_id,
        heatmap: String::new(),
        healthy_cells: 0,
        degraded_cells: 0,
        warning_cells: 0,
        critical_cells: 0,
        offline_cells: 0,
        peak_queue_depth: 0,
    };

    for bucket in buckets {
        let severity = classify_bucket(&bucket);
        row.heatmap.push(severity.glyph());
        row.peak_queue_depth = row.peak_queue_depth.max(bucket.queue_depth);
        match severity {
            HeatCellSeverity::Healthy => row.healthy_cells += 1,
            HeatCellSeverity::Degraded => row.degraded_cells += 1,
            HeatCellSeverity::Warning => row.warning_cells += 1,
            HeatCellSeverity::Critical => row.critical_cells += 1,
            HeatCellSeverity::Offline => row.offline_cells += 1,
        }
    }
    Some(row)
}

fn classify_bucket(bucket: &LoopHealthBucket) -> HeatCellSeverity {
    let state = bucket.state.trim().to_ascii_lowercase();
    if is_offline_state(&state) {
        return HeatCellSeverity::Offline;
    }

    let mut score = 0_i32;
    if state == "error" || state == "failed" {
        score += 4;
    } else if state == "waiting" || state == "stopped" {
        score += 1;
    }

    if bucket.error_count >= 5 {
        score += 5;
    } else if bucket.error_count >= 2 {
        score += 3;
    } else if bucket.error_count >= 1 {
        score += 2;
    }

    if bucket.queue_depth >= 25 {
        score += 3;
    } else if bucket.queue_depth >= 10 {
        score += 2;
    } else if bucket.queue_depth >= 5 {
        score += 1;
    }

    if bucket.stalled {
        score += 2;
    }

    if bucket.run_count == 0 && (state == "running" || state == "active") {
        score += 1;
    }

    if score >= 8 {
        HeatCellSeverity::Critical
    } else if score >= 5 {
        HeatCellSeverity::Warning
    } else if score >= 2 {
        HeatCellSeverity::Degraded
    } else {
        HeatCellSeverity::Healthy
    }
}

fn is_offline_state(state: &str) -> bool {
    matches!(state, "offline" | "down" | "terminated" | "dead")
}

fn trend_indicator(row: &LoopHealthHeatmapRow) -> char {
    if row.critical_cells > 0 {
        'X'
    } else if row.warning_cells > 0 {
        '!'
    } else if row.degraded_cells > 0 {
        ':'
    } else if row.offline_cells > 0 {
        'o'
    } else {
        '.'
    }
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_cross_loop_health_timeline, render_cross_loop_heatmap_lines, LoopHealthBucket,
        LoopHealthTimelineInput,
    };

    fn bucket(
        ts: i64,
        state: &str,
        run_count: u32,
        error_count: u32,
        queue_depth: usize,
        stalled: bool,
    ) -> LoopHealthBucket {
        LoopHealthBucket {
            timestamp_epoch_s: ts,
            state: state.to_owned(),
            run_count,
            error_count,
            queue_depth,
            stalled,
        }
    }

    #[test]
    fn builds_ranked_cross_loop_heatmap() {
        let timeline = build_cross_loop_health_timeline(
            &[
                LoopHealthTimelineInput {
                    loop_id: "loop-safe".to_owned(),
                    buckets: vec![
                        bucket(10, "running", 6, 0, 1, false),
                        bucket(20, "running", 7, 0, 1, false),
                    ],
                },
                LoopHealthTimelineInput {
                    loop_id: "loop-risk".to_owned(),
                    buckets: vec![
                        bucket(10, "running", 5, 2, 12, false),
                        bucket(20, "error", 1, 6, 25, true),
                    ],
                },
            ],
            24,
        );

        assert_eq!(timeline.rows.len(), 2);
        assert_eq!(timeline.rows[0].loop_id, "loop-risk");
        assert_eq!(timeline.rows[0].heatmap.chars().nth(0), Some('!'));
        assert_eq!(timeline.rows[0].heatmap.chars().nth(1), Some('X'));
        assert!(timeline.summary.critical_cells >= 1);
        assert!(timeline.summary.warning_cells >= 1);
    }

    #[test]
    fn max_buckets_applies_tail_window() {
        let timeline = build_cross_loop_health_timeline(
            &[LoopHealthTimelineInput {
                loop_id: "loop-tail".to_owned(),
                buckets: vec![
                    bucket(10, "running", 3, 0, 0, false),
                    bucket(20, "running", 3, 0, 0, false),
                    bucket(30, "running", 3, 1, 8, false),
                    bucket(40, "running", 2, 1, 8, false),
                ],
            }],
            2,
        );
        assert_eq!(timeline.rows[0].heatmap.chars().count(), 2);
        assert_eq!(timeline.rows[0].heatmap, "::");
    }

    #[test]
    fn offline_state_maps_to_offline_glyph() {
        let timeline = build_cross_loop_health_timeline(
            &[LoopHealthTimelineInput {
                loop_id: "loop-offline".to_owned(),
                buckets: vec![
                    bucket(10, "running", 3, 0, 0, false),
                    bucket(20, "offline", 0, 0, 0, false),
                ],
            }],
            24,
        );
        assert_eq!(timeline.rows[0].heatmap, ".o");
        assert_eq!(timeline.rows[0].offline_cells, 1);
    }

    #[test]
    fn render_lines_include_summary_legend_and_rows() {
        let timeline = build_cross_loop_health_timeline(
            &[LoopHealthTimelineInput {
                loop_id: "loop-a".to_owned(),
                buckets: vec![bucket(10, "running", 3, 0, 0, false)],
            }],
            24,
        );
        let lines = render_cross_loop_heatmap_lines(&timeline, 80, 8);
        assert!(lines[0].contains("Health Heatmap"));
        assert!(lines[1].contains("legend"));
        assert!(lines.iter().any(|line| line.contains("loop-a")));
    }

    #[test]
    fn skips_invalid_rows() {
        let timeline = build_cross_loop_health_timeline(
            &[
                LoopHealthTimelineInput {
                    loop_id: "   ".to_owned(),
                    buckets: vec![bucket(10, "running", 1, 0, 0, false)],
                },
                LoopHealthTimelineInput {
                    loop_id: "loop-empty".to_owned(),
                    buckets: vec![],
                },
                LoopHealthTimelineInput {
                    loop_id: "loop-ok".to_owned(),
                    buckets: vec![bucket(10, "running", 1, 0, 0, false)],
                },
            ],
            24,
        );
        assert_eq!(timeline.rows.len(), 1);
        assert_eq!(timeline.rows[0].loop_id, "loop-ok");
    }
}

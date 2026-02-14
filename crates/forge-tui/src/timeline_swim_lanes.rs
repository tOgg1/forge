//! Concurrent run timeline swim-lane planner + renderer.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwimLaneRunSample {
    pub loop_id: String,
    pub loop_label: String,
    pub run_id: String,
    pub status: String,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSwimLaneConfig {
    pub columns: usize,
    pub lane_limit: usize,
    pub window_start_ms: Option<i64>,
    pub window_end_ms: Option<i64>,
    pub pan_ms: i64,
}

impl Default for TimelineSwimLaneConfig {
    fn default() -> Self {
        Self {
            columns: 80,
            lane_limit: 12,
            window_start_ms: None,
            window_end_ms: None,
            pan_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwimLaneSegment {
    pub run_id: String,
    pub status: String,
    pub start_col: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwimLane {
    pub loop_id: String,
    pub loop_label: String,
    pub segments: Vec<SwimLaneSegment>,
    pub overlap_cells: usize,
    pub max_parallel_runs: usize,
    pub contention_score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSwimLaneReport {
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub columns: usize,
    pub lanes: Vec<SwimLane>,
    pub excluded_invalid: usize,
}

#[must_use]
pub fn build_timeline_swim_lanes(
    samples: &[SwimLaneRunSample],
    config: &TimelineSwimLaneConfig,
) -> TimelineSwimLaneReport {
    let columns = config.columns.max(8);
    let lane_limit = config.lane_limit.max(1);

    let mut normalized = Vec::new();
    let mut excluded_invalid = 0usize;
    for sample in samples {
        let loop_id = normalize(&sample.loop_id);
        let run_id = normalize(&sample.run_id);
        let status = normalize(&sample.status);
        if loop_id.is_empty() || run_id.is_empty() || status.is_empty() {
            excluded_invalid += 1;
            continue;
        }
        let started_at_ms = sample.started_at_ms.max(0);
        let finished_at_ms = sample.finished_at_ms.max(started_at_ms + 1);

        normalized.push(SwimLaneRunSample {
            loop_id: loop_id.clone(),
            loop_label: normalize_label(&sample.loop_label, &loop_id),
            run_id,
            status,
            started_at_ms,
            finished_at_ms,
        });
    }

    if normalized.is_empty() {
        return TimelineSwimLaneReport {
            window_start_ms: 0,
            window_end_ms: 1,
            columns,
            lanes: Vec::new(),
            excluded_invalid,
        };
    }

    let min_start = normalized
        .iter()
        .map(|sample| sample.started_at_ms)
        .min()
        .unwrap_or(0);
    let max_end = normalized
        .iter()
        .map(|sample| sample.finished_at_ms)
        .max()
        .unwrap_or(min_start + 1);

    let mut window_start_ms = config.window_start_ms.unwrap_or(min_start) + config.pan_ms;
    let mut window_end_ms = config.window_end_ms.unwrap_or(max_end) + config.pan_ms;
    if window_end_ms <= window_start_ms {
        window_end_ms = window_start_ms + 1;
    }
    window_start_ms = window_start_ms.max(0);

    let mut grouped: BTreeMap<String, SwimLane> = BTreeMap::new();
    for sample in normalized {
        if sample.finished_at_ms <= window_start_ms || sample.started_at_ms >= window_end_ms {
            continue;
        }
        let entry = grouped
            .entry(sample.loop_id.clone())
            .or_insert_with(|| SwimLane {
                loop_id: sample.loop_id.clone(),
                loop_label: sample.loop_label.clone(),
                segments: Vec::new(),
                overlap_cells: 0,
                max_parallel_runs: 0,
                contention_score: 0,
            });

        let start_col = project_time_to_col(
            sample.started_at_ms.max(window_start_ms),
            window_start_ms,
            window_end_ms,
            columns,
        );
        let end_col_raw = project_time_to_col(
            sample.finished_at_ms.min(window_end_ms),
            window_start_ms,
            window_end_ms,
            columns,
        );
        let end_col = end_col_raw.max(start_col + 1).min(columns);

        entry.segments.push(SwimLaneSegment {
            run_id: sample.run_id,
            status: sample.status,
            start_col,
            end_col,
        });
    }

    let mut lanes = grouped.into_values().collect::<Vec<_>>();
    for lane in &mut lanes {
        let mut occupancy = vec![0usize; columns];
        for segment in &lane.segments {
            for value in occupancy
                .iter_mut()
                .take(segment.end_col.min(columns))
                .skip(segment.start_col.min(columns))
            {
                *value += 1;
            }
        }
        lane.overlap_cells = occupancy.iter().filter(|value| **value > 1).count();
        lane.max_parallel_runs = occupancy.iter().copied().max().unwrap_or(0);
        lane.contention_score =
            lane.overlap_cells + lane.max_parallel_runs * 4 + lane.segments.len();
        lane.segments
            .sort_by(|a, b| a.start_col.cmp(&b.start_col).then(a.run_id.cmp(&b.run_id)));
    }

    lanes.sort_by(|a, b| {
        b.contention_score
            .cmp(&a.contention_score)
            .then(b.overlap_cells.cmp(&a.overlap_cells))
            .then(a.loop_id.cmp(&b.loop_id))
    });
    lanes.truncate(lane_limit);

    TimelineSwimLaneReport {
        window_start_ms,
        window_end_ms,
        columns,
        lanes,
        excluded_invalid,
    }
}

#[must_use]
pub fn render_timeline_swim_lanes(
    report: &TimelineSwimLaneReport,
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let timeline_width = report.columns.min(width.saturating_sub(24)).max(8);
    let mut lines = vec![
        fit_width("TIMELINE SWIM LANES", width),
        fit_width(
            &format!(
                "window:{}..{} lanes:{}",
                report.window_start_ms,
                report.window_end_ms,
                report.lanes.len()
            ),
            width,
        ),
    ];

    if report.lanes.is_empty() {
        lines.push(fit_width("no timeline samples in window", width));
        lines.truncate(height);
        return lines;
    }

    for lane in &report.lanes {
        if lines.len() >= height {
            break;
        }
        let mut bar = vec!['.'; timeline_width];
        for segment in &lane.segments {
            let start = segment.start_col.min(report.columns);
            let end = segment.end_col.min(report.columns).max(start + 1);
            let projected_start = start * timeline_width / report.columns;
            let projected_end = (end * timeline_width / report.columns).max(projected_start + 1);
            for cell in bar
                .iter_mut()
                .take(projected_end.min(timeline_width))
                .skip(projected_start.min(timeline_width))
            {
                let marker = status_marker(&segment.status);
                if *cell == '.' {
                    *cell = marker;
                } else if *cell != marker {
                    *cell = '*';
                }
            }
        }

        lines.push(fit_width(
            &format!(
                "{:10} |{}| c:{} o:{}",
                trim_label(&lane.loop_label, 10),
                bar.iter().collect::<String>(),
                lane.contention_score,
                lane.overlap_cells
            ),
            width,
        ));
    }

    lines.truncate(height);
    lines
}

fn project_time_to_col(time_ms: i64, start_ms: i64, end_ms: i64, columns: usize) -> usize {
    let span = (end_ms - start_ms).max(1);
    let relative = (time_ms - start_ms).clamp(0, span);
    ((relative as f64 / span as f64) * columns as f64).floor() as usize
}

fn status_marker(status: &str) -> char {
    match status {
        "running" => '=',
        "success" | "done" => '+',
        "error" | "failed" => 'x',
        "queued" | "pending" => '~',
        "stopped" | "canceled" => '-',
        _ => '#',
    }
}

fn trim_label(label: &str, max_len: usize) -> String {
    label.chars().take(max_len).collect()
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_label(label: &str, loop_id: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        loop_id.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if value.len() <= width {
        value.to_owned()
    } else {
        value.chars().take(width).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_timeline_swim_lanes, render_timeline_swim_lanes, SwimLaneRunSample,
        TimelineSwimLaneConfig,
    };

    fn run(
        loop_id: &str,
        run_id: &str,
        status: &str,
        started_at_ms: i64,
        finished_at_ms: i64,
    ) -> SwimLaneRunSample {
        SwimLaneRunSample {
            loop_id: loop_id.to_owned(),
            loop_label: loop_id.to_owned(),
            run_id: run_id.to_owned(),
            status: status.to_owned(),
            started_at_ms,
            finished_at_ms,
        }
    }

    #[test]
    fn groups_runs_into_loop_lanes() {
        let report = build_timeline_swim_lanes(
            &[
                run("loop-a", "r1", "running", 1000, 2000),
                run("loop-b", "r2", "success", 1200, 1800),
            ],
            &TimelineSwimLaneConfig {
                columns: 40,
                ..TimelineSwimLaneConfig::default()
            },
        );
        assert_eq!(report.lanes.len(), 2);
        assert!(report.lanes.iter().any(|lane| lane.loop_id == "loop-a"));
        assert!(report.lanes.iter().any(|lane| lane.loop_id == "loop-b"));
    }

    #[test]
    fn overlap_cells_increase_contention_score() {
        let report = build_timeline_swim_lanes(
            &[
                run("loop-a", "r1", "running", 1000, 2200),
                run("loop-a", "r2", "failed", 1300, 2300),
                run("loop-b", "r3", "running", 1000, 1200),
            ],
            &TimelineSwimLaneConfig {
                columns: 60,
                ..TimelineSwimLaneConfig::default()
            },
        );
        assert_eq!(report.lanes.len(), 2);
        assert_eq!(report.lanes[0].loop_id, "loop-a");
        assert!(report.lanes[0].overlap_cells > 0);
        assert!(report.lanes[0].contention_score > report.lanes[1].contention_score);
    }

    #[test]
    fn lane_limit_is_applied() {
        let report = build_timeline_swim_lanes(
            &[
                run("loop-a", "r1", "running", 1000, 2000),
                run("loop-b", "r2", "running", 1000, 2000),
                run("loop-c", "r3", "running", 1000, 2000),
            ],
            &TimelineSwimLaneConfig {
                lane_limit: 2,
                ..TimelineSwimLaneConfig::default()
            },
        );
        assert_eq!(report.lanes.len(), 2);
    }

    #[test]
    fn render_contains_lane_rows_and_markers() {
        let report = build_timeline_swim_lanes(
            &[
                run("loop-a", "r1", "running", 1000, 2000),
                run("loop-a", "r2", "failed", 1200, 2100),
            ],
            &TimelineSwimLaneConfig {
                columns: 30,
                ..TimelineSwimLaneConfig::default()
            },
        );
        let lines = render_timeline_swim_lanes(&report, 120, 10);
        assert!(lines[0].contains("TIMELINE SWIM LANES"));
        assert!(lines.iter().any(|line| line.contains("loop-a")));
        assert!(lines.iter().any(|line| line.contains("|")));
    }
}

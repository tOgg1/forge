//! Throughput, cycle-time, queue-aging, and completion-velocity dashboard model.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThroughputBucketSample {
    pub bucket_label: String,
    pub started_runs: usize,
    pub completed_runs: usize,
    pub failed_runs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLifecycleSample {
    pub task_id: String,
    pub queue_entered_at_epoch_s: i64,
    pub started_at_epoch_s: Option<i64>,
    pub completed_at_epoch_s: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardInput {
    pub throughput_buckets: Vec<ThroughputBucketSample>,
    pub task_lifecycles: Vec<TaskLifecycleSample>,
    pub now_epoch_s: i64,
    pub velocity_window_hours: u64,
    pub queue_stale_after_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartPoint {
    pub label: String,
    pub value: usize,
    pub bar: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleTimeRow {
    pub task_id: String,
    pub cycle_time_secs: u64,
    pub cycle_time_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueAgingRow {
    pub task_id: String,
    pub age_secs: u64,
    pub age_label: String,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThroughputSummary {
    pub buckets: usize,
    pub total_started: usize,
    pub total_completed: usize,
    pub total_failed: usize,
    pub completed_sparkline: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CycleTimeSummary {
    pub measured_tasks: usize,
    pub p50_secs: u64,
    pub p90_secs: u64,
    pub max_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueAgingSummary {
    pub pending_tasks: usize,
    pub stale_tasks: usize,
    pub max_age_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionVelocitySummary {
    pub window_hours: u64,
    pub completed_in_window: usize,
    pub peak_per_hour: usize,
    pub sparkline: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DashboardSummary {
    pub throughput: ThroughputSummary,
    pub cycle_time: CycleTimeSummary,
    pub queue_aging: QueueAgingSummary,
    pub completion_velocity: CompletionVelocitySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalyticsDashboardView {
    pub throughput_chart: Vec<ChartPoint>,
    pub completion_velocity_chart: Vec<ChartPoint>,
    pub cycle_time_table: Vec<CycleTimeRow>,
    pub queue_aging_table: Vec<QueueAgingRow>,
    pub summary: DashboardSummary,
}

#[must_use]
pub fn build_analytics_dashboard(input: &DashboardInput) -> AnalyticsDashboardView {
    let now_epoch_s = input.now_epoch_s.max(0);
    let stale_after_secs = if input.queue_stale_after_secs == 0 {
        3_600
    } else {
        input.queue_stale_after_secs
    };
    let velocity_window_hours = if input.velocity_window_hours == 0 {
        24
    } else {
        input.velocity_window_hours
    };

    let mut throughput_buckets = input
        .throughput_buckets
        .iter()
        .filter(|bucket| !bucket.bucket_label.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    throughput_buckets.sort_by(|a, b| a.bucket_label.cmp(&b.bucket_label));

    let throughput_values = throughput_buckets
        .iter()
        .map(|bucket| bucket.completed_runs)
        .collect::<Vec<_>>();
    let throughput_max = throughput_values.iter().copied().max().unwrap_or(0);
    let throughput_chart = throughput_buckets
        .iter()
        .map(|bucket| ChartPoint {
            label: bucket.bucket_label.clone(),
            value: bucket.completed_runs,
            bar: ascii_bar(bucket.completed_runs, throughput_max, 12),
            detail: format!(
                "started={} completed={} failed={}",
                bucket.started_runs, bucket.completed_runs, bucket.failed_runs
            ),
        })
        .collect::<Vec<_>>();

    let mut cycle_rows = Vec::new();
    let mut cycle_durations = Vec::new();
    let mut queue_rows = Vec::new();

    for task in &input.task_lifecycles {
        let task_id = task.task_id.trim();
        if task_id.is_empty() {
            continue;
        }

        if let (Some(started), Some(completed)) =
            (task.started_at_epoch_s, task.completed_at_epoch_s)
        {
            if completed >= started {
                let cycle_secs = (completed - started) as u64;
                cycle_rows.push(CycleTimeRow {
                    task_id: task_id.to_owned(),
                    cycle_time_secs: cycle_secs,
                    cycle_time_label: format_duration(cycle_secs),
                });
                cycle_durations.push(cycle_secs);
            }
        }

        if task.completed_at_epoch_s.is_none() {
            let age_secs = if now_epoch_s > task.queue_entered_at_epoch_s {
                (now_epoch_s - task.queue_entered_at_epoch_s) as u64
            } else {
                0
            };
            queue_rows.push(QueueAgingRow {
                task_id: task_id.to_owned(),
                age_secs,
                age_label: format_duration(age_secs),
                stale: age_secs >= stale_after_secs,
            });
        }
    }

    cycle_rows.sort_by(|a, b| {
        b.cycle_time_secs
            .cmp(&a.cycle_time_secs)
            .then(a.task_id.cmp(&b.task_id))
    });
    queue_rows.sort_by(|a, b| b.age_secs.cmp(&a.age_secs).then(a.task_id.cmp(&b.task_id)));

    cycle_durations.sort_unstable();
    let cycle_summary = CycleTimeSummary {
        measured_tasks: cycle_durations.len(),
        p50_secs: percentile_nearest_rank(&cycle_durations, 50),
        p90_secs: percentile_nearest_rank(&cycle_durations, 90),
        max_secs: cycle_durations.iter().copied().max().unwrap_or(0),
    };

    let queue_summary = QueueAgingSummary {
        pending_tasks: queue_rows.len(),
        stale_tasks: queue_rows.iter().filter(|row| row.stale).count(),
        max_age_secs: queue_rows.first().map_or(0, |row| row.age_secs),
    };

    let velocity_chart =
        completion_velocity_chart(&input.task_lifecycles, now_epoch_s, velocity_window_hours);
    let velocity_values = velocity_chart
        .iter()
        .map(|point| point.value)
        .collect::<Vec<_>>();
    let velocity_summary = CompletionVelocitySummary {
        window_hours: velocity_window_hours,
        completed_in_window: velocity_values.iter().sum(),
        peak_per_hour: velocity_values.iter().copied().max().unwrap_or(0),
        sparkline: ascii_sparkline(&velocity_values),
    };

    let throughput_summary = ThroughputSummary {
        buckets: throughput_buckets.len(),
        total_started: throughput_buckets
            .iter()
            .map(|bucket| bucket.started_runs)
            .sum(),
        total_completed: throughput_buckets
            .iter()
            .map(|bucket| bucket.completed_runs)
            .sum(),
        total_failed: throughput_buckets
            .iter()
            .map(|bucket| bucket.failed_runs)
            .sum(),
        completed_sparkline: ascii_sparkline(&throughput_values),
    };

    AnalyticsDashboardView {
        throughput_chart,
        completion_velocity_chart: velocity_chart,
        cycle_time_table: cycle_rows,
        queue_aging_table: queue_rows,
        summary: DashboardSummary {
            throughput: throughput_summary,
            cycle_time: cycle_summary,
            queue_aging: queue_summary,
            completion_velocity: velocity_summary,
        },
    }
}

fn completion_velocity_chart(
    tasks: &[TaskLifecycleSample],
    now_epoch_s: i64,
    window_hours: u64,
) -> Vec<ChartPoint> {
    let mut values = Vec::new();
    for hour_offset in (0..window_hours).rev() {
        let bucket_end = now_epoch_s - (hour_offset as i64 * 3_600);
        let bucket_start = bucket_end - 3_600;
        let completed_count = tasks
            .iter()
            .filter_map(|task| task.completed_at_epoch_s)
            .filter(|completed| *completed > bucket_start && *completed <= bucket_end)
            .count();
        values.push((format!("h-{hour_offset:02}"), completed_count));
    }
    let peak = values.iter().map(|(_, value)| *value).max().unwrap_or(0);
    values
        .into_iter()
        .map(|(label, value)| ChartPoint {
            label,
            value,
            bar: ascii_bar(value, peak, 12),
            detail: format!("completed_per_hour={value}"),
        })
        .collect()
}

fn percentile_nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = percentile.saturating_mul(sorted.len()).saturating_add(99) / 100;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

fn ascii_bar(value: usize, max_value: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if max_value == 0 {
        return "-".repeat(width);
    }
    let filled = value.saturating_mul(width).saturating_add(max_value - 1) / max_value;
    let filled = filled.min(width);
    let mut out = String::with_capacity(width);
    out.push_str(&"#".repeat(filled));
    out.push_str(&"-".repeat(width - filled));
    out
}

fn ascii_sparkline(values: &[usize]) -> String {
    if values.is_empty() {
        return "-".to_owned();
    }
    let levels = ['.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let max_value = values.iter().copied().max().unwrap_or(0);
    if max_value == 0 {
        return ".".repeat(values.len());
    }
    values
        .iter()
        .map(|value| {
            let index = value.saturating_mul(levels.len() - 1) / max_value;
            levels[index]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_analytics_dashboard, DashboardInput, TaskLifecycleSample, ThroughputBucketSample,
    };

    #[test]
    fn throughput_chart_and_summary_are_derived() {
        let input = DashboardInput {
            throughput_buckets: vec![
                ThroughputBucketSample {
                    bucket_label: "2026-02-12T08".to_owned(),
                    started_runs: 5,
                    completed_runs: 3,
                    failed_runs: 1,
                },
                ThroughputBucketSample {
                    bucket_label: "2026-02-12T09".to_owned(),
                    started_runs: 4,
                    completed_runs: 4,
                    failed_runs: 0,
                },
            ],
            task_lifecycles: vec![],
            now_epoch_s: 0,
            velocity_window_hours: 2,
            queue_stale_after_secs: 3_600,
        };

        let view = build_analytics_dashboard(&input);
        assert_eq!(view.throughput_chart.len(), 2);
        assert_eq!(view.summary.throughput.total_started, 9);
        assert_eq!(view.summary.throughput.total_completed, 7);
        assert_eq!(view.summary.throughput.total_failed, 1);
        assert_eq!(view.summary.throughput.completed_sparkline.len(), 2);
    }

    #[test]
    fn cycle_time_table_and_percentiles_are_stable() {
        let input = DashboardInput {
            throughput_buckets: vec![],
            task_lifecycles: vec![
                TaskLifecycleSample {
                    task_id: "task-a".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(100),
                    completed_at_epoch_s: Some(220),
                },
                TaskLifecycleSample {
                    task_id: "task-b".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(200),
                    completed_at_epoch_s: Some(500),
                },
                TaskLifecycleSample {
                    task_id: "task-c".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(300),
                    completed_at_epoch_s: Some(360),
                },
            ],
            now_epoch_s: 1_000,
            velocity_window_hours: 3,
            queue_stale_after_secs: 3_600,
        };

        let view = build_analytics_dashboard(&input);
        assert_eq!(view.cycle_time_table[0].task_id, "task-b");
        assert_eq!(view.summary.cycle_time.measured_tasks, 3);
        assert_eq!(view.summary.cycle_time.p50_secs, 120);
        assert_eq!(view.summary.cycle_time.p90_secs, 300);
        assert_eq!(view.summary.cycle_time.max_secs, 300);
    }

    #[test]
    fn queue_aging_table_orders_and_flags_stale_rows() {
        let input = DashboardInput {
            throughput_buckets: vec![],
            task_lifecycles: vec![
                TaskLifecycleSample {
                    task_id: "task-old".to_owned(),
                    queue_entered_at_epoch_s: 1_000,
                    started_at_epoch_s: None,
                    completed_at_epoch_s: None,
                },
                TaskLifecycleSample {
                    task_id: "task-fresh".to_owned(),
                    queue_entered_at_epoch_s: 9_700,
                    started_at_epoch_s: None,
                    completed_at_epoch_s: None,
                },
            ],
            now_epoch_s: 10_000,
            velocity_window_hours: 2,
            queue_stale_after_secs: 3_600,
        };

        let view = build_analytics_dashboard(&input);
        assert_eq!(view.queue_aging_table[0].task_id, "task-old");
        assert!(view.queue_aging_table[0].stale);
        assert!(!view.queue_aging_table[1].stale);
        assert_eq!(view.summary.queue_aging.pending_tasks, 2);
        assert_eq!(view.summary.queue_aging.stale_tasks, 1);
    }

    #[test]
    fn completion_velocity_chart_counts_windowed_completions() {
        let now = 10_000;
        let input = DashboardInput {
            throughput_buckets: vec![],
            task_lifecycles: vec![
                TaskLifecycleSample {
                    task_id: "task-a".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(0),
                    completed_at_epoch_s: Some(now - 200),
                },
                TaskLifecycleSample {
                    task_id: "task-b".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(0),
                    completed_at_epoch_s: Some(now - 3_800),
                },
                TaskLifecycleSample {
                    task_id: "task-c".to_owned(),
                    queue_entered_at_epoch_s: 0,
                    started_at_epoch_s: Some(0),
                    completed_at_epoch_s: Some(now - 7_500),
                },
            ],
            now_epoch_s: now,
            velocity_window_hours: 3,
            queue_stale_after_secs: 3_600,
        };

        let view = build_analytics_dashboard(&input);
        assert_eq!(view.completion_velocity_chart.len(), 3);
        assert_eq!(
            view.completion_velocity_chart
                .iter()
                .map(|point| point.value)
                .collect::<Vec<_>>(),
            vec![1, 1, 1]
        );
        assert_eq!(view.summary.completion_velocity.completed_in_window, 3);
        assert_eq!(view.summary.completion_velocity.peak_per_hour, 1);
        assert_eq!(view.summary.completion_velocity.sparkline.len(), 3);
    }
}

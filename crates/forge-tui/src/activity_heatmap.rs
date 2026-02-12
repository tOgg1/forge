//! Compact per-loop trend visuals for run-rate, error-rate, and duration/latency.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopTrendBucket {
    pub timestamp_epoch_s: i64,
    pub run_count: u32,
    pub error_count: u32,
    pub avg_duration_ms: u64,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopTrendInput {
    pub loop_id: String,
    pub buckets: Vec<LoopTrendBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopTrendSummary {
    pub bucket_count: usize,
    pub total_runs: u64,
    pub total_errors: u64,
    pub error_rate_pct: u8,
    pub avg_duration_ms: u64,
    pub avg_latency_ms: u64,
    pub peak_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopTrendVisual {
    pub loop_id: String,
    pub run_rate_sparkline: String,
    pub error_rate_sparkline: String,
    pub duration_sparkline: String,
    pub latency_sparkline: String,
    pub activity_heatmap: String,
    pub summary: LoopTrendSummary,
}

#[must_use]
pub fn build_loop_activity_trends(
    loops: &[LoopTrendInput],
    max_buckets: usize,
) -> Vec<LoopTrendVisual> {
    let mut visuals = loops
        .iter()
        .filter_map(|loop_input| build_loop_visual(loop_input, max_buckets))
        .collect::<Vec<_>>();

    visuals.sort_by(|a, b| {
        b.summary
            .error_rate_pct
            .cmp(&a.summary.error_rate_pct)
            .then(b.summary.total_errors.cmp(&a.summary.total_errors))
            .then(a.loop_id.cmp(&b.loop_id))
    });
    visuals
}

fn build_loop_visual(input: &LoopTrendInput, max_buckets: usize) -> Option<LoopTrendVisual> {
    let loop_id = input.loop_id.trim().to_owned();
    if loop_id.is_empty() {
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

    let limit = if max_buckets == 0 { 24 } else { max_buckets };
    if buckets.len() > limit {
        let drop_count = buckets.len() - limit;
        buckets.drain(0..drop_count);
    }

    let run_values = buckets
        .iter()
        .map(|bucket| u64::from(bucket.run_count))
        .collect::<Vec<_>>();
    let error_values = buckets
        .iter()
        .map(|bucket| u64::from(bucket.error_count))
        .collect::<Vec<_>>();
    let error_rate_values = buckets
        .iter()
        .map(|bucket| error_rate_pct(bucket.run_count, bucket.error_count))
        .collect::<Vec<_>>();
    let duration_values = buckets
        .iter()
        .map(|bucket| bucket.avg_duration_ms)
        .collect::<Vec<_>>();
    let latency_values = buckets
        .iter()
        .map(|bucket| bucket.avg_latency_ms)
        .collect::<Vec<_>>();

    let run_max = run_values.iter().copied().max().unwrap_or(0);
    let error_max = error_values.iter().copied().max().unwrap_or(0);
    let latency_max = latency_values.iter().copied().max().unwrap_or(0);
    let activity_heatmap = buckets
        .iter()
        .map(|bucket| {
            heatmap_glyph(
                u64::from(bucket.run_count),
                u64::from(bucket.error_count),
                bucket.avg_latency_ms,
                run_max,
                error_max,
                latency_max,
            )
        })
        .collect::<String>();

    let total_runs = run_values.iter().sum::<u64>();
    let total_errors = error_values.iter().sum::<u64>();

    Some(LoopTrendVisual {
        loop_id,
        run_rate_sparkline: ascii_sparkline_u64(&run_values),
        error_rate_sparkline: ascii_sparkline_u64(&error_rate_values),
        duration_sparkline: ascii_sparkline_u64(&duration_values),
        latency_sparkline: ascii_sparkline_u64(&latency_values),
        activity_heatmap,
        summary: LoopTrendSummary {
            bucket_count: buckets.len(),
            total_runs,
            total_errors,
            error_rate_pct: aggregate_error_rate_pct(total_runs, total_errors),
            avg_duration_ms: mean_u64(&duration_values),
            avg_latency_ms: mean_u64(&latency_values),
            peak_latency_ms: latency_max,
        },
    })
}

fn error_rate_pct(run_count: u32, error_count: u32) -> u64 {
    if run_count == 0 {
        return if error_count > 0 { 100 } else { 0 };
    }
    let rate = u64::from(error_count)
        .saturating_mul(100)
        .saturating_div(u64::from(run_count));
    rate.min(100)
}

fn aggregate_error_rate_pct(total_runs: u64, total_errors: u64) -> u8 {
    if total_runs == 0 {
        return if total_errors > 0 { 100 } else { 0 };
    }
    total_errors
        .saturating_mul(100)
        .saturating_div(total_runs)
        .min(100) as u8
}

fn mean_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.iter().sum::<u64>() / values.len() as u64
}

fn heatmap_glyph(
    run_count: u64,
    error_count: u64,
    latency_ms: u64,
    run_max: u64,
    error_max: u64,
    latency_max: u64,
) -> char {
    if error_max > 0 {
        let error_level = level(error_count, error_max);
        if error_level >= 7 {
            return 'X';
        }
        if error_level >= 4 {
            return '!';
        }
    }

    let activity_level = level(run_count, run_max);
    let latency_level = level(latency_ms, latency_max);
    let glyph_levels = ['.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let index = activity_level
        .max(latency_level)
        .min(glyph_levels.len() - 1);
    glyph_levels[index]
}

fn level(value: u64, max_value: u64) -> usize {
    if max_value == 0 {
        return 0;
    }
    let max_level = 8usize;
    (value
        .saturating_mul(max_level as u64)
        .saturating_div(max_value)) as usize
}

fn ascii_sparkline_u64(values: &[u64]) -> String {
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
            let index = value.saturating_mul((levels.len() - 1) as u64) / max_value;
            levels[index as usize]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_loop_activity_trends, LoopTrendBucket, LoopTrendInput};

    fn trend_bucket(
        ts: i64,
        run_count: u32,
        error_count: u32,
        avg_duration_ms: u64,
        avg_latency_ms: u64,
    ) -> LoopTrendBucket {
        LoopTrendBucket {
            timestamp_epoch_s: ts,
            run_count,
            error_count,
            avg_duration_ms,
            avg_latency_ms,
        }
    }

    #[test]
    fn derives_compact_visuals_and_summary_per_loop() {
        let trends = build_loop_activity_trends(
            &[LoopTrendInput {
                loop_id: "loop-a".to_owned(),
                buckets: vec![
                    trend_bucket(10, 4, 0, 180, 95),
                    trend_bucket(20, 6, 1, 260, 140),
                    trend_bucket(30, 2, 0, 120, 80),
                ],
            }],
            12,
        );

        assert_eq!(trends.len(), 1);
        let trend = &trends[0];
        assert_eq!(trend.loop_id, "loop-a");
        assert_eq!(trend.summary.total_runs, 12);
        assert_eq!(trend.summary.total_errors, 1);
        assert_eq!(trend.summary.error_rate_pct, 8);
        assert_eq!(trend.summary.avg_duration_ms, 186);
        assert_eq!(trend.summary.avg_latency_ms, 105);
        assert_eq!(trend.summary.peak_latency_ms, 140);
        assert_eq!(trend.run_rate_sparkline.len(), 3);
        assert_eq!(trend.error_rate_sparkline.len(), 3);
        assert_eq!(trend.duration_sparkline.len(), 3);
        assert_eq!(trend.latency_sparkline.len(), 3);
        assert_eq!(trend.activity_heatmap.len(), 3);
    }

    #[test]
    fn max_buckets_uses_tail_window() {
        let trends = build_loop_activity_trends(
            &[LoopTrendInput {
                loop_id: "loop-tail".to_owned(),
                buckets: vec![
                    trend_bucket(10, 1, 0, 100, 100),
                    trend_bucket(20, 2, 0, 100, 100),
                    trend_bucket(30, 3, 0, 100, 100),
                    trend_bucket(40, 4, 0, 100, 100),
                    trend_bucket(50, 5, 0, 100, 100),
                ],
            }],
            3,
        );
        assert_eq!(trends[0].summary.bucket_count, 3);
        // Tail buckets are 3,4,5 after truncation to max_buckets=3.
        assert_eq!(trends[0].run_rate_sparkline, "+#@");
    }

    #[test]
    fn ranking_prefers_higher_error_rate_then_error_volume() {
        let trends = build_loop_activity_trends(
            &[
                LoopTrendInput {
                    loop_id: "loop-safe".to_owned(),
                    buckets: vec![trend_bucket(10, 10, 0, 120, 90)],
                },
                LoopTrendInput {
                    loop_id: "loop-risky".to_owned(),
                    buckets: vec![trend_bucket(10, 2, 1, 180, 130)],
                },
                LoopTrendInput {
                    loop_id: "loop-medium".to_owned(),
                    buckets: vec![trend_bucket(10, 10, 1, 180, 130)],
                },
            ],
            24,
        );
        assert_eq!(trends[0].loop_id, "loop-risky");
        assert_eq!(trends[1].loop_id, "loop-medium");
        assert_eq!(trends[2].loop_id, "loop-safe");
    }

    #[test]
    fn skips_empty_loop_ids_and_empty_buckets() {
        let trends = build_loop_activity_trends(
            &[
                LoopTrendInput {
                    loop_id: "   ".to_owned(),
                    buckets: vec![trend_bucket(10, 1, 0, 10, 10)],
                },
                LoopTrendInput {
                    loop_id: "loop-empty".to_owned(),
                    buckets: vec![],
                },
                LoopTrendInput {
                    loop_id: "loop-valid".to_owned(),
                    buckets: vec![trend_bucket(10, 0, 0, 0, 0)],
                },
            ],
            24,
        );
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].loop_id, "loop-valid");
    }

    #[test]
    fn heatmap_marks_error_spikes_with_alert_glyphs() {
        let trends = build_loop_activity_trends(
            &[LoopTrendInput {
                loop_id: "loop-alert".to_owned(),
                buckets: vec![
                    trend_bucket(10, 6, 0, 120, 80),
                    trend_bucket(20, 6, 3, 120, 80),
                    trend_bucket(30, 6, 6, 120, 80),
                ],
            }],
            24,
        );
        assert_eq!(trends[0].activity_heatmap.chars().nth(1), Some('!'));
        assert_eq!(trends[0].activity_heatmap.chars().nth(2), Some('X'));
    }
}

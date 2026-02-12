//! Timeline scrubber model with density/error heatmap and anchored seeking.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedLogLine {
    pub timestamp_ms: i64,
    pub line_index: usize,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineBucket {
    pub index: usize,
    pub start_timestamp_ms: i64,
    pub end_timestamp_ms: i64,
    pub first_line: Option<usize>,
    pub last_line: Option<usize>,
    pub line_count: usize,
    pub error_count: usize,
}

impl TimelineBucket {
    #[must_use]
    pub fn midpoint_line(&self) -> Option<usize> {
        match (self.first_line, self.last_line) {
            (Some(first), Some(last)) => Some(first.saturating_add(last.saturating_sub(first) / 2)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineHeatmap {
    pub buckets: Vec<TimelineBucket>,
    pub max_line_count: usize,
    pub max_error_count: usize,
}

impl TimelineHeatmap {
    /// Render one character per bucket. Higher activity => denser glyph.
    /// Buckets with errors override with `!`/`X`.
    #[must_use]
    pub fn render_density_line(&self) -> String {
        self.buckets
            .iter()
            .map(|bucket| bucket_glyph(bucket, self.max_line_count))
            .collect()
    }

    /// Render one marker per bucket where errors exist.
    #[must_use]
    pub fn render_error_line(&self) -> String {
        self.buckets
            .iter()
            .map(|bucket| if bucket.error_count > 0 { '!' } else { ' ' })
            .collect()
    }

    /// Render a caret marker aligned with currently selected bucket.
    #[must_use]
    pub fn render_selection_line(&self, selected_bucket: usize) -> String {
        let mut line = vec![' '; self.buckets.len()];
        if let Some(marker) = line.get_mut(selected_bucket) {
            *marker = '^';
        }
        line.into_iter().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorAnchor {
    pub row_in_viewport: usize,
}

impl CursorAnchor {
    #[must_use]
    pub fn new(row_in_viewport: usize) -> Self {
        Self { row_in_viewport }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekWindow {
    pub target_line: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub scroll_from_bottom: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrubResult {
    pub bucket_index: usize,
    pub window: SeekWindow,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineScrubber {
    pub heatmap: TimelineHeatmap,
    pub selected_bucket: usize,
}

impl TimelineScrubber {
    #[must_use]
    pub fn from_lines(lines: &[TimedLogLine], bucket_count: usize) -> Self {
        Self {
            heatmap: build_timeline_heatmap(lines, bucket_count),
            selected_bucket: 0,
        }
    }

    pub fn scrub_to_ratio(
        &mut self,
        ratio: f32,
        total_lines: usize,
        viewport_lines: usize,
        anchor: CursorAnchor,
    ) -> Option<ScrubResult> {
        let result = seek_to_ratio(
            &self.heatmap,
            ratio,
            total_lines,
            viewport_lines,
            anchor.row_in_viewport,
        )?;
        self.selected_bucket = result.bucket_index;
        Some(result)
    }
}

#[must_use]
pub fn build_timeline_heatmap(lines: &[TimedLogLine], bucket_count: usize) -> TimelineHeatmap {
    if bucket_count == 0 {
        return TimelineHeatmap::default();
    }

    if lines.is_empty() {
        return TimelineHeatmap {
            buckets: empty_buckets(bucket_count, 0, 1),
            max_line_count: 0,
            max_error_count: 0,
        };
    }

    let mut min_ts = i64::MAX;
    let mut max_ts = i64::MIN;
    for line in lines {
        min_ts = min_ts.min(line.timestamp_ms);
        max_ts = max_ts.max(line.timestamp_ms);
    }

    let span = (max_ts - min_ts).max(1);
    let bucket_width = ((span + bucket_count as i64) / bucket_count as i64).max(1);
    let mut buckets = empty_buckets(bucket_count, min_ts, bucket_width);

    for line in lines {
        let delta = line.timestamp_ms.saturating_sub(min_ts);
        let mut bucket_idx = usize::try_from(delta / bucket_width).unwrap_or(bucket_count - 1);
        if bucket_idx >= bucket_count {
            bucket_idx = bucket_count - 1;
        }
        let bucket = &mut buckets[bucket_idx];
        bucket.line_count = bucket.line_count.saturating_add(1);
        if line.is_error {
            bucket.error_count = bucket.error_count.saturating_add(1);
        }
        bucket.first_line = Some(match bucket.first_line {
            Some(first) => first.min(line.line_index),
            None => line.line_index,
        });
        bucket.last_line = Some(match bucket.last_line {
            Some(last) => last.max(line.line_index),
            None => line.line_index,
        });
    }

    let max_line_count = buckets
        .iter()
        .map(|bucket| bucket.line_count)
        .max()
        .unwrap_or(0);
    let max_error_count = buckets
        .iter()
        .map(|bucket| bucket.error_count)
        .max()
        .unwrap_or(0);

    TimelineHeatmap {
        buckets,
        max_line_count,
        max_error_count,
    }
}

#[must_use]
pub fn seek_to_ratio(
    heatmap: &TimelineHeatmap,
    ratio: f32,
    total_lines: usize,
    viewport_lines: usize,
    anchor_row: usize,
) -> Option<ScrubResult> {
    if heatmap.buckets.is_empty() || total_lines == 0 {
        return None;
    }

    let bucket_index = ratio_to_bucket(ratio, heatmap.buckets.len());
    let target_line = resolve_target_line(&heatmap.buckets, bucket_index, total_lines, ratio);
    let window = anchored_seek(total_lines, viewport_lines, target_line, anchor_row);

    Some(ScrubResult {
        bucket_index,
        window,
    })
}

#[must_use]
pub fn anchored_seek(
    total_lines: usize,
    viewport_lines: usize,
    target_line: usize,
    anchor_row: usize,
) -> SeekWindow {
    if total_lines == 0 {
        return SeekWindow {
            target_line: 0,
            start_line: 0,
            end_line: 0,
            scroll_from_bottom: 0,
        };
    }

    let target_line = target_line.min(total_lines.saturating_sub(1));
    let viewport_lines = viewport_lines.max(1);
    let anchor_row = anchor_row.min(viewport_lines.saturating_sub(1));
    let max_start = total_lines.saturating_sub(viewport_lines);
    let start_line = target_line.saturating_sub(anchor_row).min(max_start);
    let end_line = start_line.saturating_add(viewport_lines).min(total_lines);
    let scroll_from_bottom = total_lines.saturating_sub(end_line);

    SeekWindow {
        target_line,
        start_line,
        end_line,
        scroll_from_bottom,
    }
}

fn ratio_to_bucket(ratio: f32, bucket_count: usize) -> usize {
    if bucket_count == 0 {
        return 0;
    }
    let clamped = ratio.clamp(0.0, 1.0);
    let idx = (clamped * bucket_count as f32).floor() as usize;
    idx.min(bucket_count.saturating_sub(1))
}

fn resolve_target_line(
    buckets: &[TimelineBucket],
    preferred_bucket: usize,
    total_lines: usize,
    ratio: f32,
) -> usize {
    if let Some(midpoint) = buckets
        .get(preferred_bucket)
        .and_then(TimelineBucket::midpoint_line)
    {
        return midpoint.min(total_lines.saturating_sub(1));
    }

    for distance in 1..buckets.len() {
        if preferred_bucket >= distance {
            let candidate = preferred_bucket - distance;
            if let Some(midpoint) = buckets[candidate].midpoint_line() {
                return midpoint.min(total_lines.saturating_sub(1));
            }
        }
        let candidate = preferred_bucket + distance;
        if candidate < buckets.len() {
            if let Some(midpoint) = buckets[candidate].midpoint_line() {
                return midpoint.min(total_lines.saturating_sub(1));
            }
        }
    }

    // Fully empty timeline: degrade to ratio-based line seek.
    let clamped = ratio.clamp(0.0, 1.0);
    ((clamped * total_lines.saturating_sub(1) as f32).round() as usize)
        .min(total_lines.saturating_sub(1))
}

fn empty_buckets(bucket_count: usize, min_ts: i64, bucket_width: i64) -> Vec<TimelineBucket> {
    (0..bucket_count)
        .map(|idx| {
            let start = min_ts.saturating_add((idx as i64).saturating_mul(bucket_width));
            let end = start.saturating_add(bucket_width.saturating_sub(1));
            TimelineBucket {
                index: idx,
                start_timestamp_ms: start,
                end_timestamp_ms: end,
                first_line: None,
                last_line: None,
                line_count: 0,
                error_count: 0,
            }
        })
        .collect()
}

fn bucket_glyph(bucket: &TimelineBucket, max_line_count: usize) -> char {
    const LEVELS: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    if bucket.line_count == 0 || max_line_count == 0 {
        return ' ';
    }

    let scaled = bucket
        .line_count
        .saturating_mul(LEVELS.len().saturating_sub(1))
        / max_line_count.max(1);
    let density = LEVELS[scaled.min(LEVELS.len().saturating_sub(1))];
    if bucket.error_count == 0 {
        density
    } else if scaled >= 7 {
        'X'
    } else {
        '!'
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{
        anchored_seek, build_timeline_heatmap, ratio_to_bucket, seek_to_ratio, CursorAnchor,
        TimedLogLine, TimelineScrubber,
    };

    fn sample_lines() -> Vec<TimedLogLine> {
        vec![
            TimedLogLine {
                timestamp_ms: 1_000,
                line_index: 0,
                is_error: false,
            },
            TimedLogLine {
                timestamp_ms: 1_200,
                line_index: 1,
                is_error: false,
            },
            TimedLogLine {
                timestamp_ms: 1_300,
                line_index: 2,
                is_error: true,
            },
            TimedLogLine {
                timestamp_ms: 2_100,
                line_index: 3,
                is_error: false,
            },
            TimedLogLine {
                timestamp_ms: 2_200,
                line_index: 4,
                is_error: true,
            },
            TimedLogLine {
                timestamp_ms: 2_300,
                line_index: 5,
                is_error: false,
            },
        ]
    }

    #[test]
    fn builds_bucket_counts_and_ranges() {
        let heatmap = build_timeline_heatmap(&sample_lines(), 4);
        assert_eq!(heatmap.buckets.len(), 4);
        assert_eq!(heatmap.max_line_count, 3);
        assert_eq!(heatmap.max_error_count, 1);
        let non_empty: Vec<_> = heatmap
            .buckets
            .iter()
            .filter(|bucket| bucket.line_count > 0)
            .collect();
        assert_eq!(non_empty.len(), 2);
        assert_eq!(non_empty[0].line_count, 3);
        assert_eq!(non_empty[1].line_count, 3);
        assert_eq!(non_empty[0].first_line, Some(0));
        assert_eq!(non_empty[1].last_line, Some(5));
    }

    #[test]
    fn empty_input_produces_empty_buckets() {
        let heatmap = build_timeline_heatmap(&[], 5);
        assert_eq!(heatmap.buckets.len(), 5);
        assert_eq!(heatmap.max_line_count, 0);
        assert_eq!(heatmap.render_density_line(), "     ");
        assert_eq!(heatmap.render_error_line(), "     ");
    }

    #[test]
    fn render_density_marks_error_buckets() {
        let heatmap = build_timeline_heatmap(&sample_lines(), 4);
        let density = heatmap.render_density_line();
        let errors = heatmap.render_error_line();
        assert_eq!(density.len(), 4);
        assert_eq!(errors.len(), 4);
        assert!(density.contains('!') || density.contains('X'));
        assert!(errors.contains('!'));
    }

    #[test]
    fn selection_line_marks_only_selected_bucket() {
        let heatmap = build_timeline_heatmap(&sample_lines(), 6);
        let marker = heatmap.render_selection_line(3);
        assert_eq!(marker.len(), 6);
        assert_eq!(marker.chars().filter(|ch| *ch == '^').count(), 1);
        assert_eq!(marker.chars().nth(3), Some('^'));
    }

    #[test]
    fn ratio_to_bucket_clamps_edges() {
        assert_eq!(ratio_to_bucket(-1.0, 8), 0);
        assert_eq!(ratio_to_bucket(0.0, 8), 0);
        assert_eq!(ratio_to_bucket(0.5, 8), 4);
        assert_eq!(ratio_to_bucket(1.0, 8), 7);
        assert_eq!(ratio_to_bucket(9.0, 8), 7);
    }

    #[test]
    fn anchored_seek_preserves_anchor_row_when_possible() {
        let window = anchored_seek(1_000, 20, 500, 9);
        assert_eq!(window.start_line, 491);
        assert_eq!(window.target_line.saturating_sub(window.start_line), 9);
        assert_eq!(window.end_line, 511);
        assert_eq!(window.scroll_from_bottom, 489);

        let second = anchored_seek(1_000, 20, 540, 9);
        assert_eq!(second.target_line.saturating_sub(second.start_line), 9);
    }

    #[test]
    fn anchored_seek_clamps_at_head_and_tail() {
        let top = anchored_seek(100, 20, 2, 10);
        assert_eq!(top.start_line, 0);
        assert_eq!(top.end_line, 20);

        let tail = anchored_seek(100, 20, 99, 10);
        assert_eq!(tail.start_line, 80);
        assert_eq!(tail.end_line, 100);
        assert_eq!(tail.scroll_from_bottom, 0);
    }

    #[test]
    fn seek_to_ratio_uses_bucket_midpoint() {
        let heatmap = build_timeline_heatmap(&sample_lines(), 4);
        let Some(result) = seek_to_ratio(&heatmap, 0.0, 6, 3, 1) else {
            panic!("expected seek result");
        };
        assert_eq!(result.bucket_index, 0);
        assert_eq!(result.window.target_line, 1);
        assert_eq!(result.window.start_line, 0);
    }

    #[test]
    fn seek_to_ratio_falls_back_when_bucket_is_empty() {
        let lines = vec![
            TimedLogLine {
                timestamp_ms: 1_000,
                line_index: 0,
                is_error: false,
            },
            TimedLogLine {
                timestamp_ms: 4_000,
                line_index: 1_000,
                is_error: true,
            },
        ];
        let heatmap = build_timeline_heatmap(&lines, 8);
        let Some(result) = seek_to_ratio(&heatmap, 0.4, 1_001, 30, 10) else {
            panic!("expected seek result");
        };
        assert!(result.window.target_line == 0 || result.window.target_line == 1_000);
    }

    #[test]
    fn timeline_scrubber_updates_selected_bucket() {
        let lines = sample_lines();
        let mut scrubber = TimelineScrubber::from_lines(&lines, 5);
        assert_eq!(scrubber.selected_bucket, 0);
        let Some(result) = scrubber.scrub_to_ratio(0.75, lines.len(), 3, CursorAnchor::new(1))
        else {
            panic!("expected scrub result");
        };
        assert_eq!(scrubber.selected_bucket, result.bucket_index);
    }

    #[test]
    fn large_log_scrub_produces_valid_windows() {
        let total = 200_000usize;
        let lines: Vec<TimedLogLine> = (0..total)
            .map(|idx| TimedLogLine {
                timestamp_ms: 1_700_000_000_000i64 + (idx as i64 * 15),
                line_index: idx,
                is_error: idx % 47 == 0,
            })
            .collect();
        let mut scrubber = TimelineScrubber::from_lines(&lines, 120);
        let checkpoints = [0.0_f32, 0.05, 0.21, 0.5, 0.88, 1.0];
        for ratio in checkpoints {
            let Some(result) = scrubber.scrub_to_ratio(ratio, total, 80, CursorAnchor::new(24))
            else {
                panic!("expected scrub result");
            };
            assert!(result.window.start_line <= result.window.target_line);
            assert!(result.window.end_line <= total);
            assert!(result.window.scroll_from_bottom <= total);
        }
    }
}

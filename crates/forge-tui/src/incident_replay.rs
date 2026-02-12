//! Incident replay model with timeline reconstruction, time controls, and annotations.

use std::collections::BTreeSet;

use crate::timeline_scrubber::{build_timeline_heatmap, TimedLogLine, TimelineHeatmap};

pub const INCIDENT_REPLAY_DEFAULT_BUCKETS: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplayEventKind {
    Log,
    Task,
    System,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplaySeverity {
    Info,
    Warn,
    Error,
    Critical,
}

impl ReplaySeverity {
    #[must_use]
    pub fn is_error(self) -> bool {
        matches!(self, Self::Error | Self::Critical)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedIncidentEvent {
    pub event_id: String,
    pub timestamp_ms: i64,
    pub kind: ReplayEventKind,
    pub severity: ReplaySeverity,
    pub source: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayAnnotation {
    pub annotation_id: String,
    pub timestamp_ms: i64,
    pub author: String,
    pub body: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySpeed {
    X1,
    X5,
    X10,
    X30,
}

impl ReplaySpeed {
    #[must_use]
    pub fn multiplier(self) -> i64 {
        match self {
            Self::X1 => 1,
            Self::X5 => 5,
            Self::X10 => 10,
            Self::X30 => 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayControls {
    pub range_start_ms: i64,
    pub range_end_ms: i64,
    pub cursor_ms: i64,
    pub playing: bool,
    pub speed: ReplaySpeed,
}

impl ReplayControls {
    #[must_use]
    pub fn new(range_start_ms: i64, range_end_ms: i64) -> Self {
        let (range_start_ms, range_end_ms) = normalize_time_range(range_start_ms, range_end_ms);
        Self {
            range_start_ms,
            range_end_ms,
            cursor_ms: range_start_ms,
            playing: false,
            speed: ReplaySpeed::X1,
        }
    }

    #[must_use]
    pub fn for_events(events: &[RecordedIncidentEvent]) -> Self {
        if events.is_empty() {
            return Self::new(0, 1);
        }
        let (start_ms, end_ms) = event_time_range(events);
        Self::new(start_ms, end_ms)
    }

    #[must_use]
    pub fn clamped_to(self, start_ms: i64, end_ms: i64) -> Self {
        let (range_start_ms, range_end_ms) = normalize_time_range(start_ms, end_ms);
        let cursor_ms = self.cursor_ms.clamp(range_start_ms, range_end_ms);
        Self {
            range_start_ms,
            range_end_ms,
            cursor_ms,
            playing: self.playing && cursor_ms < range_end_ms,
            speed: self.speed,
        }
    }

    pub fn seek_ratio(&mut self, ratio: f32) {
        let clamped = ratio.clamp(0.0, 1.0);
        let span = self.range_end_ms.saturating_sub(self.range_start_ms);
        let delta = (span as f64 * f64::from(clamped)).round() as i64;
        self.cursor_ms = self
            .range_start_ms
            .saturating_add(delta)
            .clamp(self.range_start_ms, self.range_end_ms);
    }

    pub fn step_prev_event(&mut self, events: &[RecordedIncidentEvent]) -> bool {
        let Some(target) = events
            .iter()
            .filter(|event| event.timestamp_ms < self.cursor_ms)
            .max_by_key(|event| event.timestamp_ms)
            .map(|event| event.timestamp_ms)
        else {
            return false;
        };
        self.cursor_ms = target;
        true
    }

    pub fn step_next_event(&mut self, events: &[RecordedIncidentEvent]) -> bool {
        let Some(target) = events
            .iter()
            .filter(|event| event.timestamp_ms > self.cursor_ms)
            .min_by_key(|event| event.timestamp_ms)
            .map(|event| event.timestamp_ms)
        else {
            return false;
        };
        self.cursor_ms = target;
        true
    }

    pub fn advance_playback(&mut self, elapsed_ms: i64) {
        if !self.playing {
            return;
        }

        let elapsed_ms = elapsed_ms.max(0);
        let delta = elapsed_ms.saturating_mul(self.speed.multiplier());
        self.cursor_ms = self.cursor_ms.saturating_add(delta).min(self.range_end_ms);
        if self.cursor_ms >= self.range_end_ms {
            self.playing = false;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayHotspot {
    pub start_ms: i64,
    pub end_ms: i64,
    pub event_count: usize,
    pub error_count: usize,
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IncidentReplaySnapshot {
    pub controls: Option<ReplayControls>,
    pub timeline: TimelineHeatmap,
    pub hotspots: Vec<ReplayHotspot>,
    pub visible_events: Vec<RecordedIncidentEvent>,
    pub visible_annotations: Vec<ReplayAnnotation>,
    pub total_events: usize,
    pub dropped_duplicate_events: usize,
}

#[must_use]
pub fn build_incident_replay_snapshot(
    events: &[RecordedIncidentEvent],
    annotations: &[ReplayAnnotation],
    controls: ReplayControls,
    bucket_count: usize,
) -> IncidentReplaySnapshot {
    let (events, dropped_duplicate_events) = normalize_events(events);
    let annotations = normalize_annotations(annotations);
    if events.is_empty() {
        return IncidentReplaySnapshot {
            controls: Some(controls.clamped_to(0, 1)),
            timeline: build_timeline_heatmap(&[], bucket_count.max(1)),
            hotspots: Vec::new(),
            visible_events: Vec::new(),
            visible_annotations: Vec::new(),
            total_events: 0,
            dropped_duplicate_events,
        };
    }

    let (start_ms, end_ms) = event_time_range(&events);
    let controls = controls.clamped_to(start_ms, end_ms);
    let timeline = reconstruct_timeline(&events, bucket_count.max(1));
    let hotspots = detect_replay_hotspots(&timeline);
    let visible_events = events
        .iter()
        .filter(|event| {
            event.timestamp_ms >= controls.range_start_ms
                && event.timestamp_ms <= controls.cursor_ms
        })
        .cloned()
        .collect::<Vec<_>>();
    let visible_annotations = annotations
        .iter()
        .filter(|annotation| {
            annotation.timestamp_ms >= controls.range_start_ms
                && annotation.timestamp_ms <= controls.cursor_ms
        })
        .cloned()
        .collect::<Vec<_>>();

    IncidentReplaySnapshot {
        controls: Some(controls),
        timeline,
        hotspots,
        visible_events,
        visible_annotations,
        total_events: events.len(),
        dropped_duplicate_events,
    }
}

#[must_use]
pub fn reconstruct_timeline(
    events: &[RecordedIncidentEvent],
    bucket_count: usize,
) -> TimelineHeatmap {
    let lines = events
        .iter()
        .enumerate()
        .map(|(line_index, event)| TimedLogLine {
            timestamp_ms: event.timestamp_ms,
            line_index,
            is_error: event.severity.is_error(),
        })
        .collect::<Vec<_>>();

    build_timeline_heatmap(&lines, bucket_count.max(1))
}

#[must_use]
pub fn detect_replay_hotspots(timeline: &TimelineHeatmap) -> Vec<ReplayHotspot> {
    if timeline.buckets.is_empty() {
        return Vec::new();
    }

    let dense_threshold = std::cmp::max(2, timeline.max_line_count.saturating_mul(2) / 3);
    let mut hotspots = timeline
        .buckets
        .iter()
        .filter(|bucket| bucket.line_count > 0)
        .filter(|bucket| bucket.error_count > 0 || bucket.line_count >= dense_threshold)
        .map(|bucket| ReplayHotspot {
            start_ms: bucket.start_timestamp_ms,
            end_ms: bucket.end_timestamp_ms,
            event_count: bucket.line_count,
            error_count: bucket.error_count,
            score: bucket
                .error_count
                .saturating_mul(10)
                .saturating_add(bucket.line_count),
        })
        .collect::<Vec<_>>();

    hotspots.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then(left.start_ms.cmp(&right.start_ms))
    });
    hotspots
}

fn normalize_events(events: &[RecordedIncidentEvent]) -> (Vec<RecordedIncidentEvent>, usize) {
    let mut normalized = events
        .iter()
        .map(|event| RecordedIncidentEvent {
            event_id: normalize_required(&event.event_id),
            timestamp_ms: event.timestamp_ms.max(0),
            kind: event.kind,
            severity: event.severity,
            source: normalize_required(&event.source),
            summary: normalize_summary(&event.summary),
        })
        .filter(|event| !event.event_id.is_empty() && !event.summary.is_empty())
        .collect::<Vec<_>>();

    normalized.sort_by(|left, right| {
        left.timestamp_ms
            .cmp(&right.timestamp_ms)
            .then(left.event_id.cmp(&right.event_id))
    });

    let mut deduped = Vec::with_capacity(normalized.len());
    let mut seen = BTreeSet::new();
    let mut dropped = 0usize;
    for event in normalized {
        if !seen.insert(event.event_id.clone()) {
            dropped = dropped.saturating_add(1);
            continue;
        }
        deduped.push(event);
    }

    (deduped, dropped)
}

fn normalize_annotations(annotations: &[ReplayAnnotation]) -> Vec<ReplayAnnotation> {
    let mut normalized = annotations
        .iter()
        .map(|annotation| ReplayAnnotation {
            annotation_id: normalize_required(&annotation.annotation_id),
            timestamp_ms: annotation.timestamp_ms.max(0),
            author: normalize_required(&annotation.author),
            body: normalize_summary(&annotation.body),
            tags: normalize_tags(&annotation.tags),
        })
        .filter(|annotation| !annotation.annotation_id.is_empty() && !annotation.body.is_empty())
        .collect::<Vec<_>>();

    normalized.sort_by(|left, right| {
        left.timestamp_ms
            .cmp(&right.timestamp_ms)
            .then(left.annotation_id.cmp(&right.annotation_id))
    });
    normalized
}

fn event_time_range(events: &[RecordedIncidentEvent]) -> (i64, i64) {
    let start_ms = events.first().map(|event| event.timestamp_ms).unwrap_or(0);
    let end_ms = events.last().map(|event| event.timestamp_ms).unwrap_or(1);
    normalize_time_range(start_ms, end_ms)
}

fn normalize_time_range(start_ms: i64, end_ms: i64) -> (i64, i64) {
    let start_ms = start_ms.max(0);
    let end_ms = end_ms.max(0);
    if start_ms <= end_ms {
        (start_ms, end_ms.max(start_ms.saturating_add(1)))
    } else {
        (end_ms, start_ms.max(end_ms.saturating_add(1)))
    }
}

fn normalize_required(value: &str) -> String {
    value.trim().to_owned()
}

fn normalize_summary(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut tags = tags
        .iter()
        .map(|tag| normalize_required(tag))
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    tags.sort();
    tags
}

#[cfg(test)]
#[path = "incident_replay_tests.rs"]
mod tests;

//! Root-cause waterfall model for tracing cascading failures.

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WaterfallSeverity {
    Info,
    Warn,
    Error,
    Critical,
}

impl WaterfallSeverity {
    #[must_use]
    fn rank(self) -> i32 {
        match self {
            Self::Info => 0,
            Self::Warn => 1,
            Self::Error => 2,
            Self::Critical => 3,
        }
    }

    #[must_use]
    fn bar_glyph(self) -> char {
        match self {
            Self::Info => '-',
            Self::Warn => '=',
            Self::Error => '#',
            Self::Critical => 'X',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterfallEvent {
    pub event_id: String,
    pub loop_id: String,
    pub timestamp_ms: i64,
    pub duration_ms: i64,
    pub severity: WaterfallSeverity,
    pub summary: String,
    pub upstream_event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterfallRow {
    pub event_id: String,
    pub loop_id: String,
    pub depth: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub severity: WaterfallSeverity,
    pub in_root_path: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RootCauseWaterfall {
    pub root_event_id: Option<String>,
    pub terminal_event_id: Option<String>,
    pub root_path: Vec<String>,
    pub hop_count: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub rows: Vec<WaterfallRow>,
}

#[must_use]
pub fn build_root_cause_waterfall(
    events: &[WaterfallEvent],
    timeline_width: usize,
) -> RootCauseWaterfall {
    let normalized = normalize_events(events);
    if normalized.is_empty() {
        return RootCauseWaterfall::default();
    }

    let event_by_id = normalized
        .iter()
        .map(|event| (event.event_id.clone(), event.clone()))
        .collect::<HashMap<_, _>>();
    let downstream = build_downstream_edges(&normalized);
    let terminal_event_id = pick_terminal_event(&normalized).map(|event| event.event_id.clone());
    let root_path = terminal_event_id
        .as_ref()
        .map(|terminal| trace_root_path(&event_by_id, terminal))
        .unwrap_or_default();
    let root_event_id = root_path.first().cloned();
    let path_set = root_path.iter().cloned().collect::<HashSet<_>>();

    let (start_ms, end_ms) = time_bounds(&normalized);
    let depth_by_id = root_event_id
        .as_ref()
        .map(|root| compute_depths(root, &downstream))
        .unwrap_or_default();

    let width = timeline_width.max(8);
    let mut rows = normalized
        .iter()
        .map(|event| {
            let (start_col, end_col) = scale_range(
                event.timestamp_ms,
                event_end_ms(event),
                start_ms,
                end_ms,
                width,
            );
            WaterfallRow {
                event_id: event.event_id.clone(),
                loop_id: event.loop_id.clone(),
                depth: depth_by_id.get(&event.event_id).copied().unwrap_or(0),
                start_col,
                end_col,
                severity: event.severity,
                in_root_path: path_set.contains(&event.event_id),
                summary: event.summary.clone(),
            }
        })
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        a.start_col
            .cmp(&b.start_col)
            .then(a.depth.cmp(&b.depth))
            .then(a.event_id.cmp(&b.event_id))
    });

    RootCauseWaterfall {
        root_event_id,
        terminal_event_id,
        hop_count: root_path.len().saturating_sub(1),
        root_path,
        start_ms,
        end_ms,
        rows,
    }
}

#[must_use]
pub fn render_root_cause_waterfall_lines(
    waterfall: &RootCauseWaterfall,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    if waterfall.rows.is_empty() {
        lines.push(truncate("Waterfall: no events", width));
        return lines;
    }

    lines.push(truncate(
        &format!(
            "Waterfall root={} terminal={} hops={} span={}ms",
            waterfall.root_event_id.as_deref().unwrap_or("-"),
            waterfall.terminal_event_id.as_deref().unwrap_or("-"),
            waterfall.hop_count,
            waterfall.end_ms.saturating_sub(waterfall.start_ms),
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }
    lines.push(truncate(
        "legend: R root  T terminal  * path  #/X failure",
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    let bar_width = width.saturating_sub(34).max(8);
    for row in &waterfall.rows {
        if lines.len() >= max_rows {
            break;
        }
        let marker = event_marker(row, waterfall);
        let bar = render_bar(row, bar_width);
        let line = format!(
            "{} d{:02} {:<12} {} {}",
            marker,
            row.depth,
            trim_to_len(&row.loop_id, 12),
            bar,
            row.summary
        );
        lines.push(truncate(&line, width));
    }
    lines
}

fn normalize_events(events: &[WaterfallEvent]) -> Vec<WaterfallEvent> {
    let mut normalized = events
        .iter()
        .filter_map(|event| {
            let event_id = event.event_id.trim().to_ascii_lowercase();
            if event_id.is_empty() {
                return None;
            }
            Some(WaterfallEvent {
                event_id,
                loop_id: normalize_display(&event.loop_id, "loop"),
                timestamp_ms: event.timestamp_ms.max(0),
                duration_ms: event.duration_ms.max(0),
                severity: event.severity,
                summary: normalize_display(&event.summary, "event"),
                upstream_event_ids: event
                    .upstream_event_ids
                    .iter()
                    .map(|id| id.trim().to_ascii_lowercase())
                    .filter(|id| !id.is_empty())
                    .collect(),
            })
        })
        .collect::<Vec<_>>();

    normalized.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then(a.event_id.cmp(&b.event_id))
    });

    let known_ids = normalized
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<HashSet<_>>();
    for event in &mut normalized {
        event
            .upstream_event_ids
            .retain(|upstream| known_ids.contains(upstream));
        event.upstream_event_ids.sort();
        event.upstream_event_ids.dedup();
    }

    normalized
}

fn normalize_display(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn event_end_ms(event: &WaterfallEvent) -> i64 {
    event.timestamp_ms.saturating_add(event.duration_ms.max(1))
}

fn time_bounds(events: &[WaterfallEvent]) -> (i64, i64) {
    let start_ms = events.first().map(|event| event.timestamp_ms).unwrap_or(0);
    let end_ms = events
        .iter()
        .map(event_end_ms)
        .max()
        .unwrap_or(start_ms.saturating_add(1))
        .max(start_ms.saturating_add(1));
    (start_ms, end_ms)
}

fn build_downstream_edges(events: &[WaterfallEvent]) -> HashMap<String, Vec<String>> {
    let mut downstream = HashMap::<String, Vec<String>>::new();
    for event in events {
        downstream.entry(event.event_id.clone()).or_default();
    }
    for event in events {
        for upstream in &event.upstream_event_ids {
            downstream
                .entry(upstream.clone())
                .or_default()
                .push(event.event_id.clone());
        }
    }
    for children in downstream.values_mut() {
        children.sort();
        children.dedup();
    }
    downstream
}

fn pick_terminal_event(events: &[WaterfallEvent]) -> Option<&WaterfallEvent> {
    events.iter().max_by(|a, b| {
        let a_fail = matches!(
            a.severity,
            WaterfallSeverity::Error | WaterfallSeverity::Critical
        );
        let b_fail = matches!(
            b.severity,
            WaterfallSeverity::Error | WaterfallSeverity::Critical
        );
        a_fail
            .cmp(&b_fail)
            .then(a.timestamp_ms.cmp(&b.timestamp_ms))
            .then(a.severity.rank().cmp(&b.severity.rank()))
            .then(a.event_id.cmp(&b.event_id))
    })
}

fn trace_root_path(
    events: &HashMap<String, WaterfallEvent>,
    terminal_event_id: &str,
) -> Vec<String> {
    let mut path_rev = Vec::new();
    let mut visited = HashSet::new();
    let mut current = terminal_event_id.to_owned();

    while visited.insert(current.clone()) {
        path_rev.push(current.clone());
        let Some(event) = events.get(&current) else {
            break;
        };
        let Some(next) = event
            .upstream_event_ids
            .iter()
            .filter_map(|upstream| events.get(upstream))
            .max_by(|a, b| {
                a.severity
                    .rank()
                    .cmp(&b.severity.rank())
                    .then(b.timestamp_ms.cmp(&a.timestamp_ms))
                    .then(a.event_id.cmp(&b.event_id))
            })
        else {
            break;
        };
        current = next.event_id.clone();
    }

    path_rev.reverse();
    path_rev
}

fn compute_depths(
    root_event_id: &str,
    downstream: &HashMap<String, Vec<String>>,
) -> HashMap<String, usize> {
    let mut depths = HashMap::new();
    let mut queue = VecDeque::new();
    queue.push_back((root_event_id.to_owned(), 0usize));

    while let Some((event_id, depth)) = queue.pop_front() {
        let previous = depths.get(&event_id).copied();
        if previous.is_some_and(|prev| prev <= depth) {
            continue;
        }
        depths.insert(event_id.clone(), depth);
        if let Some(children) = downstream.get(&event_id) {
            for child in children {
                queue.push_back((child.clone(), depth.saturating_add(1)));
            }
        }
    }
    depths
}

fn scale_range(
    start_ms: i64,
    end_ms: i64,
    min_ms: i64,
    max_ms: i64,
    width: usize,
) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }
    let span = max_ms.saturating_sub(min_ms).max(1);
    let width_i64 = width.saturating_sub(1) as i64;
    let start = start_ms.saturating_sub(min_ms).clamp(0, span);
    let end = end_ms.saturating_sub(min_ms).clamp(0, span);
    let start_col = (start.saturating_mul(width_i64).saturating_div(span)) as usize;
    let mut end_col = (end.saturating_mul(width_i64).saturating_div(span)) as usize;
    if end_col < start_col {
        end_col = start_col;
    }
    (start_col, end_col)
}

fn event_marker(row: &WaterfallRow, waterfall: &RootCauseWaterfall) -> char {
    if waterfall.root_event_id.as_deref() == Some(row.event_id.as_str()) {
        'R'
    } else if waterfall.terminal_event_id.as_deref() == Some(row.event_id.as_str()) {
        'T'
    } else if row.in_root_path {
        '*'
    } else if matches!(
        row.severity,
        WaterfallSeverity::Error | WaterfallSeverity::Critical
    ) {
        '!'
    } else {
        ' '
    }
}

fn render_bar(row: &WaterfallRow, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut chars = vec![' '; width];
    let start = row.start_col.min(width.saturating_sub(1));
    let end = row.end_col.min(width.saturating_sub(1));
    for ch in chars.iter_mut().take(end + 1).skip(start) {
        *ch = row.severity.bar_glyph();
    }
    if row.in_root_path {
        chars[start] = '|';
        chars[end] = '|';
    }
    chars.iter().collect()
}

fn trim_to_len(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn truncate(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_root_cause_waterfall, render_root_cause_waterfall_lines, WaterfallEvent,
        WaterfallSeverity,
    };

    fn event(
        id: &str,
        loop_id: &str,
        ts: i64,
        dur: i64,
        severity: WaterfallSeverity,
        summary: &str,
        upstream: &[&str],
    ) -> WaterfallEvent {
        WaterfallEvent {
            event_id: id.to_owned(),
            loop_id: loop_id.to_owned(),
            timestamp_ms: ts,
            duration_ms: dur,
            severity,
            summary: summary.to_owned(),
            upstream_event_ids: upstream.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    #[test]
    fn traces_three_hop_root_path() {
        let waterfall = build_root_cause_waterfall(
            &[
                event(
                    "a",
                    "loop-a",
                    10,
                    20,
                    WaterfallSeverity::Warn,
                    "trigger",
                    &[],
                ),
                event(
                    "b",
                    "loop-b",
                    40,
                    30,
                    WaterfallSeverity::Warn,
                    "dependency fail",
                    &["a"],
                ),
                event(
                    "c",
                    "loop-c",
                    90,
                    35,
                    WaterfallSeverity::Error,
                    "service retry",
                    &["b"],
                ),
                event(
                    "d",
                    "loop-d",
                    150,
                    20,
                    WaterfallSeverity::Critical,
                    "terminal crash",
                    &["c"],
                ),
            ],
            48,
        );
        assert_eq!(waterfall.root_event_id.as_deref(), Some("a"));
        assert_eq!(waterfall.terminal_event_id.as_deref(), Some("d"));
        assert_eq!(waterfall.root_path, vec!["a", "b", "c", "d"]);
        assert_eq!(waterfall.hop_count, 3);
    }

    #[test]
    fn terminal_prefers_latest_failing_event() {
        let waterfall = build_root_cause_waterfall(
            &[
                event(
                    "x",
                    "loop-x",
                    10,
                    20,
                    WaterfallSeverity::Critical,
                    "old crash",
                    &[],
                ),
                event(
                    "y",
                    "loop-y",
                    50,
                    10,
                    WaterfallSeverity::Error,
                    "new crash",
                    &[],
                ),
            ],
            48,
        );
        assert_eq!(waterfall.terminal_event_id.as_deref(), Some("y"));
    }

    #[test]
    fn ignores_unknown_upstream_ids() {
        let waterfall = build_root_cause_waterfall(
            &[event(
                "solo",
                "loop-1",
                10,
                5,
                WaterfallSeverity::Error,
                "single failure",
                &["missing"],
            )],
            20,
        );
        assert_eq!(waterfall.root_path, vec!["solo"]);
        assert_eq!(waterfall.hop_count, 0);
    }

    #[test]
    fn render_lines_include_header_and_bar_rows() {
        let waterfall = build_root_cause_waterfall(
            &[
                event(
                    "a",
                    "loop-a",
                    10,
                    10,
                    WaterfallSeverity::Warn,
                    "upstream",
                    &[],
                ),
                event(
                    "b",
                    "loop-b",
                    30,
                    10,
                    WaterfallSeverity::Critical,
                    "downstream fail",
                    &["a"],
                ),
            ],
            32,
        );
        let lines = render_root_cause_waterfall_lines(&waterfall, 120, 8);
        assert!(lines[0].contains("Waterfall root="));
        assert!(lines[1].contains("legend"));
        assert!(lines.iter().any(|line| line.contains("loop-a")));
        assert!(lines.iter().any(|line| line.contains("loop-b")));
    }

    #[test]
    fn empty_input_returns_empty_waterfall() {
        let waterfall = build_root_cause_waterfall(&[], 40);
        assert!(waterfall.rows.is_empty());
        let lines = render_root_cause_waterfall_lines(&waterfall, 60, 4);
        assert_eq!(lines[0], "Waterfall: no events");
    }
}

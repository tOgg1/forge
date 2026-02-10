use std::collections::HashMap;

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Heatmap mode & sort (Go parity: heatmapMode, heatmapSort)
// ---------------------------------------------------------------------------

/// Heatmap grouping mode: rows represent agents or topics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatmapMode {
    Agents,
    Topics,
}

impl HeatmapMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Topics => "topics",
        }
    }

    #[must_use]
    pub fn toggle(self) -> Self {
        match self {
            Self::Agents => Self::Topics,
            Self::Topics => Self::Agents,
        }
    }
}

/// Heatmap row sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatmapSort {
    Total,
    Name,
    Peak,
    Recency,
}

impl HeatmapSort {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Name => "name",
            Self::Peak => "peak",
            Self::Recency => "recent",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Total => Self::Name,
            Self::Name => Self::Peak,
            Self::Peak => Self::Recency,
            Self::Recency => Self::Total,
        }
    }
}

// ---------------------------------------------------------------------------
// Time window configuration (Go parity: heatmapWindow)
// ---------------------------------------------------------------------------

/// A time window preset with label, total duration, and bucket size (seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapWindow {
    pub label: &'static str,
    pub window_secs: i64,
    pub bucket_secs: i64,
}

/// Available time windows matching Go's `heatmapWindow` presets.
pub const HEATMAP_WINDOWS: &[HeatmapWindow] = &[
    HeatmapWindow {
        label: "4h",
        window_secs: 4 * 3600,
        bucket_secs: 10 * 60,
    },
    HeatmapWindow {
        label: "12h",
        window_secs: 12 * 3600,
        bucket_secs: 30 * 60,
    },
    HeatmapWindow {
        label: "24h",
        window_secs: 24 * 3600,
        bucket_secs: 3600,
    },
    HeatmapWindow {
        label: "7d",
        window_secs: 7 * 24 * 3600,
        bucket_secs: 4 * 3600,
    },
    HeatmapWindow {
        label: "30d",
        window_secs: 30 * 24 * 3600,
        bucket_secs: 24 * 3600,
    },
];

/// Default window index (24h).
pub const DEFAULT_WINDOW_IDX: usize = 2;

// ---------------------------------------------------------------------------
// HeatmapMessage
// ---------------------------------------------------------------------------

/// A lightweight message record for heatmap computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub reply_to: String,
    pub time_secs: i64,
}

impl HeatmapMessage {
    /// Dedup key matching Go's `statsDedupKey`.
    #[must_use]
    pub fn dedup_key(&self) -> String {
        format!("{}|{}|{}", self.id.trim(), self.from.trim(), self.to.trim())
    }
}

// ---------------------------------------------------------------------------
// HeatmapRow / HeatmapMatrix (Go parity)
// ---------------------------------------------------------------------------

/// A single row in the heatmap matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapRow {
    pub label: String,
    pub counts: Vec<i32>,
    pub total: i32,
    pub peak_idx: usize,
    pub last_secs: i64,
}

/// The computed heatmap matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapMatrix {
    pub start_secs: i64,
    pub end_secs: i64,
    pub bucket_secs: i64,
    pub cols: usize,
    pub rows: Vec<HeatmapRow>,
    pub max_cell: i32,
    /// Threshold values: <=t0 ░, <=t1 ▒, <=t2 ▓, >t2 █
    pub threshold: [i32; 3],
}

impl Default for HeatmapMatrix {
    fn default() -> Self {
        Self {
            start_secs: 0,
            end_secs: 0,
            bucket_secs: 0,
            cols: 0,
            rows: Vec::new(),
            max_cell: 0,
            threshold: [5, 15, 30],
        }
    }
}

// ---------------------------------------------------------------------------
// Matrix computation (Go parity: buildHeatmapMatrix)
// ---------------------------------------------------------------------------

/// Build a heatmap matrix from messages in `[start_secs, end_secs)`.
#[must_use]
pub fn build_heatmap_matrix(
    messages: &[HeatmapMessage],
    start_secs: i64,
    end_secs: i64,
    bucket_secs: i64,
    mode: HeatmapMode,
) -> HeatmapMatrix {
    let mut out = HeatmapMatrix {
        start_secs,
        end_secs,
        bucket_secs,
        ..HeatmapMatrix::default()
    };
    if bucket_secs <= 0 || end_secs <= start_secs {
        return out;
    }
    let cols = ((end_secs - start_secs) / bucket_secs) as usize;
    if cols == 0 {
        return out;
    }
    out.cols = cols;

    struct Agg {
        counts: Vec<i32>,
        total: i32,
        peak: usize,
        last: i64,
    }

    let mut by_label: HashMap<String, Agg> = HashMap::new();
    let mut non_zero: Vec<i32> = Vec::new();
    let mut max_cell: i32 = 0;

    for msg in messages {
        let ts = msg.time_secs;
        if ts == 0 || ts < start_secs || ts >= end_secs {
            continue;
        }
        let label = match mode {
            HeatmapMode::Topics => msg.to.trim().to_owned(),
            HeatmapMode::Agents => msg.from.trim().to_owned(),
        };
        if label.is_empty() {
            continue;
        }
        let col = ((ts - start_secs) / bucket_secs) as usize;
        if col >= cols {
            continue;
        }

        let a = by_label.entry(label).or_insert_with(|| Agg {
            counts: vec![0; cols],
            total: 0,
            peak: 0,
            last: 0,
        });
        a.counts[col] += 1;
        a.total += 1;
        if a.counts[col] > a.counts[a.peak] {
            a.peak = col;
        }
        if ts > a.last {
            a.last = ts;
        }
    }

    let mut rows = Vec::with_capacity(by_label.len());
    for (label, a) in by_label {
        for &c in &a.counts {
            if c > 0 {
                non_zero.push(c);
                if c > max_cell {
                    max_cell = c;
                }
            }
        }
        rows.push(HeatmapRow {
            label,
            counts: a.counts,
            total: a.total,
            peak_idx: a.peak,
            last_secs: a.last,
        });
    }

    out.rows = rows;
    out.max_cell = max_cell;
    out.threshold = heatmap_thresholds(&mut non_zero);
    out
}

/// Sort matrix rows (Go parity: heatmapMatrix.sortRows).
pub fn sort_heatmap_rows(rows: &mut [HeatmapRow], sort: HeatmapSort) {
    rows.sort_by(|a, b| {
        match sort {
            HeatmapSort::Name => {
                let la = a.label.trim().to_lowercase();
                let lb = b.label.trim().to_lowercase();
                if la != lb {
                    return la.cmp(&lb);
                }
            }
            HeatmapSort::Peak => {
                if a.peak_idx != b.peak_idx {
                    return a.peak_idx.cmp(&b.peak_idx);
                }
                if a.total != b.total {
                    return b.total.cmp(&a.total);
                }
            }
            HeatmapSort::Recency => {
                if a.last_secs != b.last_secs {
                    return b.last_secs.cmp(&a.last_secs);
                }
                if a.total != b.total {
                    return b.total.cmp(&a.total);
                }
            }
            HeatmapSort::Total => {
                if a.total != b.total {
                    return b.total.cmp(&a.total);
                }
            }
        }
        a.label.trim().cmp(b.label.trim())
    });
}

fn heatmap_thresholds(non_zero: &mut [i32]) -> [i32; 3] {
    if non_zero.len() < 8 {
        return [5, 15, 30];
    }
    non_zero.sort_unstable();
    let mut p25 = percentile_i32(non_zero, 0.25);
    let mut p50 = percentile_i32(non_zero, 0.50);
    let mut p75 = percentile_i32(non_zero, 0.75);
    if p25 < 1 {
        p25 = 1;
    }
    if p50 < p25 {
        p50 = p25;
    }
    if p75 < p50 {
        p75 = p50;
    }
    if p50 == p25 {
        p50 = p25 + 1;
    }
    if p75 == p50 {
        p75 = p50 + 1;
    }
    [p25, p50, p75]
}

fn percentile_i32(sorted: &[i32], p: f64) -> i32 {
    if sorted.is_empty() {
        return 0;
    }
    if p <= 0.0 {
        return sorted[0];
    }
    if p >= 1.0 {
        return sorted[sorted.len() - 1];
    }
    let idx = ((sorted.len() - 1) as f64 * p) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// HeatmapViewModel (Go parity: heatmapView)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatmapViewModel {
    window_idx: usize,
    window_end_secs: i64,
    now_secs: i64,

    mode: HeatmapMode,
    sort: HeatmapSort,

    start_secs: i64,
    end_secs: i64,
    bucket_secs: i64,
    messages: Vec<HeatmapMessage>,
    seen: HashMap<String, ()>,

    matrix: HeatmapMatrix,

    selected_row: usize,
    selected_col: usize,
    top: usize,
    grid_h: usize,

    loading: bool,
    error: Option<String>,
    refresh_requested: bool,
}

impl Default for HeatmapViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl HeatmapViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            window_idx: DEFAULT_WINDOW_IDX,
            window_end_secs: 0,
            now_secs: 0,
            mode: HeatmapMode::Agents,
            sort: HeatmapSort::Total,
            start_secs: 0,
            end_secs: 0,
            bucket_secs: 0,
            messages: Vec::new(),
            seen: HashMap::new(),
            matrix: HeatmapMatrix::default(),
            selected_row: 0,
            selected_col: 0,
            top: 0,
            grid_h: 10,
            loading: false,
            error: None,
            refresh_requested: false,
        }
    }

    // -- accessors --

    #[must_use]
    pub fn mode(&self) -> HeatmapMode {
        self.mode
    }

    #[must_use]
    pub fn sort(&self) -> HeatmapSort {
        self.sort
    }

    #[must_use]
    pub fn window_idx(&self) -> usize {
        self.window_idx
    }

    #[must_use]
    pub fn window_label(&self) -> &'static str {
        HEATMAP_WINDOWS
            .get(self.window_idx)
            .map_or("?", |w| w.label)
    }

    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[must_use]
    pub fn matrix(&self) -> &HeatmapMatrix {
        &self.matrix
    }

    #[must_use]
    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    #[must_use]
    pub fn selected_col(&self) -> usize {
        self.selected_col
    }

    #[must_use]
    pub fn now_secs(&self) -> i64 {
        self.now_secs
    }

    /// Returns true if a refresh/reload was requested.
    #[must_use]
    pub fn take_refresh(&mut self) -> bool {
        let r = self.refresh_requested;
        self.refresh_requested = false;
        r
    }

    #[must_use]
    pub fn selected_cell(&self) -> (usize, usize, i32) {
        if self.matrix.rows.is_empty() || self.matrix.cols == 0 {
            return (0, 0, 0);
        }
        let r = self.selected_row.min(self.matrix.rows.len() - 1);
        let c = self.selected_col.min(self.matrix.cols.saturating_sub(1));
        let count = self.matrix.rows[r].counts.get(c).copied().unwrap_or(0);
        (r, c, count)
    }

    #[must_use]
    pub fn selected_label(&self) -> &str {
        if self.matrix.rows.is_empty() {
            return "";
        }
        let r = self.selected_row.min(self.matrix.rows.len() - 1);
        &self.matrix.rows[r].label
    }

    // -- setters --

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn set_error(&mut self, err: Option<String>) {
        self.error = err;
        self.loading = false;
    }

    pub fn set_now(&mut self, now_secs: i64) {
        self.now_secs = now_secs;
    }

    // -- window bounds (Go parity: windowBounds) --

    #[must_use]
    pub fn window_bounds(&self, now_secs: i64) -> (i64, i64, i64) {
        if HEATMAP_WINDOWS.is_empty() {
            return (0, 0, 0);
        }
        let idx = self.window_idx.min(HEATMAP_WINDOWS.len() - 1);
        let cfg = &HEATMAP_WINDOWS[idx];
        let end = if self.window_end_secs == 0 {
            now_secs
        } else {
            self.window_end_secs
        };
        (end - cfg.window_secs, end, cfg.bucket_secs)
    }

    #[must_use]
    pub fn pan_step(&self) -> i64 {
        if HEATMAP_WINDOWS.is_empty() {
            return 3600;
        }
        let idx = self.window_idx.min(HEATMAP_WINDOWS.len() - 1);
        let step = HEATMAP_WINDOWS[idx].window_secs / 6;
        if step < 3600 {
            3600
        } else {
            step
        }
    }

    // -- mode/sort mutations --

    pub fn toggle_mode(&mut self) {
        self.mode = self.mode.toggle();
        self.rebuild_matrix();
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        sort_heatmap_rows(&mut self.matrix.rows, self.sort);
        self.restore_selection();
    }

    // -- window mutations --

    pub fn prev_window(&mut self) {
        if self.window_idx > 0 {
            self.window_idx -= 1;
            self.window_end_secs = 0;
            self.loading = true;
            self.refresh_requested = true;
        }
    }

    pub fn next_window(&mut self) {
        if self.window_idx < HEATMAP_WINDOWS.len() - 1 {
            self.window_idx += 1;
            self.window_end_secs = 0;
            self.loading = true;
            self.refresh_requested = true;
        }
    }

    pub fn pan_left(&mut self) {
        self.window_end_secs = self.effective_end() - self.pan_step();
        self.loading = true;
        self.refresh_requested = true;
    }

    pub fn pan_right(&mut self) {
        self.window_end_secs = self.effective_end() + self.pan_step();
        self.loading = true;
        self.refresh_requested = true;
    }

    fn effective_end(&self) -> i64 {
        if self.window_end_secs == 0 {
            self.now_secs
        } else {
            self.window_end_secs
        }
    }

    // -- data loading (Go parity: applyLoaded) --

    pub fn apply_loaded(
        &mut self,
        now_secs: i64,
        start_secs: i64,
        end_secs: i64,
        bucket_secs: i64,
        messages: Vec<HeatmapMessage>,
    ) {
        self.loading = false;
        self.now_secs = now_secs;
        self.start_secs = start_secs;
        self.end_secs = end_secs;
        self.bucket_secs = bucket_secs;
        self.seen.clear();
        for msg in &messages {
            self.seen.insert(msg.dedup_key(), ());
        }
        self.messages = messages;
        self.rebuild_matrix();
    }

    /// Apply incoming message (live update, Go parity: applyIncoming).
    pub fn apply_incoming(&mut self, msg: HeatmapMessage, now_secs: i64) {
        self.now_secs = now_secs;
        let key = msg.dedup_key();
        if self.seen.contains_key(&key) {
            return;
        }
        self.seen.insert(key, ());

        let following = self.window_end_secs == 0 || (now_secs - self.window_end_secs).abs() <= 2;
        if following {
            self.messages.push(msg);
            self.messages.sort_by_key(|m| m.time_secs);
            self.window_end_secs = now_secs;
            let (start, end, bucket) = self.window_bounds(now_secs);
            self.start_secs = start;
            self.end_secs = end;
            self.bucket_secs = bucket;
            self.rebuild_matrix();
        }
    }

    fn rebuild_matrix(&mut self) {
        self.matrix = build_heatmap_matrix(
            &self.messages,
            self.start_secs,
            self.end_secs,
            self.bucket_secs,
            self.mode,
        );
        sort_heatmap_rows(&mut self.matrix.rows, self.sort);
        self.restore_selection();
    }

    fn restore_selection(&mut self) {
        if self.matrix.rows.is_empty() {
            self.selected_row = 0;
            self.selected_col = 0;
            self.top = 0;
            return;
        }
        self.selected_row = self.selected_row.min(self.matrix.rows.len() - 1);
        self.selected_col = self.selected_col.min(self.matrix.cols.saturating_sub(1));
        if self.top > self.selected_row {
            self.top = self.selected_row;
        }
    }

    // -- selection movement --

    pub fn move_selection(&mut self, row_delta: i32, col_delta: i32) {
        if self.matrix.rows.is_empty() || self.matrix.cols == 0 {
            return;
        }
        let max_row = self.matrix.rows.len() - 1;
        let max_col = self.matrix.cols.saturating_sub(1);
        self.selected_row = clamp_add(self.selected_row, row_delta, 0, max_row);
        self.selected_col = clamp_add(self.selected_col, col_delta, 0, max_col);
        self.ensure_row_visible();
    }

    fn ensure_row_visible(&mut self) {
        if self.selected_row < self.top {
            self.top = self.selected_row;
            return;
        }
        let visible = visible_rows_for_height(self.grid_h);
        if visible > 0 && self.selected_row >= self.top + visible {
            self.top = self.selected_row - visible + 1;
        }
    }

    fn visible_row_count(&self) -> usize {
        visible_rows_for_height(self.grid_h)
            .max(1)
            .saturating_sub(1)
    }

    // -- tooltip (Go parity: renderTooltip, cellBreakdown) --

    #[must_use]
    pub fn tooltip(&self) -> String {
        if self.matrix.rows.is_empty() || self.matrix.cols == 0 || self.bucket_secs <= 0 {
            return String::new();
        }
        let r = self.selected_row.min(self.matrix.rows.len() - 1);
        let c = self.selected_col.min(self.matrix.cols.saturating_sub(1));
        let row = &self.matrix.rows[r];
        let count = row.counts.get(c).copied().unwrap_or(0);
        let cell_start = self.start_secs + (c as i64) * self.bucket_secs;
        let cell_end = cell_start + self.bucket_secs;
        let tlabel = time_range_label(cell_start, cell_end);
        let detail = self.cell_breakdown(row.label.trim(), cell_start, cell_end);
        let mut line = format!("{}, {}: {} msgs", row.label.trim(), tlabel, count);
        if !detail.is_empty() {
            line.push_str(" (");
            line.push_str(&detail);
            line.push(')');
        }
        line
    }

    fn cell_breakdown(&self, row_label: &str, start: i64, end: i64) -> String {
        if row_label.is_empty() || start == 0 || end == 0 || end <= start {
            return String::new();
        }

        let mut dm_count = 0i32;
        let mut counts: HashMap<String, i32> = HashMap::new();

        for msg in &self.messages {
            let ts = msg.time_secs;
            if ts == 0 || ts < start || ts >= end {
                continue;
            }
            match self.mode {
                HeatmapMode::Topics => {
                    if msg.to.trim() != row_label {
                        continue;
                    }
                    let from = msg.from.trim();
                    if !from.is_empty() {
                        *counts.entry(from.to_owned()).or_insert(0) += 1;
                    }
                }
                HeatmapMode::Agents => {
                    if msg.from.trim() != row_label {
                        continue;
                    }
                    let to = msg.to.trim();
                    if to.starts_with('@') {
                        dm_count += 1;
                        continue;
                    }
                    if !to.is_empty() {
                        *counts.entry(to.to_owned()).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut top: Vec<(String, i32)> = counts.into_iter().collect();
        top.sort_by(|a, b| {
            if a.1 != b.1 {
                b.1.cmp(&a.1)
            } else {
                a.0.cmp(&b.0)
            }
        });

        let limit = if self.mode == HeatmapMode::Agents {
            2
        } else {
            3
        };
        let mut parts: Vec<String> = Vec::new();
        for (k, v) in top.iter().take(limit) {
            parts.push(format!("{}: {}", k, v));
        }
        if dm_count > 0 && self.mode == HeatmapMode::Agents {
            parts.push(format!("DMs: {}", dm_count));
        }
        parts.join(", ")
    }

    // -- summary (Go parity: renderSummary) --

    #[must_use]
    pub fn summary(&self) -> String {
        if self.messages.is_empty() || self.start_secs == 0 || self.end_secs == 0 {
            return "Summary: no data".to_owned();
        }

        let mut total = 0i32;
        let mut agents: HashMap<String, i32> = HashMap::new();
        let mut topics: HashMap<String, i32> = HashMap::new();
        let mut by_id: HashMap<String, &HeatmapMessage> = HashMap::new();

        for msg in &self.messages {
            if msg.time_secs == 0
                || msg.time_secs < self.start_secs
                || msg.time_secs >= self.end_secs
            {
                continue;
            }
            total += 1;
            let from = msg.from.trim();
            if !from.is_empty() {
                *agents.entry(from.to_owned()).or_insert(0) += 1;
            }
            let to = msg.to.trim();
            if !to.is_empty() {
                *topics.entry(to.to_owned()).or_insert(0) += 1;
            }
            let id = msg.id.trim();
            if !id.is_empty() {
                by_id.insert(id.to_owned(), msg);
            }
        }

        let mut reply_deltas: HashMap<String, i64> = HashMap::new();
        for msg in &self.messages {
            let parent_id = msg.reply_to.trim();
            if parent_id.is_empty() {
                continue;
            }
            if let Some(parent) = by_id.get(parent_id) {
                if parent.time_secs == 0 || msg.time_secs == 0 {
                    continue;
                }
                let delta = msg.time_secs - parent.time_secs;
                if delta < 0 {
                    continue;
                }
                let prev = reply_deltas.get(parent_id).copied();
                if prev.is_none() || delta < prev.unwrap_or(i64::MAX) {
                    reply_deltas.insert(parent_id.to_owned(), delta);
                }
            }
        }
        let avg_first_secs = if reply_deltas.is_empty() {
            0i64
        } else {
            let sum: i64 = reply_deltas.values().sum();
            sum / reply_deltas.len() as i64
        };

        let (peak_start, peak_count) = self.peak_bucket();
        let (most_agent, most_agent_count) = max_count_entry(&agents);
        let (most_topic, most_topic_count) = max_count_entry(&topics);

        let mut parts = vec![
            format!(
                "Summary: total {} msgs, {} active agents",
                total,
                agents.len()
            ),
            format!(
                "Peak: {} ({} msgs)",
                hour_range_label(peak_start),
                peak_count
            ),
            format!("Most active: {} ({})", most_agent, most_agent_count),
            format!("Busiest topic: {} ({})", most_topic, most_topic_count),
        ];
        if avg_first_secs > 0 {
            parts.push(format!(
                "Avg response: {}",
                format_duration_compact(avg_first_secs)
            ));
        }
        parts.join("  |  ")
    }

    fn peak_bucket(&self) -> (i64, i32) {
        if self.matrix.cols == 0 || self.bucket_secs <= 0 {
            return (0, 0);
        }
        let mut counts = vec![0i32; self.matrix.cols];
        for row in &self.matrix.rows {
            for (i, &c) in row.counts.iter().enumerate() {
                if i < counts.len() {
                    counts[i] += c;
                }
            }
        }
        let mut best_idx = 0usize;
        let mut best = 0i32;
        for (i, &c) in counts.iter().enumerate() {
            if c > best {
                best = c;
                best_idx = i;
            }
        }
        (self.start_secs + (best_idx as i64) * self.bucket_secs, best)
    }

    // -- legacy simple API (backward compat) --

    #[must_use]
    pub fn simple(row_labels: Vec<String>, columns: usize) -> Self {
        let cols = columns.max(1);
        let labels = if row_labels.is_empty() {
            vec!["All".to_owned()]
        } else {
            row_labels
        };
        let heatmap_rows: Vec<HeatmapRow> = labels
            .iter()
            .map(|l| HeatmapRow {
                label: l.clone(),
                counts: vec![0; cols],
                total: 0,
                peak_idx: 0,
                last_secs: 0,
            })
            .collect();
        let mut vm = Self::new();
        vm.matrix = HeatmapMatrix {
            cols,
            rows: heatmap_rows,
            ..HeatmapMatrix::default()
        };
        vm
    }

    pub fn set(&mut self, row: usize, col: usize, count: i32) {
        if row >= self.matrix.rows.len() || col >= self.matrix.cols {
            return;
        }
        self.matrix.rows[row].counts[col] = count;
        let mut max = 0i32;
        for r in &self.matrix.rows {
            for &c in &r.counts {
                if c > max {
                    max = c;
                }
            }
        }
        self.matrix.max_cell = max;
    }

    pub fn increment(&mut self, row: usize, col: usize) {
        if row >= self.matrix.rows.len() || col >= self.matrix.cols {
            return;
        }
        self.matrix.rows[row].counts[col] = self.matrix.rows[row].counts[col].saturating_add(1);
        if self.matrix.rows[row].counts[col] > self.matrix.max_cell {
            self.matrix.max_cell = self.matrix.rows[row].counts[col];
        }
    }

    #[must_use]
    pub fn max_count(&self) -> i32 {
        self.matrix.max_cell
    }

    pub fn move_up(&mut self) {
        self.move_selection(-1, 0);
    }

    pub fn move_down(&mut self) {
        self.move_selection(1, 0);
    }

    pub fn move_left(&mut self) {
        self.move_selection(0, -1);
    }

    pub fn move_right(&mut self) {
        self.move_selection(0, 1);
    }
}

// ---------------------------------------------------------------------------
// Input handler (Go parity: heatmapView.handleKey)
// ---------------------------------------------------------------------------

pub fn apply_heatmap_input(view: &mut HeatmapViewModel, event: InputEvent) {
    match translate_input(&event) {
        UiAction::MoveUp => {
            view.move_selection(-1, 0);
        }
        UiAction::MoveDown => {
            view.move_selection(1, 0);
        }
        _ => {}
    }

    if let InputEvent::Key(ref key_event) = event {
        if !key_event.modifiers.ctrl && !key_event.modifiers.alt {
            match key_event.key {
                Key::Char('t') => view.toggle_mode(),
                Key::Char('s') => view.cycle_sort(),
                Key::Char('[') => view.prev_window(),
                Key::Char(']') => view.next_window(),
                Key::Char('h') => view.pan_left(),
                Key::Char('l') => view.pan_right(),
                Key::Left => {
                    if view.selected_col == 0 {
                        view.pan_left();
                    } else {
                        view.move_selection(0, -1);
                    }
                }
                Key::Right => {
                    let max_col = view.matrix.cols.saturating_sub(1);
                    if view.selected_col >= max_col {
                        view.pan_right();
                    } else {
                        view.move_selection(0, 1);
                    }
                }
                _ => {}
            }
        }
        // Ctrl+u / Ctrl+d for page scroll
        if key_event.modifiers.ctrl {
            match key_event.key {
                Key::Char('u') => {
                    let n = view.visible_row_count().max(1) as i32;
                    view.move_selection(-n, 0);
                }
                Key::Char('d') => {
                    let n = view.visible_row_count().max(1) as i32;
                    view.move_selection(n, 0);
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render (Go parity: heatmapView.View)
// ---------------------------------------------------------------------------

#[must_use]
pub fn render_heatmap_frame(
    view: &HeatmapViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let header = truncate(
        &format!(
            "ACTIVITY HEATMAP  last {}  mode:{}  sort:{}",
            view.window_label(),
            view.mode.label(),
            view.sort.label(),
        ),
        width,
    );
    frame.draw_text(0, 0, &header, TextRole::Accent);

    if height <= 1 {
        return frame;
    }

    if let Some(err) = &view.error {
        if height > 2 {
            frame.draw_text(
                0,
                2,
                &truncate(&format!("error: {}", err), width),
                TextRole::Muted,
            );
        }
        return frame;
    }

    if view.loading {
        if height > 2 {
            frame.draw_text(0, 2, "Loading...", TextRole::Muted);
        }
        return frame;
    }

    // Layout: header(1) + blank(1) + grid + tooltip + legend + summary + blank + footer
    let _grid_h = if height > 8 {
        height - 8
    } else {
        height.saturating_sub(1)
    };
    let mut y = 2; // after header + blank

    let name_w = 8usize;
    let cell_w = 3usize;
    let cols = view.matrix.cols;

    if cols > 0 && !view.matrix.rows.is_empty() {
        let max_cols = if name_w + 1 + cols * cell_w > width {
            ((width.saturating_sub(name_w + 1)) / cell_w).max(1)
        } else {
            cols
        };
        let visible_cols = max_cols.min(cols);

        // Axis line
        if y < height {
            let axis = render_axis(
                view.start_secs,
                view.bucket_secs,
                name_w,
                visible_cols,
                cell_w,
            );
            frame.draw_text(0, y, &truncate(&axis, width), TextRole::Muted);
            y += 1;
        }

        // Grid rows
        let rows_available = height.saturating_sub(y).saturating_sub(4).max(1);
        let total_rows = view.matrix.rows.len();
        let start_row = view.top.min(total_rows.saturating_sub(1));
        let visible_rows = rows_available.min(total_rows - start_row);

        for i in 0..visible_rows {
            if y >= height {
                break;
            }
            let row_idx = start_row + i;
            let row = &view.matrix.rows[row_idx];
            let name = truncate(&row.label, name_w);
            let is_selected = row_idx == view.selected_row;
            let mut line = String::with_capacity(width);
            let name_len = name.chars().count();
            line.push_str(&name);
            for _ in name_len..name_w {
                line.push(' ');
            }
            line.push(' ');

            for c in 0..visible_cols {
                let count = row.counts.get(c).copied().unwrap_or(0);
                let glyph = glyph_for_threshold(count, &view.matrix.threshold);
                if is_selected && c == view.selected_col {
                    line.push('[');
                    line.push_str(glyph);
                    line.push(']');
                } else {
                    line.push(' ');
                    line.push_str(glyph);
                    line.push(' ');
                }
            }

            let role = if is_selected {
                TextRole::Accent
            } else {
                TextRole::Primary
            };
            frame.draw_text(0, y, &truncate(&line, width), role);
            y += 1;
        }
    }

    // Tooltip
    if y < height {
        let tooltip = view.tooltip();
        if !tooltip.is_empty() {
            frame.draw_text(0, y, &truncate(&tooltip, width), TextRole::Muted);
        }
        y += 1;
    }

    // Legend
    if y < height {
        frame.draw_text(
            0,
            y,
            &truncate(
                "Legend: \u{2591}  low   \u{2592}  mid   \u{2593}  high   \u{2588}  max",
                width,
            ),
            TextRole::Muted,
        );
        y += 1;
    }

    // Summary
    if y < height {
        let summary = view.summary();
        frame.draw_text(0, y, &truncate(&summary, width), TextRole::Muted);
        y += 1;
    }

    // Blank + footer
    y += 1;
    if y < height {
        frame.draw_text(
            0,
            y,
            &truncate(
                "[/]: range  h/l: pan  t: toggle  s: sort  Esc: back  (H: heatmap)",
                width,
            ),
            TextRole::Muted,
        );
    }

    frame
}

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

fn render_axis(
    start_secs: i64,
    bucket_secs: i64,
    name_w: usize,
    cols: usize,
    cell_w: usize,
) -> String {
    let mut b = String::new();
    for _ in 0..name_w {
        b.push(' ');
    }
    b.push(' ');
    for c in 0..cols {
        let ts = start_secs + (c as i64) * bucket_secs;
        let label = if bucket_secs >= 86400 {
            format_day(ts)
        } else {
            format_hour(ts)
        };
        if c % 2 == 0 {
            b.push_str(&pad_center(&label, cell_w));
        } else {
            for _ in 0..cell_w {
                b.push(' ');
            }
        }
    }
    b
}

fn glyph_for_threshold(count: i32, threshold: &[i32; 3]) -> &'static str {
    if count <= 0 {
        return " \u{00b7} "; // " · "
    }
    if count <= threshold[0] {
        "\u{2591}" // ░
    } else if count <= threshold[1] {
        "\u{2592}" // ▒
    } else if count <= threshold[2] {
        "\u{2593}" // ▓
    } else {
        "\u{2588}" // █
    }
}

fn visible_rows_for_height(height: usize) -> usize {
    if height <= 2 {
        return 1.min(height);
    }
    height - 2
}

// ---------------------------------------------------------------------------
// String / time helpers
// ---------------------------------------------------------------------------

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max_chars {
        return input.to_owned();
    }
    if max_chars == 1 {
        return "\u{2026}".to_owned();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('\u{2026}');
    out
}

fn pad_center(s: &str, width: usize) -> String {
    let w = s.chars().count();
    if w >= width {
        return s.to_owned();
    }
    let left = (width - w) / 2;
    let right = width - w - left;
    let mut out = String::with_capacity(width);
    for _ in 0..left {
        out.push(' ');
    }
    out.push_str(s);
    for _ in 0..right {
        out.push(' ');
    }
    out
}

fn clamp_add(val: usize, delta: i32, min: usize, max: usize) -> usize {
    let result = val as i64 + delta as i64;
    if result < min as i64 {
        min
    } else if result > max as i64 {
        max
    } else {
        result as usize
    }
}

fn max_count_entry(m: &HashMap<String, i32>) -> (String, i32) {
    let mut best_k = String::new();
    let mut best_v = 0i32;
    for (k, &v) in m {
        if v > best_v || (v == best_v && (best_k.is_empty() || k < &best_k)) {
            best_k = k.clone();
            best_v = v;
        }
    }
    (best_k, best_v)
}

fn time_range_label(start_secs: i64, end_secs: i64) -> String {
    if start_secs == 0 || end_secs == 0 || end_secs <= start_secs {
        return "-".to_owned();
    }
    if end_secs - start_secs >= 86400 {
        let (y, m, d) = epoch_to_ymd(start_secs / 86400);
        return format!("{:04}-{:02}-{:02}", y, m, d);
    }
    let sh = ((start_secs % 86400) / 3600).unsigned_abs();
    let sm = ((start_secs % 3600) / 60).unsigned_abs();
    let eh = ((end_secs % 86400) / 3600).unsigned_abs();
    let em = ((end_secs % 3600) / 60).unsigned_abs();
    format!("{:02}:{:02}-{:02}:{:02}", sh, sm, eh, em)
}

fn hour_range_label(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "-".to_owned();
    }
    let h = ((epoch_secs % 86400) / 3600).unsigned_abs();
    let m = ((epoch_secs % 3600) / 60).unsigned_abs();
    format!("{:02}:{:02}", h, m)
}

fn format_duration_compact(secs: i64) -> String {
    if secs < 60 {
        return format!("{}s", secs);
    }
    if secs < 3600 {
        return format!("{}m", secs / 60);
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if m == 0 {
        format!("{}h", h)
    } else {
        format!("{}h{}m", h, m)
    }
}

fn format_hour(epoch_secs: i64) -> String {
    let hour = ((epoch_secs % 86400) / 3600).unsigned_abs();
    format!("{:02}", hour)
}

fn format_day(epoch_secs: i64) -> String {
    let days_since_epoch = epoch_secs / 86400;
    let (_, _, day) = epoch_to_ymd(days_since_epoch);
    format!("{:02}", day)
}

/// Epoch-days to (year, month, day).
fn epoch_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::style::ThemeSpec;

    fn frame_text(frame: &RenderFrame, height: usize) -> String {
        (0..height)
            .map(|y| frame.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // -- legacy API compat --

    #[test]
    fn glyph_density_and_navigation() {
        let mut view = HeatmapViewModel::simple(vec!["Mon".to_owned(), "Tue".to_owned()], 3);
        view.set(0, 0, 1);
        view.set(0, 1, 3);
        view.set(0, 2, 6);
        view.set(1, 0, 9);
        assert_eq!(view.max_count(), 9);

        apply_heatmap_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Down)));
        apply_heatmap_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Right)));
        let selected = view.selected_cell();
        assert_eq!(selected, (1, 1, 0));
        assert_eq!(view.selected_label(), "Tue");
    }

    #[test]
    fn heatmap_snapshot() {
        let mut view = HeatmapViewModel::simple(
            vec!["Mon".to_owned(), "Tue".to_owned(), "Wed".to_owned()],
            4,
        );
        view.set(0, 0, 1);
        view.set(0, 1, 2);
        view.set(0, 2, 3);
        view.set(0, 3, 4);
        view.set(1, 0, 4);
        view.set(2, 2, 4);

        let frame = render_heatmap_frame(&view, 42, 10, ThemeSpec::default());
        let text = frame_text(&frame, 10);
        assert!(text.contains("ACTIVITY HEATMAP"), "header missing");
        assert!(text.contains("Mon"), "Mon row missing");
        assert!(text.contains("Tue"), "Tue row missing");
        assert!(text.contains("Wed"), "Wed row missing");
    }

    // -- mode/sort/window tests --

    #[test]
    fn mode_toggle() {
        let mut vm = HeatmapViewModel::new();
        assert_eq!(vm.mode(), HeatmapMode::Agents);
        vm.toggle_mode();
        assert_eq!(vm.mode(), HeatmapMode::Topics);
        vm.toggle_mode();
        assert_eq!(vm.mode(), HeatmapMode::Agents);
    }

    #[test]
    fn sort_cycle() {
        let mut vm = HeatmapViewModel::new();
        assert_eq!(vm.sort(), HeatmapSort::Total);
        vm.cycle_sort();
        assert_eq!(vm.sort(), HeatmapSort::Name);
        vm.cycle_sort();
        assert_eq!(vm.sort(), HeatmapSort::Peak);
        vm.cycle_sort();
        assert_eq!(vm.sort(), HeatmapSort::Recency);
        vm.cycle_sort();
        assert_eq!(vm.sort(), HeatmapSort::Total);
    }

    #[test]
    fn window_navigation() {
        let mut vm = HeatmapViewModel::new();
        vm.set_now(1_700_000_000);
        assert_eq!(vm.window_idx(), DEFAULT_WINDOW_IDX);
        vm.next_window();
        assert_eq!(vm.window_idx(), 3);
        assert_eq!(vm.window_label(), "7d");
        assert!(vm.take_refresh());
        vm.prev_window();
        assert_eq!(vm.window_idx(), 2);
        vm.prev_window();
        vm.prev_window();
        vm.prev_window();
        assert_eq!(vm.window_idx(), 0);
    }

    #[test]
    fn window_bounds_default() {
        let vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let (start, end, bucket) = vm.window_bounds(now);
        assert_eq!(end, now);
        assert_eq!(start, now - 24 * 3600);
        assert_eq!(bucket, 3600);
    }

    #[test]
    fn pan_step_values() {
        let mut vm = HeatmapViewModel::new();
        assert_eq!(vm.pan_step(), 14400); // 24h / 6
        vm.prev_window();
        vm.prev_window();
        assert_eq!(vm.window_idx(), 0);
        assert_eq!(vm.pan_step(), 3600); // 4h / 6 = 2400, clamped to 3600
    }

    // -- matrix computation tests --

    #[test]
    fn build_matrix_agents() {
        let start = 1_700_000_000i64;
        let end = start + 3 * 3600;
        let bucket = 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "a".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 300,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "a".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 3900,
            },
            HeatmapMessage {
                id: "3".into(),
                from: "b".into(),
                to: "build".into(),
                reply_to: String::new(),
                time_secs: start + 4200,
            },
            HeatmapMessage {
                id: "4".into(),
                from: "a".into(),
                to: "@b".into(),
                reply_to: String::new(),
                time_secs: start + 7500,
            },
        ];

        let mut m = build_heatmap_matrix(&msgs, start, end, bucket, HeatmapMode::Agents);
        assert_eq!(m.cols, 3);
        assert_eq!(m.rows.len(), 2);
        sort_heatmap_rows(&mut m.rows, HeatmapSort::Total);
        assert_eq!(m.rows[0].label, "a");
        assert_eq!(m.rows[0].total, 3);
        assert_eq!(m.rows[0].counts, vec![1, 1, 1]);
        assert_eq!(m.rows[1].label, "b");
        assert_eq!(m.rows[1].total, 1);
        assert_eq!(m.rows[1].counts, vec![0, 1, 0]);
    }

    #[test]
    fn build_matrix_topics() {
        let start = 1_700_000_000i64;
        let end = start + 2 * 3600;
        let bucket = 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "a".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 600,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "b".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 1200,
            },
            HeatmapMessage {
                id: "3".into(),
                from: "a".into(),
                to: "build".into(),
                reply_to: String::new(),
                time_secs: start + 4200,
            },
        ];

        let mut m = build_heatmap_matrix(&msgs, start, end, bucket, HeatmapMode::Topics);
        assert_eq!(m.cols, 2);
        assert_eq!(m.rows.len(), 2);
        sort_heatmap_rows(&mut m.rows, HeatmapSort::Name);
        assert_eq!(m.rows[0].label, "build");
        assert_eq!(m.rows[0].counts, vec![0, 1]);
        assert_eq!(m.rows[1].label, "task");
        assert_eq!(m.rows[1].counts, vec![2, 0]);
    }

    #[test]
    fn build_matrix_invalid_bounds() {
        let m = build_heatmap_matrix(&[], 200, 100, 10, HeatmapMode::Agents);
        assert_eq!(m.cols, 0);
        let m2 = build_heatmap_matrix(&[], 100, 200, 0, HeatmapMode::Agents);
        assert_eq!(m2.cols, 0);
    }

    // -- sort tests --

    #[test]
    fn sort_by_name() {
        let mut rows = vec![
            HeatmapRow {
                label: "charlie".into(),
                counts: vec![1],
                total: 1,
                peak_idx: 0,
                last_secs: 100,
            },
            HeatmapRow {
                label: "alice".into(),
                counts: vec![2],
                total: 2,
                peak_idx: 0,
                last_secs: 200,
            },
        ];
        sort_heatmap_rows(&mut rows, HeatmapSort::Name);
        assert_eq!(rows[0].label, "alice");
        assert_eq!(rows[1].label, "charlie");
    }

    #[test]
    fn sort_by_recency() {
        let mut rows = vec![
            HeatmapRow {
                label: "old".into(),
                counts: vec![5],
                total: 5,
                peak_idx: 0,
                last_secs: 100,
            },
            HeatmapRow {
                label: "new".into(),
                counts: vec![1],
                total: 1,
                peak_idx: 0,
                last_secs: 300,
            },
        ];
        sort_heatmap_rows(&mut rows, HeatmapSort::Recency);
        assert_eq!(rows[0].label, "new");
        assert_eq!(rows[1].label, "old");
    }

    // -- threshold tests --

    #[test]
    fn thresholds_small_dataset() {
        let mut data: Vec<i32> = vec![];
        assert_eq!(heatmap_thresholds(&mut data), [5, 15, 30]);
    }

    #[test]
    fn thresholds_large_dataset() {
        let mut data: Vec<i32> = (1..=100).collect();
        let t = heatmap_thresholds(&mut data);
        assert!(t[0] > 0);
        assert!(t[1] > t[0]);
        assert!(t[2] > t[1]);
    }

    // -- data loading tests --

    #[test]
    fn apply_loaded_rebuilds_matrix() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "a".into(),
                to: "t".into(),
                reply_to: String::new(),
                time_secs: start + 300,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "b".into(),
                to: "t".into(),
                reply_to: String::new(),
                time_secs: start + 900,
            },
        ];
        vm.apply_loaded(now, start, now, 600, msgs);
        assert!(!vm.is_loading());
        assert_eq!(vm.matrix().cols, 6);
        assert_eq!(vm.matrix().rows.len(), 2);
    }

    #[test]
    fn apply_incoming_dedup() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 3600;
        vm.apply_loaded(now, start, now, 600, vec![]);
        let msg = HeatmapMessage {
            id: "1".into(),
            from: "a".into(),
            to: "t".into(),
            reply_to: String::new(),
            time_secs: now - 100,
        };
        vm.apply_incoming(msg.clone(), now);
        assert_eq!(vm.matrix().rows.len(), 1);
        vm.apply_incoming(msg, now);
        assert_eq!(vm.matrix().rows.len(), 1); // dedup
    }

    // -- input handler tests --

    #[test]
    fn key_toggle_mode() {
        let mut vm = HeatmapViewModel::new();
        apply_heatmap_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('t'))));
        assert_eq!(vm.mode(), HeatmapMode::Topics);
    }

    #[test]
    fn key_cycle_sort() {
        let mut vm = HeatmapViewModel::new();
        apply_heatmap_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('s'))));
        assert_eq!(vm.sort(), HeatmapSort::Name);
    }

    #[test]
    fn key_window_change() {
        let mut vm = HeatmapViewModel::new();
        apply_heatmap_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('['))));
        assert_eq!(vm.window_idx(), 1);
        apply_heatmap_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        assert_eq!(vm.window_idx(), 2);
    }

    #[test]
    fn key_pan() {
        let mut vm = HeatmapViewModel::new();
        vm.set_now(1_700_000_000);
        apply_heatmap_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('h'))));
        assert!(vm.take_refresh());
    }

    // -- tooltip and summary --

    #[test]
    fn tooltip_and_summary() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 2 * 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "agent-x".into(),
                to: "build".into(),
                reply_to: String::new(),
                time_secs: start + 300,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "agent-x".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 600,
            },
            HeatmapMessage {
                id: "3".into(),
                from: "agent-y".into(),
                to: "task".into(),
                reply_to: "1".into(),
                time_secs: start + 900,
            },
        ];
        vm.apply_loaded(now, start, now, 3600, msgs);
        let tooltip = vm.tooltip();
        assert!(!tooltip.is_empty());
        let summary = vm.summary();
        assert!(summary.contains("total"));
    }

    #[test]
    fn summary_no_data() {
        let vm = HeatmapViewModel::new();
        assert_eq!(vm.summary(), "Summary: no data");
    }

    #[test]
    fn cell_breakdown_agents_mode() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "alice".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 100,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "alice".into(),
                to: "build".into(),
                reply_to: String::new(),
                time_secs: start + 200,
            },
            HeatmapMessage {
                id: "3".into(),
                from: "alice".into(),
                to: "@bob".into(),
                reply_to: String::new(),
                time_secs: start + 300,
            },
        ];
        vm.apply_loaded(now, start, now, 3600, msgs);
        let breakdown = vm.cell_breakdown("alice", start, now);
        assert!(breakdown.contains("task") || breakdown.contains("build"));
        assert!(breakdown.contains("DMs: 1"));
    }

    // -- render tests --

    #[test]
    fn render_loading() {
        let mut vm = HeatmapViewModel::new();
        vm.set_loading(true);
        let frame = render_heatmap_frame(&vm, 60, 10, ThemeSpec::default());
        let text = frame_text(&frame, 10);
        assert!(text.contains("Loading..."));
    }

    #[test]
    fn render_error() {
        let mut vm = HeatmapViewModel::new();
        vm.set_error(Some("connection failed".into()));
        let frame = render_heatmap_frame(&vm, 60, 10, ThemeSpec::default());
        let text = frame_text(&frame, 10);
        assert!(text.contains("error: connection failed"));
    }

    #[test]
    fn render_empty() {
        let frame = render_heatmap_frame(&HeatmapViewModel::new(), 0, 0, ThemeSpec::default());
        assert_eq!(frame_text(&frame, 0), "");
    }

    #[test]
    fn render_with_data() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 3 * 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "alpha".into(),
                to: "task".into(),
                reply_to: String::new(),
                time_secs: start + 100,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "beta".into(),
                to: "build".into(),
                reply_to: String::new(),
                time_secs: start + 4000,
            },
        ];
        vm.apply_loaded(now, start, now, 3600, msgs);
        let frame = render_heatmap_frame(&vm, 80, 20, ThemeSpec::default());
        let text = frame_text(&frame, 20);
        assert!(text.contains("ACTIVITY HEATMAP"));
        assert!(text.contains("alpha") || text.contains("beta"));
        assert!(text.contains("Legend:"));
    }

    #[test]
    fn selection_clamps() {
        let mut vm = HeatmapViewModel::new();
        let now = 1_700_000_000i64;
        let start = now - 3600;
        let msgs = vec![
            HeatmapMessage {
                id: "1".into(),
                from: "a".into(),
                to: "t".into(),
                reply_to: String::new(),
                time_secs: start + 100,
            },
            HeatmapMessage {
                id: "2".into(),
                from: "b".into(),
                to: "t".into(),
                reply_to: String::new(),
                time_secs: start + 200,
            },
        ];
        vm.apply_loaded(now, start, now, 600, msgs);
        vm.move_selection(100, 100);
        let (r, c, _) = vm.selected_cell();
        assert!(r < vm.matrix().rows.len());
        assert!(c < vm.matrix().cols);
        vm.move_selection(-100, -100);
        let (r, c, _) = vm.selected_cell();
        assert_eq!(r, 0);
        assert_eq!(c, 0);
    }

    // -- helper tests --

    #[test]
    fn epoch_to_ymd_known() {
        let (y, m, d) = epoch_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
        let (y, m, d) = epoch_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn format_duration_values() {
        assert_eq!(format_duration_compact(30), "30s");
        assert_eq!(format_duration_compact(90), "1m");
        assert_eq!(format_duration_compact(3600), "1h");
        assert_eq!(format_duration_compact(3660), "1h1m");
    }
}

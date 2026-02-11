use std::collections::HashMap;

use forge_ftui_adapter::input::{translate_input, InputEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Data types (Go parity: stats_compute.go)
// ---------------------------------------------------------------------------

/// A single bar in a top-N list (Go: statsBar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsBar {
    pub label: String,
    pub count: usize,
}

/// A latency distribution bucket (Go: statsBucket).
#[derive(Debug, Clone, PartialEq)]
pub struct StatsBucket {
    pub label: String,
    pub count: usize,
    pub pct: f64,
}

/// Thread size distribution (Go: statsThreadDist).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatsThreadDist {
    pub standalone: usize, // 1 msg
    pub small: usize,      // 2-3
    pub medium: usize,     // 4-10
    pub large: usize,      // 10+
}

/// A lightweight message record for stats computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub reply_to: String,
    pub time_secs: i64, // UTC epoch seconds (0 = unknown)
    pub body: String,
}

impl StatsMessage {
    #[must_use]
    pub fn dedup_key(&self) -> String {
        format!("{}|{}|{}", self.id.trim(), self.from.trim(), self.to.trim())
    }
}

/// Complete stats computation result (Go: statsSnapshot).
#[derive(Debug, Clone, PartialEq)]
pub struct StatsSnapshot {
    pub total_messages: usize,
    pub active_agents: usize,
    pub active_topics: usize,

    pub reply_samples: usize,
    pub avg_reply_secs: f64,
    pub median_reply_secs: f64,

    pub longest_thread_messages: usize,
    pub most_replied_id: String,
    pub most_replied_count: usize,

    pub top_agents: Vec<StatsBar>,
    pub topic_volumes: Vec<StatsBar>,

    pub over_time_counts: Vec<usize>,
    pub over_time_start_secs: i64,
    pub over_time_interval_secs: i64,

    pub response_latency: Vec<StatsBucket>,

    pub busiest_hour_start_secs: i64,
    pub busiest_hour_count: usize,
    pub quietest_hour_start_secs: i64,
    pub quietest_hour_count: usize,

    pub thread_avg_messages: f64,
    pub thread_dist: StatsThreadDist,
}

impl Default for StatsSnapshot {
    fn default() -> Self {
        Self {
            total_messages: 0,
            active_agents: 0,
            active_topics: 0,
            reply_samples: 0,
            avg_reply_secs: 0.0,
            median_reply_secs: 0.0,
            longest_thread_messages: 0,
            most_replied_id: String::new(),
            most_replied_count: 0,
            top_agents: Vec::new(),
            topic_volumes: Vec::new(),
            over_time_counts: Vec::new(),
            over_time_start_secs: 0,
            over_time_interval_secs: 0,
            response_latency: Vec::new(),
            busiest_hour_start_secs: 0,
            busiest_hour_count: 0,
            quietest_hour_start_secs: 0,
            quietest_hour_count: 0,
            thread_avg_messages: 0.0,
            thread_dist: StatsThreadDist::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Time window constants (seconds, Go parity: newStatsView windows)
// ---------------------------------------------------------------------------

/// Available time windows: 4h, 12h, 24h, 7d, 30d, 0 (all-time).
pub const TIME_WINDOWS: &[i64] = &[
    4 * 3600,
    12 * 3600,
    24 * 3600,
    7 * 24 * 3600,
    30 * 24 * 3600,
    0, // all-time
];

/// Default window index (24h).
pub const DEFAULT_WINDOW_IDX: usize = 2;

// ---------------------------------------------------------------------------
// StatsViewModel (Go parity: statsView — view-model portion)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct StatsViewModel {
    window_idx: usize,
    /// Explicit window end in epoch secs; 0 means "follow tail".
    window_end_secs: i64,
    /// Current time for rendering purposes.
    now_secs: i64,

    /// Loaded data boundaries (epoch secs).
    loaded_start_secs: i64,
    loaded_end_secs: i64,

    /// All messages in the current window.
    messages: Vec<StatsMessage>,

    /// Computed stats.
    snap: StatsSnapshot,

    /// Loading / error state.
    loading: bool,
    error: Option<String>,
}

impl Default for StatsViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            window_idx: DEFAULT_WINDOW_IDX,
            window_end_secs: 0,
            now_secs: 0,
            loaded_start_secs: 0,
            loaded_end_secs: 0,
            messages: Vec::new(),
            snap: StatsSnapshot::default(),
            loading: false,
            error: None,
        }
    }

    /// Set loading state.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Set error state.
    pub fn set_error(&mut self, err: Option<String>) {
        self.error = err;
        self.loading = false;
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Set current time (epoch secs).
    pub fn set_now(&mut self, now_secs: i64) {
        self.now_secs = now_secs;
    }

    #[must_use]
    pub fn now_secs(&self) -> i64 {
        self.now_secs
    }

    /// Current window index.
    #[must_use]
    pub fn window_idx(&self) -> usize {
        self.window_idx
    }

    /// Current window duration in seconds (0 = all-time).
    #[must_use]
    pub fn window_duration_secs(&self) -> i64 {
        TIME_WINDOWS.get(self.window_idx).copied().unwrap_or(0)
    }

    /// Navigate to previous (shorter) time window.
    pub fn prev_window(&mut self) {
        if self.window_idx > 0 {
            self.window_idx -= 1;
            self.window_end_secs = 0;
        }
    }

    /// Navigate to next (longer) time window.
    pub fn next_window(&mut self) {
        if self.window_idx < TIME_WINDOWS.len().saturating_sub(1) {
            self.window_idx += 1;
            self.window_end_secs = 0;
        }
    }

    /// Pan left (earlier in time).
    pub fn pan_left(&mut self) {
        let step = self.pan_step();
        if step > 0 {
            self.window_end_secs = self.effective_end().saturating_sub(step);
        }
    }

    /// Pan right (later in time).
    pub fn pan_right(&mut self) {
        let step = self.pan_step();
        if step > 0 {
            self.window_end_secs = self.effective_end().saturating_add(step);
        }
    }

    /// Request refresh (caller should reload data).
    pub fn request_refresh(&mut self) {
        self.loading = true;
    }

    /// Load messages and recompute stats. This replaces all current data.
    pub fn load_messages(
        &mut self,
        messages: Vec<StatsMessage>,
        start_secs: i64,
        end_secs: i64,
        now_secs: i64,
    ) {
        self.loading = false;
        self.error = None;
        self.now_secs = now_secs;
        self.messages = messages;

        let d = self.window_duration_secs();
        if d == 0 {
            // All-time: derive bounds from data.
            let (min_t, max_t) = min_max_time(&self.messages);
            self.loaded_start_secs = min_t;
            self.loaded_end_secs = if max_t > 0 { max_t + 1 } else { now_secs };
        } else {
            self.loaded_start_secs = start_secs;
            self.loaded_end_secs = end_secs;
            if self.following_tail() {
                self.window_end_secs = now_secs;
            }
        }

        self.snap = compute_stats(&self.messages, self.loaded_start_secs, self.loaded_end_secs);
    }

    /// Add a single incoming message (real-time update).
    pub fn add_message(&mut self, msg: StatsMessage, now_secs: i64) {
        self.now_secs = now_secs;

        let d = self.window_duration_secs();
        if d > 0 && !self.following_tail() {
            return;
        }

        self.messages.push(msg);
        sort_messages(&mut self.messages);

        if d > 0 {
            if self.loaded_end_secs == 0 || now_secs > self.loaded_end_secs {
                self.loaded_end_secs = now_secs;
            }
            if self.window_end_secs == 0 {
                self.window_end_secs = now_secs;
            }
        }
        if self.loaded_start_secs == 0 || self.loaded_end_secs == 0 {
            let (min_t, max_t) = min_max_time(&self.messages);
            self.loaded_start_secs = min_t;
            self.loaded_end_secs = if max_t > 0 { max_t + 1 } else { max_t };
        }

        self.snap = compute_stats(&self.messages, self.loaded_start_secs, self.loaded_end_secs);
    }

    /// Access the computed snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &StatsSnapshot {
        &self.snap
    }

    /// Window bounds: (start_secs, end_secs, all_time).
    #[must_use]
    pub fn window_bounds(&self) -> (i64, i64, bool) {
        let d = self.window_duration_secs();
        if d == 0 {
            return (0, 0, true);
        }
        let end = self.effective_end();
        let start = end.saturating_sub(d);
        (start, end, false)
    }

    /// Range label for header display.
    #[must_use]
    pub fn range_label(&self) -> String {
        let d = self.window_duration_secs();
        if d == 0 {
            return "all-time".to_owned();
        }
        if d < 24 * 3600 {
            return format!("last {}", format_duration_compact(d));
        }
        let days = d / (24 * 3600);
        if days % 7 == 0 && days >= 7 {
            let weeks = days / 7;
            if weeks == 1 {
                return "last 7d".to_owned();
            }
            return format!("last {weeks}w");
        }
        format!("last {days}d")
    }

    // -- internal helpers --

    fn effective_end(&self) -> i64 {
        if self.window_end_secs > 0 {
            self.window_end_secs
        } else {
            self.now_secs
        }
    }

    fn following_tail(&self) -> bool {
        let d = self.window_duration_secs();
        if d == 0 {
            return false;
        }
        if self.window_end_secs == 0 {
            return true;
        }
        let diff = self.now_secs - self.window_end_secs;
        if diff < 0 {
            return true;
        }
        diff <= 2
    }

    fn pan_step(&self) -> i64 {
        let d = self.window_duration_secs();
        if d <= 0 {
            return 0;
        }
        let step = d / 6;
        if step < 15 * 60 {
            15 * 60
        } else {
            step
        }
    }
}

// ---------------------------------------------------------------------------
// Input handler (Go parity: statsView.handleKey)
// ---------------------------------------------------------------------------

pub fn apply_stats_input(view: &mut StatsViewModel, event: InputEvent) {
    match translate_input(&event) {
        UiAction::MoveLeft => view.pan_left(),
        UiAction::MoveRight => view.pan_right(),
        _ => {}
    }
    // Also handle raw keys for [, ], r.
    if let InputEvent::Key(key_event) = &event {
        if !key_event.modifiers.ctrl && !key_event.modifiers.alt {
            match key_event.key {
                forge_ftui_adapter::input::Key::Char('[') => view.prev_window(),
                forge_ftui_adapter::input::Key::Char(']') => view.next_window(),
                forge_ftui_adapter::input::Key::Char('r') => view.request_refresh(),
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render (Go parity: statsView.View, renderLeft, renderRight)
// ---------------------------------------------------------------------------

#[must_use]
pub fn render_stats_frame(
    view: &StatsViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let range_label = view.range_label();
    let header = truncate(&format!("STATS  {range_label}"), width);
    frame.draw_text(0, 0, &header, TextRole::Accent);

    if height == 1 {
        return frame;
    }

    if let Some(err) = &view.error {
        let msg = truncate(&format!("error: {err}"), width);
        frame.draw_text(0, 2.min(height - 1), &msg, TextRole::Muted);
        return frame;
    }

    if view.loading {
        frame.draw_text(0, 2.min(height - 1), "Loading...", TextRole::Muted);
        return frame;
    }

    let s = &view.snap;

    // Determine layout: two columns or single.
    let inner_w = width;
    let left_w = inner_w / 2;
    let right_w = if left_w < 24 { 0 } else { inner_w - left_w - 1 };

    let left_lines = render_left(s, &view.messages, left_w.max(inner_w.min(width)));
    let right_lines = if right_w > 0 {
        render_right(s, right_w, view)
    } else {
        Vec::new()
    };

    // Interleave into frame starting at row 2 (row 0 = header, row 1 = blank).
    let start_row = 2.min(height);
    let avail = height.saturating_sub(start_row);

    if right_w == 0 {
        // Single column.
        for (i, line) in left_lines.iter().enumerate() {
            if i >= avail {
                break;
            }
            frame.draw_text(0, start_row + i, &truncate(line, width), TextRole::Primary);
        }
    } else {
        // Two columns side by side.
        let max_rows = left_lines.len().max(right_lines.len()).min(avail);
        for i in 0..max_rows {
            let left_text = left_lines
                .get(i)
                .map(|s| truncate(s, left_w))
                .unwrap_or_default();
            let right_text = right_lines
                .get(i)
                .map(|s| truncate(s, right_w))
                .unwrap_or_default();

            // Pad left to column width, add divider, then right.
            let padded_left = format!("{:<width$}", left_text, width = left_w);
            let combined = format!("{padded_left}|{right_text}");
            frame.draw_text(
                0,
                start_row + i,
                &truncate(&combined, width),
                TextRole::Primary,
            );
        }
    }

    // Footer.
    let footer_row = height.saturating_sub(1);
    if footer_row > start_row {
        let footer = truncate("[/]: range  \u{2190}/\u{2192}: pan  r: refresh", width);
        frame.draw_text(0, footer_row, &footer, TextRole::Muted);
    }

    frame
}

fn render_left(s: &StatsSnapshot, messages: &[StatsMessage], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::with_capacity(32);
    if width == 0 {
        return lines;
    }

    // OVERVIEW
    lines.push("OVERVIEW".to_owned());
    lines.push(format!("Total messages: {}", s.total_messages));
    lines.push(format!("Active agents:  {}", s.active_agents));
    lines.push(format!("Active topics:  {}", s.active_topics));

    if s.reply_samples > 0 {
        lines.push(format!(
            "Avg reply time: {}",
            format_duration_compact(s.avg_reply_secs as i64)
        ));
        lines.push(format!(
            "Median reply:   {}",
            format_duration_compact(s.median_reply_secs as i64)
        ));
    } else {
        lines.push("Avg reply time: -".to_owned());
        lines.push("Median reply:   -".to_owned());
    }

    if s.longest_thread_messages > 0 {
        lines.push(format!(
            "Longest thread: {} msgs",
            s.longest_thread_messages
        ));
    } else {
        lines.push("Longest thread: -".to_owned());
    }

    if s.most_replied_count > 0 && !s.most_replied_id.trim().is_empty() {
        let parent_line = find_by_id(messages, &s.most_replied_id)
            .map(|m| first_non_empty_line(&m.body))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| s.most_replied_id.clone());
        lines.push(truncate(
            &format!("Most replied:  {} ({})", s.most_replied_count, parent_line),
            width,
        ));
    } else {
        lines.push("Most replied:  -".to_owned());
    }

    lines.push(String::new());

    // TOP AGENTS
    lines.push("TOP AGENTS (by msgs sent)".to_owned());
    if s.top_agents.is_empty() {
        lines.push("No data".to_owned());
    } else {
        let max_c = s.top_agents.iter().map(|a| a.count).max().unwrap_or(0);
        let bar_w = width.saturating_sub(18).min(24);
        for (i, a) in s.top_agents.iter().enumerate() {
            let bar = render_bar(a.count, max_c, bar_w);
            let line = format!(
                "{:2}. {:<10} {:4} {}",
                i + 1,
                truncate(&a.label, 10),
                a.count,
                bar
            );
            lines.push(truncate(&line, width));
        }
    }

    lines.push(String::new());

    // BUSIEST / QUIETEST HOUR
    lines.push("BUSIEST / QUIETEST HOUR".to_owned());
    if s.busiest_hour_start_secs > 0 || s.quietest_hour_start_secs > 0 {
        if s.busiest_hour_start_secs > 0 {
            lines.push(format!(
                "Busiest:  {} ({} msgs)",
                hour_range_label(s.busiest_hour_start_secs),
                s.busiest_hour_count
            ));
        }
        if s.quietest_hour_start_secs > 0 {
            lines.push(format!(
                "Quietest: {} ({} msgs)",
                hour_range_label(s.quietest_hour_start_secs),
                s.quietest_hour_count
            ));
        }
    } else {
        lines.push("No data".to_owned());
    }

    lines.push(String::new());

    // THREAD DEPTH
    lines.push("THREAD DEPTH".to_owned());
    if s.thread_avg_messages > 0.0 {
        lines.push(format!("Avg msgs/thread: {:.1}", s.thread_avg_messages));
        lines.push(format!("Standalone: {}", s.thread_dist.standalone));
        lines.push(format!("2-3 msgs:   {}", s.thread_dist.small));
        lines.push(format!("4-10 msgs:  {}", s.thread_dist.medium));
        lines.push(format!("10+ msgs:   {}", s.thread_dist.large));
    } else {
        lines.push("No threads".to_owned());
    }

    lines
}

fn render_right(s: &StatsSnapshot, width: usize, view: &StatsViewModel) -> Vec<String> {
    let mut lines: Vec<String> = Vec::with_capacity(32);
    if width == 0 {
        return lines;
    }

    // MESSAGES OVER TIME
    lines.push("MESSAGES OVER TIME".to_owned());
    if s.over_time_counts.is_empty() {
        lines.push("No data".to_owned());
    } else {
        let spark = render_spark(&s.over_time_counts);
        lines.push(truncate(&spark, width));
        if s.over_time_start_secs > 0
            && s.over_time_interval_secs > 0
            && view.window_duration_secs() > 0
        {
            let start_label = format_utc_hhmm(view.loaded_start_secs);
            let end_label = format_utc_hhmm(view.loaded_end_secs);
            lines.push(truncate(&format!("{start_label}  ...  {end_label}"), width));
        }
    }

    lines.push(String::new());

    // TOPIC VOLUME
    lines.push("TOPIC VOLUME".to_owned());
    if s.topic_volumes.is_empty() {
        lines.push("No data".to_owned());
    } else {
        let max_c = s.topic_volumes.iter().map(|t| t.count).max().unwrap_or(0);
        let bar_w = width.saturating_sub(18).min(24);
        for t in &s.topic_volumes {
            let bar = render_bar(t.count, max_c, bar_w);
            let line = format!("{:<10} {:4} {}", truncate(&t.label, 10), t.count, bar);
            lines.push(truncate(&line, width));
        }
    }

    lines.push(String::new());

    // RESPONSE LATENCY
    lines.push("RESPONSE LATENCY".to_owned());
    if s.response_latency.is_empty() || s.reply_samples == 0 {
        lines.push("No replies".to_owned());
    } else {
        let max_c = s
            .response_latency
            .iter()
            .map(|b| b.count)
            .max()
            .unwrap_or(0);
        let bar_w = width.saturating_sub(18).min(24);
        for b in &s.response_latency {
            let bar = render_bar(b.count, max_c, bar_w);
            let line = format!("{:<7} {} {:4.0}%", b.label, bar, b.pct);
            lines.push(truncate(&line, width));
        }
    }

    lines
}

// ---------------------------------------------------------------------------
// Stats computation (Go parity: stats_compute.go)
// ---------------------------------------------------------------------------

/// Compute stats from messages within a time window (Go: computeStats).
#[must_use]
pub fn compute_stats(
    messages: &[StatsMessage],
    window_start_secs: i64,
    window_end_secs: i64,
) -> StatsSnapshot {
    let filtered = filter_messages_by_time(messages, window_start_secs, window_end_secs);
    let mut out = StatsSnapshot {
        total_messages: filtered.len(),
        ..StatsSnapshot::default()
    };
    if filtered.is_empty() {
        return out;
    }

    // Active agents/topics.
    let mut agents: HashMap<&str, usize> = HashMap::new();
    let mut topics: HashMap<&str, usize> = HashMap::new();
    let mut by_id: HashMap<&str, &StatsMessage> = HashMap::with_capacity(filtered.len());

    for msg in &filtered {
        let from = msg.from.trim();
        if !from.is_empty() {
            *agents.entry(from).or_insert(0) += 1;
        }
        let to = msg.to.trim();
        if !to.is_empty() {
            *topics.entry(to).or_insert(0) += 1;
        }
        let id = msg.id.trim();
        if !id.is_empty() {
            by_id.insert(id, msg);
        }
    }
    out.active_agents = agents.len();
    out.active_topics = topics.len();

    out.top_agents = top_n(&agents, 10);
    out.topic_volumes = top_n(&topics, 10);

    // Reply latency + most-replied.
    let mut reply_counts: HashMap<&str, usize> = HashMap::new();
    let mut reply_deltas: Vec<i64> = Vec::new();

    for msg in &filtered {
        let parent_id = msg.reply_to.trim();
        if parent_id.is_empty() {
            continue;
        }
        *reply_counts.entry(parent_id).or_insert(0) += 1;
        if let Some(parent) = by_id.get(parent_id) {
            if parent.time_secs > 0 && msg.time_secs > 0 {
                let delta = msg.time_secs - parent.time_secs;
                if delta >= 0 {
                    reply_deltas.push(delta);
                }
            }
        }
    }

    if !reply_counts.is_empty() {
        let (best_id, best_count) = reply_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&id, &count)| (id.to_owned(), count))
            .unwrap_or_default();
        out.most_replied_id = best_id;
        out.most_replied_count = best_count;
    }

    out.reply_samples = reply_deltas.len();
    if !reply_deltas.is_empty() {
        let total: i64 = reply_deltas.iter().sum();
        out.avg_reply_secs = total as f64 / reply_deltas.len() as f64;

        reply_deltas.sort_unstable();
        let mid = reply_deltas.len() / 2;
        out.median_reply_secs = if reply_deltas.len() % 2 == 1 {
            reply_deltas[mid] as f64
        } else if reply_deltas.len() > 1 {
            (reply_deltas[mid - 1] + reply_deltas[mid]) as f64 / 2.0
        } else {
            reply_deltas[0] as f64
        };
    }

    out.response_latency = latency_buckets(&reply_deltas);

    // Threads (simplified: group by reply chains).
    let threads = build_thread_sizes(&filtered);
    if !threads.is_empty() {
        let mut total_msgs: usize = 0;
        for &n in &threads {
            total_msgs += n;
            if n > out.longest_thread_messages {
                out.longest_thread_messages = n;
            }
            match n {
                0 => {}
                1 => out.thread_dist.standalone += 1,
                2..=3 => out.thread_dist.small += 1,
                4..=10 => out.thread_dist.medium += 1,
                _ => out.thread_dist.large += 1,
            }
        }
        out.thread_avg_messages = total_msgs as f64 / threads.len() as f64;
    }

    // Time buckets.
    out.over_time_interval_secs = choose_bucket_interval(window_start_secs, window_end_secs, 48);
    out.over_time_start_secs = bucket_start_time(window_start_secs, out.over_time_interval_secs);
    out.over_time_counts = bucket_counts(
        &filtered,
        out.over_time_start_secs,
        window_end_secs,
        out.over_time_interval_secs,
    );

    // Busiest/quietest hour.
    let (busy_start, busy_count, quiet_start, quiet_count) =
        busiest_quietest_hour(&filtered, window_start_secs, window_end_secs);
    out.busiest_hour_start_secs = busy_start;
    out.busiest_hour_count = busy_count;
    out.quietest_hour_start_secs = quiet_start;
    out.quietest_hour_count = quiet_count;

    out
}

fn filter_messages_by_time(
    messages: &[StatsMessage],
    start_secs: i64,
    end_secs: i64,
) -> Vec<&StatsMessage> {
    messages
        .iter()
        .filter(|msg| {
            let ts = msg.time_secs;
            if start_secs > 0 && (ts == 0 || ts < start_secs) {
                return false;
            }
            if end_secs > 0 && (ts == 0 || ts >= end_secs) {
                return false;
            }
            true
        })
        .collect()
}

fn top_n(counts: &HashMap<&str, usize>, limit: usize) -> Vec<StatsBar> {
    if counts.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut bars: Vec<StatsBar> = counts
        .iter()
        .filter(|(k, &v)| !k.trim().is_empty() && v > 0)
        .map(|(&k, &v)| StatsBar {
            label: k.trim().to_owned(),
            count: v,
        })
        .collect();
    bars.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    bars.truncate(limit);
    bars
}

fn latency_buckets(deltas: &[i64]) -> Vec<StatsBucket> {
    let total = deltas.len();
    let mut buckets = vec![
        StatsBucket {
            label: "<30s".to_owned(),
            count: 0,
            pct: 0.0,
        },
        StatsBucket {
            label: "30s-5m".to_owned(),
            count: 0,
            pct: 0.0,
        },
        StatsBucket {
            label: "5m-30m".to_owned(),
            count: 0,
            pct: 0.0,
        },
        StatsBucket {
            label: "30m-2h".to_owned(),
            count: 0,
            pct: 0.0,
        },
        StatsBucket {
            label: ">2h".to_owned(),
            count: 0,
            pct: 0.0,
        },
    ];
    for &d in deltas {
        let idx = if d < 30 {
            0
        } else if d < 5 * 60 {
            1
        } else if d < 30 * 60 {
            2
        } else if d < 2 * 3600 {
            3
        } else {
            4
        };
        buckets[idx].count += 1;
    }
    if total > 0 {
        for b in &mut buckets {
            b.pct = b.count as f64 / total as f64 * 100.0;
        }
    }
    buckets
}

/// Choose a "nice" bucket interval. (Go: chooseBucketInterval)
#[must_use]
pub fn choose_bucket_interval(start_secs: i64, end_secs: i64, max_buckets: usize) -> i64 {
    let max_buckets = if max_buckets == 0 { 48 } else { max_buckets };
    if start_secs == 0 || end_secs == 0 || end_secs <= start_secs {
        return 3600; // 1h fallback
    }
    let duration = end_secs - start_secs;
    let candidates: &[i64] = &[
        60,            // 1m
        5 * 60,        // 5m
        10 * 60,       // 10m
        15 * 60,       // 15m
        30 * 60,       // 30m
        3600,          // 1h
        2 * 3600,      // 2h
        4 * 3600,      // 4h
        6 * 3600,      // 6h
        12 * 3600,     // 12h
        24 * 3600,     // 24h
        48 * 3600,     // 48h
        7 * 24 * 3600, // 7d
    ];
    let target = duration / max_buckets as i64;
    for &cand in candidates {
        if cand >= target {
            return cand;
        }
    }
    *candidates.last().unwrap_or(&3600)
}

/// Align a start time to a bucket boundary. (Go: bucketStartTime)
#[must_use]
pub fn bucket_start_time(start_secs: i64, interval_secs: i64) -> i64 {
    if start_secs == 0 || interval_secs <= 0 {
        return start_secs;
    }
    if interval_secs >= 24 * 3600 {
        // Align to UTC day start.
        start_secs - (start_secs % (24 * 3600))
    } else if interval_secs >= 3600 {
        // Align to UTC hour start.
        start_secs - (start_secs % 3600)
    } else {
        // Align to interval.
        start_secs - (start_secs % interval_secs)
    }
}

fn bucket_counts(
    messages: &[&StatsMessage],
    start_secs: i64,
    end_secs: i64,
    interval_secs: i64,
) -> Vec<usize> {
    if interval_secs <= 0 || start_secs == 0 || end_secs == 0 || end_secs <= start_secs {
        return Vec::new();
    }
    let n = ((end_secs - start_secs) / interval_secs + 1) as usize;
    if n == 0 {
        return Vec::new();
    }
    let mut counts = vec![0usize; n];
    for msg in messages {
        let ts = msg.time_secs;
        if ts == 0 || ts < start_secs || ts >= end_secs {
            continue;
        }
        let idx = ((ts - start_secs) / interval_secs) as usize;
        if idx < counts.len() {
            counts[idx] += 1;
        }
    }
    counts
}

fn busiest_quietest_hour(
    messages: &[&StatsMessage],
    start_secs: i64,
    end_secs: i64,
) -> (i64, usize, i64, usize) {
    if start_secs == 0 || end_secs == 0 || end_secs <= start_secs {
        return (0, 0, 0, 0);
    }

    let mut counts: HashMap<i64, usize> = HashMap::new();
    for msg in messages {
        let ts = msg.time_secs;
        if ts == 0 || ts < start_secs || ts >= end_secs {
            continue;
        }
        let h = ts - (ts % 3600);
        *counts.entry(h).or_insert(0) += 1;
    }

    let first = start_secs - (start_secs % 3600);
    let mut last = end_secs - (end_secs % 3600);
    if last < end_secs {
        last += 3600;
    }

    let mut busy_start: i64 = 0;
    let mut busy_count: i64 = -1;
    let mut quiet_start: i64 = 0;
    let mut quiet_count: i64 = -1;

    let mut cur = first;
    while cur < last {
        let c = *counts.get(&cur).unwrap_or(&0) as i64;
        if busy_count < 0 || c > busy_count {
            busy_count = c;
            busy_start = cur;
        }
        if quiet_count < 0 || c < quiet_count {
            quiet_count = c;
            quiet_start = cur;
        }
        cur += 3600;
    }

    (
        busy_start,
        busy_count.max(0) as usize,
        quiet_start,
        quiet_count.max(0) as usize,
    )
}

/// Build simple thread sizes from messages using reply_to chains.
/// Returns a Vec of thread sizes (number of messages per thread).
fn build_thread_sizes(messages: &[&StatsMessage]) -> Vec<usize> {
    if messages.is_empty() {
        return Vec::new();
    }

    // Build parent map.
    let id_set: HashMap<&str, usize> = messages
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let id = m.id.trim();
            if id.is_empty() {
                None
            } else {
                Some((id, i))
            }
        })
        .collect();

    // Union-Find for grouping by reply chains.
    let n = messages.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    for (i, msg) in messages.iter().enumerate() {
        let reply_to = msg.reply_to.trim();
        if reply_to.is_empty() || reply_to == msg.id.trim() {
            continue;
        }
        if let Some(&j) = id_set.get(reply_to) {
            union(&mut parent, i, j);
        }
    }

    // Collect thread sizes.
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        *sizes.entry(root).or_insert(0) += 1;
    }

    sizes.into_values().collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sort_messages(messages: &mut [StatsMessage]) {
    messages.sort_by(|a, b| {
        a.time_secs
            .cmp(&b.time_secs)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.from.cmp(&b.from))
    });
}

fn min_max_time(messages: &[StatsMessage]) -> (i64, i64) {
    let mut min_t: i64 = 0;
    let mut max_t: i64 = 0;
    for msg in messages {
        let ts = msg.time_secs;
        if ts == 0 {
            continue;
        }
        if min_t == 0 || ts < min_t {
            min_t = ts;
        }
        if max_t == 0 || ts > max_t {
            max_t = ts;
        }
    }
    (min_t, max_t)
}

fn find_by_id<'a>(messages: &'a [StatsMessage], id: &str) -> Option<&'a StatsMessage> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    messages.iter().find(|m| m.id.trim() == id)
}

fn first_non_empty_line(s: &str) -> String {
    for line in s.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    String::new()
}

/// Render a sparkline from integer values. (Go: renderSpark)
fn render_spark(values: &[usize]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let max_v = values.iter().copied().max().unwrap_or(0);
    let levels: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut out = String::with_capacity(values.len() * 3);
    for &v in values {
        if max_v == 0 || v == 0 {
            out.push(levels[0]);
            continue;
        }
        let mut idx = (v as f64 / max_v as f64 * (levels.len() - 1) as f64) as usize;
        if idx >= levels.len() {
            idx = levels.len() - 1;
        }
        out.push(levels[idx]);
    }
    out
}

/// Render a bar chart string. (Go: renderBar)
fn render_bar(value: usize, max_value: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if max_value == 0 || value == 0 {
        return " ".repeat(width);
    }
    let n = ((value as f64 / max_value as f64) * width as f64) as usize;
    let n = n.min(width);
    format!("{}{}", "\u{2588}".repeat(n), " ".repeat(width - n))
}

/// Format a duration in seconds compactly. (Go: formatDurationCompact)
#[must_use]
pub fn format_duration_compact(secs: i64) -> String {
    if secs <= 0 {
        return "0s".to_owned();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            return format!("{m}m");
        }
        return format!("{m}m{s}s");
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if m == 0 {
        return format!("{h}h");
    }
    format!("{h}h{m}m")
}

/// Format UTC HH:MM from epoch seconds.
fn format_utc_hhmm(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return "--:--".to_owned();
    }
    let secs_in_day = epoch_secs.rem_euclid(86400);
    let hh = secs_in_day / 3600;
    let mm = (secs_in_day % 3600) / 60;
    format!("{hh:02}:{mm:02}")
}

/// Format an hour range label from epoch secs. (Go: hourRangeLabel)
fn hour_range_label(start_secs: i64) -> String {
    if start_secs <= 0 {
        return "-".to_owned();
    }
    let start_label = format_utc_hhmm(start_secs);
    let end_label = format_utc_hhmm(start_secs + 3600);
    format!("{start_label}-{end_label}")
}

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;

    fn make_msg(id: &str, from: &str, to: &str, time_secs: i64, body: &str) -> StatsMessage {
        StatsMessage {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            reply_to: String::new(),
            time_secs,
            body: body.to_owned(),
        }
    }

    fn make_reply(
        id: &str,
        from: &str,
        to: &str,
        time_secs: i64,
        body: &str,
        reply_to: &str,
    ) -> StatsMessage {
        StatsMessage {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            reply_to: reply_to.to_owned(),
            time_secs,
            body: body.to_owned(),
        }
    }

    // Base time: 2026-02-09T10:00:00Z = 1770631200
    const T0: i64 = 1_770_631_200;

    fn sample_messages() -> Vec<StatsMessage> {
        vec![
            make_msg("1", "architect", "task", T0, "root"),
            make_reply("2", "coder", "task", T0 + 10, "reply1", "1"),
            make_msg("3", "architect", "build", T0 + 120, "note"),
            make_msg("4", "tester", "@architect", T0 + 3600, "dm root"),
            make_reply("5", "architect", "@tester", T0 + 3640, "dm reply", "4"),
            make_reply("6", "reviewer", "task", T0 + 20, "reply2", "1"),
        ]
    }

    // -- compute_stats tests (Go parity: TestComputeStats_Basics) --

    #[test]
    fn compute_stats_basics() {
        let msgs = sample_messages();
        let start = T0;
        let end = T0 + 2 * 3600;
        let s = compute_stats(&msgs, start, end);

        assert_eq!(s.total_messages, 6);
        assert_eq!(s.active_agents, 4);
        assert_eq!(s.active_topics, 4);

        assert_eq!(s.reply_samples, 3);
        assert_eq!(s.median_reply_secs, 20.0);
        // Avg: (10 + 20 + 40) / 3 ≈ 23.33
        assert!((s.avg_reply_secs - 70.0 / 3.0).abs() < 1.0);

        assert_eq!(s.longest_thread_messages, 3);
        assert_eq!(s.most_replied_id, "1");
        assert_eq!(s.most_replied_count, 2);

        assert!(!s.top_agents.is_empty());
        assert_eq!(s.top_agents[0].label, "architect");
        assert_eq!(s.top_agents[0].count, 3);

        assert!(!s.topic_volumes.is_empty());
        assert_eq!(s.topic_volumes[0].label, "task");
        assert_eq!(s.topic_volumes[0].count, 3);

        assert!(!s.response_latency.is_empty());
        assert_eq!(s.response_latency[0].label, "<30s");
        assert_eq!(s.response_latency[0].count, 2);
        assert_eq!(s.response_latency[1].label, "30s-5m");
        assert_eq!(s.response_latency[1].count, 1);

        assert_eq!(s.busiest_hour_start_secs, T0);
        assert_eq!(s.busiest_hour_count, 4);
        assert_eq!(s.quietest_hour_start_secs, T0 + 3600);
        assert_eq!(s.quietest_hour_count, 2);

        assert!((s.thread_avg_messages - 2.0).abs() < 0.01);
        assert_eq!(s.thread_dist.standalone, 1);
        assert_eq!(s.thread_dist.small, 2);
        assert_eq!(s.thread_dist.medium, 0);
        assert_eq!(s.thread_dist.large, 0);

        let sum: usize = s.over_time_counts.iter().sum();
        assert_eq!(sum, 6);
    }

    #[test]
    fn compute_stats_empty() {
        let s = compute_stats(&[], T0, T0 + 3600);
        assert_eq!(s.total_messages, 0);
        assert_eq!(s.active_agents, 0);
    }

    // -- choose_bucket_interval tests (Go parity: TestChooseBucketInterval_PrefersNiceSteps) --

    #[test]
    fn choose_bucket_interval_nice_steps() {
        let start = T0;
        assert_eq!(choose_bucket_interval(start, start + 2 * 3600, 48), 5 * 60);
        assert_eq!(
            choose_bucket_interval(start, start + 24 * 3600, 48),
            30 * 60
        );
        assert_eq!(
            choose_bucket_interval(start, start + 30 * 24 * 3600, 48),
            24 * 3600
        );
    }

    #[test]
    fn choose_bucket_interval_fallback() {
        assert_eq!(choose_bucket_interval(0, 0, 48), 3600);
        assert_eq!(choose_bucket_interval(T0, T0, 48), 3600);
    }

    // -- format_duration_compact tests --

    #[test]
    fn format_duration_compact_cases() {
        assert_eq!(format_duration_compact(0), "0s");
        assert_eq!(format_duration_compact(-5), "0s");
        assert_eq!(format_duration_compact(45), "45s");
        assert_eq!(format_duration_compact(120), "2m");
        assert_eq!(format_duration_compact(150), "2m30s");
        assert_eq!(format_duration_compact(3600), "1h");
        assert_eq!(format_duration_compact(3720), "1h2m");
    }

    // -- hour_range_label tests --

    #[test]
    fn hour_range_label_formatting() {
        assert_eq!(hour_range_label(0), "-");
        assert_eq!(hour_range_label(T0), "10:00-11:00");
        assert_eq!(hour_range_label(T0 + 3600), "11:00-12:00");
    }

    // -- render_spark tests --

    #[test]
    fn render_spark_basic() {
        assert_eq!(render_spark(&[]), "");
        assert_eq!(render_spark(&[0, 0]), "▁▁");
        let spark = render_spark(&[0, 1, 2, 4, 8]);
        assert_eq!(spark.chars().count(), 5);
        assert_eq!(spark.chars().last(), Some('█'));
    }

    // -- render_bar tests --

    #[test]
    fn render_bar_basic() {
        assert_eq!(render_bar(0, 0, 10), "          ");
        assert_eq!(render_bar(5, 10, 10), "█████     ");
        assert_eq!(render_bar(10, 10, 10), "██████████");
    }

    // -- latency_buckets tests --

    #[test]
    fn latency_buckets_distribution() {
        let deltas = vec![5, 10, 60, 600, 7200, 10000];
        let buckets = latency_buckets(&deltas);
        assert_eq!(buckets.len(), 5);
        assert_eq!(buckets[0].count, 2); // <30s
        assert_eq!(buckets[1].count, 1); // 30s-5m
        assert_eq!(buckets[2].count, 1); // 5m-30m
        assert_eq!(buckets[3].count, 0); // 30m-2h
        assert_eq!(buckets[4].count, 2); // >2h
    }

    // -- StatsViewModel tests --

    #[test]
    fn view_model_default_window() {
        let vm = StatsViewModel::new();
        assert_eq!(vm.window_idx(), DEFAULT_WINDOW_IDX);
        assert_eq!(vm.window_duration_secs(), 24 * 3600);
        assert_eq!(vm.range_label(), "last 1d");
    }

    #[test]
    fn view_model_window_navigation() {
        let mut vm = StatsViewModel::new();
        vm.prev_window(); // 12h
        assert_eq!(vm.range_label(), "last 12h");
        vm.prev_window(); // 4h
        assert_eq!(vm.range_label(), "last 4h");
        vm.prev_window(); // still 4h (at min)
        assert_eq!(vm.range_label(), "last 4h");

        vm.next_window(); // 12h
        vm.next_window(); // 24h
        vm.next_window(); // 7d
        assert_eq!(vm.range_label(), "last 7d");
        vm.next_window(); // 30d
        assert_eq!(vm.range_label(), "last 30d");
        vm.next_window(); // all-time
        assert_eq!(vm.range_label(), "all-time");
        vm.next_window(); // still all-time
        assert_eq!(vm.range_label(), "all-time");
    }

    #[test]
    fn view_model_load_and_snapshot() {
        let mut vm = StatsViewModel::new();
        vm.set_now(T0 + 2 * 3600);
        vm.load_messages(sample_messages(), T0, T0 + 2 * 3600, T0 + 2 * 3600);

        let s = vm.snapshot();
        assert_eq!(s.total_messages, 6);
        assert_eq!(s.active_agents, 4);
        assert!(!vm.is_loading());
    }

    #[test]
    fn view_model_add_message() {
        let mut vm = StatsViewModel::new();
        vm.set_now(T0 + 2 * 3600);
        vm.load_messages(sample_messages(), T0, T0 + 2 * 3600, T0 + 2 * 3600);
        assert_eq!(vm.snapshot().total_messages, 6);

        vm.add_message(
            make_msg("7", "newagent", "task", T0 + 100, "new"),
            T0 + 2 * 3600,
        );
        assert_eq!(vm.snapshot().total_messages, 7);
    }

    #[test]
    fn view_model_error_state() {
        let mut vm = StatsViewModel::new();
        vm.set_error(Some("test error".to_owned()));
        assert_eq!(vm.error(), Some("test error"));
        assert!(!vm.is_loading());
    }

    // -- Input handling tests --

    #[test]
    fn input_bracket_keys() {
        let mut vm = StatsViewModel::new();
        assert_eq!(vm.window_idx(), 2); // 24h

        apply_stats_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('['))));
        assert_eq!(vm.window_idx(), 1); // 12h

        apply_stats_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        assert_eq!(vm.window_idx(), 2); // back to 24h
    }

    #[test]
    fn input_refresh() {
        let mut vm = StatsViewModel::new();
        assert!(!vm.is_loading());

        apply_stats_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('r'))));
        assert!(vm.is_loading());
    }

    #[test]
    fn input_pan_left_right() {
        let mut vm = StatsViewModel::new();
        vm.set_now(T0 + 24 * 3600);
        let initial_end = vm.window_bounds().1;

        apply_stats_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Left)));
        let after_left = vm.window_bounds().1;
        assert!(after_left < initial_end);

        apply_stats_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Right)));
        let after_right = vm.window_bounds().1;
        assert!(after_right > after_left);
    }

    // -- Render snapshot test --

    #[test]
    fn render_stats_empty_snapshot() {
        let mut vm = StatsViewModel::new();
        vm.set_loading(true);
        let frame = render_stats_frame(&vm, 40, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_stats_loading",
            &frame,
            "STATS  last 1d                          \n                                        \nLoading...                              \n                                        \n                                        ",
        );
    }

    #[test]
    fn render_stats_with_data_snapshot() {
        let mut vm = StatsViewModel::new();
        vm.set_now(T0 + 2 * 3600);
        vm.load_messages(sample_messages(), T0, T0 + 2 * 3600, T0 + 2 * 3600);

        let frame = render_stats_frame(&vm, 60, 20, ThemeSpec::default());
        // Verify header is present.
        let rendered = frame_to_string(&frame, 60, 20);
        assert!(rendered.contains("STATS  last 1d"));
        assert!(rendered.contains("OVERVIEW"));
        assert!(rendered.contains("Total messages: 6"));
    }

    #[test]
    fn render_stats_error_snapshot() {
        let mut vm = StatsViewModel::new();
        vm.set_error(Some("disk full".to_owned()));

        let frame = render_stats_frame(&vm, 40, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_stats_error",
            &frame,
            "STATS  last 1d                          \n                                        \nerror: disk full                        \n                                        \n                                        ",
        );
    }

    #[test]
    fn render_stats_zero_size() {
        let vm = StatsViewModel::new();
        let frame = render_stats_frame(&vm, 0, 0, ThemeSpec::default());
        assert_eq!(frame.size().width, 0);
        assert_eq!(frame.size().height, 0);
    }

    #[test]
    fn render_stats_single_line() {
        let vm = StatsViewModel::new();
        let frame = render_stats_frame(&vm, 30, 1, ThemeSpec::default());
        let rendered = frame_to_string(&frame, 30, 1);
        assert!(rendered.contains("STATS"));
    }

    // -- Thread size tests --

    #[test]
    fn build_thread_sizes_basic() {
        let msgs = sample_messages();
        let refs: Vec<&StatsMessage> = msgs.iter().collect();
        let sizes = build_thread_sizes(&refs);
        assert!(!sizes.is_empty());
        let total: usize = sizes.iter().sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn build_thread_sizes_empty() {
        let sizes = build_thread_sizes(&[]);
        assert!(sizes.is_empty());
    }

    #[test]
    fn build_thread_sizes_no_replies() {
        let msgs = [
            make_msg("a", "alice", "task", T0, "hello"),
            make_msg("b", "bob", "task", T0 + 1, "world"),
        ];
        let refs: Vec<&StatsMessage> = msgs.iter().collect();
        let sizes = build_thread_sizes(&refs);
        assert_eq!(sizes.len(), 2);
        for &s in &sizes {
            assert_eq!(s, 1);
        }
    }

    // -- Bucket start time tests --

    #[test]
    fn bucket_start_time_alignment() {
        // T0 = 2026-02-09T10:00:00Z, already aligned.
        assert_eq!(bucket_start_time(T0, 3600), T0);
        // Unaligned: T0 + 1800 (10:30) -> truncates to 10:00.
        assert_eq!(bucket_start_time(T0 + 1800, 3600), T0);
        // Day alignment.
        assert_eq!(bucket_start_time(T0, 24 * 3600), T0 - (T0 % (24 * 3600)));
    }

    // -- Truncate tests --

    #[test]
    fn truncate_cases() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
        assert_eq!(truncate("hi", 1), "\u{2026}");
        assert_eq!(truncate("anything", 0), "");
    }

    // -- first_non_empty_line tests --

    #[test]
    fn first_non_empty_line_cases() {
        assert_eq!(first_non_empty_line(""), "");
        assert_eq!(first_non_empty_line("hello"), "hello");
        assert_eq!(first_non_empty_line("\n\nhello\nworld"), "hello");
        assert_eq!(first_non_empty_line("  \n  foo  "), "foo");
    }

    // -- dedup_key tests --

    #[test]
    fn dedup_key_format() {
        let msg = make_msg("  abc  ", "  alice  ", "  task  ", T0, "body");
        assert_eq!(msg.dedup_key(), "abc|alice|task");
    }

    // Helper: convert frame to string for partial assertions.
    fn frame_to_string(frame: &RenderFrame, _width: usize, _height: usize) -> String {
        frame.snapshot()
    }
}

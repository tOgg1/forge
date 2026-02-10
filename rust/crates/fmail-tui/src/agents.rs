use std::collections::HashMap;

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Agent record analogous to Go `fmail.AgentRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    pub name: String,
    pub host: String,
    pub status: String,
    /// Seconds since epoch (UTC).  0 means unknown.
    pub last_seen: i64,
}

impl AgentRecord {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            host: String::new(),
            status: String::new(),
            last_seen: 0,
        }
    }
}

/// A search result entry used when computing agent detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSearchResult {
    pub topic: String,
    pub message_id: String,
    pub from: String,
    pub body: String,
    /// Seconds since epoch (UTC).
    pub time: i64,
}

/// Row displayed in the agent roster list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub rec: AgentRecord,
    pub msg_count: usize,
}

/// Per-target message count in the agent detail panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetCount {
    pub target: String,
    pub count: usize,
}

/// A recent message summary in the agent detail panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecent {
    pub ts: i64,
    pub target: String,
    pub body: String,
    pub id: String,
}

/// Computed detail for the selected agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDetail {
    pub name: String,
    pub rec: AgentRecord,
    pub msg_count_24h: usize,
    pub top_targets: Vec<TargetCount>,
    pub recent: Vec<AgentRecent>,
    pub spark: Vec<usize>,
    /// 48 × 30-minute buckets for uptime bar (last 24h).
    pub uptime: Vec<bool>,
}

impl Default for AgentDetail {
    fn default() -> Self {
        Self {
            name: String::new(),
            rec: AgentRecord::new(""),
            msg_count_24h: 0,
            top_targets: Vec::new(),
            recent: Vec::new(),
            spark: Vec::new(),
            uptime: vec![false; 48],
        }
    }
}

// ---------------------------------------------------------------------------
// Sort key
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSortKey {
    LastSeen,
    Name,
    MsgCount,
    Host,
}

impl AgentSortKey {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::LastSeen => Self::Name,
            Self::Name => Self::MsgCount,
            Self::MsgCount => Self::Host,
            Self::Host => Self::LastSeen,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LastSeen => "seen",
            Self::Name => "name",
            Self::MsgCount => "count(24h)",
            Self::Host => "host",
        }
    }
}

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsMode {
    Roster,
    History,
}

// ---------------------------------------------------------------------------
// View-model
// ---------------------------------------------------------------------------

/// Time windows for the sparkline (seconds).
const WINDOWS: [u64; 5] = [3600, 7200, 14400, 28800, 43200]; // 1h, 2h, 4h, 8h, 12h

pub struct AgentsViewModel {
    /// Current UTC time (epoch seconds), set from outside.
    now: i64,
    mode: AgentsMode,
    sort_key: AgentSortKey,
    filter: String,
    editing: bool,
    records: Vec<AgentRecord>,
    rows: Vec<AgentRow>,
    selected: usize,
    counts: HashMap<String, usize>,
    window_idx: usize,
    detail: AgentDetail,
    detail_agent: String,
    detail_cached: HashMap<String, Vec<AgentSearchResult>>,
    history_selected: usize,
    last_err: Option<String>,
}

impl AgentsViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            now: 0,
            mode: AgentsMode::Roster,
            sort_key: AgentSortKey::LastSeen,
            filter: String::new(),
            editing: false,
            records: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            counts: HashMap::new(),
            window_idx: 2, // 4h default
            detail: AgentDetail::default(),
            detail_agent: String::new(),
            detail_cached: HashMap::new(),
            history_selected: 0,
            last_err: None,
        }
    }

    /// Set the current UTC time (epoch seconds).
    pub fn set_now(&mut self, epoch: i64) {
        self.now = epoch;
    }

    #[must_use]
    pub fn now(&self) -> i64 {
        self.now
    }

    #[must_use]
    pub fn mode(&self) -> AgentsMode {
        self.mode
    }

    #[must_use]
    pub fn sort_key(&self) -> AgentSortKey {
        self.sort_key
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    #[must_use]
    pub fn editing(&self) -> bool {
        self.editing
    }

    #[must_use]
    pub fn rows(&self) -> &[AgentRow] {
        &self.rows
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn detail(&self) -> &AgentDetail {
        &self.detail
    }

    #[must_use]
    pub fn history_selected(&self) -> usize {
        self.history_selected
    }

    #[must_use]
    pub fn window_label(&self) -> String {
        let secs = WINDOWS[self.window_idx.min(WINDOWS.len() - 1)];
        format_window(secs)
    }

    #[must_use]
    pub fn last_err(&self) -> Option<&str> {
        self.last_err.as_deref()
    }

    // -- Data loading entry points --

    /// Load agent records. Call after fetching from provider.
    pub fn load_agents(&mut self, records: Vec<AgentRecord>) {
        self.records = records;
        self.last_err = None;
        self.rebuild_rows();
    }

    /// Load agent records with error.
    pub fn load_agents_err(&mut self, err: String) {
        self.last_err = Some(err);
    }

    /// Load per-agent message counts (24h). Call after counting from provider.
    pub fn load_counts(&mut self, counts: HashMap<String, usize>) {
        self.counts = counts;
        self.rebuild_rows();
    }

    /// Load detail search results for a specific agent.
    pub fn load_detail(&mut self, agent: &str, results: Vec<AgentSearchResult>) {
        self.detail_agent = agent.to_owned();
        self.detail_cached.insert(agent.to_owned(), results.clone());
        self.recompute_detail(agent, &results);
        self.last_err = None;
    }

    /// Invalidate caches for a specific agent (e.g., after new message).
    pub fn invalidate_agent(&mut self, agent: &str) {
        self.detail_cached.remove(agent);
    }

    // -- Selection helpers --

    #[must_use]
    pub fn selected_agent(&self) -> &str {
        if self.rows.is_empty() || self.selected >= self.rows.len() {
            return "";
        }
        self.rows[self.selected].rec.name.trim()
    }

    /// Return the topic of the currently selected history entry, if any.
    #[must_use]
    pub fn history_target(&self) -> &str {
        let sorted = self.sorted_history_results();
        if sorted.is_empty() || self.history_selected >= sorted.len() {
            return "";
        }
        sorted[self.history_selected].topic.trim()
    }

    /// Whether we need detail data for the current selection.
    #[must_use]
    pub fn needs_detail(&self) -> bool {
        let name = self.selected_agent();
        if name.is_empty() {
            return false;
        }
        match self.detail_cached.get(name) {
            Some(results) => results.is_empty(),
            None => true,
        }
    }

    // -- Internal rebuild --

    fn rebuild_rows(&mut self) {
        let filter = self.filter.trim().to_ascii_lowercase();
        let mut rows: Vec<AgentRow> = self
            .records
            .iter()
            .filter_map(|rec| {
                let name = rec.name.trim();
                if name.is_empty() {
                    return None;
                }
                let count = self.counts.get(name).copied().unwrap_or(0);
                let row = AgentRow {
                    rec: rec.clone(),
                    msg_count: count,
                };
                if !filter.is_empty() && !agent_matches_filter(&row, &filter) {
                    return None;
                }
                Some(row)
            })
            .collect();

        let sort_key = self.sort_key;
        rows.sort_by(|a, b| {
            let primary = match sort_key {
                AgentSortKey::Name => {
                    let la = a.rec.name.trim().to_ascii_lowercase();
                    let lb = b.rec.name.trim().to_ascii_lowercase();
                    la.cmp(&lb)
                }
                AgentSortKey::MsgCount => b.msg_count.cmp(&a.msg_count),
                AgentSortKey::Host => {
                    let ha = a.rec.host.trim().to_ascii_lowercase();
                    let hb = b.rec.host.trim().to_ascii_lowercase();
                    ha.cmp(&hb)
                }
                AgentSortKey::LastSeen => b.rec.last_seen.cmp(&a.rec.last_seen),
            };
            if primary != std::cmp::Ordering::Equal {
                return primary;
            }
            // Tiebreaker: name ascending.
            let la = a.rec.name.trim().to_ascii_lowercase();
            let lb = b.rec.name.trim().to_ascii_lowercase();
            la.cmp(&lb)
        });

        self.rows = rows;
        if self.rows.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.rows.len() - 1);
        }
    }

    fn recompute_detail(&mut self, agent: &str, results: &[AgentSearchResult]) {
        let rec = self.find_record(agent);
        let mut detail = AgentDetail {
            name: agent.to_owned(),
            rec,
            uptime: vec![false; 48],
            ..AgentDetail::default()
        };

        let mut target_counts: HashMap<String, usize> = HashMap::new();
        let mut recent: Vec<AgentRecent> = Vec::with_capacity(16);

        for res in results {
            *target_counts.entry(res.topic.clone()).or_insert(0) += 1;
            recent.push(AgentRecent {
                ts: res.time,
                target: res.topic.clone(),
                body: first_line(&res.body),
                id: res.message_id.clone(),
            });

            if res.time > 0 {
                let diff = (self.now - res.time).unsigned_abs();
                if diff <= 86400 {
                    // 24h in seconds
                    let idx = (diff / 1800) as usize; // 30-minute buckets
                    if idx < 48 {
                        detail.uptime[47 - idx] = true;
                    }
                }
            }
        }

        // Sort recent: newest first, tiebreak by ID descending.
        recent.sort_by(|a, b| {
            let ts_cmp = b.ts.cmp(&a.ts);
            if ts_cmp != std::cmp::Ordering::Equal {
                return ts_cmp;
            }
            b.id.cmp(&a.id)
        });
        recent.truncate(10);
        detail.recent = recent;
        detail.msg_count_24h = results.len();

        // Top targets.
        let mut top_targets: Vec<TargetCount> = target_counts
            .into_iter()
            .map(|(target, count)| TargetCount { target, count })
            .collect();
        top_targets.sort_by(|a, b| {
            let c = b.count.cmp(&a.count);
            if c != std::cmp::Ordering::Equal {
                return c;
            }
            a.target.cmp(&b.target)
        });
        top_targets.truncate(8);
        detail.top_targets = top_targets;

        // Sparkline (15-minute buckets over window).
        let window_secs = WINDOWS[self.window_idx.min(WINDOWS.len() - 1)];
        let buckets = (window_secs / 900).max(1) as usize; // 15min = 900s
        let mut spark = vec![0usize; buckets];
        for res in results {
            if res.time <= 0 {
                continue;
            }
            let age = self.now - res.time;
            if age < 0 || age as u64 > window_secs {
                continue;
            }
            let idx = ((window_secs - age as u64) / 900) as usize;
            if idx < spark.len() {
                spark[idx] += 1;
            }
        }
        detail.spark = spark;

        self.detail = detail;
    }

    fn find_record(&self, name: &str) -> AgentRecord {
        let name_lower = name.trim().to_ascii_lowercase();
        for rec in &self.records {
            if rec.name.trim().to_ascii_lowercase() == name_lower {
                return rec.clone();
            }
        }
        AgentRecord::new(name)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        let next = if delta < 0 {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected.saturating_add(delta as usize)
        };
        self.selected = next.min(self.rows.len() - 1);
    }

    fn move_history(&mut self, delta: isize) {
        let len = self.sorted_history_results().len();
        if len == 0 {
            self.history_selected = 0;
            return;
        }
        let next = if delta < 0 {
            self.history_selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.history_selected.saturating_add(delta as usize)
        };
        self.history_selected = next.min(len - 1);
    }

    fn refresh_detail_from_cache(&mut self) {
        let name = self.selected_agent().to_owned();
        if name.is_empty() {
            return;
        }
        let cached = match self.detail_cached.get(&name) {
            Some(c) => c.clone(),
            None => return,
        };
        self.recompute_detail(&name, &cached);
    }

    fn sorted_history_results(&self) -> Vec<&AgentSearchResult> {
        let agent = self.selected_agent();
        let Some(results) = self.detail_cached.get(agent) else {
            return Vec::new();
        };
        let mut sorted: Vec<&AgentSearchResult> = results.iter().collect();
        sorted.sort_by(|a, b| {
            let id_cmp = b.message_id.cmp(&a.message_id);
            if id_cmp != std::cmp::Ordering::Equal {
                return id_cmp;
            }
            b.topic.cmp(&a.topic)
        });
        sorted
    }
}

impl Default for AgentsViewModel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Input handling
// ---------------------------------------------------------------------------

/// Apply keyboard input to the agents view. Returns true if the view should
/// pop (esc in roster mode).
#[must_use]
pub fn apply_agents_input(view: &mut AgentsViewModel, event: InputEvent) -> AgentsAction {
    match event {
        InputEvent::Key(key_event) => handle_key(view, key_event),
        _ => AgentsAction::None,
    }
}

/// Actions the host layer may take after input is processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentsAction {
    None,
    Pop,
    OpenThread { target: String },
    NeedCounts,
    NeedDetail { agent: String },
    Batch(Vec<AgentsAction>),
}

fn handle_key(view: &mut AgentsViewModel, msg: KeyEvent) -> AgentsAction {
    // History mode.
    if view.mode == AgentsMode::History {
        match msg.key {
            Key::Escape | Key::Backspace => {
                view.mode = AgentsMode::Roster;
                view.history_selected = 0;
                return AgentsAction::None;
            }
            Key::Char('j') | Key::Down => {
                view.move_history(1);
                return AgentsAction::None;
            }
            Key::Char('k') | Key::Up => {
                view.move_history(-1);
                return AgentsAction::None;
            }
            Key::Enter => {
                let target = view.history_target().to_owned();
                if target.is_empty() {
                    return AgentsAction::None;
                }
                return AgentsAction::OpenThread { target };
            }
            _ => return AgentsAction::None,
        }
    }

    // Filter editing mode.
    if view.editing {
        match msg.key {
            Key::Escape => {
                view.editing = false;
                return AgentsAction::None;
            }
            Key::Backspace => {
                let mut chars: Vec<char> = view.filter.chars().collect();
                chars.pop();
                view.filter = chars.into_iter().collect();
                view.rebuild_rows();
                return detail_action_if_needed(view);
            }
            Key::Enter => {
                view.editing = false;
                return AgentsAction::None;
            }
            Key::Char(c) => {
                view.filter.push(c);
                view.rebuild_rows();
                return detail_action_if_needed(view);
            }
            _ => return AgentsAction::None,
        }
    }

    // Normal roster mode.
    match msg.key {
        Key::Escape => AgentsAction::Pop,
        Key::Char('/') => {
            view.editing = true;
            AgentsAction::None
        }
        Key::Char('s') => {
            view.sort_key = view.sort_key.next();
            if view.sort_key == AgentSortKey::MsgCount {
                return AgentsAction::NeedCounts;
            }
            view.rebuild_rows();
            detail_action_if_needed(view)
        }
        Key::Char('j') | Key::Down => {
            view.move_selection(1);
            detail_action_if_needed(view)
        }
        Key::Char('k') | Key::Up => {
            view.move_selection(-1);
            detail_action_if_needed(view)
        }
        Key::Char('[') => {
            view.window_idx = view.window_idx.saturating_sub(1);
            view.refresh_detail_from_cache();
            AgentsAction::None
        }
        Key::Char(']') => {
            view.window_idx = (view.window_idx + 1).min(WINDOWS.len() - 1);
            view.refresh_detail_from_cache();
            AgentsAction::None
        }
        Key::Enter => {
            if view.selected_agent().is_empty() {
                return AgentsAction::None;
            }
            view.detail_agent = view.selected_agent().to_owned();
            view.mode = AgentsMode::History;
            view.history_selected = 0;
            AgentsAction::None
        }
        _ => AgentsAction::None,
    }
}

fn detail_action_if_needed(view: &AgentsViewModel) -> AgentsAction {
    if view.needs_detail() {
        AgentsAction::NeedDetail {
            agent: view.selected_agent().to_owned(),
        }
    } else {
        AgentsAction::None
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

#[must_use]
pub fn render_agents_frame(
    view: &AgentsViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    match view.mode {
        AgentsMode::Roster => render_roster(view, &mut frame, width, height),
        AgentsMode::History => render_history(view, &mut frame, width, height),
    }

    if let Some(err) = &view.last_err {
        let err_line = format!("data error: {}", truncate(err, width.saturating_sub(2)));
        if height > 1 {
            frame.draw_text(0, height - 1, &truncate(&err_line, width), TextRole::Danger);
        }
    }

    frame
}

fn render_roster(view: &AgentsViewModel, frame: &mut RenderFrame, width: usize, height: usize) {
    let title = format!(
        "Agents  sort:{}  / filter  s sort  [ ] window  Enter history",
        view.sort_key.label()
    );
    frame.draw_text(0, 0, &truncate(&title, width), TextRole::Accent);
    if height <= 1 {
        return;
    }

    let filter_suffix = if view.editing { "_" } else { "" };
    let filter_line = format!("Filter: {}{}", view.filter, filter_suffix);
    frame.draw_text(0, 1, &truncate(&filter_line, width), TextRole::Muted);
    if height <= 2 {
        return;
    }

    let header = "AGENT           STATUS                 HOST              SEEN     MSGS";
    frame.draw_text(0, 2, &truncate(header, width), TextRole::Muted);
    if height <= 3 {
        return;
    }

    // Split: top half = list, bottom half = detail, separated by divider.
    let used = 4; // title + filter + header + divider
    let remaining = height.saturating_sub(used);
    let list_h = remaining / 2;
    let list_h = list_h.max(3);
    let detail_start = 3 + list_h + 1; // after header rows + list + divider

    // Render agent rows.
    render_agent_rows(view, frame, width, 3, list_h);

    // Divider.
    let div_y = 3 + list_h;
    if div_y < height {
        let divider: String = "─".repeat(width);
        frame.draw_text(0, div_y, &truncate(&divider, width), TextRole::Muted);
    }

    // Render detail.
    if detail_start < height {
        let detail_h = height.saturating_sub(detail_start);
        render_agent_detail(view, frame, width, detail_start, detail_h);
    }
}

fn render_agent_rows(
    view: &AgentsViewModel,
    frame: &mut RenderFrame,
    width: usize,
    start_y: usize,
    max_rows: usize,
) {
    if view.rows.is_empty() {
        frame.draw_text(0, start_y, "No agents", TextRole::Muted);
        return;
    }

    let selected = view.selected.min(view.rows.len() - 1);
    let mut start = selected.saturating_sub(max_rows / 2);
    if start + max_rows > view.rows.len() {
        start = view.rows.len().saturating_sub(max_rows);
    }

    for (row_offset, idx) in (start..view.rows.len()).take(max_rows).enumerate() {
        let row = &view.rows[idx];
        let rec = &row.rec;
        let name = rec.name.trim();
        let host = rec.host.trim();
        let status = if rec.status.trim().is_empty() {
            String::new()
        } else {
            format!("\"{}\"", rec.status.trim())
        };
        let pres = agent_presence_indicator(view.now, rec.last_seen);
        let seen = if rec.last_seen == 0 {
            "-".to_owned()
        } else {
            relative_time(rec.last_seen, view.now)
        };
        let cursor = if idx == selected { "▸" } else { " " };
        let line = format!(
            "{}{} {:<14} {:<22} {:<17} {:<8} {:>4}",
            cursor,
            pres,
            truncate(name, 14),
            truncate(&status, 22),
            truncate(host, 17),
            truncate(&seen, 8),
            row.msg_count,
        );
        let role = if idx == selected {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        frame.draw_text(0, start_y + row_offset, &truncate(&line, width), role);
    }
}

fn render_agent_detail(
    view: &AgentsViewModel,
    frame: &mut RenderFrame,
    width: usize,
    start_y: usize,
    max_h: usize,
) {
    if max_h == 0 {
        return;
    }
    let name = view.selected_agent();
    if name.is_empty() {
        frame.draw_text(0, start_y, "Select an agent", TextRole::Muted);
        return;
    }
    let d = &view.detail;
    if !d.name.eq_ignore_ascii_case(name) {
        frame.draw_text(0, start_y, "Loading...", TextRole::Muted);
        return;
    }

    let mut y = start_y;
    let mut draw = |text: &str, role: TextRole| {
        if y < start_y + max_h {
            frame.draw_text(0, y, &truncate(text, width), role);
            y += 1;
        }
    };

    let seen_str = if d.rec.last_seen == 0 {
        "-".to_owned()
    } else {
        relative_time(d.rec.last_seen, view.now)
    };
    draw(
        &format!("{}  host:{}  last:{}", name, d.rec.host.trim(), seen_str),
        TextRole::Primary,
    );
    if !d.rec.status.trim().is_empty() {
        draw(
            &format!("status: {}", d.rec.status.trim()),
            TextRole::Primary,
        );
    }

    // Sparkline.
    draw("", TextRole::Primary);
    draw(
        &format!(
            "Activity ({}): {}",
            view.window_label(),
            render_spark(&d.spark)
        ),
        TextRole::Primary,
    );

    // Uptime bar.
    draw(
        &format!("Uptime (24h): {}", render_uptime(&d.uptime)),
        TextRole::Primary,
    );

    // Top targets.
    if !d.top_targets.is_empty() {
        let parts: Vec<String> = d
            .top_targets
            .iter()
            .map(|t| format!("{} ({})", t.target, t.count))
            .collect();
        draw(&format!("Active: {}", parts.join(", ")), TextRole::Primary);
    }

    // Recent messages.
    if !d.recent.is_empty() {
        draw("", TextRole::Primary);
        draw("Recent:", TextRole::Primary);
        for msg in &d.recent {
            let ts_str = format_utc_time(msg.ts);
            let line = format!("{} -> {}: {}", ts_str, msg.target, msg.body);
            draw(&line, TextRole::Primary);
        }
    }
}

fn render_history(view: &AgentsViewModel, frame: &mut RenderFrame, width: usize, height: usize) {
    let agent = view.selected_agent();
    let title = format!("History: {}  (Enter open thread, Esc back)", agent);
    frame.draw_text(0, 0, &truncate(&title, width), TextRole::Accent);
    if height <= 1 {
        return;
    }

    let sorted = view.sorted_history_results();
    if sorted.is_empty() {
        frame.draw_text(0, 1, "No messages", TextRole::Muted);
        return;
    }

    let max_rows = height.saturating_sub(2).max(1);
    let selected = view.history_selected.min(sorted.len().saturating_sub(1));
    let mut start = selected.saturating_sub(max_rows / 2);
    if start + max_rows > sorted.len() {
        start = sorted.len().saturating_sub(max_rows);
    }

    for (row_offset, res) in sorted.iter().skip(start).take(max_rows).enumerate() {
        let i = start + row_offset;
        let cursor = if i == selected { "▸" } else { " " };
        let ts_str = format_utc_time(res.time);
        let line = format!(
            "{} {} -> {}: {}",
            cursor,
            ts_str,
            res.topic,
            first_line(&res.body)
        );
        let role = if i == selected {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        frame.draw_text(0, 1 + row_offset, &truncate(&line, width), role);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn agent_matches_filter(row: &AgentRow, filter: &str) -> bool {
    let blob = format!(
        "{} {} {}",
        row.rec.name.trim(),
        row.rec.host.trim(),
        row.rec.status.trim()
    )
    .to_ascii_lowercase();
    blob.contains(filter)
}

/// Presence indicator matching Go `agentPresenceIndicator`.
#[must_use]
pub fn agent_presence_indicator(now: i64, last_seen: i64) -> &'static str {
    if last_seen == 0 {
        return "\u{2715}"; // ✕
    }
    let diff = now - last_seen;
    if diff <= 60 {
        "\u{25CF}" // ●
    } else if diff <= 600 {
        "\u{25CB}" // ○
    } else if diff <= 3600 {
        "\u{25CC}" // ◌
    } else {
        "\u{2715}" // ✕
    }
}

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

fn render_uptime(buckets: &[bool]) -> String {
    if buckets.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(buckets.len());
    for &on in buckets {
        out.push(if on { '█' } else { '░' });
    }
    out
}

fn relative_time(epoch: i64, now: i64) -> String {
    if epoch == 0 {
        return "-".to_owned();
    }
    let diff = (now - epoch).unsigned_abs();
    if diff < 60 {
        format!("{}s", diff)
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86400)
    }
}

fn format_utc_time(epoch: i64) -> String {
    if epoch == 0 {
        return "??:??".to_owned();
    }
    let secs_in_day = epoch % 86400;
    let hours = secs_in_day / 3600;
    let minutes = (secs_in_day % 3600) / 60;
    format!("{:02}:{:02}", hours, minutes)
}

fn format_window(secs: u64) -> String {
    if secs < 3600 {
        format!("{}m0s", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h0m0s", h)
        } else {
            format!("{}h{}m0s", h, m)
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_owned()
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
        return "\u{2026}".to_owned(); // …
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
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::style::ThemeSpec;

    fn frame_text(frame: &RenderFrame, height: usize) -> String {
        (0..height)
            .map(|y| frame.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn make_records(now: i64) -> Vec<AgentRecord> {
        vec![
            AgentRecord {
                name: "coder-1".to_owned(),
                host: "build".to_owned(),
                status: String::new(),
                last_seen: now - 120, // 2 min ago
            },
            AgentRecord {
                name: "architect".to_owned(),
                host: "build".to_owned(),
                status: String::new(),
                last_seen: now - 60, // 1 min ago
            },
            AgentRecord {
                name: "reviewer".to_owned(),
                host: "mac".to_owned(),
                status: String::new(),
                last_seen: now - 1800, // 30 min ago
            },
        ]
    }

    #[test]
    fn presence_indicator_zero() {
        assert_eq!(agent_presence_indicator(1000, 0), "✕");
    }

    #[test]
    fn presence_indicator_online() {
        let now = 100_000;
        assert_eq!(agent_presence_indicator(now, now - 30), "●");
    }

    #[test]
    fn presence_indicator_recent() {
        let now = 100_000;
        assert_eq!(agent_presence_indicator(now, now - 300), "○");
    }

    #[test]
    fn presence_indicator_stale() {
        let now = 100_000;
        assert_eq!(agent_presence_indicator(now, now - 1800), "◌");
    }

    #[test]
    fn presence_indicator_offline() {
        let now = 100_000;
        assert_eq!(agent_presence_indicator(now, now - 7200), "✕");
    }

    #[test]
    fn presence_indicator_future_timestamp_matches_go_behavior() {
        let now = 100_000;
        assert_eq!(agent_presence_indicator(now, now + 7_200), "●");
    }

    #[test]
    fn render_spark_empty() {
        assert_eq!(render_spark(&[]), "");
    }

    #[test]
    fn render_spark_scales() {
        let out = render_spark(&[0, 1, 2, 4, 8]);
        assert_eq!(out.chars().count(), 5);
        let first = out.chars().next();
        let last = out.chars().last();
        assert_ne!(first, last);
    }

    #[test]
    fn render_uptime_display() {
        let mut buckets = vec![false; 48];
        buckets[0] = true;
        buckets[47] = true;
        let out = render_uptime(&buckets);
        assert_eq!(out.chars().count(), 48);
        assert!(out.starts_with('█'));
        assert!(out.ends_with('█'));
    }

    #[test]
    fn rebuild_rows_sort_by_name() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));
        view.sort_key = AgentSortKey::Name;
        view.rebuild_rows();
        assert_eq!(view.rows[0].rec.name, "architect");
    }

    #[test]
    fn rebuild_rows_sort_by_msg_count() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        let counts = [("architect", 2), ("coder-1", 5), ("reviewer", 1)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v as usize))
            .collect();
        view.load_agents(make_records(now));
        view.load_counts(counts);
        view.sort_key = AgentSortKey::MsgCount;
        view.rebuild_rows();
        assert_eq!(view.rows[0].rec.name, "coder-1");
    }

    #[test]
    fn rebuild_rows_filter() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));
        view.filter = "mac".to_owned();
        view.rebuild_rows();
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].rec.name, "reviewer");
    }

    #[test]
    fn move_selection_clamps() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));
        view.move_selection(100);
        assert_eq!(view.selected, 2);
        view.move_selection(-100);
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn selected_agent_empty() {
        let view = AgentsViewModel::new();
        assert_eq!(view.selected_agent(), "");
    }

    #[test]
    fn sort_key_cycle() {
        assert_eq!(AgentSortKey::LastSeen.next(), AgentSortKey::Name);
        assert_eq!(AgentSortKey::Name.next(), AgentSortKey::MsgCount);
        assert_eq!(AgentSortKey::MsgCount.next(), AgentSortKey::Host);
        assert_eq!(AgentSortKey::Host.next(), AgentSortKey::LastSeen);
    }

    #[test]
    fn sort_key_labels() {
        assert_eq!(AgentSortKey::LastSeen.label(), "seen");
        assert_eq!(AgentSortKey::Name.label(), "name");
        assert_eq!(AgentSortKey::MsgCount.label(), "count(24h)");
        assert_eq!(AgentSortKey::Host.label(), "host");
    }

    #[test]
    fn filter_editing_input() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));

        // Enter filter mode.
        let action =
            apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('/'))));
        assert_eq!(action, AgentsAction::None);
        assert!(view.editing());

        // Type 'm'.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('m'))));
        assert_eq!(view.filter(), "m");

        // Backspace.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Backspace)));
        assert_eq!(view.filter(), "");

        // Exit editing.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert!(!view.editing());
    }

    #[test]
    fn enter_history_mode() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));

        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(action, AgentsAction::None);
        assert_eq!(view.mode(), AgentsMode::History);

        // Esc returns to roster.
        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert_eq!(action, AgentsAction::None);
        assert_eq!(view.mode(), AgentsMode::Roster);
    }

    #[test]
    fn esc_pops_in_roster() {
        let mut view = AgentsViewModel::new();
        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert_eq!(action, AgentsAction::Pop);
    }

    #[test]
    fn window_navigation() {
        let mut view = AgentsViewModel::new();
        assert_eq!(view.window_idx, 2);
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('['))));
        assert_eq!(view.window_idx, 1);
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        assert_eq!(view.window_idx, 2);
        // Clamp at max.
        for _ in 0..10 {
            let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        }
        assert_eq!(view.window_idx, WINDOWS.len() - 1);
    }

    #[test]
    fn recompute_detail_basic() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));

        let results = vec![
            AgentSearchResult {
                topic: "task".to_owned(),
                message_id: "001".to_owned(),
                from: "coder-1".to_owned(),
                body: "hello world".to_owned(),
                time: now - 300,
            },
            AgentSearchResult {
                topic: "task".to_owned(),
                message_id: "002".to_owned(),
                from: "coder-1".to_owned(),
                body: "second message\nmore lines".to_owned(),
                time: now - 600,
            },
            AgentSearchResult {
                topic: "@ops".to_owned(),
                message_id: "003".to_owned(),
                from: "coder-1".to_owned(),
                body: "dm message".to_owned(),
                time: now - 1200,
            },
        ];
        view.load_detail("coder-1", results);

        let detail = view.detail();
        assert_eq!(detail.name, "coder-1");
        assert_eq!(detail.msg_count_24h, 3);
        assert_eq!(detail.top_targets.len(), 2);
        assert_eq!(detail.top_targets[0].target, "task");
        assert_eq!(detail.top_targets[0].count, 2);
        assert_eq!(detail.recent.len(), 3);
        assert_eq!(detail.recent[0].body, "hello world");
        assert_eq!(detail.recent[1].body, "second message");
        assert!(detail.uptime.iter().any(|&b| b));
    }

    #[test]
    fn uptime_bar_computation() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord::new("a")]);

        // Message right at now -> last bucket should be active.
        let results = vec![AgentSearchResult {
            topic: "t".to_owned(),
            message_id: "1".to_owned(),
            from: "a".to_owned(),
            body: "msg".to_owned(),
            time: now,
        }];
        view.load_detail("a", results);
        assert!(view.detail().uptime[47]);
    }

    #[test]
    fn history_navigation() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord {
            name: "agent-x".to_owned(),
            host: "h".to_owned(),
            status: String::new(),
            last_seen: now,
        }]);

        let results: Vec<AgentSearchResult> = (0..5)
            .map(|i| AgentSearchResult {
                topic: format!("topic-{}", i),
                message_id: format!("{:03}", i),
                from: "agent-x".to_owned(),
                body: format!("msg {}", i),
                time: now - i * 60,
            })
            .collect();
        view.load_detail("agent-x", results);

        // Enter history mode.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(view.mode(), AgentsMode::History);

        // Navigate down.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('j'))));
        assert_eq!(view.history_selected(), 1);

        // Navigate up.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('k'))));
        assert_eq!(view.history_selected(), 0);
    }

    #[test]
    fn history_enter_opens_thread() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord {
            name: "agent-y".to_owned(),
            host: "h".to_owned(),
            status: String::new(),
            last_seen: now,
        }]);
        let results = vec![AgentSearchResult {
            topic: "build-logs".to_owned(),
            message_id: "001".to_owned(),
            from: "agent-y".to_owned(),
            body: "log data".to_owned(),
            time: now,
        }];
        view.load_detail("agent-y", results);

        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(
            action,
            AgentsAction::OpenThread {
                target: "build-logs".to_owned()
            }
        );
    }

    #[test]
    fn history_enter_uses_sorted_order() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord {
            name: "agent-z".to_owned(),
            host: "h".to_owned(),
            status: String::new(),
            last_seen: now,
        }]);
        // Intentionally unsorted input order.
        view.load_detail(
            "agent-z",
            vec![
                AgentSearchResult {
                    topic: "older-topic".to_owned(),
                    message_id: "001".to_owned(),
                    from: "agent-z".to_owned(),
                    body: "old".to_owned(),
                    time: now - 120,
                },
                AgentSearchResult {
                    topic: "newer-topic".to_owned(),
                    message_id: "002".to_owned(),
                    from: "agent-z".to_owned(),
                    body: "new".to_owned(),
                    time: now - 60,
                },
            ],
        );

        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(
            action,
            AgentsAction::OpenThread {
                target: "newer-topic".to_owned()
            }
        );
    }

    #[test]
    fn history_uses_selected_agent_not_stale_detail_agent() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![
            AgentRecord {
                name: "agent-a".to_owned(),
                host: "h".to_owned(),
                status: String::new(),
                last_seen: now,
            },
            AgentRecord {
                name: "agent-b".to_owned(),
                host: "h".to_owned(),
                status: String::new(),
                last_seen: now - 10,
            },
        ]);
        // Cache both details, leaving detail_agent stale on purpose.
        view.load_detail(
            "agent-a",
            vec![AgentSearchResult {
                topic: "topic-a".to_owned(),
                message_id: "010".to_owned(),
                from: "agent-a".to_owned(),
                body: "a".to_owned(),
                time: now,
            }],
        );
        view.load_detail(
            "agent-b",
            vec![AgentSearchResult {
                topic: "topic-b".to_owned(),
                message_id: "020".to_owned(),
                from: "agent-b".to_owned(),
                body: "b".to_owned(),
                time: now,
            }],
        );
        assert_eq!(view.selected_agent(), "agent-a");

        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(
            action,
            AgentsAction::OpenThread {
                target: "topic-a".to_owned()
            }
        );
    }

    #[test]
    fn empty_cached_detail_still_requires_refresh() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord::new("agent-empty")]);
        view.load_detail("agent-empty", Vec::new());
        assert!(view.needs_detail());
    }

    #[test]
    fn relative_time_cases() {
        assert_eq!(relative_time(0, 1000), "-");
        assert_eq!(relative_time(970, 1000), "30s");
        assert_eq!(relative_time(700, 1000), "5m");
        assert_eq!(relative_time(1000 - 7200, 1000), "2h");
        assert_eq!(relative_time(1000 - 172800, 1000), "2d");
    }

    #[test]
    fn format_utc_time_cases() {
        assert_eq!(format_utc_time(0), "??:??");
        // 3661 seconds = 1h 1m 1s -> 01:01
        assert_eq!(format_utc_time(3661), "01:01");
        // 86399 = 23:59:59 -> 23:59
        assert_eq!(format_utc_time(86399), "23:59");
    }

    #[test]
    fn format_window_labels() {
        assert_eq!(format_window(3600), "1h0m0s");
        assert_eq!(format_window(7200), "2h0m0s");
        assert_eq!(format_window(14400), "4h0m0s");
    }

    #[test]
    fn render_roster_empty() {
        let view = AgentsViewModel::new();
        let frame = render_agents_frame(&view, 60, 10, ThemeSpec::default());
        let text = frame_text(&frame, 10);
        assert!(text.contains("Agents"));
        assert!(text.contains("No agents"));
    }

    #[test]
    fn render_roster_with_agents() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));

        let frame = render_agents_frame(&view, 80, 20, ThemeSpec::default());
        let text = frame_text(&frame, 20);
        assert!(text.contains("Agents"));
        assert!(text.contains("coder-1"));
        assert!(text.contains("architect"));
        assert!(text.contains("reviewer"));
    }

    #[test]
    fn render_detail_selected() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(make_records(now));
        // Default sort = LastSeen, so architect (60s ago) is first.
        let results = vec![AgentSearchResult {
            topic: "task".to_owned(),
            message_id: "001".to_owned(),
            from: "architect".to_owned(),
            body: "design doc ready".to_owned(),
            time: now - 120,
        }];
        view.load_detail("architect", results);

        let frame = render_agents_frame(&view, 80, 20, ThemeSpec::default());
        let text = frame_text(&frame, 20);
        assert!(text.contains("Activity"));
        assert!(text.contains("Uptime"));
        assert!(text.contains("Recent:"));
        assert!(text.contains("design doc ready"));
    }

    #[test]
    fn render_zero_dimensions() {
        let view = AgentsViewModel::new();
        let frame = render_agents_frame(&view, 0, 0, ThemeSpec::default());
        assert_eq!(frame_text(&frame, 0), "");
    }

    #[test]
    fn render_snapshot_roster() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![
            AgentRecord {
                name: "alpha".to_owned(),
                host: "srv1".to_owned(),
                status: "idle".to_owned(),
                last_seen: now - 30,
            },
            AgentRecord {
                name: "beta".to_owned(),
                host: "srv2".to_owned(),
                status: String::new(),
                last_seen: now - 600,
            },
        ]);

        let frame = render_agents_frame(&view, 78, 14, ThemeSpec::default());
        // Verify key elements present.
        let text = frame_text(&frame, 14);
        assert!(text.contains("Agents  sort:seen"));
        assert!(text.contains("Filter:"));
        assert!(text.contains("AGENT"));
        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
    }

    #[test]
    fn render_snapshot_history() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord {
            name: "agent-z".to_owned(),
            host: "h".to_owned(),
            status: String::new(),
            last_seen: now,
        }]);
        view.load_detail(
            "agent-z",
            vec![
                AgentSearchResult {
                    topic: "task".to_owned(),
                    message_id: "002".to_owned(),
                    from: "agent-z".to_owned(),
                    body: "second msg".to_owned(),
                    time: now - 120,
                },
                AgentSearchResult {
                    topic: "ops".to_owned(),
                    message_id: "001".to_owned(),
                    from: "agent-z".to_owned(),
                    body: "first msg".to_owned(),
                    time: now - 300,
                },
            ],
        );

        // Enter history mode.
        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));

        let frame = render_agents_frame(&view, 60, 8, ThemeSpec::default());
        let text = frame_text(&frame, 8);
        assert!(text.contains("History: agent-z"));
        assert!(text.contains("task"));
        assert!(text.contains("ops"));
    }

    #[test]
    fn invalidate_agent_clears_cache() {
        let now: i64 = 1_707_480_000;
        let mut view = AgentsViewModel::new();
        view.set_now(now);
        view.load_agents(vec![AgentRecord::new("test")]);
        view.load_detail(
            "test",
            vec![AgentSearchResult {
                topic: "t".to_owned(),
                message_id: "1".to_owned(),
                from: "test".to_owned(),
                body: "msg".to_owned(),
                time: now,
            }],
        );
        assert!(!view.needs_detail());
        view.invalidate_agent("test");
        // After invalidation, selected agent should need detail.
        // But selected is at index 0 which is "test", and cache is gone.
        assert!(view.needs_detail());
    }

    #[test]
    fn truncate_edge_cases() {
        assert_eq!(truncate("", 5), "");
        assert_eq!(truncate("hi", 5), "hi");
        assert_eq!(truncate("hello world", 5), "hell…");
        assert_eq!(truncate("ab", 1), "…");
        assert_eq!(truncate("x", 0), "");
    }

    #[test]
    fn first_line_multiline() {
        assert_eq!(first_line("a\nb\nc"), "a");
        assert_eq!(first_line("only"), "only");
        assert_eq!(first_line(""), "");
    }
}

use std::collections::{BTreeMap, HashMap};

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

/// Replay playback mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayMode {
    #[default]
    Feed,
    Timeline,
}

/// Replay entry modeled after Go's `fmail.Message` feed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEntry {
    /// Stable ID (often `YYYYMMDD-HHMMSS-NNNN`).
    pub id: String,
    /// Sender agent.
    pub from: String,
    /// Destination (`topic` or `@agent`).
    pub to: String,
    /// Raw body payload.
    pub body: String,
    /// Optional precomputed timestamp in UTC epoch seconds.
    pub epoch_secs: i64,
}

impl ReplayEntry {
    #[must_use]
    pub fn new(id: &str, from: &str, to: &str, body: &str) -> Self {
        Self {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            body: body.to_owned(),
            epoch_secs: 0,
        }
    }

    #[must_use]
    pub fn with_epoch_secs(mut self, epoch_secs: i64) -> Self {
        self.epoch_secs = epoch_secs;
        self
    }
}

/// Actions emitted by replay view input handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayAction {
    None,
    PopView,
    ExportRequested { markdown: String },
}

const REPLAY_SPEED_PRESETS: [f64; 4] = [1.0, 5.0, 10.0, 50.0];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplayViewModel {
    loading: bool,
    last_err: Option<String>,

    entries: Vec<ReplayEntry>,
    times: Vec<i64>,
    start_secs: i64,
    end_secs: i64,

    idx: usize,
    playing: bool,
    speed_idx: usize,
    highlight_ticks: u8,
    mode: ReplayMode,
    status_line: String,

    pending_mark: bool,
    pending_jump: bool,
    marks: BTreeMap<char, usize>,
}

impl ReplayViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn set_error(&mut self, err: Option<String>) {
        self.last_err = err;
        if self.last_err.is_some() {
            self.loading = false;
        }
    }

    pub fn clear_status(&mut self) {
        self.status_line.clear();
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ReplayMode::Feed => ReplayMode::Timeline,
            ReplayMode::Timeline => ReplayMode::Feed,
        };
    }

    pub fn set_entries(&mut self, mut entries: Vec<ReplayEntry>) {
        entries.sort_by(|a, b| {
            let ta = replay_message_time_secs(a);
            let tb = replay_message_time_secs(b);
            ta.cmp(&tb).then_with(|| a.id.cmp(&b.id))
        });
        self.entries = entries;
        self.times = self
            .entries
            .iter()
            .map(replay_message_time_secs)
            .collect::<Vec<_>>();
        self.idx = 0;
        self.playing = false;
        self.pending_mark = false;
        self.pending_jump = false;
        self.highlight_ticks = 0;
        self.status_line.clear();
        self.recompute_range();
    }

    fn recompute_range(&mut self) {
        let mut start = 0i64;
        let mut end = 0i64;
        for &t in &self.times {
            if t == 0 {
                continue;
            }
            if start == 0 || t < start {
                start = t;
            }
            if end == 0 || t > end {
                end = t;
            }
        }
        self.start_secs = start;
        self.end_secs = end;
    }

    pub fn toggle_playing(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.playing = !self.playing;
    }

    pub fn set_speed_idx(&mut self, idx: usize) {
        self.speed_idx = idx.min(REPLAY_SPEED_PRESETS.len().saturating_sub(1));
    }

    pub fn step(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.idx = 0;
            return;
        }
        self.set_index(clamp_isize(
            self.idx as isize + delta,
            0,
            self.entries.len().saturating_sub(1) as isize,
        ) as usize);
    }

    pub fn seek_by_secs(&mut self, delta_secs: i64) {
        if self.entries.is_empty() || self.times.is_empty() {
            return;
        }
        let curr = self.cursor_time_secs();
        if curr == 0 {
            return;
        }
        let target = curr.saturating_add(delta_secs);
        let idx = replay_seek_index_before_or_at(&self.times, target);
        self.set_index(idx);
    }

    pub fn set_index(&mut self, idx: usize) {
        if self.entries.is_empty() {
            self.idx = 0;
            return;
        }
        self.idx = idx.min(self.entries.len().saturating_sub(1));
        self.highlight_ticks = 0;
    }

    pub fn handle_tick(&mut self) {
        if !self.playing || self.entries.is_empty() {
            return;
        }
        if self.idx >= self.entries.len().saturating_sub(1) {
            self.playing = false;
            return;
        }
        self.idx += 1;
        self.highlight_ticks = 1;
    }

    #[must_use]
    pub fn next_tick_delay_ms(&self) -> u64 {
        if !self.playing || self.entries.len() < 2 || self.idx >= self.entries.len() - 1 {
            return 0;
        }
        let curr = self.times.get(self.idx).copied().unwrap_or(0);
        let next = self.times.get(self.idx + 1).copied().unwrap_or(0);
        let speed = REPLAY_SPEED_PRESETS[self.speed_idx.min(REPLAY_SPEED_PRESETS.len() - 1)];
        replay_next_interval_ms(curr, next, speed)
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.idx
    }

    #[must_use]
    pub fn playing(&self) -> bool {
        self.playing
    }

    #[must_use]
    pub fn current(&self) -> Option<&ReplayEntry> {
        self.entries.get(self.idx)
    }

    #[must_use]
    pub fn cursor_time_secs(&self) -> i64 {
        if self.entries.is_empty() || self.idx >= self.entries.len() {
            return self.start_secs;
        }
        let t = self.times.get(self.idx).copied().unwrap_or(0);
        if t != 0 {
            return t;
        }
        let parsed = replay_parse_id_epoch_secs(&self.entries[self.idx].id);
        if parsed != 0 {
            return parsed;
        }
        self.start_secs
    }

    #[must_use]
    pub fn export_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# fmail replay export\n\n");
        out.push_str(&format!(
            "time range: {} .. {}\n\n",
            epoch_secs_to_rfc3339(self.start_secs),
            epoch_secs_to_rfc3339(self.end_secs)
        ));
        for (i, entry) in self.entries.iter().enumerate() {
            let mut t = self.times.get(i).copied().unwrap_or(0);
            if t == 0 {
                t = replay_message_time_secs(entry);
            }
            let mut line = format!(
                "{} {} -> {}",
                format_hhmmss(t),
                entry.from.trim(),
                entry.to.trim()
            );
            let body = first_line(&entry.body);
            if !body.trim().is_empty() {
                line.push_str(": ");
                line.push_str(body.trim());
            }
            out.push_str("- ");
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

#[must_use]
pub fn apply_replay_input(vm: &mut ReplayViewModel, event: InputEvent) -> ReplayAction {
    // Any manual key cancels highlight.
    if matches!(event, InputEvent::Key(_)) {
        vm.highlight_ticks = 0;
    }

    // Pending mark/jump prompts take precedence.
    if vm.pending_mark {
        if let InputEvent::Key(key) = event {
            return handle_mark_key(vm, key);
        }
        return ReplayAction::None;
    }
    if vm.pending_jump {
        if let InputEvent::Key(key) = event {
            return handle_jump_key(vm, key);
        }
        return ReplayAction::None;
    }

    // Direct key handling for replay-specific shortcuts.
    if let InputEvent::Key(key) = event {
        match key.key {
            Key::Escape | Key::Backspace => {
                vm.playing = false;
                return ReplayAction::PopView;
            }
            Key::Char(' ') => {
                vm.toggle_playing();
                return ReplayAction::None;
            }
            Key::Char('t') => {
                vm.toggle_mode();
                return ReplayAction::None;
            }
            Key::Char('1') => {
                vm.set_speed_idx(0);
                return ReplayAction::None;
            }
            Key::Char('2') => {
                vm.set_speed_idx(1);
                return ReplayAction::None;
            }
            Key::Char('3') => {
                vm.set_speed_idx(2);
                return ReplayAction::None;
            }
            Key::Char('4') => {
                vm.set_speed_idx(3);
                return ReplayAction::None;
            }
            Key::Left if key.modifiers.shift => {
                vm.playing = false;
                vm.seek_by_secs(-60);
                return ReplayAction::None;
            }
            Key::Right if key.modifiers.shift => {
                vm.playing = false;
                vm.seek_by_secs(60);
                return ReplayAction::None;
            }
            Key::Char('m') => {
                if vm.entries.is_empty() {
                    return ReplayAction::None;
                }
                vm.pending_mark = true;
                vm.status_line = "mark: press letter".to_owned();
                return ReplayAction::None;
            }
            Key::Char('\'') => {
                if vm.entries.is_empty() {
                    return ReplayAction::None;
                }
                vm.pending_jump = true;
                vm.status_line = "jump: press letter".to_owned();
                return ReplayAction::None;
            }
            Key::Char('e') => {
                return ReplayAction::ExportRequested {
                    markdown: vm.export_markdown(),
                };
            }
            _ => {}
        }
    }

    // Arrow/Vim navigation + tick/refresh.
    match translate_input(&event) {
        UiAction::MoveLeft | UiAction::MoveUp => {
            vm.playing = false;
            vm.step(-1);
        }
        UiAction::MoveRight | UiAction::MoveDown => {
            vm.playing = false;
            vm.step(1);
        }
        UiAction::Refresh => {
            // Replay tick semantics (driven by outer loop) are modeled on Refresh/Tick.
            vm.handle_tick();
        }
        _ => {}
    }
    ReplayAction::None
}

fn handle_mark_key(vm: &mut ReplayViewModel, key: KeyEvent) -> ReplayAction {
    match key.key {
        Key::Escape => {
            vm.pending_mark = false;
            vm.status_line.clear();
            return ReplayAction::None;
        }
        Key::Char(c) => {
            if !c.is_ascii_lowercase() {
                vm.status_line = "mark: use a-z".to_owned();
                return ReplayAction::None;
            }
            vm.marks.insert(c, vm.idx);
            vm.pending_mark = false;
            vm.status_line = format!("marked '{}'", c);
            return ReplayAction::None;
        }
        _ => {}
    }
    ReplayAction::None
}

fn handle_jump_key(vm: &mut ReplayViewModel, key: KeyEvent) -> ReplayAction {
    match key.key {
        Key::Escape => {
            vm.pending_jump = false;
            vm.status_line.clear();
            return ReplayAction::None;
        }
        Key::Char(c) => {
            let idx = match vm.marks.get(&c).copied() {
                Some(v) => v,
                None => {
                    vm.status_line = format!("no mark '{}'", c);
                    vm.pending_jump = false;
                    return ReplayAction::None;
                }
            };
            vm.pending_jump = false;
            vm.playing = false;
            vm.set_index(idx);
            vm.status_line = format!("jumped '{}'", c);
            return ReplayAction::None;
        }
        _ => {}
    }
    ReplayAction::None
}

#[must_use]
pub fn render_replay_frame(
    vm: &ReplayViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    if vm.loading {
        frame.draw_text(
            0,
            0,
            &truncate_vis("REPLAY  loading…", width),
            TextRole::Accent,
        );
        return frame;
    }
    if let Some(err) = &vm.last_err {
        frame.draw_text(
            0,
            0,
            &truncate_vis(&format!("REPLAY  error: {}", err.trim()), width),
            TextRole::Danger,
        );
        return frame;
    }
    if vm.entries.is_empty() {
        frame.draw_text(
            0,
            0,
            &truncate_vis("REPLAY  (no messages)", width),
            TextRole::Muted,
        );
        return frame;
    }

    let cursor_t = vm.cursor_time_secs();
    let speed = REPLAY_SPEED_PRESETS[vm.speed_idx.min(REPLAY_SPEED_PRESETS.len() - 1)];
    let play_glyph = if vm.playing { "▶" } else { "▌▌" };
    let mode = match vm.mode {
        ReplayMode::Feed => "feed",
        ReplayMode::Timeline => "timeline",
    };
    let header = format!(
        "REPLAY  {}  {:.0}x  {} / {}  mode:{}",
        play_glyph,
        speed,
        format_hhmmss(cursor_t),
        format_hhmmss(vm.end_secs),
        mode
    );
    frame.draw_text(0, 0, &truncate_vis(&header, width), TextRole::Accent);
    if height == 1 {
        return frame;
    }

    let (agents_line, mut topics_line) = presence_lines(vm, cursor_t);
    if !vm.status_line.trim().is_empty() {
        topics_line = format!("{}  |  {}", topics_line, vm.status_line.trim());
    }
    frame.draw_text(0, 1, &truncate_vis(&agents_line, width), TextRole::Muted);
    if height == 2 {
        return frame;
    }
    frame.draw_text(0, 2, &truncate_vis(&topics_line, width), TextRole::Muted);
    if height == 3 {
        return frame;
    }

    // Header (3 lines) + footer (3 lines).
    let footer_h = 3usize;
    let content_start = 3usize;
    let feed_h = height.saturating_sub(content_start + footer_h);
    let mut y = content_start;

    if feed_h > 0 {
        match vm.mode {
            ReplayMode::Feed => {
                let lines = render_feed(vm, width, feed_h);
                for line in lines {
                    if y >= height {
                        break;
                    }
                    frame.draw_text(0, y, &truncate_vis(&line.text, width), line.role);
                    y += 1;
                }
            }
            ReplayMode::Timeline => {
                let lines = render_timeline(vm, width, feed_h);
                for line in lines {
                    if y >= height {
                        break;
                    }
                    frame.draw_text(0, y, &truncate_vis(&line.text, width), line.role);
                    y += 1;
                }
            }
        }
    }

    // Footer.
    if height >= 1 {
        let scrub_y = height.saturating_sub(3);
        frame.draw_text(
            0,
            scrub_y,
            &truncate_vis(&render_scrubber(vm, width), width),
            TextRole::Muted,
        );
    }
    if height >= 2 {
        let controls_y = height.saturating_sub(2);
        frame.draw_text(
            0,
            controls_y,
            &truncate_vis(
                "Space:play/pause  \u{2190}/\u{2192}:step  Shift+\u{2190}/\u{2192}:\u{00b1}1m  1-4:speed  t:mode  m/':marks  e:export  Esc:back  (R: replay)",
                width,
            ),
            TextRole::Muted,
        );
    }
    if height >= 3 {
        let prompt_y = height.saturating_sub(1);
        let prompt = if vm.pending_mark {
            "mark: press a-z (Esc cancel)"
        } else if vm.pending_jump {
            "jump: press a-z (Esc cancel)"
        } else {
            ""
        };
        frame.draw_text(0, prompt_y, &truncate_vis(prompt, width), TextRole::Muted);
    }

    frame
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderLine {
    text: String,
    role: TextRole,
}

fn presence_lines(vm: &ReplayViewModel, cursor_secs: i64) -> (String, String) {
    let cursor_secs = if cursor_secs == 0 {
        // Best-effort; parity doesn't require wall-clock correctness.
        vm.end_secs.max(vm.start_secs)
    } else {
        cursor_secs
    };
    let cutoff = cursor_secs.saturating_sub(5 * 60);

    let mut agent_last: HashMap<String, i64> = HashMap::new();
    let mut topic_last: HashMap<String, i64> = HashMap::new();

    // Walk backwards from cursor until cutoff.
    for i in (0..=vm.idx.min(vm.entries.len().saturating_sub(1))).rev() {
        let mut t = vm.times.get(i).copied().unwrap_or(0);
        if t == 0 {
            t = replay_message_time_secs(&vm.entries[i]);
        }
        if t != 0 && t < cutoff {
            break;
        }

        let from = vm.entries[i].from.trim();
        if !from.is_empty() {
            let prev = agent_last.get(from).copied().unwrap_or(0);
            if prev == 0 || t > prev {
                agent_last.insert(from.to_owned(), t);
            }
        }
        let to = vm.entries[i].to.trim();
        if !to.is_empty() && !to.starts_with('@') {
            let prev = topic_last.get(to).copied().unwrap_or(0);
            if prev == 0 || t > prev {
                topic_last.insert(to.to_owned(), t);
            }
        }
    }

    #[derive(Debug, Clone)]
    struct Kv {
        k: String,
        t: i64,
    }

    let mut agents = agent_last
        .into_iter()
        .map(|(k, t)| Kv { k, t })
        .collect::<Vec<_>>();
    agents.sort_by(|a, b| b.t.cmp(&a.t).then_with(|| a.k.cmp(&b.k)));
    if agents.len() > 6 {
        agents.truncate(6);
    }
    let mut agent_parts = Vec::new();
    for a in agents {
        let active = if a.t != 0 && a.t < cutoff {
            "◌"
        } else {
            "●"
        };
        agent_parts.push(format!("{} {}", active, a.k));
    }
    if agent_parts.is_empty() {
        agent_parts.push("no recent agents".to_owned());
    }

    let mut topics = topic_last
        .into_iter()
        .map(|(k, t)| Kv { k, t })
        .collect::<Vec<_>>();
    topics.sort_by(|a, b| b.t.cmp(&a.t).then_with(|| a.k.cmp(&b.k)));
    if topics.len() > 6 {
        topics.truncate(6);
    }
    let mut topic_parts = topics.into_iter().map(|kv| kv.k).collect::<Vec<_>>();
    if topic_parts.is_empty() {
        topic_parts.push("no recent topics".to_owned());
    }

    (
        format!("Agents: {}", agent_parts.join("  ")),
        format!("Topics: {}", topic_parts.join("  ")),
    )
}

fn render_feed(vm: &ReplayViewModel, width: usize, height: usize) -> Vec<RenderLine> {
    if height == 0 {
        return Vec::new();
    }
    let start = vm.idx.saturating_sub(height.saturating_sub(1));
    let mut lines = Vec::with_capacity(height);
    for i in start..=vm.idx.min(vm.entries.len().saturating_sub(1)) {
        let mut t = vm.times.get(i).copied().unwrap_or(0);
        if t == 0 {
            t = replay_message_time_secs(&vm.entries[i]);
        }
        let mut head = format!(
            "{} {} -> {}",
            format_hhmmss(t),
            vm.entries[i].from.trim(),
            vm.entries[i].to.trim()
        );
        let body = first_line(&vm.entries[i].body);
        if !body.trim().is_empty() {
            head.push_str(": ");
            head.push_str(body.trim());
        }
        let role = if i == vm.idx && vm.highlight_ticks > 0 {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        lines.push(RenderLine {
            text: truncate_vis(&head, width),
            role,
        });
    }
    while lines.len() < height {
        lines.push(RenderLine {
            text: String::new(),
            role: TextRole::Primary,
        });
    }
    lines
}

fn render_timeline(vm: &ReplayViewModel, width: usize, height: usize) -> Vec<RenderLine> {
    if height == 0 {
        return Vec::new();
    }
    let mut lines = Vec::with_capacity(height);
    let mut prev_bucket: i64 = 0;

    for i in (0..=vm.idx.min(vm.entries.len().saturating_sub(1))).rev() {
        if lines.len() >= height {
            break;
        }
        let mut t = vm.times.get(i).copied().unwrap_or(0);
        if t == 0 {
            t = replay_message_time_secs(&vm.entries[i]);
        }
        let bucket = if t == 0 { 0 } else { t - (t % 60) };
        if prev_bucket == 0 {
            prev_bucket = bucket;
        }
        if bucket != 0 && prev_bucket != 0 && bucket != prev_bucket && lines.len() < height {
            lines.push(RenderLine {
                text: truncate_vis(&format!("-- {} --", format_hhmm(prev_bucket)), width),
                role: TextRole::Muted,
            });
            prev_bucket = bucket;
        }

        let mut head = format!(
            "{} {} -> {}",
            format_hhmmss(t),
            vm.entries[i].from.trim(),
            vm.entries[i].to.trim()
        );
        let body = first_line(&vm.entries[i].body);
        if !body.trim().is_empty() {
            head.push_str(": ");
            head.push_str(body.trim());
        }
        let role = if i == vm.idx && vm.highlight_ticks > 0 {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        lines.push(RenderLine {
            text: truncate_vis(&head, width),
            role,
        });
    }

    if lines.len() < height && prev_bucket != 0 {
        lines.push(RenderLine {
            text: truncate_vis(&format!("-- {} --", format_hhmm(prev_bucket)), width),
            role: TextRole::Muted,
        });
    }

    lines.reverse();
    while lines.len() < height {
        lines.push(RenderLine {
            text: String::new(),
            role: TextRole::Primary,
        });
    }
    lines
}

fn render_scrubber(vm: &ReplayViewModel, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut bar_w = 10usize.max(width.saturating_sub(22));
    if bar_w > 80 {
        bar_w = 80;
    }

    let total = vm.end_secs.saturating_sub(vm.start_secs);
    let pos_t = vm.cursor_time_secs();
    let mut ratio = 0.0f64;
    if total > 0 && pos_t != 0 {
        ratio = (pos_t.saturating_sub(vm.start_secs)) as f64 / (total as f64);
    }
    ratio = ratio.clamp(0.0, 1.0);
    // Go uses truncation (`int(...)`), not rounding.
    let mut pos = (ratio * (bar_w.saturating_sub(1) as f64)) as isize;
    pos = clamp_isize(pos, 0, bar_w.saturating_sub(1) as isize);

    let mut bar = vec!['-'; bar_w];
    for ch in bar.iter_mut().take(pos as usize) {
        *ch = '=';
    }
    bar[pos as usize] = '>';

    // Mark positions with `|`.
    for &idx in vm.marks.values() {
        if idx >= vm.times.len() {
            continue;
        }
        let mt = vm.times[idx];
        if mt == 0 || total <= 0 {
            continue;
        }
        let mr = (mt.saturating_sub(vm.start_secs)) as f64 / (total as f64);
        // Go uses truncation (`int(...)`), not rounding.
        let mp = (mr * (bar_w.saturating_sub(1) as f64)) as isize;
        let mp = clamp_isize(mp, 0, bar_w.saturating_sub(1) as isize) as usize;
        if mp != pos as usize {
            bar[mp] = '|';
        }
    }

    format!(
        "[{}] {} - {}",
        bar.into_iter().collect::<String>(),
        format_hhmm(vm.start_secs),
        format_hhmm(vm.end_secs)
    )
}

fn replay_message_time_secs(entry: &ReplayEntry) -> i64 {
    if entry.epoch_secs != 0 {
        return entry.epoch_secs;
    }
    replay_parse_id_epoch_secs(&entry.id)
}

fn replay_parse_id_epoch_secs(id: &str) -> i64 {
    if id.len() < 15 {
        return 0;
    }
    // Message IDs are sortable: YYYYMMDD-HHMMSS-....
    let prefix = &id[..15];
    let bytes = prefix.as_bytes();
    if bytes.len() != 15 || bytes[8] != b'-' {
        return 0;
    }
    let Some(year) = parse_4(&prefix[0..4]) else {
        return 0;
    };
    let Some(month) = parse_2(&prefix[4..6]) else {
        return 0;
    };
    let Some(day) = parse_2(&prefix[6..8]) else {
        return 0;
    };
    let Some(hour) = parse_2(&prefix[9..11]) else {
        return 0;
    };
    let Some(min) = parse_2(&prefix[11..13]) else {
        return 0;
    };
    let Some(sec) = parse_2(&prefix[13..15]) else {
        return 0;
    };

    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || min > 59 || sec > 59 {
        return 0;
    }
    let days = days_from_civil(year as i64, month as i64, day as i64);
    days.saturating_mul(86_400)
        .saturating_add((hour as i64) * 3600)
        .saturating_add((min as i64) * 60)
        .saturating_add(sec as i64)
}

fn parse_2(s: &str) -> Option<u32> {
    if s.len() != 2 {
        return None;
    }
    let b = s.as_bytes();
    if !b[0].is_ascii_digit() || !b[1].is_ascii_digit() {
        return None;
    }
    Some(((b[0] - b'0') as u32) * 10 + ((b[1] - b'0') as u32))
}

fn parse_4(s: &str) -> Option<u32> {
    if s.len() != 4 {
        return None;
    }
    let b = s.as_bytes();
    if b.iter().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    Some(
        ((b[0] - b'0') as u32) * 1000
            + ((b[1] - b'0') as u32) * 100
            + ((b[2] - b'0') as u32) * 10
            + ((b[3] - b'0') as u32),
    )
}

fn replay_seek_index_before_or_at(times: &[i64], target: i64) -> usize {
    if times.is_empty() {
        return 0;
    }
    let mut lo = 0usize;
    let mut hi = times.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if times[mid] >= target {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    if lo == 0 {
        return 0;
    }
    if lo >= times.len() {
        return times.len() - 1;
    }
    if times[lo] > target {
        return lo - 1;
    }
    lo
}

fn replay_next_interval_ms(curr: i64, next: i64, speed: f64) -> u64 {
    let speed = if speed <= 0.0 { 1.0 } else { speed };
    let delta = next.saturating_sub(curr);
    if delta <= 0 {
        return 50;
    }
    let mut scaled = ((delta as f64) / speed * 1000.0).round() as i64;
    if scaled < 10 {
        scaled = 10;
    }
    // Fast-forward large gaps.
    if scaled > 200 || delta > 30 {
        scaled = 200;
    }
    scaled as u64
}

fn first_line(body: &str) -> &str {
    let s = body.trim();
    match s.find('\n') {
        Some(idx) => s[..idx].trim(),
        None => s,
    }
}

fn format_hhmmss(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "--:--:--".to_owned();
    }
    let secs = epoch_secs.rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn format_hhmm(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "--:--".to_owned();
    }
    let secs = epoch_secs.rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{:02}:{:02}", h, m)
}

fn epoch_secs_to_rfc3339(epoch_secs: i64) -> String {
    if epoch_secs == 0 {
        return "-".to_owned();
    }
    let days = epoch_secs.div_euclid(86_400);
    let secs = epoch_secs.rem_euclid(86_400);
    let (y, m, d) = epoch_to_ymd(days);
    let h = secs / 3600;
    let mm = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, h, mm, s)
}

/// Date -> epoch days (1970-01-01 = 0). Howard Hinnant algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    doe + era * 146_097 - 719_468
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

fn clamp_isize(value: isize, lo: isize, hi: isize) -> isize {
    value.max(lo).min(hi)
}

fn truncate_vis(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars = input.chars().collect::<Vec<_>>();
    chars.into_iter().take(max_chars).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;

    #[test]
    fn parse_id_epoch_seconds() {
        // 1970-01-01T00:00:00Z -> 0
        assert_eq!(replay_parse_id_epoch_secs("19700101-000000-0000"), 0);

        // 2026-02-10T05:58:56Z
        let t = replay_parse_id_epoch_secs("20260210-055856-3803");
        assert_eq!(format_hhmmss(t), "05:58:56");
        assert_eq!(epoch_secs_to_rfc3339(t), "2026-02-10T05:58:56Z");
    }

    #[test]
    fn seek_index_before_or_at_matches_go_semantics() {
        let base = 1_000_000i64;
        let times = vec![base, base + 60, base + 120];
        assert_eq!(replay_seek_index_before_or_at(&times, base + 90), 1);
        assert_eq!(replay_seek_index_before_or_at(&times, base - 60), 0);
        assert_eq!(replay_seek_index_before_or_at(&times, base + 10_000), 2);
    }

    #[test]
    fn next_interval_clamps_and_fast_forwards() {
        assert_eq!(replay_next_interval_ms(10, 10, 1.0), 50);
        assert_eq!(replay_next_interval_ms(10, 11, 1.0), 200); // 1s gap -> cap
        assert_eq!(replay_next_interval_ms(10, 40, 10.0), 200); // big gap -> cap
        assert_eq!(replay_next_interval_ms(10, 11, 50.0), 20); // 1s / 50x -> 20ms
    }

    #[test]
    fn marks_and_jumps() {
        let mut vm = ReplayViewModel::new();
        vm.set_entries(vec![
            ReplayEntry::new("20260210-055800-0000", "a", "topic", "one"),
            ReplayEntry::new("20260210-055801-0000", "b", "topic", "two"),
        ]);
        vm.set_index(1);

        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('m'))));
        assert!(vm.pending_mark);

        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));
        assert!(!vm.pending_mark);
        assert_eq!(vm.marks.get(&'a').copied(), Some(1));

        vm.set_index(0);
        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('\''))));
        assert!(vm.pending_jump);

        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));
        assert!(!vm.pending_jump);
        assert_eq!(vm.cursor(), 1);
        assert!(vm.status_line.contains("jumped"));
    }

    #[test]
    fn replay_render_feed_snapshot() {
        let mut vm = ReplayViewModel::new();
        vm.set_entries(vec![
            ReplayEntry::new(
                "20260210-055800-0000",
                "architect",
                "task",
                "bootstrap\nmore",
            ),
            ReplayEntry::new(
                "20260210-055803-0000",
                "coder-1",
                "@architect",
                "need context",
            ),
            ReplayEntry::new(
                "20260210-055809-0000",
                "architect",
                "@coder-1",
                "use sliding window",
            ),
        ]);
        vm.set_index(1);

        // Place a mark to show scrubber `|`.
        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('m'))));
        let _ = apply_replay_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));

        let frame = render_replay_frame(&vm, 80, 10, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_replay_feed_frame",
            &frame,
            "REPLAY  ▌▌  1x  05:58:03 / 05:58:09  mode:feed                                  \nAgents: ● coder-1  ● architect                                                  \nTopics: task  |  marked 'a'                                                     \n05:58:00 architect -> task: bootstrap                                           \n05:58:03 coder-1 -> @architect: need context                                    \n                                                                                \n                                                                                \n[===================>--------------------------------------] 05:58 - 05:58      \nSpace:play/pause  ←/→:step  Shift+←/→:±1m  1-4:speed  t:mode  m/':marks  e:expor\n                                                                                ",
        );
    }

    #[test]
    fn replay_render_timeline_snapshot() {
        let mut vm = ReplayViewModel::new();
        vm.set_entries(vec![
            ReplayEntry::new("20260210-055800-0000", "a", "topic", "one"),
            ReplayEntry::new("20260210-055801-0000", "b", "topic", "two"),
            ReplayEntry::new("20260210-055900-0000", "c", "topic", "three"),
        ]);
        vm.mode = ReplayMode::Timeline;
        vm.set_index(2);

        let frame = render_replay_frame(&vm, 72, 9, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_replay_timeline_frame",
            &frame,
            "REPLAY  ▌▌  1x  05:59:00 / 05:59:00  mode:timeline                      \nAgents: ● c  ● b  ● a                                                   \nTopics: topic                                                           \n05:58:01 b -> topic: two                                                \n-- 05:59 --                                                             \n05:59:00 c -> topic: three                                              \n[=================================================>] 05:58 - 05:59      \nSpace:play/pause  ←/→:step  Shift+←/→:±1m  1-4:speed  t:mode  m/':marks \n                                                                        ",
        );
    }

    #[test]
    fn shift_seek_moves_by_minute() {
        let mut vm = ReplayViewModel::new();
        vm.set_entries(vec![
            ReplayEntry::new("20260210-055800-0000", "a", "topic", "one"),
            ReplayEntry::new("20260210-055900-0000", "b", "topic", "two"),
            ReplayEntry::new("20260210-060000-0000", "c", "topic", "three"),
        ]);
        vm.set_index(1);

        let _ = apply_replay_input(
            &mut vm,
            InputEvent::Key(KeyEvent {
                key: Key::Right,
                modifiers: Modifiers {
                    shift: true,
                    ctrl: false,
                    alt: false,
                },
            }),
        );
        assert_eq!(vm.cursor(), 2);
    }
}

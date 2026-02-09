//! Dashboard view for the fmail TUI, ported from Go `dashboardView`.
//!
//! Shows a three-panel layout: agents (with presence), hot topics (with heat
//! bars), and a live message feed with scroll/follow behaviour.

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Feed capacity before oldest messages are pruned.
pub const DASHBOARD_FEED_LIMIT: usize = 500;

// ---------------------------------------------------------------------------
// DashboardFocus
// ---------------------------------------------------------------------------

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DashboardFocus {
    #[default]
    Agents,
    Topics,
    Feed,
}

impl DashboardFocus {
    fn next(self) -> Self {
        match self {
            Self::Agents => Self::Topics,
            Self::Topics => Self::Feed,
            Self::Feed => Self::Agents,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Topics => "topics",
            Self::Feed => "feed",
        }
    }
}

// ---------------------------------------------------------------------------
// AgentEntry
// ---------------------------------------------------------------------------

/// An agent record displayed in the agents panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub name: String,
    pub status: String,
    /// Seconds since epoch of last seen timestamp.
    pub last_seen_secs: i64,
}

impl AgentEntry {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            status: String::new(),
            last_seen_secs: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// TopicEntry
// ---------------------------------------------------------------------------

/// A topic displayed in the topics panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicEntry {
    pub name: String,
    pub message_count: usize,
    /// Seconds since epoch of last activity.
    pub last_activity_secs: i64,
    /// Hot message count (messages in last 5 minutes).
    pub hot_count: usize,
}

impl TopicEntry {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            message_count: 0,
            last_activity_secs: 0,
            hot_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// FeedMessage
// ---------------------------------------------------------------------------

/// A message in the live feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedMessage {
    pub time_label: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub priority: String,
}

// ---------------------------------------------------------------------------
// DashboardViewModel
// ---------------------------------------------------------------------------

/// View-model for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardViewModel {
    pub focus: DashboardFocus,

    agents: Vec<AgentEntry>,
    agent_idx: usize,

    topics: Vec<TopicEntry>,
    topic_idx: usize,

    feed: Vec<FeedMessage>,
    feed_offset: usize, // 0 = follow tail; >0 = paused

    /// Current time as seconds since epoch.
    pub now_secs: i64,

    error: Option<String>,
    status_line: String,
}

impl Default for DashboardViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            focus: DashboardFocus::default(),
            agents: Vec::new(),
            agent_idx: 0,
            topics: Vec::new(),
            topic_idx: 0,
            feed: Vec::new(),
            feed_offset: 0,
            now_secs: 0,
            error: None,
            status_line: String::new(),
        }
    }

    // -- data population -----------------------------------------------------

    /// Replace agent list. Sorted by last_seen descending, then name.
    pub fn set_agents(&mut self, mut agents: Vec<AgentEntry>) {
        agents.sort_by(|a, b| {
            let ord = b.last_seen_secs.cmp(&a.last_seen_secs);
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
            a.name.cmp(&b.name)
        });
        self.agents = agents;
        self.clamp_agent();
    }

    /// Replace topic list. Sorted by last_activity descending, then name.
    pub fn set_topics(&mut self, mut topics: Vec<TopicEntry>) {
        topics.sort_by(|a, b| {
            let ord = b.last_activity_secs.cmp(&a.last_activity_secs);
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
            a.name.cmp(&b.name)
        });
        self.topics = topics;
        self.clamp_topic();
    }

    /// Append a message to the feed. Prunes if over limit.
    pub fn append_feed(&mut self, msg: FeedMessage) {
        self.feed.push(msg);
        if self.feed.len() > DASHBOARD_FEED_LIMIT {
            let excess = self.feed.len() - DASHBOARD_FEED_LIMIT;
            self.feed.drain(..excess);
        }
    }

    /// Set error message.
    pub fn set_error(&mut self, err: Option<String>) {
        self.error = err;
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn agents(&self) -> &[AgentEntry] {
        &self.agents
    }

    #[must_use]
    pub fn topics(&self) -> &[TopicEntry] {
        &self.topics
    }

    #[must_use]
    pub fn feed(&self) -> &[FeedMessage] {
        &self.feed
    }

    #[must_use]
    pub fn agent_idx(&self) -> usize {
        self.agent_idx
    }

    #[must_use]
    pub fn topic_idx(&self) -> usize {
        self.topic_idx
    }

    #[must_use]
    pub fn feed_offset(&self) -> usize {
        self.feed_offset
    }

    #[must_use]
    pub fn is_following(&self) -> bool {
        self.feed_offset == 0
    }

    // -- focus / navigation --------------------------------------------------

    pub fn cycle_focus(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn move_up(&mut self) {
        match self.focus {
            DashboardFocus::Agents => {
                self.agent_idx = self.agent_idx.saturating_sub(1);
            }
            DashboardFocus::Topics => {
                self.topic_idx = self.topic_idx.saturating_sub(1);
            }
            DashboardFocus::Feed => {
                self.feed_offset = self.feed_offset.saturating_add(1);
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.focus {
            DashboardFocus::Agents => {
                let max = self.agents.len().saturating_sub(1);
                self.agent_idx = (self.agent_idx + 1).min(max);
            }
            DashboardFocus::Topics => {
                let max = self.topics.len().saturating_sub(1);
                self.topic_idx = (self.topic_idx + 1).min(max);
            }
            DashboardFocus::Feed => {
                self.feed_offset = self.feed_offset.saturating_sub(1);
            }
        }
    }

    /// Resume following the feed tail.
    pub fn resume_follow(&mut self) {
        self.feed_offset = 0;
    }

    // -- internal ------------------------------------------------------------

    fn clamp_agent(&mut self) {
        let max = self.agents.len().saturating_sub(1);
        self.agent_idx = self.agent_idx.min(max);
    }

    fn clamp_topic(&mut self) {
        let max = self.topics.len().saturating_sub(1);
        self.topic_idx = self.topic_idx.min(max);
    }
}

// ---------------------------------------------------------------------------
// Presence indicator
// ---------------------------------------------------------------------------

fn presence_indicator(now_secs: i64, last_seen_secs: i64) -> &'static str {
    if last_seen_secs == 0 {
        return "\u{25cc}"; // ◌ offline
    }
    let diff = now_secs.saturating_sub(last_seen_secs);
    if diff <= 60 {
        "\u{25cf}" // ● online
    } else if diff <= 600 {
        "\u{25cb}" // ○ recently seen
    } else {
        "\u{25cc}" // ◌ offline
    }
}

// ---------------------------------------------------------------------------
// Heat bar
// ---------------------------------------------------------------------------

fn topic_heat_bar(count: usize, max: usize) -> &'static str {
    if max == 0 || count == 0 {
        return "\u{2591}\u{2591}"; // ░░
    }
    let ratio = count as f64 / max as f64;
    if ratio >= 0.75 {
        "\u{2588}\u{2588}" // ██
    } else if ratio >= 0.5 {
        "\u{2593}\u{2593}" // ▓▓
    } else if ratio >= 0.25 {
        "\u{2592}\u{2592}" // ▒▒
    } else {
        "\u{2591}\u{2591}" // ░░
    }
}

// ---------------------------------------------------------------------------
// Truncation helper
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

fn first_line(s: &str) -> &str {
    let s = s.trim();
    s.lines().next().unwrap_or(s)
}

// ---------------------------------------------------------------------------
// Input handler
// ---------------------------------------------------------------------------

/// Process an input event on the dashboard view model.
/// Returns `true` if the event was consumed.
pub fn apply_dashboard_input(view: &mut DashboardViewModel, event: InputEvent) -> bool {
    if let InputEvent::Key(KeyEvent { key, modifiers }) = event {
        // Tab cycles focus.
        if key == Key::Tab && !modifiers.ctrl && !modifiers.alt {
            view.cycle_focus();
            return true;
        }
        // G resumes feed follow.
        if view.focus == DashboardFocus::Feed
            && !modifiers.ctrl
            && !modifiers.alt
            && matches!(key, Key::Char('G'))
        {
            view.resume_follow();
            return true;
        }
    }

    match translate_input(&event) {
        UiAction::MoveUp => {
            view.move_up();
            true
        }
        UiAction::MoveDown => {
            view.move_down();
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the dashboard into a frame.
#[must_use]
pub fn render_dashboard_frame(
    view: &DashboardViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    if width < 80 {
        // Narrow: feed only.
        render_feed_panel(view, &mut frame, 0, 0, width, height);
    } else {
        // Wide: left panel + feed.
        let left_w = (width / 3).clamp(30, 50).min(width);
        let gap = 1;
        let right_w = width.saturating_sub(left_w + gap);
        if right_w < 20 {
            render_feed_panel(view, &mut frame, 0, 0, width, height);
        } else {
            render_left_panel(view, &mut frame, 0, 0, left_w, height);
            render_feed_panel(view, &mut frame, left_w + gap, 0, right_w, height);
        }
    }

    // Error line.
    if let Some(ref err) = view.error {
        let y = height.saturating_sub(1);
        frame.draw_text(0, y, &truncate(err, width), TextRole::Danger);
    }

    frame
}

fn render_left_panel(
    view: &DashboardViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y_off: usize,
    width: usize,
    height: usize,
) {
    if width == 0 || height == 0 {
        return;
    }
    let mut y = y_off;

    // --- Agents section ---
    let online_count = view
        .agents
        .iter()
        .filter(|a| {
            let diff = view.now_secs.saturating_sub(a.last_seen_secs);
            a.last_seen_secs > 0 && diff <= 60
        })
        .count();

    let agents_title = format!("AGENTS ({online_count} online)  Enter:open");
    frame.draw_text(x_off, y, &truncate(&agents_title, width), TextRole::Accent);
    y += 1;

    // Agents get ~40% of height.
    let agents_max = (height / 2).max(4).min(height.saturating_sub(4));
    for (idx, agent) in view.agents.iter().enumerate().take(agents_max) {
        if y >= y_off + height {
            break;
        }
        let presence = presence_indicator(view.now_secs, agent.last_seen_secs);
        let prefix = if view.focus == DashboardFocus::Agents && idx == view.agent_idx {
            "\u{25b8} "
        } else {
            "  "
        };
        let mut line = format!("{prefix}{presence} {}", agent.name);
        if !agent.status.trim().is_empty() {
            line = format!("{line} {:?}", agent.status.trim());
        }
        frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Primary);
        y += 1;
    }

    if y >= y_off + height {
        return;
    }

    // Divider.
    let divider: String = "\u{2500}".repeat(width);
    frame.draw_text(x_off, y, &truncate(&divider, width), TextRole::Muted);
    y += 1;

    if y >= y_off + height {
        return;
    }

    // --- Topics section ---
    let topics_title = "TOPICS (hot)  Enter:thread";
    frame.draw_text(x_off, y, &truncate(topics_title, width), TextRole::Accent);
    y += 1;

    let max_hot = view.topics.iter().map(|t| t.hot_count).max().unwrap_or(0);
    for (idx, topic) in view.topics.iter().enumerate().take(6) {
        if y >= y_off + height {
            break;
        }
        let bar = topic_heat_bar(topic.hot_count, max_hot);
        let prefix = if view.focus == DashboardFocus::Topics && idx == view.topic_idx {
            "\u{25b8} "
        } else {
            "  "
        };
        let label = format!("{prefix}{bar} {}", topic.name);
        let meta = format!("{} msgs/5m", topic.hot_count);
        let remaining = width.saturating_sub(label.chars().count() + 1);
        let line = if remaining > 0 {
            format!("{label} {}", truncate(&meta, remaining))
        } else {
            label
        };
        frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Primary);
        y += 1;
    }
}

fn render_feed_panel(
    view: &DashboardViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y_off: usize,
    width: usize,
    height: usize,
) {
    if width == 0 || height == 0 {
        return;
    }
    let mut y = y_off;

    // Title.
    let feed_state = if view.feed_offset > 0 {
        format!("paused:{}", view.feed_offset)
    } else {
        "follow".to_owned()
    };
    let title = format!("LIVE FEED [{feed_state}]  Enter:thread");
    frame.draw_text(x_off, y, &truncate(&title, width), TextRole::Accent);
    y += 1;

    if y >= y_off + height {
        return;
    }

    // Feed lines.
    let body_height = (y_off + height).saturating_sub(y);
    // Reserve 1 line for paused indicator if paused.
    let lines_available = if view.feed_offset > 0 {
        body_height.saturating_sub(1)
    } else {
        body_height
    };

    if view.feed.is_empty() {
        frame.draw_text(x_off, y, "(no messages)", TextRole::Muted);
    } else {
        let n = view.feed.len();
        let start = if view.feed_offset > 0 {
            n.saturating_sub(lines_available + view.feed_offset)
        } else {
            n.saturating_sub(lines_available)
        };
        let end = n.min(start + lines_available);

        for msg in view.feed.iter().skip(start).take(end - start) {
            if y >= y_off + height {
                break;
            }
            let target = if msg.to.trim().is_empty() {
                "(unknown)"
            } else {
                msg.to.trim()
            };
            let body = first_line(&msg.body);
            let body = if body.is_empty() { "(empty)" } else { body };

            let priority_tag = match msg.priority.trim().to_ascii_lowercase().as_str() {
                "high" => "[HIGH] ",
                "low" => "[low] ",
                _ => "",
            };

            let line = format!(
                "{} {} \u{2192} {}  {}{body}",
                msg.time_label, msg.from, target, priority_tag,
            );
            frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Primary);
            y += 1;
        }
    }

    // Paused indicator.
    if view.feed_offset > 0 {
        let paused_y = (y_off + height).saturating_sub(1);
        let paused = format!("PAUSED ({})  j/k scroll  G resume", view.feed_offset);
        frame.draw_text(x_off, paused_y, &truncate(&paused, width), TextRole::Muted);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    fn key(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent::plain(k))
    }

    fn sample_agents() -> Vec<AgentEntry> {
        vec![
            {
                let mut a = AgentEntry::new("alice");
                a.last_seen_secs = 1000; // online (now_secs will be ~1050)
                a.status = "building".to_owned();
                a
            },
            {
                let mut a = AgentEntry::new("bob");
                a.last_seen_secs = 500; // recently seen
                a
            },
            {
                let mut a = AgentEntry::new("charlie");
                a.last_seen_secs = 0; // offline
                a
            },
        ]
    }

    fn sample_topics() -> Vec<TopicEntry> {
        vec![
            {
                let mut t = TopicEntry::new("build");
                t.message_count = 12;
                t.last_activity_secs = 1000;
                t.hot_count = 5;
                t
            },
            {
                let mut t = TopicEntry::new("task");
                t.message_count = 8;
                t.last_activity_secs = 900;
                t.hot_count = 2;
                t
            },
        ]
    }

    fn sample_feed() -> Vec<FeedMessage> {
        vec![
            FeedMessage {
                time_label: "12:00:01".into(),
                from: "alice".into(),
                to: "build".into(),
                body: "starting build".into(),
                priority: String::new(),
            },
            FeedMessage {
                time_label: "12:00:05".into(),
                from: "bob".into(),
                to: "task".into(),
                body: "urgent fix needed".into(),
                priority: "high".into(),
            },
            FeedMessage {
                time_label: "12:00:10".into(),
                from: "charlie".into(),
                to: "build".into(),
                body: "build passed".into(),
                priority: "low".into(),
            },
        ]
    }

    // -- ViewModel basic operations ------------------------------------------

    #[test]
    fn new_viewmodel_defaults() {
        let vm = DashboardViewModel::new();
        assert_eq!(vm.focus, DashboardFocus::Agents);
        assert!(vm.agents().is_empty());
        assert!(vm.topics().is_empty());
        assert!(vm.feed().is_empty());
        assert!(vm.is_following());
    }

    #[test]
    fn set_agents_sorts_by_last_seen() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        let agents = vec![
            {
                let mut a = AgentEntry::new("zeta");
                a.last_seen_secs = 100;
                a
            },
            {
                let mut a = AgentEntry::new("alpha");
                a.last_seen_secs = 900;
                a
            },
        ];
        vm.set_agents(agents);
        assert_eq!(vm.agents()[0].name, "alpha"); // most recent first
        assert_eq!(vm.agents()[1].name, "zeta");
    }

    #[test]
    fn set_topics_sorts_by_activity() {
        let mut vm = DashboardViewModel::new();
        vm.set_topics(sample_topics());
        assert_eq!(vm.topics()[0].name, "build"); // most recent first
        assert_eq!(vm.topics()[1].name, "task");
    }

    #[test]
    fn append_feed_prunes_at_limit() {
        let mut vm = DashboardViewModel::new();
        for i in 0..DASHBOARD_FEED_LIMIT + 10 {
            vm.append_feed(FeedMessage {
                time_label: format!("{i}"),
                from: "a".into(),
                to: "b".into(),
                body: format!("msg {i}"),
                priority: String::new(),
            });
        }
        assert_eq!(vm.feed().len(), DASHBOARD_FEED_LIMIT);
        // Oldest messages should have been pruned.
        assert_eq!(vm.feed()[0].time_label, "10");
    }

    // -- Focus / Navigation --------------------------------------------------

    #[test]
    fn cycle_focus() {
        let mut vm = DashboardViewModel::new();
        assert_eq!(vm.focus, DashboardFocus::Agents);
        vm.cycle_focus();
        assert_eq!(vm.focus, DashboardFocus::Topics);
        vm.cycle_focus();
        assert_eq!(vm.focus, DashboardFocus::Feed);
        vm.cycle_focus();
        assert_eq!(vm.focus, DashboardFocus::Agents);
    }

    #[test]
    fn move_up_down_agents() {
        let mut vm = DashboardViewModel::new();
        vm.set_agents(sample_agents());
        assert_eq!(vm.agent_idx(), 0);
        vm.move_down();
        assert_eq!(vm.agent_idx(), 1);
        vm.move_down();
        assert_eq!(vm.agent_idx(), 2);
        vm.move_down(); // clamped
        assert_eq!(vm.agent_idx(), 2);
        vm.move_up();
        assert_eq!(vm.agent_idx(), 1);
    }

    #[test]
    fn move_up_down_topics() {
        let mut vm = DashboardViewModel::new();
        vm.focus = DashboardFocus::Topics;
        vm.set_topics(sample_topics());
        assert_eq!(vm.topic_idx(), 0);
        vm.move_down();
        assert_eq!(vm.topic_idx(), 1);
        vm.move_down(); // clamped
        assert_eq!(vm.topic_idx(), 1);
        vm.move_up();
        assert_eq!(vm.topic_idx(), 0);
    }

    #[test]
    fn move_feed_pauses_and_resumes() {
        let mut vm = DashboardViewModel::new();
        vm.focus = DashboardFocus::Feed;
        for msg in sample_feed() {
            vm.append_feed(msg);
        }
        assert!(vm.is_following());
        vm.move_up(); // pause
        assert_eq!(vm.feed_offset(), 1);
        assert!(!vm.is_following());
        vm.move_up();
        assert_eq!(vm.feed_offset(), 2);
        vm.move_down(); // scroll toward tail
        assert_eq!(vm.feed_offset(), 1);
        vm.resume_follow();
        assert!(vm.is_following());
    }

    // -- Input handling ------------------------------------------------------

    #[test]
    fn input_tab_cycles_focus() {
        let mut vm = DashboardViewModel::new();
        assert!(apply_dashboard_input(&mut vm, key(Key::Tab)));
        assert_eq!(vm.focus, DashboardFocus::Topics);
    }

    #[test]
    fn input_j_k_navigates() {
        let mut vm = DashboardViewModel::new();
        vm.set_agents(sample_agents());
        assert!(apply_dashboard_input(&mut vm, key(Key::Char('j'))));
        assert_eq!(vm.agent_idx(), 1);
        assert!(apply_dashboard_input(&mut vm, key(Key::Char('k'))));
        assert_eq!(vm.agent_idx(), 0);
    }

    #[test]
    fn input_g_resumes_feed() {
        let mut vm = DashboardViewModel::new();
        vm.focus = DashboardFocus::Feed;
        vm.feed_offset = 5;
        assert!(apply_dashboard_input(&mut vm, key(Key::Char('G'))));
        assert!(vm.is_following());
    }

    #[test]
    fn input_g_only_in_feed_focus() {
        let mut vm = DashboardViewModel::new();
        vm.focus = DashboardFocus::Agents;
        vm.feed_offset = 5;
        // G should not be consumed when not in feed focus
        assert!(!apply_dashboard_input(&mut vm, key(Key::Char('G'))));
    }

    // -- Rendering -----------------------------------------------------------

    #[test]
    fn render_empty_dashboard() {
        let vm = DashboardViewModel::new();
        let frame = render_dashboard_frame(&vm, 100, 20, ThemeSpec::default());
        // Should show either agents title or feed title
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("AGENTS") || all_text.contains("LIVE FEED"),
            "should contain panel titles: {all_text}"
        );
    }

    #[test]
    fn render_wide_layout_has_agents_and_feed() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        vm.set_agents(sample_agents());
        vm.set_topics(sample_topics());
        for msg in sample_feed() {
            vm.append_feed(msg);
        }

        let frame = render_dashboard_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("AGENTS"), "should show agents panel");
        assert!(all_text.contains("TOPICS"), "should show topics panel");
        assert!(all_text.contains("LIVE FEED"), "should show feed panel");
        assert!(all_text.contains("alice"), "should show agent name");
        assert!(all_text.contains("build"), "should show topic name");
    }

    #[test]
    fn render_narrow_layout_feed_only() {
        let mut vm = DashboardViewModel::new();
        for msg in sample_feed() {
            vm.append_feed(msg);
        }

        // Narrow: < 80 cols
        let frame = render_dashboard_frame(&vm, 60, 10, ThemeSpec::default());
        let all_text: String = (0..10)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("LIVE FEED"), "narrow: should show feed");
        // Should NOT show agents in narrow mode.
        assert!(!all_text.contains("AGENTS"), "narrow: no agents panel");
    }

    #[test]
    fn render_feed_with_priority_cues() {
        let mut vm = DashboardViewModel::new();
        for msg in sample_feed() {
            vm.append_feed(msg);
        }

        let frame = render_dashboard_frame(&vm, 80, 10, ThemeSpec::default());
        let all_text: String = (0..10)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("[HIGH]"), "should show HIGH priority");
        assert!(all_text.contains("[low]"), "should show low priority");
    }

    #[test]
    fn render_paused_feed_shows_indicator() {
        let mut vm = DashboardViewModel::new();
        for msg in sample_feed() {
            vm.append_feed(msg);
        }
        vm.focus = DashboardFocus::Feed;
        vm.feed_offset = 2;

        let frame = render_dashboard_frame(&vm, 80, 10, ThemeSpec::default());
        let all_text: String = (0..10)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("paused:2"),
            "should show paused state in title"
        );
        assert!(
            all_text.contains("PAUSED (2)"),
            "should show paused indicator: {all_text}"
        );
    }

    #[test]
    fn render_agents_presence_indicators() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        vm.set_agents(sample_agents());

        let frame = render_dashboard_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        // alice (last_seen=1000, now=1050) → online ●
        assert!(
            all_text.contains("\u{25cf}"),
            "should show online indicator"
        );
        // bob (last_seen=500, now=1050) → recently ○
        assert!(
            all_text.contains("\u{25cb}"),
            "should show recently seen indicator"
        );
        // charlie (last_seen=0) → offline ◌
        assert!(
            all_text.contains("\u{25cc}"),
            "should show offline indicator"
        );
    }

    #[test]
    fn render_topics_heat_bars() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        vm.set_agents(sample_agents());
        vm.set_topics(sample_topics());

        let frame = render_dashboard_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        // build has hot_count=5 (max), should show ██
        assert!(
            all_text.contains("\u{2588}\u{2588}"),
            "should show full heat bar"
        );
        // task has hot_count=2/5 = 40%, should show ▒▒
        assert!(
            all_text.contains("\u{2592}\u{2592}"),
            "should show medium heat bar"
        );
    }

    #[test]
    fn render_agent_selection_marker() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        vm.focus = DashboardFocus::Agents;
        vm.set_agents(sample_agents());

        let frame = render_dashboard_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("\u{25b8}"),
            "should show selection marker: {all_text}"
        );
    }

    // -- Snapshot test -------------------------------------------------------

    #[test]
    fn dashboard_snapshot_render() {
        let mut vm = DashboardViewModel::new();
        vm.now_secs = 1050;
        vm.set_agents(sample_agents());
        vm.set_topics(sample_topics());
        for msg in sample_feed() {
            vm.append_feed(msg);
        }

        let frame = render_dashboard_frame(&vm, 100, 12, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_dashboard_view",
            &frame,
            &(0..12)
                .map(|r| {
                    let text = frame.row_text(r);
                    format!("{:<100}", text)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    // -- Presence indicator --------------------------------------------------

    #[test]
    fn presence_thresholds() {
        assert_eq!(presence_indicator(100, 80), "\u{25cf}"); // online (20s)
        assert_eq!(presence_indicator(1000, 500), "\u{25cb}"); // recently (500s)
        assert_eq!(presence_indicator(2000, 100), "\u{25cc}"); // offline (1900s)
        assert_eq!(presence_indicator(100, 0), "\u{25cc}"); // never seen
    }

    // -- Heat bar ------------------------------------------------------------

    #[test]
    fn heat_bar_levels() {
        assert_eq!(topic_heat_bar(0, 10), "\u{2591}\u{2591}"); // ░░
        assert_eq!(topic_heat_bar(2, 10), "\u{2591}\u{2591}"); // ░░ (20%)
        assert_eq!(topic_heat_bar(3, 10), "\u{2592}\u{2592}"); // ▒▒ (30%)
        assert_eq!(topic_heat_bar(6, 10), "\u{2593}\u{2593}"); // ▓▓ (60%)
        assert_eq!(topic_heat_bar(8, 10), "\u{2588}\u{2588}"); // ██ (80%)
        assert_eq!(topic_heat_bar(0, 0), "\u{2591}\u{2591}"); // edge: max=0
    }

    // -- Focus label ---------------------------------------------------------

    #[test]
    fn focus_labels() {
        assert_eq!(DashboardFocus::Agents.label(), "agents");
        assert_eq!(DashboardFocus::Topics.label(), "topics");
        assert_eq!(DashboardFocus::Feed.label(), "feed");
    }
}

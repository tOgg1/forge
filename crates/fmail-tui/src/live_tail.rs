use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

pub const LIVE_TAIL_MAX_MESSAGES: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTailMessage {
    pub timestamp: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub priority: String,
    pub tags: Vec<String>,
}

impl LiveTailMessage {
    #[must_use]
    pub fn new(timestamp: &str, from: &str, to: &str, body: &str) -> Self {
        Self {
            timestamp: timestamp.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            body: body.to_owned(),
            priority: String::new(),
            tags: Vec::new(),
        }
    }

    #[must_use]
    fn first_body_line(&self) -> &str {
        self.body
            .split('\n')
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LiveTailFilter {
    pub from: String,
    pub to: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub text: String,
    pub dm_only: bool,
}

impl LiveTailFilter {
    #[must_use]
    pub fn active_label(&self) -> String {
        let mut parts = Vec::with_capacity(6);
        if !self.from.trim().is_empty() {
            parts.push(format!("from:{}", self.from.trim()));
        }
        if !self.to.trim().is_empty() {
            parts.push(format!("to:{}", self.to.trim()));
        }
        if !self.priority.trim().is_empty() {
            parts.push(format!("priority:{}", self.priority.trim()));
        }
        for tag in &self.tags {
            let tag = tag.trim();
            if !tag.is_empty() {
                parts.push(format!("tag:{tag}"));
            }
        }
        if !self.text.trim().is_empty() {
            parts.push(format!("text:{}", self.text.trim()));
        }
        if self.dm_only {
            parts.push("dm:only".to_owned());
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(" ")
        }
    }

    #[must_use]
    pub fn matches(&self, msg: &LiveTailMessage) -> bool {
        if self.dm_only && !msg.to.trim_start().starts_with('@') {
            return false;
        }
        if !self.from.trim().is_empty() && !eq_ci(msg.from.trim(), self.from.trim()) {
            return false;
        }
        if !self.to.trim().is_empty() && !eq_ci(msg.to.trim(), self.to.trim()) {
            return false;
        }
        if !self.priority.trim().is_empty() && !eq_ci(msg.priority.trim(), self.priority.trim()) {
            return false;
        }
        if !self.tags.is_empty() {
            let have = msg
                .tags
                .iter()
                .map(|tag| tag.trim().to_ascii_lowercase())
                .collect::<Vec<_>>();
            for want in &self.tags {
                let want = want.trim().to_ascii_lowercase();
                if want.is_empty() {
                    continue;
                }
                if !have.iter().any(|tag| tag == &want) {
                    return false;
                }
            }
        }
        if !self.text.trim().is_empty() {
            let needle = self.text.trim().to_ascii_lowercase();
            let blob = msg.body.to_ascii_lowercase();
            if !blob.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[must_use]
pub fn parse_live_tail_filter(input: &str) -> LiveTailFilter {
    let input = input.trim();
    if input.is_empty() {
        return LiveTailFilter::default();
    }
    let mut filter = LiveTailFilter::default();
    let mut text_terms = Vec::with_capacity(2);
    for token in input.split_whitespace() {
        let Some((key, value)) = token.split_once(':') else {
            text_terms.push(token.to_owned());
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "from" => filter.from = value.to_owned(),
            "to" => filter.to = value.to_owned(),
            "priority" => filter.priority = value.to_owned(),
            "tag" => {
                if !value.is_empty() {
                    filter.tags.push(value.to_owned());
                }
            }
            "text" => {
                if !value.is_empty() {
                    text_terms.push(value.to_owned());
                }
            }
            "dm" => {
                if matches!(value, "1" | "true" | "only") {
                    filter.dm_only = true;
                }
            }
            _ => {
                if !value.is_empty() {
                    text_terms.push(value.to_owned());
                }
            }
        }
    }
    filter.text = text_terms.join(" ").trim().to_owned();
    filter
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LiveTailViewModel {
    feed: Vec<LiveTailMessage>,
    buffered: Vec<LiveTailMessage>,
    paused: bool,
    offset: usize,
    filter: LiveTailFilter,
    highlights: Vec<String>,
}

impl LiveTailViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn feed_len(&self) -> usize {
        self.feed.len()
    }

    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.buffered.len()
    }

    pub fn push(&mut self, message: LiveTailMessage) {
        if self.paused {
            capped_push(&mut self.buffered, message);
        } else {
            capped_push(&mut self.feed, message);
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
        self.offset = 0;
    }

    pub fn resume(&mut self) {
        if !self.paused {
            return;
        }
        if !self.buffered.is_empty() {
            self.feed.append(&mut self.buffered);
            trim_tail(&mut self.feed);
        }
        self.paused = false;
        self.offset = 0;
    }

    pub fn clear_filter(&mut self) {
        self.filter = LiveTailFilter::default();
        self.offset = 0;
    }

    pub fn set_filter_from_input(&mut self, input: &str) {
        self.filter = parse_live_tail_filter(input);
        self.offset = 0;
    }

    pub fn set_highlights_csv(&mut self, raw: &str) {
        self.highlights = raw
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| entry.to_ascii_lowercase())
            .collect();
    }

    #[must_use]
    pub fn visible_messages(&self) -> Vec<&LiveTailMessage> {
        self.feed
            .iter()
            .filter(|msg| self.filter.matches(msg))
            .collect::<Vec<_>>()
    }

    fn scroll_up(&mut self) {
        if !self.paused {
            return;
        }
        let max_offset = self.visible_messages().len().saturating_sub(1);
        self.offset = (self.offset + 1).min(max_offset);
    }

    fn scroll_down(&mut self) {
        if !self.paused {
            return;
        }
        self.offset = self.offset.saturating_sub(1);
    }

    fn message_role(&self, message: &LiveTailMessage) -> TextRole {
        if eq_ci(message.priority.trim(), "high") {
            return TextRole::Danger;
        }
        if self.is_highlighted(message) {
            return TextRole::Accent;
        }
        TextRole::Primary
    }

    fn is_highlighted(&self, message: &LiveTailMessage) -> bool {
        if self.highlights.is_empty() {
            return false;
        }
        let haystack = format!(
            "{} {} {}",
            message.from.to_ascii_lowercase(),
            message.to.to_ascii_lowercase(),
            message.body.to_ascii_lowercase()
        );
        self.highlights
            .iter()
            .any(|needle| !needle.is_empty() && haystack.contains(needle))
    }
}

pub fn apply_live_tail_input(view: &mut LiveTailViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent {
            key: Key::Char(' '),
            ..
        }) => {
            if view.paused {
                view.resume();
            } else {
                view.pause();
            }
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('G'),
            ..
        }) => {
            view.resume();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('c'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.clear_filter();
            return;
        }
        _ => {}
    }

    match translate_input(&event) {
        UiAction::MoveUp | UiAction::ScrollUp => view.scroll_up(),
        UiAction::MoveDown | UiAction::ScrollDown => view.scroll_down(),
        _ => {}
    }
}

#[must_use]
pub fn render_live_tail_frame(
    view: &LiveTailViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let mut header = format!("LIVE TAIL  filter: {}", view.filter.active_label());
    if view.paused {
        if view.buffered.is_empty() {
            header.push_str("  PAUSED");
        } else {
            header.push_str(&format!("  PAUSED (+{})", view.buffered.len()));
        }
    }
    frame.draw_text(0, 0, &truncate(&header, width), TextRole::Accent);

    if height == 1 {
        return frame;
    }

    let visible = view.visible_messages();
    let content_h = height - 1;
    if visible.is_empty() {
        frame.draw_text(0, 1, "(no messages yet)", TextRole::Muted);
        return frame;
    }

    let end = visible.len().saturating_sub(view.offset);
    let start = end.saturating_sub(content_h);
    let mut row = 1usize;
    for message in &visible[start..end] {
        if row >= height {
            break;
        }
        let target = if message.to.trim_start().starts_with('@') {
            "DM".to_owned()
        } else {
            truncate(message.to.trim(), 16)
        };
        let mut line = format!(
            "{} [{}] {} -> {}",
            message.timestamp.trim(),
            target,
            message.from.trim(),
            message.to.trim()
        );
        let body = message.first_body_line().trim();
        if !body.is_empty() {
            line.push_str("  ");
            line.push_str(body);
        }
        frame.draw_text(0, row, &truncate(&line, width), view.message_role(message));
        row += 1;
    }

    if !view.paused && row < height {
        frame.draw_text(
            0,
            height - 1,
            "(auto-scrolling; Space pauses)",
            TextRole::Muted,
        );
    }
    frame
}

fn capped_push(list: &mut Vec<LiveTailMessage>, message: LiveTailMessage) {
    list.push(message);
    trim_tail(list);
}

fn trim_tail(list: &mut Vec<LiveTailMessage>) {
    if list.len() > LIVE_TAIL_MAX_MESSAGES {
        let drop_n = list.len() - LIVE_TAIL_MAX_MESSAGES;
        list.drain(0..drop_n);
    }
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_owned();
    }
    if max_chars == 1 {
        return "…".to_owned();
    }
    let mut out = chars.into_iter().take(max_chars - 1).collect::<String>();
    out.push('…');
    out
}

fn eq_ci(lhs: &str, rhs: &str) -> bool {
    lhs.eq_ignore_ascii_case(rhs)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_live_tail_input, parse_live_tail_filter, render_live_tail_frame, LiveTailMessage,
        LiveTailViewModel,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn parse_filter_matches_go_shape() {
        let parsed = parse_live_tail_filter(
            "from:alice to:task priority:high tag:auth text:refresh dm:only",
        );
        assert_eq!(parsed.from, "alice");
        assert_eq!(parsed.to, "task");
        assert_eq!(parsed.priority, "high");
        assert_eq!(parsed.tags, vec!["auth"]);
        assert_eq!(parsed.text, "refresh");
        assert!(parsed.dm_only);
    }

    #[test]
    fn pause_buffers_and_resume_flushes() {
        let mut view = LiveTailViewModel::new();
        view.pause();
        view.push(LiveTailMessage::new("15:30:00", "alice", "task", "first"));
        assert_eq!(view.feed_len(), 0);
        assert_eq!(view.buffered_len(), 1);

        view.resume();
        assert_eq!(view.feed_len(), 1);
        assert_eq!(view.buffered_len(), 0);
    }

    #[test]
    fn input_pause_and_scroll_follow_tail() {
        let mut view = LiveTailViewModel::new();
        view.push(LiveTailMessage::new("15:30:00", "alice", "task", "one"));
        view.push(LiveTailMessage::new("15:30:01", "bob", "task", "two"));

        apply_live_tail_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(' '))));
        assert!(view.paused());

        apply_live_tail_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Up)));
        let paused_snapshot = render_live_tail_frame(&view, 50, 4, ThemeSpec::default()).snapshot();
        assert!(paused_snapshot.contains("PAUSED"));

        apply_live_tail_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('G'))));
        assert!(!view.paused());
    }

    #[test]
    fn render_snapshot_live_tail() {
        let mut view = LiveTailViewModel::new();
        view.push(LiveTailMessage::new(
            "15:30:00",
            "architect",
            "task",
            "implement JWT auth",
        ));
        view.push(LiveTailMessage::new(
            "15:30:01",
            "coder-1",
            "@architect",
            "need clarification",
        ));
        view.set_highlights_csv("clarification");

        let frame = render_live_tail_frame(&view, 54, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_live_tail_frame",
            &frame,
            "LIVE TAIL  filter: none                               \n15:30:00 [task] architect -> task  implement JWT auth \n15:30:01 [DM] coder-1 -> @architect  need clarificati…\n                                                      \n(auto-scrolling; Space pauses)                        ",
        );
    }

    #[test]
    fn filter_matches_dm_priority_and_sender() {
        let mut view = LiveTailViewModel::new();
        view.set_filter_from_input("from:alice priority:high dm:only");

        let mut msg = LiveTailMessage::new("15:31:00", "alice", "@bob", "x");
        msg.priority = "high".to_owned();
        view.push(msg.clone());
        assert_eq!(view.visible_messages().len(), 1);

        msg.to = "task".to_owned();
        view.push(msg);
        assert_eq!(view.visible_messages().len(), 1);
    }
}

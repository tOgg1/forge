use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineMessage {
    pub id: String,
    pub timestamp: String,
    pub from: String,
    pub to: String,
    pub topic: String,
    pub body: String,
    pub priority: String,
    pub tags: Vec<String>,
}

impl TimelineMessage {
    #[must_use]
    pub fn new(id: &str, timestamp: &str, from: &str, to: &str, body: &str) -> Self {
        Self {
            id: id.to_owned(),
            timestamp: timestamp.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            topic: to.to_owned(),
            body: body.to_owned(),
            priority: String::new(),
            tags: Vec::new(),
        }
    }

    #[must_use]
    fn body_line(&self) -> &str {
        self.body
            .split('\n')
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineMode {
    Chronological,
    Swimlane,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TimelineFilter {
    pub from: String,
    pub to: String,
    pub topic: String,
    pub priority: String,
    pub text: String,
}

impl TimelineFilter {
    #[must_use]
    pub fn active_label(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        if !self.from.trim().is_empty() {
            parts.push(format!("from:{}", self.from.trim()));
        }
        if !self.to.trim().is_empty() {
            parts.push(format!("to:{}", self.to.trim()));
        }
        if !self.topic.trim().is_empty() {
            parts.push(format!("in:{}", self.topic.trim()));
        }
        if !self.priority.trim().is_empty() {
            parts.push(format!("priority:{}", self.priority.trim()));
        }
        if !self.text.trim().is_empty() {
            parts.push(format!("text:{}", self.text.trim()));
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(" ")
        }
    }

    #[must_use]
    pub fn matches(&self, message: &TimelineMessage) -> bool {
        if !self.from.trim().is_empty() && !message.from.eq_ignore_ascii_case(self.from.trim()) {
            return false;
        }
        if !self.to.trim().is_empty() && !message.to.eq_ignore_ascii_case(self.to.trim()) {
            return false;
        }
        if !self.topic.trim().is_empty() && !message.topic.eq_ignore_ascii_case(self.topic.trim()) {
            return false;
        }
        if !self.priority.trim().is_empty()
            && !message.priority.eq_ignore_ascii_case(self.priority.trim())
        {
            return false;
        }
        if !self.text.trim().is_empty() {
            let needle = self.text.trim().to_ascii_lowercase();
            let blob = format!(
                "{} {} {}",
                message.from.to_ascii_lowercase(),
                message.to.to_ascii_lowercase(),
                message.body.to_ascii_lowercase()
            );
            if !blob.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[must_use]
pub fn parse_timeline_filter(input: &str) -> TimelineFilter {
    let input = input.trim();
    if input.is_empty() {
        return TimelineFilter::default();
    }
    let mut filter = TimelineFilter::default();
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
            "in" | "topic" => filter.topic = value.to_owned(),
            "priority" => filter.priority = value.to_owned(),
            "text" => text_terms.push(value.to_owned()),
            _ => text_terms.push(value.to_owned()),
        }
    }
    filter.text = text_terms.join(" ").trim().to_owned();
    filter
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineViewModel {
    all: Vec<TimelineMessage>,
    mode: TimelineMode,
    filter: TimelineFilter,
    selected: usize,
    top: usize,
}

impl Default for TimelineViewModel {
    fn default() -> Self {
        Self {
            all: Vec::new(),
            mode: TimelineMode::Chronological,
            filter: TimelineFilter::default(),
            selected: 0,
            top: 0,
        }
    }
}

impl TimelineViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, message: TimelineMessage) {
        self.all.push(message);
        self.all
            .sort_by(|lhs, rhs| lhs.timestamp.cmp(&rhs.timestamp));
        self.clamp_selection();
    }

    pub fn set_filter_from_input(&mut self, raw: &str) {
        self.filter = parse_timeline_filter(raw);
        self.selected = 0;
        self.top = 0;
        self.clamp_selection();
    }

    pub fn clear_filter(&mut self) {
        self.filter = TimelineFilter::default();
        self.selected = 0;
        self.top = 0;
        self.clamp_selection();
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            TimelineMode::Chronological => TimelineMode::Swimlane,
            TimelineMode::Swimlane => TimelineMode::Chronological,
        };
    }

    #[must_use]
    pub fn mode(&self) -> TimelineMode {
        self.mode
    }

    #[must_use]
    pub fn visible_messages(&self) -> Vec<&TimelineMessage> {
        self.all
            .iter()
            .filter(|msg| self.filter.matches(msg))
            .collect::<Vec<_>>()
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        if self.selected < self.top {
            self.top = self.selected;
        }
    }

    pub fn move_down(&mut self) {
        let max_idx = self.visible_messages().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max_idx);
    }

    fn clamp_selection(&mut self) {
        let max_idx = self.visible_messages().len().saturating_sub(1);
        self.selected = self.selected.min(max_idx);
        self.top = self.top.min(self.selected);
    }
}

pub fn apply_timeline_input(view: &mut TimelineViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent {
            key: Key::Char('m'),
            ..
        }) => {
            view.toggle_mode();
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
        UiAction::MoveUp => view.move_up(),
        UiAction::MoveDown => view.move_down(),
        _ => {}
    }
}

#[must_use]
pub fn render_timeline_frame(
    view: &TimelineViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let mode_label = match view.mode {
        TimelineMode::Chronological => "TIMELINE",
        TimelineMode::Swimlane => "SEQUENCE",
    };
    let visible = view.visible_messages();
    frame.draw_text(
        0,
        0,
        &truncate(
            &format!(
                "{mode_label}  filter:{}  {}/{}",
                view.filter.active_label(),
                visible.len(),
                view.all.len()
            ),
            width,
        ),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    if visible.is_empty() {
        frame.draw_text(0, 1, "(no timeline entries)", TextRole::Muted);
        return frame;
    }

    let content_h = height - 1;
    let max_top = visible.len().saturating_sub(content_h);
    let top = view.top.min(max_top).min(view.selected);
    let end = (top + content_h).min(visible.len());
    for (row_off, idx) in (top..end).enumerate() {
        let msg = visible[idx];
        let marker = if idx == view.selected { ">" } else { " " };
        let line = match view.mode {
            TimelineMode::Chronological => format!(
                "{}{} {} {} -> {}  {}",
                marker,
                msg.timestamp.trim(),
                topic_label(msg),
                msg.from.trim(),
                msg.to.trim(),
                msg.body_line().trim()
            ),
            TimelineMode::Swimlane => format!(
                "{}{} {:<12} => {:<12}  {}",
                marker,
                msg.timestamp.trim(),
                truncate(msg.from.trim(), 12),
                truncate(msg.to.trim(), 12),
                msg.body_line().trim()
            ),
        };
        frame.draw_text(
            0,
            row_off + 1,
            &truncate(&line, width),
            if idx == view.selected {
                TextRole::Accent
            } else if msg.priority.eq_ignore_ascii_case("high") {
                TextRole::Danger
            } else {
                TextRole::Primary
            },
        );
    }
    frame
}

fn topic_label(message: &TimelineMessage) -> String {
    let target = message.to.trim();
    if target.starts_with('@') {
        "[DM]".to_owned()
    } else {
        format!("[{}]", truncate(target, 10))
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

#[cfg(test)]
mod tests {
    use super::{
        apply_timeline_input, parse_timeline_filter, render_timeline_frame, TimelineMessage,
        TimelineMode, TimelineViewModel,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn parse_filter_matches_go_shape() {
        let parsed = parse_timeline_filter("from:alice to:task in:task priority:high text:refresh");
        assert_eq!(parsed.from, "alice");
        assert_eq!(parsed.to, "task");
        assert_eq!(parsed.topic, "task");
        assert_eq!(parsed.priority, "high");
        assert_eq!(parsed.text, "refresh");
    }

    #[test]
    fn timeline_mode_toggle_and_navigation() {
        let mut view = TimelineViewModel::new();
        view.push(TimelineMessage::new("1", "15:30:00", "a", "task", "one"));
        view.push(TimelineMessage::new("2", "15:31:00", "b", "task", "two"));
        assert_eq!(view.mode(), TimelineMode::Chronological);

        apply_timeline_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('m'))));
        assert_eq!(view.mode(), TimelineMode::Swimlane);

        apply_timeline_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Down)));
        let frame = render_timeline_frame(&view, 48, 4, ThemeSpec::default());
        assert!(frame.snapshot().contains(">15:31:00"));
    }

    #[test]
    fn timeline_snapshot_chronological() {
        let mut view = TimelineViewModel::new();
        view.push(TimelineMessage::new(
            "1",
            "15:30:00",
            "architect",
            "task",
            "implement jwt auth",
        ));
        view.push(TimelineMessage::new(
            "2",
            "15:30:04",
            "coder-1",
            "@architect",
            "need clarification",
        ));
        view.set_filter_from_input("text:clarification");

        let frame = render_timeline_frame(&view, 56, 4, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_timeline_frame",
            &frame,
            "TIMELINE  filter:text:clarification  1/2                \n>15:30:04 [DM] coder-1 -> @architect  need clarification\n                                                        \n                                                        ",
        );
    }
}

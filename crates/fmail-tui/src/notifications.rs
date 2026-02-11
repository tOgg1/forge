use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

pub const NOTIFICATION_MEMORY_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationItem {
    pub message_id: String,
    pub target: String,
    pub from: String,
    pub preview: String,
    pub priority: String,
    pub unread: bool,
    pub badge: bool,
    pub highlight: bool,
}

impl NotificationItem {
    #[must_use]
    pub fn new(message_id: &str, from: &str, target: &str, preview: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            target: target.to_owned(),
            from: from.to_owned(),
            preview: preview.to_owned(),
            priority: String::new(),
            unread: true,
            badge: true,
            highlight: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRule {
    pub name: String,
    pub enabled: bool,
    pub topic: String,
    pub from: String,
    pub to: String,
    pub priority: String,
}

impl NotificationRule {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            enabled: true,
            topic: String::new(),
            from: String::new(),
            to: String::new(),
            priority: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationsFocus {
    Items,
    Rules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationsViewModel {
    items: Vec<NotificationItem>,
    rules: Vec<NotificationRule>,
    focus: NotificationsFocus,
    item_idx: usize,
    rule_idx: usize,
    status_line: String,
    status_err: bool,
}

impl Default for NotificationsViewModel {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            rules: vec![NotificationRule::new("high-priority")],
            focus: NotificationsFocus::Items,
            item_idx: 0,
            rule_idx: 0,
            status_line: String::new(),
            status_err: false,
        }
    }
}

impl NotificationsViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: NotificationItem) {
        self.items.insert(0, item);
        if self.items.len() > NOTIFICATION_MEMORY_LIMIT {
            self.items.truncate(NOTIFICATION_MEMORY_LIMIT);
        }
        self.clamp_selection();
    }

    #[must_use]
    pub fn unread_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.unread && item.badge)
            .count()
    }

    pub fn mark_selected_read(&mut self) {
        let Some(item) = self.items.get_mut(self.item_idx) else {
            return;
        };
        if item.unread {
            item.unread = false;
            self.status_line = format!("read {}", item.message_id);
            self.status_err = false;
        }
    }

    pub fn dismiss_selected(&mut self) {
        if self.item_idx >= self.items.len() {
            return;
        }
        let id = self.items[self.item_idx].message_id.clone();
        self.items.remove(self.item_idx);
        self.status_line = format!("dismissed {id}");
        self.status_err = false;
        self.clamp_selection();
    }

    pub fn clear_notifications(&mut self) {
        self.items.clear();
        self.item_idx = 0;
        self.status_line = "notifications cleared".to_owned();
        self.status_err = false;
    }

    pub fn toggle_rule_enabled(&mut self) {
        let Some(rule) = self.rules.get_mut(self.rule_idx) else {
            return;
        };
        rule.enabled = !rule.enabled;
        self.status_line = format!(
            "rule {} {}",
            rule.name,
            if rule.enabled { "on" } else { "off" }
        );
        self.status_err = false;
    }

    pub fn add_or_replace_rule(&mut self, rule: NotificationRule) {
        if rule.name.trim().is_empty() {
            self.status_line = "invalid rule name".to_owned();
            self.status_err = true;
            return;
        }
        if let Some((idx, _)) = self
            .rules
            .iter()
            .enumerate()
            .find(|(_, existing)| existing.name.eq_ignore_ascii_case(rule.name.trim()))
        {
            self.rules[idx] = rule;
            self.rule_idx = idx;
        } else {
            self.rules.push(rule);
            self.rule_idx = self.rules.len() - 1;
        }
        self.status_line = "rule saved".to_owned();
        self.status_err = false;
    }

    pub fn move_up(&mut self) {
        match self.focus {
            NotificationsFocus::Items => {
                self.item_idx = self.item_idx.saturating_sub(1);
            }
            NotificationsFocus::Rules => {
                self.rule_idx = self.rule_idx.saturating_sub(1);
            }
        }
        self.clamp_selection();
    }

    pub fn move_down(&mut self) {
        match self.focus {
            NotificationsFocus::Items => {
                if self.item_idx + 1 < self.items.len() {
                    self.item_idx += 1;
                }
            }
            NotificationsFocus::Rules => {
                if self.rule_idx + 1 < self.rules.len() {
                    self.rule_idx += 1;
                }
            }
        }
        self.clamp_selection();
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            NotificationsFocus::Items => NotificationsFocus::Rules,
            NotificationsFocus::Rules => NotificationsFocus::Items,
        };
        self.clamp_selection();
    }

    #[must_use]
    pub fn focus(&self) -> NotificationsFocus {
        self.focus
    }

    #[must_use]
    pub fn rules(&self) -> &[NotificationRule] {
        &self.rules
    }

    #[must_use]
    pub fn notifications(&self) -> &[NotificationItem] {
        &self.items
    }

    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.item_idx = 0;
        } else {
            self.item_idx = self.item_idx.min(self.items.len() - 1);
        }
        if self.rules.is_empty() {
            self.rule_idx = 0;
        } else {
            self.rule_idx = self.rule_idx.min(self.rules.len() - 1);
        }
    }
}

pub fn apply_notifications_input(view: &mut NotificationsViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent { key: Key::Tab, .. }) => {
            view.toggle_focus();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('x'),
            ..
        }) => {
            view.dismiss_selected();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('c'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.clear_notifications();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char(' '),
            ..
        }) => {
            if view.focus == NotificationsFocus::Rules {
                view.toggle_rule_enabled();
            }
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Enter, ..
        }) => {
            if view.focus == NotificationsFocus::Items {
                view.mark_selected_read();
            }
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
pub fn render_notifications_frame(
    view: &NotificationsViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let mut row = 0usize;
    frame.draw_text(
        0,
        row,
        &truncate(
            &format!("Notifications ({} unread)", view.unread_count()),
            width,
        ),
        TextRole::Accent,
    );
    row += 1;
    if row >= height {
        return frame;
    }

    frame.draw_text(
        0,
        row,
        "Enter read  x dismiss  c clear  Tab section  Space toggle rule",
        TextRole::Muted,
    );
    row += 1;
    if row >= height {
        return frame;
    }

    frame.draw_text(
        0,
        row,
        match view.focus {
            NotificationsFocus::Items => "focus: notifications",
            NotificationsFocus::Rules => "focus: rules",
        },
        TextRole::Muted,
    );
    row += 1;
    if row >= height {
        return frame;
    }

    let remaining = height - row;
    let item_slots = remaining.saturating_div(2).max(2);
    let shown_items = view.items.iter().take(item_slots);
    for (idx, item) in shown_items.enumerate() {
        if row >= height {
            break;
        }
        let selected = view.focus == NotificationsFocus::Items && idx == view.item_idx;
        let marker = if selected { ">" } else { " " };
        let unread = if item.unread { "●" } else { " " };
        let line = format!(
            "{}{} {} -> {}  {}",
            marker,
            unread,
            item.from.trim(),
            item.target.trim(),
            item.preview.trim()
        );
        frame.draw_text(
            0,
            row,
            &truncate(&line, width),
            if item.highlight {
                TextRole::Accent
            } else if eq_ci(item.priority.trim(), "high") {
                TextRole::Danger
            } else {
                TextRole::Primary
            },
        );
        row += 1;
    }
    if row < height {
        frame.draw_text(0, row, &"-".repeat(width.min(64)), TextRole::Muted);
        row += 1;
    }

    let rule_slots = height.saturating_sub(row + 1);
    for (idx, rule) in view.rules.iter().take(rule_slots).enumerate() {
        if row >= height {
            break;
        }
        let selected = view.focus == NotificationsFocus::Rules && idx == view.rule_idx;
        let marker = if selected { ">" } else { " " };
        let state = if rule.enabled { "on " } else { "off" };
        let line = format!("{}[{}] {}", marker, state, rule.name.trim());
        frame.draw_text(0, row, &truncate(&line, width), TextRole::Primary);
        row += 1;
    }

    if !view.status_line.trim().is_empty() && height > 0 {
        frame.draw_text(
            0,
            height - 1,
            &truncate(view.status_line.trim(), width),
            if view.status_err {
                TextRole::Danger
            } else {
                TextRole::Muted
            },
        );
    }

    frame
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
        apply_notifications_input, render_notifications_frame, NotificationItem, NotificationRule,
        NotificationsFocus, NotificationsViewModel,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn unread_and_dismiss_flow() {
        let mut view = NotificationsViewModel::new();
        view.push(NotificationItem::new("m1", "architect", "task", "first"));
        view.push(NotificationItem::new("m2", "coder", "@architect", "second"));
        assert_eq!(view.unread_count(), 2);

        view.mark_selected_read();
        assert_eq!(view.unread_count(), 1);
        view.dismiss_selected();
        assert_eq!(view.notifications().len(), 1);

        view.clear_notifications();
        assert!(view.notifications().is_empty());
        assert_eq!(view.unread_count(), 0);
    }

    #[test]
    fn focus_and_rule_toggle_flow() {
        let mut view = NotificationsViewModel::new();
        view.add_or_replace_rule(NotificationRule::new("direct-messages"));
        assert_eq!(view.focus(), NotificationsFocus::Items);
        apply_notifications_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Tab)));
        assert_eq!(view.focus(), NotificationsFocus::Rules);

        let idx = view.rule_idx;
        let before = view.rules()[idx].enabled;
        apply_notifications_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(' '))));
        assert_eq!(view.rules()[idx].enabled, !before);
    }

    #[test]
    fn notifications_frame_snapshot() {
        let mut view = NotificationsViewModel::new();
        view.push(NotificationItem::new(
            "m1",
            "architect",
            "@viewer",
            "refresh token strategy",
        ));
        view.push(NotificationItem::new(
            "m2",
            "reviewer",
            "task",
            "run full test suite",
        ));

        let frame = render_notifications_frame(&view, 62, 8, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_notifications_frame",
            &frame,
            "Notifications (2 unread)                                      \nEnter read  x dismiss  c clear  Tab section  Space toggle rule\nfocus: notifications                                          \n>● reviewer -> task  run full test suite                      \n ● architect -> @viewer  refresh token strategy               \n--------------------------------------------------------------\n [on ] high-priority                                          \n                                                              ",
        );
    }
}

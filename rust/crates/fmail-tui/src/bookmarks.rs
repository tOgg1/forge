use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkEntry {
    pub message_id: String,
    pub target: String,
    pub preview: String,
    pub note: String,
    pub pinned: bool,
}

impl BookmarkEntry {
    #[must_use]
    pub fn new(message_id: &str, target: &str, preview: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            target: target.to_owned(),
            preview: preview.to_owned(),
            note: String::new(),
            pinned: false,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BookmarksFilter {
    pub target: String,
    pub text: String,
    pub pinned_only: bool,
}

impl BookmarksFilter {
    #[must_use]
    pub fn active_label(&self) -> String {
        let mut parts = Vec::with_capacity(3);
        if !self.target.trim().is_empty() {
            parts.push(format!("target:{}", self.target.trim()));
        }
        if !self.text.trim().is_empty() {
            parts.push(format!("text:{}", self.text.trim()));
        }
        if self.pinned_only {
            parts.push("pinned:only".to_owned());
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(" ")
        }
    }

    #[must_use]
    pub fn matches(&self, bookmark: &BookmarkEntry) -> bool {
        if self.pinned_only && !bookmark.pinned {
            return false;
        }
        if !self.target.trim().is_empty()
            && !bookmark.target.eq_ignore_ascii_case(self.target.trim())
        {
            return false;
        }
        if !self.text.trim().is_empty() {
            let needle = self.text.trim().to_ascii_lowercase();
            let blob = format!(
                "{} {} {}",
                bookmark.preview.to_ascii_lowercase(),
                bookmark.note.to_ascii_lowercase(),
                bookmark.target.to_ascii_lowercase()
            );
            if !blob.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[must_use]
pub fn parse_bookmarks_filter(input: &str) -> BookmarksFilter {
    let input = input.trim();
    if input.is_empty() {
        return BookmarksFilter::default();
    }
    let mut filter = BookmarksFilter::default();
    let mut text_terms = Vec::with_capacity(2);
    for token in input.split_whitespace() {
        let Some((key, value)) = token.split_once(':') else {
            text_terms.push(token.to_owned());
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "target" => filter.target = value.to_owned(),
            "text" => text_terms.push(value.to_owned()),
            "pinned" => {
                if matches!(value, "1" | "true" | "only") {
                    filter.pinned_only = true;
                }
            }
            _ => text_terms.push(value.to_owned()),
        }
    }
    filter.text = text_terms.join(" ").trim().to_owned();
    filter
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookmarksViewModel {
    entries: Vec<BookmarkEntry>,
    selected: usize,
    filter: BookmarksFilter,
    status_line: String,
}

impl BookmarksViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, entry: BookmarkEntry) {
        self.entries.insert(0, entry);
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            return;
        };
        let removed = self.entries.remove(idx);
        self.status_line = format!("removed {}", removed.message_id);
        self.selected = self.selected.saturating_sub(1);
        self.clamp_selection();
    }

    pub fn toggle_pin_selected(&mut self) {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            return;
        };
        let Some(entry) = self.entries.get_mut(idx) else {
            return;
        };
        entry.pinned = !entry.pinned;
        self.status_line = if entry.pinned {
            format!("pinned {}", entry.message_id)
        } else {
            format!("unpinned {}", entry.message_id)
        };
    }

    pub fn set_filter_from_input(&mut self, raw: &str) {
        self.filter = parse_bookmarks_filter(raw);
        self.selected = 0;
        self.clamp_selection();
    }

    pub fn clear_filter(&mut self) {
        self.filter = BookmarksFilter::default();
        self.selected = 0;
        self.clamp_selection();
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max_idx = self.visible_indices().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max_idx);
    }

    #[must_use]
    pub fn entries(&self) -> &[BookmarkEntry] {
        &self.entries
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| self.filter.matches(entry).then_some(idx))
            .collect::<Vec<_>>()
    }

    fn clamp_selection(&mut self) {
        let max_idx = self.visible_indices().len().saturating_sub(1);
        self.selected = self.selected.min(max_idx);
    }
}

pub fn apply_bookmarks_input(view: &mut BookmarksViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent {
            key: Key::Char('x'),
            ..
        }) => {
            view.remove_selected();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('p'),
            ..
        }) => {
            view.toggle_pin_selected();
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
pub fn render_bookmarks_frame(
    view: &BookmarksViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    frame.draw_text(
        0,
        0,
        &truncate(
            &format!(
                "BOOKMARKS  filter:{}  {} total",
                view.filter.active_label(),
                view.visible_indices().len()
            ),
            width,
        ),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    let visible = view.visible_indices();
    if visible.is_empty() {
        frame.draw_text(0, 1, "(no bookmarks)", TextRole::Muted);
        return frame;
    }

    let rows = height.saturating_sub(2);
    for (row, visible_idx) in visible.iter().take(rows).enumerate() {
        let Some(entry) = view.entries.get(*visible_idx) else {
            continue;
        };
        let marker = if row == view.selected { ">" } else { " " };
        let pin = if entry.pinned { "★" } else { " " };
        let note = if entry.note.trim().is_empty() {
            String::new()
        } else {
            format!(" [{}]", truncate(entry.note.trim(), 14))
        };
        let line = format!(
            "{}{} {} {}  {}{}",
            marker,
            pin,
            truncate(&entry.message_id, 18),
            truncate(entry.target.trim(), 10),
            truncate(entry.preview.trim(), 20),
            note
        );
        frame.draw_text(0, row + 1, &truncate(&line, width), TextRole::Primary);
    }

    if height >= 2 && !view.status_line.trim().is_empty() {
        frame.draw_text(
            0,
            height - 1,
            &truncate(view.status_line.trim(), width),
            TextRole::Muted,
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

#[cfg(test)]
mod tests {
    use super::{
        apply_bookmarks_input, parse_bookmarks_filter, render_bookmarks_frame, BookmarkEntry,
        BookmarksViewModel,
    };
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn parse_filter_shape() {
        let parsed = parse_bookmarks_filter("target:task text:urgent pinned:only");
        assert_eq!(parsed.target, "task");
        assert_eq!(parsed.text, "urgent");
        assert!(parsed.pinned_only);
    }

    #[test]
    fn pin_remove_flow() {
        let mut view = BookmarksViewModel::new();
        view.add(BookmarkEntry::new("m1", "task", "one"));
        view.add(BookmarkEntry::new("m2", "@ops", "two"));

        apply_bookmarks_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('p'))));
        assert!(view.entries()[0].pinned);

        apply_bookmarks_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('x'))));
        assert_eq!(view.entries().len(), 1);
    }

    #[test]
    fn bookmarks_snapshot() {
        let mut view = BookmarksViewModel::new();
        let mut one = BookmarkEntry::new("20260209-120000-0001", "task", "refresh token plan");
        one.note = "critical".to_owned();
        view.add(one);
        view.add(BookmarkEntry::new(
            "20260209-120200-0001",
            "@architect",
            "need review",
        ));

        let frame = render_bookmarks_frame(&view, 60, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_bookmarks_frame",
            &frame,
            "BOOKMARKS  filter:none  2 total                             \n>  20260209-120200-0… @architect  need review               \n   20260209-120000-0… task  refresh token plan [critical]   \n                                                            \n                                                            ",
        );
    }
}

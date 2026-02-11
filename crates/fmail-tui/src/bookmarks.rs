use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// BookmarkEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkEntry {
    pub message_id: String,
    pub topic: String,
    pub target: String,
    pub from: String,
    pub preview: String,
    pub note: String,
    pub pinned: bool,
    /// Bookmark creation time as seconds since Unix epoch.
    pub created_at: i64,
    /// Original message time as seconds since Unix epoch (0 = unknown).
    pub message_time: i64,
}

impl BookmarkEntry {
    #[must_use]
    pub fn new(message_id: &str, topic: &str, target: &str, preview: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            topic: topic.to_owned(),
            target: target.to_owned(),
            from: String::new(),
            preview: preview.to_owned(),
            note: String::new(),
            pinned: false,
            created_at: 0,
            message_time: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// BookmarkSort
// ---------------------------------------------------------------------------

/// Sort modes for the bookmarks list, matching Go `bookmarkSort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarkSort {
    #[default]
    BookmarkedAt,
    MessageTime,
    Topic,
    Agent,
}

impl BookmarkSort {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::BookmarkedAt => Self::MessageTime,
            Self::MessageTime => Self::Topic,
            Self::Topic => Self::Agent,
            Self::Agent => Self::BookmarkedAt,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::BookmarkedAt => "bookmarked",
            Self::MessageTime => "msg-time",
            Self::Topic => "topic",
            Self::Agent => "agent",
        }
    }
}

// ---------------------------------------------------------------------------
// BookmarksFilter
// ---------------------------------------------------------------------------

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
                "{} {} {} {}",
                bookmark.preview.to_ascii_lowercase(),
                bookmark.note.to_ascii_lowercase(),
                bookmark.target.to_ascii_lowercase(),
                bookmark.from.to_ascii_lowercase(),
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

// ---------------------------------------------------------------------------
// BookmarksViewModel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarksViewModel {
    entries: Vec<BookmarkEntry>,
    selected: usize,
    filter: BookmarksFilter,
    sort_mode: BookmarkSort,

    filter_active: bool,
    filter_input: String,

    edit_active: bool,
    edit_input: String,

    status_line: String,
    status_err: bool,
}

impl Default for BookmarksViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl BookmarksViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            filter: BookmarksFilter::default(),
            sort_mode: BookmarkSort::default(),
            filter_active: false,
            filter_input: String::new(),
            edit_active: false,
            edit_input: String::new(),
            status_line: String::new(),
            status_err: false,
        }
    }

    // -- data population -----------------------------------------------------

    /// Replace all entries and re-sort. Caller populates from external state.
    pub fn set_entries(&mut self, entries: Vec<BookmarkEntry>) {
        self.entries = entries;
        self.sort_entries();
        self.clamp_selection();
    }

    pub fn add(&mut self, entry: BookmarkEntry) {
        self.entries.insert(0, entry);
        self.sort_entries();
        self.clamp_selection();
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn entries(&self) -> &[BookmarkEntry] {
        &self.entries
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn sort_mode(&self) -> BookmarkSort {
        self.sort_mode
    }

    #[must_use]
    pub fn filter_active(&self) -> bool {
        self.filter_active
    }

    #[must_use]
    pub fn edit_active(&self) -> bool {
        self.edit_active
    }

    #[must_use]
    pub fn status_line(&self) -> &str {
        &self.status_line
    }

    /// Returns the selected entry (by visible index).
    #[must_use]
    pub fn selected_entry(&self) -> Option<&BookmarkEntry> {
        let visible = self.visible_indices();
        visible
            .get(self.selected)
            .and_then(|&idx| self.entries.get(idx))
    }

    /// Returns the edit input buffer.
    #[must_use]
    pub fn edit_input(&self) -> &str {
        &self.edit_input
    }

    // -- mutations driven by app layer ---------------------------------------

    pub fn remove_selected(&mut self) {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            return;
        };
        let removed = self.entries.remove(idx);
        self.status_line = format!("deleted bookmark {}", removed.message_id);
        self.status_err = false;
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
        self.status_err = false;
    }

    /// Update the note on the currently selected entry.
    /// Called by the app layer after edit mode saves.
    pub fn update_selected_note(&mut self, note: &str) {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            return;
        };
        if let Some(entry) = self.entries.get_mut(idx) {
            entry.note = note.trim().to_owned();
            self.status_line = "note saved".to_owned();
            self.status_err = false;
        }
    }

    pub fn set_status(&mut self, msg: &str, is_err: bool) {
        self.status_line = msg.to_owned();
        self.status_err = is_err;
    }

    // -- filter --------------------------------------------------------------

    pub fn set_filter_from_input(&mut self, raw: &str) {
        self.filter = parse_bookmarks_filter(raw);
        self.selected = 0;
        self.clamp_selection();
    }

    pub fn clear_filter(&mut self) {
        self.filter = BookmarksFilter::default();
        self.filter_input.clear();
        self.selected = 0;
        self.clamp_selection();
    }

    pub fn activate_filter(&mut self) {
        self.filter_active = true;
        self.filter_input = self.filter.active_label().to_owned();
        if self.filter_input == "none" {
            self.filter_input.clear();
        }
    }

    pub fn deactivate_filter(&mut self) {
        self.filter_active = false;
    }

    pub fn apply_filter_input(&mut self) {
        self.filter = parse_bookmarks_filter(&self.filter_input);
        self.filter_active = false;
        self.selected = 0;
        self.sort_entries();
        self.clamp_selection();
    }

    pub fn filter_push_char(&mut self, ch: char) {
        self.filter_input.push(ch);
    }

    pub fn filter_pop_char(&mut self) {
        self.filter_input.pop();
    }

    // -- edit ----------------------------------------------------------------

    pub fn activate_edit(&mut self) {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            return;
        };
        if let Some(entry) = self.entries.get(idx) {
            self.edit_active = true;
            self.edit_input = entry.note.trim().to_owned();
        }
    }

    pub fn cancel_edit(&mut self) {
        self.edit_active = false;
        self.edit_input.clear();
    }

    /// Save the current edit input to the selected entry's note.
    /// Returns `true` if save succeeded (caller may persist to disk).
    pub fn save_edit(&mut self) -> bool {
        let visible = self.visible_indices();
        let Some(idx) = visible.get(self.selected).copied() else {
            self.edit_active = false;
            return false;
        };
        let note = self.edit_input.trim().to_owned();
        if let Some(entry) = self.entries.get_mut(idx) {
            entry.note = note;
            self.status_line = "note saved".to_owned();
            self.status_err = false;
        }
        self.edit_active = false;
        self.edit_input.clear();
        true
    }

    // -- sort ----------------------------------------------------------------

    pub fn cycle_sort(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.sort_entries();
        self.clamp_selection();
    }

    // -- navigation ----------------------------------------------------------

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max_idx = self.visible_indices().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max_idx);
    }

    // -- internal ------------------------------------------------------------

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

    fn sort_entries(&mut self) {
        let mode = self.sort_mode;
        self.entries.sort_by(|a, b| {
            match mode {
                BookmarkSort::BookmarkedAt => {
                    let cmp = b.created_at.cmp(&a.created_at);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                BookmarkSort::MessageTime => {
                    let cmp = b.message_time.cmp(&a.message_time);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                BookmarkSort::Topic => {
                    let cmp = a.topic.cmp(&b.topic);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                BookmarkSort::Agent => {
                    let af = a.from.trim().to_ascii_lowercase();
                    let bf = b.from.trim().to_ascii_lowercase();
                    let cmp = af.cmp(&bf);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
            }
            // Fallback: newest message_id first.
            b.message_id.cmp(&a.message_id)
        });
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Bookmarks input result: signals the app layer what action to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookmarksAction {
    /// No app-level action needed.
    None,
    /// User pressed Enter on a bookmark — app should open the thread.
    OpenBookmark { topic: String, message_id: String },
    /// User pressed Esc/Backspace — app should pop this view.
    Back,
    /// User pressed 'x' — app should export all bookmarks.
    Export,
    /// User pressed 'd' — the selected entry was removed from the view-model.
    /// App should also remove from persistent state.
    Deleted { message_id: String },
    /// Edit was saved — app should persist the note.
    NoteSaved {
        message_id: String,
        topic: String,
        note: String,
    },
}

pub fn apply_bookmarks_input(view: &mut BookmarksViewModel, event: InputEvent) -> BookmarksAction {
    // Edit mode captures all input.
    if view.edit_active {
        return apply_edit_input(view, event);
    }

    // Filter mode captures all input.
    if view.filter_active {
        apply_filter_input(view, event);
        return BookmarksAction::None;
    }

    // Normal mode.
    if let InputEvent::Key(KeyEvent { key, modifiers }) = event {
        if !modifiers.ctrl && !modifiers.alt {
            match key {
                Key::Escape | Key::Backspace => return BookmarksAction::Back,
                Key::Char('/') => {
                    view.activate_filter();
                    return BookmarksAction::None;
                }
                Key::Char('s') => {
                    view.cycle_sort();
                    return BookmarksAction::None;
                }
                Key::Enter => {
                    if let Some(entry) = view.selected_entry() {
                        return BookmarksAction::OpenBookmark {
                            topic: entry.topic.clone(),
                            message_id: entry.message_id.clone(),
                        };
                    }
                    return BookmarksAction::None;
                }
                Key::Char('e') => {
                    view.activate_edit();
                    return BookmarksAction::None;
                }
                Key::Char('d') => {
                    if let Some(entry) = view.selected_entry() {
                        let message_id = entry.message_id.clone();
                        view.remove_selected();
                        return BookmarksAction::Deleted { message_id };
                    }
                    return BookmarksAction::None;
                }
                Key::Char('x') => {
                    return BookmarksAction::Export;
                }
                Key::Char('p') => {
                    view.toggle_pin_selected();
                    return BookmarksAction::None;
                }
                _ => {}
            }
        }
    }

    match translate_input(&event) {
        UiAction::MoveUp => view.move_up(),
        UiAction::MoveDown => view.move_down(),
        _ => {}
    }
    BookmarksAction::None
}

fn apply_filter_input(view: &mut BookmarksViewModel, event: InputEvent) {
    let InputEvent::Key(key) = event else {
        return;
    };
    match key.key {
        Key::Escape => view.deactivate_filter(),
        Key::Enter => view.apply_filter_input(),
        Key::Backspace => view.filter_pop_char(),
        Key::Char(ch) => view.filter_push_char(ch),
        _ => {}
    }
}

fn apply_edit_input(view: &mut BookmarksViewModel, event: InputEvent) -> BookmarksAction {
    let InputEvent::Key(key) = event else {
        return BookmarksAction::None;
    };
    match key.key {
        Key::Escape => {
            view.cancel_edit();
        }
        Key::Enter => {
            // Capture info before save mutates state.
            let info = view
                .selected_entry()
                .map(|e| (e.message_id.clone(), e.topic.clone()));
            if view.save_edit() {
                if let Some((message_id, topic)) = info {
                    let note = view
                        .selected_entry()
                        .map(|e| e.note.clone())
                        .unwrap_or_default();
                    return BookmarksAction::NoteSaved {
                        message_id,
                        topic,
                        note,
                    };
                }
            }
        }
        Key::Backspace => {
            view.edit_input.pop();
        }
        Key::Char(ch) => {
            view.edit_input.push(ch);
        }
        _ => {}
    }
    BookmarksAction::None
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

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

    let visible = view.visible_indices();

    // Row 0: title.
    let title = format!(
        "Bookmarks ({})  sort:{}",
        visible.len(),
        view.sort_mode.label()
    );
    frame.draw_text(0, 0, &truncate(&title, width), TextRole::Accent);
    if height == 1 {
        return frame;
    }

    // Row 1: help text.
    let help = "Enter:open  e:edit  d:delete  x:export  /:filter  s:sort  p:pin  Esc:back";
    frame.draw_text(0, 1, &truncate(help, width), TextRole::Muted);
    if height == 2 {
        return frame;
    }

    // Row 2: filter line.
    let filter_line = if view.filter_active {
        format!("filter> {}_", view.filter_input)
    } else {
        let label = view.filter.active_label();
        format!("filter: {label}")
    };
    let filter_role = if view.filter_active {
        TextRole::Accent
    } else {
        TextRole::Muted
    };
    frame.draw_text(0, 2, &truncate(&filter_line, width), filter_role);
    if height == 3 {
        return frame;
    }

    // Edit prompt (drawn at bottom if active).
    let edit_rows = if view.edit_active { 2 } else { 0 };
    let status_rows: usize = if !view.status_line.trim().is_empty() {
        1
    } else {
        0
    };
    let reserved_bottom = edit_rows + status_rows;

    // Body: bookmark list.
    let body_start = 3;
    let body_h = height.saturating_sub(body_start + reserved_bottom);

    if visible.is_empty() {
        frame.draw_text(0, body_start, "No bookmarks", TextRole::Muted);
    } else {
        render_bookmark_list(&mut frame, view, &visible, body_start, body_h, width);
    }

    // Edit prompt.
    if view.edit_active {
        let edit_row = height.saturating_sub(reserved_bottom);
        frame.draw_text(
            0,
            edit_row,
            &truncate("edit note (Enter save, Esc cancel)", width),
            TextRole::Accent,
        );
        if edit_row + 1 < height {
            let prompt = format!("note> {}_", view.edit_input);
            frame.draw_text(
                0,
                edit_row + 1,
                &truncate(&prompt, width),
                TextRole::Primary,
            );
        }
    }

    // Status line (last row).
    if !view.status_line.trim().is_empty() {
        let role = if view.status_err {
            TextRole::Danger
        } else {
            TextRole::Muted
        };
        frame.draw_text(
            0,
            height - 1,
            &truncate(view.status_line.trim(), width),
            role,
        );
    }
    frame
}

fn render_bookmark_list(
    frame: &mut RenderFrame,
    view: &BookmarksViewModel,
    visible: &[usize],
    start_row: usize,
    max_rows: usize,
    width: usize,
) {
    if max_rows == 0 || visible.is_empty() {
        return;
    }

    // Viewport scrolling: keep selected near 25% from top.
    let viewport_start = view
        .selected
        .saturating_sub(max_rows / 4)
        .min(visible.len().saturating_sub(1));

    let mut row = start_row;
    for (vis_idx_offset, &entry_idx) in visible.iter().enumerate().skip(viewport_start) {
        if row >= start_row + max_rows {
            break;
        }
        let vis_idx = vis_idx_offset;
        let Some(entry) = view.entries.get(entry_idx) else {
            continue;
        };

        // Cursor.
        let cursor = if vis_idx == view.selected {
            "\u{25b8} "
        } else {
            "  "
        };

        // Title: prefer note, fall back to preview.
        let title_raw = if entry.note.trim().is_empty() {
            if entry.preview.trim().is_empty() {
                "(no note)"
            } else {
                entry.preview.trim()
            }
        } else {
            entry.note.trim()
        };
        let title = truncate(title_raw, width.saturating_sub(4).max(10));

        // From / topic / time.
        let from = if entry.from.trim().is_empty() {
            "unknown"
        } else {
            entry.from.trim()
        };
        let pin_marker = if entry.pinned { " \u{2605}" } else { "" };
        let ts = if entry.message_time > 0 {
            format_utc_hhmm(entry.message_time)
        } else {
            "-".to_owned()
        };

        let head = format!(
            "{cursor}{title}{pin_marker} \u{2014} {from} in {topic} ({ts})",
            topic = entry.topic
        );
        frame.draw_text(0, row, &truncate(&head, width), TextRole::Primary);
        row += 1;
        if row >= start_row + max_rows {
            break;
        }

        // Note sub-line (if note present and different from title).
        if !entry.note.trim().is_empty() {
            let note_line = format!(
                "  Note: {}",
                truncate(entry.note.trim(), width.saturating_sub(8))
            );
            frame.draw_text(0, row, &truncate(&note_line, width), TextRole::Muted);
            row += 1;
            if row >= start_row + max_rows {
                break;
            }
        }

        // Preview sub-line.
        if !entry.preview.trim().is_empty() {
            let preview_line = format!(
                "  {}",
                truncate(entry.preview.trim(), width.saturating_sub(4))
            );
            frame.draw_text(0, row, &truncate(&preview_line, width), TextRole::Muted);
            row += 1;
            if row >= start_row + max_rows {
                break;
            }
        }
    }
}

/// Format seconds-since-epoch as "HH:MM" in UTC.
fn format_utc_hhmm(secs: i64) -> String {
    if secs <= 0 {
        return "-".to_owned();
    }
    let s = secs % 86400;
    let h = (s / 3600) % 24;
    let m = (s % 3600) / 60;
    format!("{h:02}:{m:02}")
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Render all bookmarks as markdown for export. Caller writes to file.
#[must_use]
pub fn render_bookmarks_markdown(entries: &[BookmarkEntry], export_time_label: &str) -> String {
    let mut b = String::with_capacity(entries.len() * 256);
    b.push_str("# Bookmarks - Exported ");
    b.push_str(export_time_label);
    b.push_str("\n\n");
    for entry in entries {
        let id = entry.message_id.trim();
        let topic = entry.topic.trim();
        if id.is_empty() || topic.is_empty() {
            continue;
        }
        let title = if entry.note.trim().is_empty() {
            if entry.preview.trim().is_empty() {
                id.to_owned()
            } else {
                first_line(entry.preview.trim())
            }
        } else {
            entry.note.trim().to_owned()
        };
        b.push_str("## ");
        b.push_str(&title);
        b.push('\n');

        let from = entry.from.trim();
        if !from.is_empty() {
            b.push_str("**From:** ");
            b.push_str(from);
            b.push_str(" \u{2192} ");
            b.push_str(topic);
            if entry.message_time > 0 {
                b.push_str(" | **Time:** ");
                b.push_str(&format_utc_hhmm(entry.message_time));
            }
            b.push('\n');
        }
        let note = entry.note.trim();
        if !note.is_empty() {
            b.push_str("**Note:** ");
            b.push_str(note);
            b.push('\n');
        }
        b.push('\n');
        let body = entry.preview.trim();
        if body.is_empty() {
            b.push_str("> (empty)\n");
        } else {
            for line in body.lines() {
                b.push_str("> ");
                b.push_str(line);
                b.push('\n');
            }
        }
        b.push_str("\n---\n\n");
    }
    b
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_owned()
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
        return "\u{2026}".to_owned();
    }
    let mut out = chars.into_iter().take(max_chars - 1).collect::<String>();
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

    fn make_entry(id: &str, topic: &str, from: &str, preview: &str) -> BookmarkEntry {
        let mut e = BookmarkEntry::new(id, topic, "task", preview);
        e.from = from.to_owned();
        e
    }

    // -- filter parsing ------------------------------------------------------

    #[test]
    fn parse_filter_shape() {
        let parsed = parse_bookmarks_filter("target:task text:urgent pinned:only");
        assert_eq!(parsed.target, "task");
        assert_eq!(parsed.text, "urgent");
        assert!(parsed.pinned_only);
    }

    #[test]
    fn parse_filter_empty() {
        let parsed = parse_bookmarks_filter("");
        assert_eq!(parsed.active_label(), "none");
    }

    #[test]
    fn parse_filter_bare_text() {
        let parsed = parse_bookmarks_filter("hello world");
        assert_eq!(parsed.text, "hello world");
        assert!(parsed.target.is_empty());
    }

    // -- sort ----------------------------------------------------------------

    #[test]
    fn sort_cycle() {
        let s = BookmarkSort::default();
        assert_eq!(s, BookmarkSort::BookmarkedAt);
        assert_eq!(s.next(), BookmarkSort::MessageTime);
        assert_eq!(s.next().next(), BookmarkSort::Topic);
        assert_eq!(s.next().next().next(), BookmarkSort::Agent);
        assert_eq!(s.next().next().next().next(), BookmarkSort::BookmarkedAt);
    }

    #[test]
    fn sort_by_bookmarked_at() {
        let mut vm = BookmarksViewModel::new();
        let mut a = make_entry("m1", "task", "alice", "first");
        a.created_at = 100;
        let mut b = make_entry("m2", "task", "bob", "second");
        b.created_at = 200;
        vm.set_entries(vec![a, b]);
        // Default sort is BookmarkedAt (newest first).
        assert_eq!(vm.entries()[0].message_id, "m2");
        assert_eq!(vm.entries()[1].message_id, "m1");
    }

    #[test]
    fn sort_by_message_time() {
        let mut vm = BookmarksViewModel::new();
        vm.sort_mode = BookmarkSort::MessageTime;
        let mut a = make_entry("m1", "task", "alice", "first");
        a.message_time = 300;
        let mut b = make_entry("m2", "task", "bob", "second");
        b.message_time = 100;
        vm.set_entries(vec![a, b]);
        assert_eq!(vm.entries()[0].message_id, "m1");
        assert_eq!(vm.entries()[1].message_id, "m2");
    }

    #[test]
    fn sort_by_topic() {
        let mut vm = BookmarksViewModel::new();
        vm.sort_mode = BookmarkSort::Topic;
        vm.set_entries(vec![
            make_entry("m1", "zeta", "alice", "a"),
            make_entry("m2", "alpha", "bob", "b"),
        ]);
        assert_eq!(vm.entries()[0].topic, "alpha");
        assert_eq!(vm.entries()[1].topic, "zeta");
    }

    #[test]
    fn sort_by_agent() {
        let mut vm = BookmarksViewModel::new();
        vm.sort_mode = BookmarkSort::Agent;
        vm.set_entries(vec![
            make_entry("m1", "task", "Zoe", "a"),
            make_entry("m2", "task", "Alice", "b"),
        ]);
        assert_eq!(vm.entries()[0].from, "Alice");
        assert_eq!(vm.entries()[1].from, "Zoe");
    }

    #[test]
    fn cycle_sort_via_input() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        assert_eq!(vm.sort_mode(), BookmarkSort::BookmarkedAt);
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('s'))));
        assert_eq!(vm.sort_mode(), BookmarkSort::MessageTime);
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('s'))));
        assert_eq!(vm.sort_mode(), BookmarkSort::Topic);
    }

    // -- filter mode ---------------------------------------------------------

    #[test]
    fn filter_mode_activate_type_apply() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "refresh token"));
        vm.add(make_entry("m2", "ops", "bob", "deploy status"));

        // Activate filter.
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('/'))));
        assert!(vm.filter_active());

        // Type filter text.
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('r'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('e'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('f'))));

        // Apply with Enter.
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert!(!vm.filter_active());

        // Filter applied — only "refresh" entry visible.
        assert_eq!(vm.visible_indices().len(), 1);
    }

    #[test]
    fn filter_mode_cancel() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('/'))));
        assert!(vm.filter_active());
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert!(!vm.filter_active());
    }

    #[test]
    fn filter_backspace() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('/'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('b'))));
        assert_eq!(vm.filter_input, "ab");
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Backspace)));
        assert_eq!(vm.filter_input, "a");
    }

    // -- edit mode -----------------------------------------------------------

    #[test]
    fn edit_mode_save() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));

        // Activate edit.
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('e'))));
        assert!(vm.edit_active());

        // Type note.
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('h'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('i'))));

        // Save.
        let action = apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert!(!vm.edit_active());
        assert_eq!(vm.entries()[0].note, "hi");
        match action {
            BookmarksAction::NoteSaved { note, .. } => assert_eq!(note, "hi"),
            other => panic!("expected NoteSaved, got {other:?}"),
        }
    }

    #[test]
    fn edit_mode_cancel() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));

        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('e'))));
        assert!(vm.edit_active());
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('x'))));
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert!(!vm.edit_active());
        assert!(vm.entries()[0].note.is_empty());
    }

    // -- pin/remove ----------------------------------------------------------

    #[test]
    fn pin_remove_flow() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.add(make_entry("m2", "ops", "bob", "two"));

        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('p'))));
        assert!(vm.entries()[0].pinned);

        let action =
            apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('d'))));
        assert_eq!(vm.entries().len(), 1);
        match action {
            BookmarksAction::Deleted { message_id } => {
                assert!(!message_id.is_empty());
            }
            other => panic!("expected Deleted, got {other:?}"),
        }
    }

    // -- navigation ----------------------------------------------------------

    #[test]
    fn enter_returns_open_action() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        let action = apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        match action {
            BookmarksAction::OpenBookmark { topic, message_id } => {
                assert_eq!(topic, "task");
                assert_eq!(message_id, "m1");
            }
            other => panic!("expected OpenBookmark, got {other:?}"),
        }
    }

    #[test]
    fn esc_returns_back() {
        let mut vm = BookmarksViewModel::new();
        let action = apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert_eq!(action, BookmarksAction::Back);
    }

    #[test]
    fn export_returns_action() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        let action =
            apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('x'))));
        assert_eq!(action, BookmarksAction::Export);
    }

    #[test]
    fn move_up_down() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "a", "one"));
        vm.add(make_entry("m2", "task", "b", "two"));
        assert_eq!(vm.selected(), 0);
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Down)));
        assert_eq!(vm.selected(), 1);
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Up)));
        assert_eq!(vm.selected(), 0);
    }

    // -- render snapshots ----------------------------------------------------

    #[test]
    fn bookmarks_snapshot_basic() {
        let mut vm = BookmarksViewModel::new();
        let mut one =
            BookmarkEntry::new("20260209-120000-0001", "task", "task", "refresh token plan");
        one.note = "critical".to_owned();
        one.from = "architect".to_owned();
        one.message_time = 43200; // 12:00 UTC
        one.created_at = 100;
        vm.add(one);
        let mut two = BookmarkEntry::new("20260209-120200-0001", "@ops", "@ops", "need review");
        two.from = "reviewer".to_owned();
        two.message_time = 43320; // 12:02 UTC
        two.created_at = 200;
        vm.add(two);

        let frame = render_bookmarks_frame(&vm, 72, 10, ThemeSpec::default());
        // Verify key structural elements.
        let snap = frame.snapshot();
        assert!(snap.contains("Bookmarks (2)"));
        assert!(snap.contains("sort:bookmarked"));
        assert!(snap.contains("Enter:open"));
        assert!(snap.contains("filter: none"));
        assert!(snap.contains("\u{25b8}")); // cursor
        assert!(snap.contains("need review"));
    }

    #[test]
    fn render_empty_bookmarks() {
        let vm = BookmarksViewModel::new();
        let frame = render_bookmarks_frame(&vm, 40, 6, ThemeSpec::default());
        let snap = frame.snapshot();
        assert!(snap.contains("Bookmarks (0)"));
        assert!(snap.contains("No bookmarks"));
    }

    #[test]
    fn render_filter_active() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.activate_filter();
        vm.filter_push_char('t');
        vm.filter_push_char('e');
        let frame = render_bookmarks_frame(&vm, 50, 6, ThemeSpec::default());
        let snap = frame.snapshot();
        assert!(snap.contains("filter> te_"));
    }

    #[test]
    fn render_edit_active() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.activate_edit();
        vm.edit_input = "my note".to_owned();
        let frame = render_bookmarks_frame(&vm, 60, 10, ThemeSpec::default());
        let snap = frame.snapshot();
        assert!(snap.contains("edit note (Enter save, Esc cancel)"));
        assert!(snap.contains("note> my note_"));
    }

    // -- export markdown -----------------------------------------------------

    #[test]
    fn export_markdown_basic() {
        let mut entry = BookmarkEntry::new("m1", "task", "task", "hello world");
        entry.from = "alice".to_owned();
        entry.note = "important".to_owned();
        entry.message_time = 43200;
        let md = render_bookmarks_markdown(&[entry], "2026-02-09T12:00:00Z");
        assert!(md.contains("# Bookmarks - Exported 2026-02-09T12:00:00Z"));
        assert!(md.contains("## important"));
        assert!(md.contains("**From:** alice"));
        assert!(md.contains("**Note:** important"));
        assert!(md.contains("> hello world"));
        assert!(md.contains("---"));
    }

    #[test]
    fn export_markdown_no_note_uses_preview() {
        let mut entry = BookmarkEntry::new("m1", "task", "task", "body text here");
        entry.from = "bob".to_owned();
        let md = render_bookmarks_markdown(&[entry], "now");
        assert!(md.contains("## body text here"));
    }

    #[test]
    fn export_markdown_empty_body() {
        let entry = BookmarkEntry::new("m1", "task", "task", "");
        let md = render_bookmarks_markdown(&[entry], "now");
        assert!(md.contains("> (empty)"));
    }

    // -- format_utc_hhmm ----------------------------------------------------

    #[test]
    fn format_utc_hhmm_basic() {
        assert_eq!(format_utc_hhmm(43200), "12:00");
        assert_eq!(format_utc_hhmm(0), "-");
        assert_eq!(format_utc_hhmm(3661), "01:01");
    }

    // -- truncate ------------------------------------------------------------

    #[test]
    fn truncate_basic() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
        assert_eq!(truncate("ab", 1), "\u{2026}");
        assert_eq!(truncate("ab", 0), "");
    }

    // -- first_line ----------------------------------------------------------

    #[test]
    fn first_line_basic() {
        assert_eq!(first_line("hello\nworld"), "hello");
        assert_eq!(first_line("single"), "single");
        assert_eq!(first_line(""), "");
    }

    // -- additional vm tests -------------------------------------------------

    #[test]
    fn selected_entry_accessor() {
        let mut vm = BookmarksViewModel::new();
        assert!(vm.selected_entry().is_none());
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.add(make_entry("m2", "ops", "bob", "two"));
        let entry = vm.selected_entry();
        assert!(entry.is_some());
    }

    #[test]
    fn set_status() {
        let mut vm = BookmarksViewModel::new();
        vm.set_status("exported: /tmp/bm.md", false);
        assert_eq!(vm.status_line(), "exported: /tmp/bm.md");
        assert!(!vm.status_err);

        vm.set_status("export failed", true);
        assert_eq!(vm.status_line(), "export failed");
        assert!(vm.status_err);
    }

    #[test]
    fn update_selected_note() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.update_selected_note("new note");
        assert_eq!(vm.entries()[0].note, "new note");
        assert_eq!(vm.status_line(), "note saved");
    }

    #[test]
    fn navigation_clamped_at_bounds() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "a", "one"));
        // Already at 0, move_up should stay at 0.
        vm.move_up();
        assert_eq!(vm.selected(), 0);
        // Only 1 entry, move_down should stay at 0.
        vm.move_down();
        assert_eq!(vm.selected(), 0);
    }

    #[test]
    fn filter_matches_pinned_only() {
        let f = BookmarksFilter {
            pinned_only: true,
            ..Default::default()
        };
        let mut e = make_entry("m1", "task", "arch", "one");
        assert!(!f.matches(&e));
        e.pinned = true;
        assert!(f.matches(&e));
    }

    #[test]
    fn filter_matches_target_case_insensitive() {
        let f = BookmarksFilter {
            target: "TASK".to_owned(),
            ..Default::default()
        };
        let e = make_entry("m1", "task", "arch", "one");
        assert!(f.matches(&e));
    }

    #[test]
    fn filter_matches_from_field() {
        let f = BookmarksFilter {
            text: "arch".to_owned(),
            ..Default::default()
        };
        let e = make_entry("m1", "task", "arch", "something else");
        assert!(f.matches(&e));
    }

    #[test]
    fn export_markdown_skips_empty_id() {
        let entries = vec![
            BookmarkEntry::new("", "task", "task", "preview"),
            BookmarkEntry::new("m1", "", "", "preview"),
        ];
        let md = render_bookmarks_markdown(&entries, "ts");
        // Both entries should be skipped (empty id or empty topic).
        assert!(!md.contains("## "));
    }

    #[test]
    fn render_status_error() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        vm.set_status("export failed: disk full", true);
        let frame = render_bookmarks_frame(&vm, 60, 10, ThemeSpec::default());
        let snap = frame.snapshot();
        assert!(snap.contains("export failed: disk full"));
    }

    #[test]
    fn render_height_1_only_header() {
        let mut vm = BookmarksViewModel::new();
        vm.add(make_entry("m1", "task", "arch", "one"));
        let frame = render_bookmarks_frame(&vm, 40, 1, ThemeSpec::default());
        let snap = frame.snapshot();
        assert!(snap.contains("Bookmarks"));
        // Should not contain body content at height 1.
        assert!(!snap.contains("one"));
    }

    #[test]
    fn render_zero_width() {
        let vm = BookmarksViewModel::new();
        let frame = render_bookmarks_frame(&vm, 0, 10, ThemeSpec::default());
        assert_eq!(frame.size().width, 0);
    }

    #[test]
    fn delete_on_empty_is_noop() {
        let mut vm = BookmarksViewModel::new();
        let action =
            apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('d'))));
        assert_eq!(action, BookmarksAction::None);
        assert_eq!(vm.entries().len(), 0);
    }

    #[test]
    fn edit_on_empty_is_noop() {
        let mut vm = BookmarksViewModel::new();
        apply_bookmarks_input(&mut vm, InputEvent::Key(KeyEvent::plain(Key::Char('e'))));
        // Should not enter edit mode when there are no entries.
        assert!(!vm.edit_active());
    }
}

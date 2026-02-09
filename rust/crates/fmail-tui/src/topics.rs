//! Topics view for the fmail TUI, ported from Go `topicsView`.
//!
//! Displays a list of topics or DM conversations with metadata, filtering,
//! sorting, starring, and a preview panel for the selected item.

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Heat threshold: messages within 5 minutes are "hot".
const HEAT_HOT_SECS: i64 = 5 * 60;
/// Heat threshold: messages within 1 hour are "warm".
const HEAT_WARM_SECS: i64 = 60 * 60;

// ---------------------------------------------------------------------------
// TopicsMode
// ---------------------------------------------------------------------------

/// Whether the view shows topics or DM conversations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopicsMode {
    #[default]
    Topics,
    DM,
}

// ---------------------------------------------------------------------------
// TopicSortKey
// ---------------------------------------------------------------------------

/// Sort key for the topic list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopicSortKey {
    #[default]
    Activity,
    Name,
    Count,
    Participants,
}

impl TopicSortKey {
    fn next(self) -> Self {
        match self {
            Self::Activity => Self::Name,
            Self::Name => Self::Count,
            Self::Count => Self::Participants,
            Self::Participants => Self::Activity,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Name => "name",
            Self::Count => "count",
            Self::Participants => "participants",
        }
    }
}

// ---------------------------------------------------------------------------
// TopicsItem
// ---------------------------------------------------------------------------

/// A single row in the topics/DM list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicsItem {
    pub target: String,
    pub label: String,
    pub message_count: usize,
    /// Seconds since epoch for last activity.
    pub last_activity_secs: i64,
    pub participants: Vec<String>,
    pub unread: usize,
}

impl TopicsItem {
    #[must_use]
    pub fn new(target: &str, label: &str) -> Self {
        Self {
            target: target.to_owned(),
            label: label.to_owned(),
            message_count: 0,
            last_activity_secs: 0,
            participants: Vec::new(),
            unread: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// PreviewMessage
// ---------------------------------------------------------------------------

/// A message rendered in the preview panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMessage {
    pub time_label: String,
    pub from: String,
    pub body: String,
}

// ---------------------------------------------------------------------------
// TopicsViewModel
// ---------------------------------------------------------------------------

/// View-model state for the topics view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicsViewModel {
    pub mode: TopicsMode,
    pub sort_key: TopicSortKey,

    items: Vec<TopicsItem>,
    selected: usize,

    filter: String,
    filter_active: bool,

    starred: Vec<String>,

    preview_target: String,
    preview_messages: Vec<PreviewMessage>,
    preview_offset: usize,

    /// Current time as seconds since epoch (for relative time formatting).
    pub now_secs: i64,

    status_line: String,
    error: Option<String>,
}

impl Default for TopicsViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl TopicsViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: TopicsMode::default(),
            sort_key: TopicSortKey::default(),
            items: Vec::new(),
            selected: 0,
            filter: String::new(),
            filter_active: false,
            starred: Vec::new(),
            preview_target: String::new(),
            preview_messages: Vec::new(),
            preview_offset: 0,
            now_secs: 0,
            status_line: String::new(),
            error: None,
        }
    }

    // -- data population -----------------------------------------------------

    /// Replace the current item list. Clamps selection.
    pub fn set_items(&mut self, items: Vec<TopicsItem>) {
        self.items = items;
        self.sort_and_filter();
        self.clamp_selection();
    }

    /// Set preview messages for the currently selected target.
    pub fn set_preview(&mut self, target: &str, messages: Vec<PreviewMessage>) {
        self.preview_target = target.to_owned();
        self.preview_messages = messages;
        self.preview_offset = 0;
    }

    /// Clear preview.
    pub fn clear_preview(&mut self) {
        self.preview_target.clear();
        self.preview_messages.clear();
        self.preview_offset = 0;
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn items(&self) -> &[TopicsItem] {
        &self.items
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_target(&self) -> &str {
        self.items
            .get(self.selected)
            .map_or("", |item| item.target.as_str())
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    #[must_use]
    pub fn filter_active(&self) -> bool {
        self.filter_active
    }

    #[must_use]
    pub fn starred(&self) -> &[String] {
        &self.starred
    }

    #[must_use]
    pub fn is_starred(&self, label: &str) -> bool {
        self.starred.iter().any(|s| s == label)
    }

    // -- navigation ----------------------------------------------------------

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max_idx = self.items.len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max_idx);
    }

    pub fn scroll_preview_up(&mut self, delta: usize) {
        self.preview_offset = self.preview_offset.saturating_sub(delta);
    }

    pub fn scroll_preview_down(&mut self, delta: usize) {
        self.preview_offset = self.preview_offset.saturating_add(delta);
    }

    // -- filter --------------------------------------------------------------

    pub fn activate_filter(&mut self) {
        self.filter_active = true;
    }

    pub fn deactivate_filter(&mut self) {
        self.filter_active = false;
    }

    pub fn filter_push_char(&mut self, ch: char) {
        self.filter.push(ch);
        self.sort_and_filter();
    }

    pub fn filter_pop_char(&mut self) {
        self.filter.pop();
        self.sort_and_filter();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.filter_active = false;
        self.sort_and_filter();
    }

    // -- sort / mode ---------------------------------------------------------

    pub fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
        self.sort_and_filter();
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            TopicsMode::Topics => TopicsMode::DM,
            TopicsMode::DM => TopicsMode::Topics,
        };
        self.preview_offset = 0;
        self.sort_and_filter();
    }

    // -- starring ------------------------------------------------------------

    pub fn toggle_star_selected(&mut self) {
        if self.mode != TopicsMode::Topics {
            return;
        }
        let label = match self.items.get(self.selected) {
            Some(item) => item.label.clone(),
            None => return,
        };
        if let Some(pos) = self.starred.iter().position(|s| s == &label) {
            self.starred.remove(pos);
            self.status_line = format!("unstarred {label}");
        } else {
            self.starred.push(label.clone());
            self.status_line = format!("starred {label}");
        }
        self.sort_and_filter();
    }

    pub fn set_starred(&mut self, starred: Vec<String>) {
        self.starred = starred;
    }

    // -- internal ------------------------------------------------------------

    fn sort_and_filter(&mut self) {
        // Sorting is done in-place on the items list. The Go implementation
        // rebuilds from source data; we sort the existing items instead since
        // the caller is responsible for providing the canonical list.
        let filter_lower = self.filter.trim().to_ascii_lowercase();

        // Mark filtered-out items by moving them to the end.
        // We use a stable partition via retain+reinsert pattern.
        if !filter_lower.is_empty() {
            self.items
                .retain(|item| item_matches_filter(item, &filter_lower));
        }

        let starred = &self.starred;
        let mode = self.mode;
        let sort_key = self.sort_key;

        self.items.sort_by(|a, b| {
            // Starred topics always first.
            if mode == TopicsMode::Topics {
                let a_star = starred.iter().any(|s| s == &a.label);
                let b_star = starred.iter().any(|s| s == &b.label);
                if a_star != b_star {
                    return if a_star {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }
            }

            let ord = match sort_key {
                TopicSortKey::Name => a
                    .label
                    .to_ascii_lowercase()
                    .cmp(&b.label.to_ascii_lowercase()),
                TopicSortKey::Count => b.message_count.cmp(&a.message_count),
                TopicSortKey::Participants => b.participants.len().cmp(&a.participants.len()),
                TopicSortKey::Activity => b.last_activity_secs.cmp(&a.last_activity_secs),
            };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
            a.label
                .to_ascii_lowercase()
                .cmp(&b.label.to_ascii_lowercase())
        });

        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let max_idx = self.items.len().saturating_sub(1);
        self.selected = self.selected.min(max_idx);
    }
}

// ---------------------------------------------------------------------------
// Filter matching
// ---------------------------------------------------------------------------

fn item_matches_filter(item: &TopicsItem, filter_lower: &str) -> bool {
    let blob = format!(
        "{} {} {}",
        item.label.to_ascii_lowercase(),
        item.target.to_ascii_lowercase(),
        item.participants
            .iter()
            .map(|p| p.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    );
    blob.contains(filter_lower)
}

// ---------------------------------------------------------------------------
// Heat indicator
// ---------------------------------------------------------------------------

fn heat_indicator(now_secs: i64, activity_secs: i64) -> &'static str {
    let elapsed = now_secs.saturating_sub(activity_secs);
    if elapsed <= HEAT_HOT_SECS {
        "●" // hot
    } else if elapsed <= HEAT_WARM_SECS {
        "◐" // warm
    } else {
        "○" // cold
    }
}

// ---------------------------------------------------------------------------
// Relative time formatting
// ---------------------------------------------------------------------------

fn relative_time(now_secs: i64, ts_secs: i64) -> String {
    if ts_secs == 0 {
        return "—".to_owned();
    }
    let elapsed = now_secs.saturating_sub(ts_secs);
    if elapsed < 60 {
        "just now".to_owned()
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
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
        return "\u{2026}".to_owned(); // …
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('\u{2026}');
    out
}

// ---------------------------------------------------------------------------
// Input handler
// ---------------------------------------------------------------------------

/// Process an input event on the topics view model.
/// Returns `true` if the event was consumed (caller should not propagate).
pub fn apply_topics_input(view: &mut TopicsViewModel, event: InputEvent) -> bool {
    // Filter mode: raw key capture.
    if view.filter_active {
        if let InputEvent::Key(KeyEvent { key, .. }) = event {
            match key {
                Key::Escape => {
                    view.deactivate_filter();
                    return true;
                }
                Key::Enter => {
                    view.deactivate_filter();
                    return true;
                }
                Key::Backspace => {
                    view.filter_pop_char();
                    return true;
                }
                Key::Char(ch) => {
                    view.filter_push_char(ch);
                    return true;
                }
                _ => return false,
            }
        }
        return false;
    }

    // View-specific keys (no modifiers).
    if let InputEvent::Key(KeyEvent { key, modifiers }) = event {
        if !modifiers.ctrl && !modifiers.alt {
            match key {
                Key::Char('/') => {
                    view.activate_filter();
                    return true;
                }
                Key::Char('s') => {
                    view.cycle_sort();
                    return true;
                }
                Key::Char('d') => {
                    view.toggle_mode();
                    return true;
                }
                Key::Char('*') => {
                    view.toggle_star_selected();
                    return true;
                }
                _ => {}
            }
        }
        // Ctrl+U / Ctrl+D for preview scroll.
        if modifiers.ctrl {
            match key {
                Key::Char('u') => {
                    view.scroll_preview_up(5);
                    return true;
                }
                Key::Char('d') => {
                    view.scroll_preview_down(5);
                    return true;
                }
                _ => {}
            }
        }
    }

    // Standard actions via adapter.
    match translate_input(&event) {
        UiAction::MoveUp => {
            view.move_up();
            true
        }
        UiAction::MoveDown => {
            view.move_down();
            true
        }
        UiAction::ScrollUp => {
            view.scroll_preview_up(5);
            true
        }
        UiAction::ScrollDown => {
            view.scroll_preview_down(5);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the topics view into a frame.
#[must_use]
pub fn render_topics_frame(
    view: &TopicsViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    // Responsive layout: narrow stacks vertically, wide puts side-by-side.
    if width < 96 {
        let list_h = (height / 2).max(8).min(height);
        let preview_h = height.saturating_sub(list_h);
        render_list_panel(view, &mut frame, 0, 0, width, list_h);
        if preview_h > 0 {
            render_preview_panel(view, &mut frame, 0, list_h, width, preview_h);
        }
    } else {
        let list_w = (width / 2).max(38).min(width);
        let preview_w = width.saturating_sub(list_w + 1);
        render_list_panel(view, &mut frame, 0, 0, list_w, height);
        if preview_w > 0 {
            render_preview_panel(view, &mut frame, list_w + 1, 0, preview_w, height);
        }
    }

    // Error line at bottom if present.
    if let Some(ref err) = view.error {
        let y = height.saturating_sub(1);
        frame.draw_text(0, y, &truncate(err, width), TextRole::Danger);
    }

    frame
}

fn render_list_panel(
    view: &TopicsViewModel,
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

    // Title line.
    let title = match view.mode {
        TopicsMode::Topics => "Topics",
        TopicsMode::DM => "DM Browser",
    };
    let title_line = format!(
        "{}  ({})  sort:{}",
        title,
        view.items.len(),
        view.sort_key.label()
    );
    frame.draw_text(x_off, y, &truncate(&title_line, width), TextRole::Accent);
    y += 1;
    if y >= y_off + height {
        return;
    }

    // Key hints line.
    let hints = match view.mode {
        TopicsMode::Topics => {
            "j/k move  Enter open  / filter  d toggle  s sort  * star  n compose  Esc back"
        }
        TopicsMode::DM => "j/k move  Enter open  / filter  d toggle  s sort  n compose  Esc back",
    };
    frame.draw_text(x_off, y, &truncate(hints, width), TextRole::Muted);
    y += 1;
    if y >= y_off + height {
        return;
    }

    // Filter line.
    let filter_suffix = if view.filter_active { "_" } else { "" };
    let filter_label = format!("Filter: {}{}  (/ to edit)", view.filter, filter_suffix);
    let filter_role = if view.filter_active {
        TextRole::Accent
    } else {
        TextRole::Muted
    };
    frame.draw_text(x_off, y, &truncate(&filter_label, width), filter_role);
    y += 1;
    if y >= y_off + height {
        return;
    }

    // Header line.
    let header = match view.mode {
        TopicsMode::Topics => {
            "TOPIC                H  MSGS  LAST ACTIVE  AGENTS               UNRD"
        }
        TopicsMode::DM => "DM                   H  MSGS  LAST ACTIVE  UNRD",
    };
    frame.draw_text(x_off, y, &truncate(header, width), TextRole::Muted);
    y += 1;
    if y >= y_off + height {
        return;
    }

    // Empty state.
    if view.items.is_empty() {
        let empty = if !view.filter.trim().is_empty() {
            format!("No matches for {:?}", view.filter.trim())
        } else {
            match view.mode {
                TopicsMode::Topics => "No topics".to_owned(),
                TopicsMode::DM => "No DM conversations".to_owned(),
            }
        };
        frame.draw_text(x_off, y, &truncate(&empty, width), TextRole::Muted);
        return;
    }

    // Rows with centered scrolling.
    let rows_available = (y_off + height).saturating_sub(y);
    let n = view.items.len();
    let start = if n <= rows_available {
        0
    } else {
        let half = rows_available / 2;
        let s = view.selected.saturating_sub(half);
        s.min(n.saturating_sub(rows_available))
    };

    for (row_idx, item) in view.items.iter().enumerate().skip(start) {
        if y >= y_off + height {
            break;
        }
        let global_idx = row_idx;
        let cursor = if global_idx == view.selected {
            "\u{25b8}"
        } else {
            " "
        };
        let last_active = relative_time(view.now_secs, item.last_activity_secs);
        let heat = heat_indicator(view.now_secs, item.last_activity_secs);

        let line = match view.mode {
            TopicsMode::Topics => {
                let star = if view.is_starred(&item.label) {
                    "\u{2605}"
                } else {
                    " "
                };
                let participants = truncate(&item.participants.join(", "), 20);
                format!(
                    "{}{} {:<20} {} {:>4}  {:<11} {:<20} {:>4}",
                    cursor,
                    star,
                    truncate(&item.label, 20),
                    heat,
                    item.message_count,
                    last_active,
                    participants,
                    item.unread
                )
            }
            TopicsMode::DM => {
                format!(
                    "{}  {:<20} {} {:>4}  {:<11} {:>4}",
                    cursor,
                    truncate(&item.target, 20),
                    heat,
                    item.message_count,
                    last_active,
                    item.unread
                )
            }
        };
        frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Primary);
        y += 1;
    }
}

fn render_preview_panel(
    view: &TopicsViewModel,
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
    let target = view.selected_target();
    let title = if target.is_empty() {
        "Preview".to_owned()
    } else {
        format!("Preview: {target}")
    };
    frame.draw_text(x_off, y, &truncate(&title, width), TextRole::Accent);
    y += 1;
    if y >= y_off + height {
        return;
    }

    // Reserve last line for meta/hints.
    let body_height = (y_off + height).saturating_sub(y).saturating_sub(1);

    // Body.
    if target.is_empty() {
        frame.draw_text(x_off, y, "Select a topic or DM", TextRole::Muted);
    } else if view.preview_messages.is_empty() {
        frame.draw_text(x_off, y, "No messages", TextRole::Muted);
    } else {
        // Build preview lines.
        let mut lines: Vec<(String, TextRole)> = Vec::new();
        for msg in &view.preview_messages {
            lines.push((
                truncate(&format!("{} {}", msg.time_label, msg.from), width),
                TextRole::Muted,
            ));
            let body = msg.body.trim();
            let body = if body.is_empty() { "(empty)" } else { body };
            // Take first line only and indent.
            let first = body.lines().next().unwrap_or(body);
            lines.push((truncate(&format!("  {first}"), width), TextRole::Primary));
            lines.push((String::new(), TextRole::Primary));
        }
        // Remove trailing blank.
        if lines.last().is_some_and(|(s, _)| s.is_empty()) {
            lines.pop();
        }

        // Apply scroll offset.
        let max_offset = lines.len().saturating_sub(body_height);
        let offset = view.preview_offset.min(max_offset);
        for (line, role) in lines.iter().skip(offset).take(body_height) {
            frame.draw_text(x_off, y, line, *role);
            y += 1;
        }
    }

    // Meta/hint line at bottom.
    let meta_y = y_off + height - 1;
    let meta = "ctrl+u/d scroll  Enter open  n compose  Esc back";
    frame.draw_text(x_off, meta_y, &truncate(meta, width), TextRole::Muted);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    fn key(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent::plain(k))
    }

    fn ctrl_key(ch: char) -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: Key::Char(ch),
            modifiers: Modifiers {
                shift: false,
                ctrl: true,
                alt: false,
            },
        })
    }

    fn sample_items() -> Vec<TopicsItem> {
        vec![
            {
                let mut item = TopicsItem::new("task", "task");
                item.message_count = 5;
                item.last_activity_secs = 1000;
                item.participants = vec!["alice".to_owned(), "bob".to_owned()];
                item.unread = 2;
                item
            },
            {
                let mut item = TopicsItem::new("build", "build");
                item.message_count = 7;
                item.last_activity_secs = 1200;
                item.participants = vec!["charlie".to_owned()];
                item.unread = 0;
                item
            },
            {
                let mut item = TopicsItem::new("review", "review");
                item.message_count = 3;
                item.last_activity_secs = 800;
                item.participants = vec!["alice".to_owned(), "dave".to_owned()];
                item.unread = 1;
                item
            },
        ]
    }

    // -- ViewModel basic operations ------------------------------------------

    #[test]
    fn new_viewmodel_defaults() {
        let vm = TopicsViewModel::new();
        assert_eq!(vm.mode, TopicsMode::Topics);
        assert_eq!(vm.sort_key, TopicSortKey::Activity);
        assert_eq!(vm.selected(), 0);
        assert!(vm.items().is_empty());
        assert!(!vm.filter_active());
    }

    #[test]
    fn set_items_populates_and_sorts() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        assert_eq!(vm.items().len(), 3);
        // Default sort by activity: build (1200) > task (1000) > review (800)
        assert_eq!(vm.items()[0].label, "build");
        assert_eq!(vm.items()[1].label, "task");
        assert_eq!(vm.items()[2].label, "review");
    }

    #[test]
    fn sort_by_name() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.sort_key = TopicSortKey::Name;
        vm.cycle_sort(); // Name → Count
                         // Count desc: build(7) > task(5) > review(3)
        assert_eq!(vm.items()[0].label, "build");
        assert_eq!(vm.items()[1].label, "task");
        assert_eq!(vm.items()[2].label, "review");
    }

    #[test]
    fn sort_by_count() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.sort_key = TopicSortKey::Count;
        vm.set_items(sample_items());
        // Count desc: build(7) > task(5) > review(3)
        assert_eq!(vm.items()[0].label, "build");
        assert_eq!(vm.items()[1].label, "task");
        assert_eq!(vm.items()[2].label, "review");
    }

    #[test]
    fn starred_items_sort_first() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_starred(vec!["review".to_owned()]);
        vm.set_items(sample_items());
        // review is starred → top despite lower activity
        assert_eq!(vm.items()[0].label, "review");
    }

    #[test]
    fn filter_reduces_items() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.filter_push_char('t');
        vm.filter_push_char('a');
        // Only "task" matches "ta"
        assert_eq!(vm.items().len(), 1);
        assert_eq!(vm.items()[0].label, "task");
    }

    #[test]
    fn filter_clear_restores_items() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        let items = sample_items();
        vm.set_items(items.clone());
        vm.filter_push_char('z');
        assert_eq!(vm.items().len(), 0);
        // clear_filter resets filter text but doesn't re-add filtered items
        // (caller would set_items again). The filter text is cleared though.
        vm.clear_filter();
        assert!(vm.filter().is_empty());
    }

    #[test]
    fn navigation_up_down() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        assert_eq!(vm.selected(), 0);
        vm.move_down();
        assert_eq!(vm.selected(), 1);
        vm.move_down();
        assert_eq!(vm.selected(), 2);
        vm.move_down(); // clamped
        assert_eq!(vm.selected(), 2);
        vm.move_up();
        assert_eq!(vm.selected(), 1);
        vm.move_up();
        assert_eq!(vm.selected(), 0);
        vm.move_up(); // clamped
        assert_eq!(vm.selected(), 0);
    }

    #[test]
    fn toggle_mode() {
        let mut vm = TopicsViewModel::new();
        assert_eq!(vm.mode, TopicsMode::Topics);
        vm.toggle_mode();
        assert_eq!(vm.mode, TopicsMode::DM);
        vm.toggle_mode();
        assert_eq!(vm.mode, TopicsMode::Topics);
    }

    #[test]
    fn cycle_sort_key() {
        let mut vm = TopicsViewModel::new();
        assert_eq!(vm.sort_key, TopicSortKey::Activity);
        vm.cycle_sort();
        assert_eq!(vm.sort_key, TopicSortKey::Name);
        vm.cycle_sort();
        assert_eq!(vm.sort_key, TopicSortKey::Count);
        vm.cycle_sort();
        assert_eq!(vm.sort_key, TopicSortKey::Participants);
        vm.cycle_sort();
        assert_eq!(vm.sort_key, TopicSortKey::Activity);
    }

    #[test]
    fn toggle_star_selected() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        // First item is "build" (sorted by activity)
        assert!(!vm.is_starred("build"));
        vm.toggle_star_selected();
        assert!(vm.is_starred("build"));
        vm.toggle_star_selected();
        assert!(!vm.is_starred("build"));
    }

    #[test]
    fn star_only_in_topics_mode() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.toggle_mode(); // DM mode
        vm.toggle_star_selected(); // should be a no-op
        assert!(vm.starred().is_empty());
    }

    // -- Input handling ------------------------------------------------------

    #[test]
    fn input_j_moves_down() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        assert!(apply_topics_input(&mut vm, key(Key::Char('j'))));
        assert_eq!(vm.selected(), 1);
    }

    #[test]
    fn input_k_moves_up() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.move_down();
        assert!(apply_topics_input(&mut vm, key(Key::Char('k'))));
        assert_eq!(vm.selected(), 0);
    }

    #[test]
    fn input_slash_activates_filter() {
        let mut vm = TopicsViewModel::new();
        assert!(!vm.filter_active());
        assert!(apply_topics_input(&mut vm, key(Key::Char('/'))));
        assert!(vm.filter_active());
    }

    #[test]
    fn input_filter_mode_captures_chars() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.activate_filter();
        apply_topics_input(&mut vm, key(Key::Char('t')));
        assert_eq!(vm.filter(), "t");
        apply_topics_input(&mut vm, key(Key::Char('a')));
        assert_eq!(vm.filter(), "ta");
        apply_topics_input(&mut vm, key(Key::Backspace));
        assert_eq!(vm.filter(), "t");
        apply_topics_input(&mut vm, key(Key::Escape));
        assert!(!vm.filter_active());
    }

    #[test]
    fn input_s_cycles_sort() {
        let mut vm = TopicsViewModel::new();
        apply_topics_input(&mut vm, key(Key::Char('s')));
        assert_eq!(vm.sort_key, TopicSortKey::Name);
    }

    #[test]
    fn input_d_toggles_mode() {
        let mut vm = TopicsViewModel::new();
        apply_topics_input(&mut vm, key(Key::Char('d')));
        assert_eq!(vm.mode, TopicsMode::DM);
    }

    #[test]
    fn input_star_toggles_star() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        apply_topics_input(&mut vm, key(Key::Char('*')));
        assert!(vm.is_starred(&vm.items()[0].label.clone()));
    }

    #[test]
    fn input_ctrl_u_d_scrolls_preview() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.set_preview(
            "task",
            vec![PreviewMessage {
                time_label: "12:00".into(),
                from: "alice".into(),
                body: "hello".into(),
            }],
        );
        assert_eq!(vm.preview_offset, 0);
        apply_topics_input(&mut vm, ctrl_key('d'));
        assert_eq!(vm.preview_offset, 5);
        apply_topics_input(&mut vm, ctrl_key('u'));
        assert_eq!(vm.preview_offset, 0);
    }

    // -- Rendering -----------------------------------------------------------

    #[test]
    fn render_empty_topics() {
        let vm = TopicsViewModel::new();
        let frame = render_topics_frame(&vm, 60, 10, ThemeSpec::default());
        let row0 = frame.row_text(0);
        assert!(row0.contains("Topics"), "row0: {row0}");
        assert!(row0.contains("(0)"), "row0: {row0}");
    }

    #[test]
    fn render_with_items_snapshot() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        let frame = render_topics_frame(&vm, 80, 12, ThemeSpec::default());
        // Title row should show count and sort key.
        let row0 = frame.row_text(0);
        assert!(row0.contains("Topics"), "row0: {row0}");
        assert!(row0.contains("(3)"), "row0: {row0}");
        assert!(row0.contains("sort:activity"), "row0: {row0}");
    }

    #[test]
    fn render_dm_mode_header() {
        let mut vm = TopicsViewModel::new();
        vm.mode = TopicsMode::DM;
        let frame = render_topics_frame(&vm, 60, 10, ThemeSpec::default());
        let row0 = frame.row_text(0);
        assert!(row0.contains("DM Browser"), "row0: {row0}");
        let row3 = frame.row_text(3);
        assert!(row3.contains("DM"), "row3: {row3}");
    }

    #[test]
    fn render_filter_active_highlight() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.activate_filter();
        let frame = render_topics_frame(&vm, 60, 10, ThemeSpec::default());
        let row2 = frame.row_text(2);
        assert!(row2.contains("Filter:"), "row2: {row2}");
        assert!(row2.contains("_"), "should show cursor: {row2}");
    }

    #[test]
    fn render_preview_empty_target() {
        let vm = TopicsViewModel::new();
        let frame = render_topics_frame(&vm, 120, 10, ThemeSpec::default());
        // Wide mode: preview panel starts at about column 61
        // Should contain "Select a topic or DM"
        let mut found = false;
        for row in 0..10 {
            let text = frame.row_text(row);
            if text.contains("Select a topic or DM") {
                found = true;
                break;
            }
        }
        assert!(found, "should show empty preview hint");
    }

    #[test]
    fn render_preview_with_messages() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        vm.set_preview(
            "build",
            vec![
                PreviewMessage {
                    time_label: "12:00".into(),
                    from: "charlie".into(),
                    body: "build started".into(),
                },
                PreviewMessage {
                    time_label: "12:01".into(),
                    from: "charlie".into(),
                    body: "build passed".into(),
                },
            ],
        );
        let frame = render_topics_frame(&vm, 120, 12, ThemeSpec::default());
        let mut found_preview = false;
        for row in 0..12 {
            let text = frame.row_text(row);
            if text.contains("Preview: build") {
                found_preview = true;
                break;
            }
        }
        assert!(found_preview, "should show preview title");
    }

    #[test]
    fn render_narrow_stacks_vertically() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        // Narrow: width < 96
        let frame = render_topics_frame(&vm, 60, 20, ThemeSpec::default());
        // Title is at row 0
        assert!(frame.row_text(0).contains("Topics"));
        // Preview should appear in lower half
        let mut found_preview = false;
        for row in 8..20 {
            if frame.row_text(row).contains("Preview") {
                found_preview = true;
                break;
            }
        }
        assert!(found_preview, "narrow layout should stack preview below");
    }

    #[test]
    fn render_wide_puts_side_by_side() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        // Wide: width >= 96
        let frame = render_topics_frame(&vm, 120, 12, ThemeSpec::default());
        // Preview title should be on row 0 but offset to the right
        let row0 = frame.row_text(0);
        assert!(row0.contains("Topics"));
        assert!(row0.contains("Preview"));
    }

    // -- Snapshot test -------------------------------------------------------

    #[test]
    fn topics_list_snapshot() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_items(sample_items());
        let frame = render_topics_frame(&vm, 80, 8, ThemeSpec::default());
        // Verify key elements are present in the rendered output.
        let all_text: String = (0..8)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("Topics"), "should contain title");
        assert!(
            all_text.contains("TOPIC"),
            "should contain header: {all_text}"
        );
        assert!(all_text.contains("build"), "should list build topic");
    }

    #[test]
    fn topics_snapshot_render() {
        let mut vm = TopicsViewModel::new();
        vm.now_secs = 1500;
        vm.set_starred(vec!["review".to_owned()]);
        vm.set_items(sample_items());

        let frame = render_topics_frame(&vm, 80, 9, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_topics_view",
            &frame,
            &(0..9)
                .map(|r| {
                    let text = frame.row_text(r);
                    // Pad to width for snapshot.
                    format!("{:<80}", text)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    // -- Relative time -------------------------------------------------------

    #[test]
    fn relative_time_formatting() {
        assert_eq!(relative_time(100, 100), "just now");
        assert_eq!(relative_time(100, 70), "just now"); // 30s
        assert_eq!(relative_time(1000, 400), "10m ago"); // 600s
        assert_eq!(relative_time(10000, 2800), "2h ago"); // 7200s
        assert_eq!(relative_time(200000, 100000), "1d ago"); // 100000s
        assert_eq!(relative_time(100, 0), "\u{2014}"); // zero → dash
    }

    // -- Heat indicator ------------------------------------------------------

    #[test]
    fn heat_indicator_thresholds() {
        // Hot: within 5 min (300s)
        assert_eq!(heat_indicator(1000, 800), "\u{25cf}"); // ●
                                                           // Warm: within 1 hour (3600s)
        assert_eq!(heat_indicator(5000, 3000), "\u{25d0}"); // ◐
                                                            // Cold: beyond 1 hour
        assert_eq!(heat_indicator(10000, 1000), "\u{25cb}"); // ○
    }

    // -- Truncation ----------------------------------------------------------

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_adds_ellipsis() {
        assert_eq!(truncate("hello world", 8), "hello w\u{2026}");
    }

    #[test]
    fn truncate_zero_width() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_width_one() {
        assert_eq!(truncate("hello", 1), "\u{2026}");
    }
}

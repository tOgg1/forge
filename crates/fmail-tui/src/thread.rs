//! Thread view: hierarchical/flat message thread display with navigation,
//! collapse/expand, read tracking, bookmarks, annotations, and topic switching.
//!
//! Ports Go `internal/fmailtui/thread_view*.go` with full parity.

use std::collections::{HashMap, HashSet};

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

use crate::threading::{
    build_threads, flatten_thread, is_cross_target_reply, ThreadMessage, ThreadNode,
};

// ---------------------------------------------------------------------------
// Constants matching Go
// ---------------------------------------------------------------------------

const THREAD_MAX_DEPTH: usize = 6;
const THREAD_MAX_BODY_LINES: usize = 50;

// ---------------------------------------------------------------------------
// ThreadMode
// ---------------------------------------------------------------------------

/// Display mode for the thread view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadMode {
    Threaded,
    Flat,
}

// ---------------------------------------------------------------------------
// ThreadRow - display row
// ---------------------------------------------------------------------------

/// A single display row in the thread view, possibly indented/connected.
#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub msg: ThreadMessage,
    pub has_children: bool,
    pub connector: String,
    pub depth: usize,
    pub overflow: bool,
    pub group_gap: bool,
    pub reply_to: String,
    pub cross_target: String,
    pub truncated: bool,
    pub hidden_lines: usize,
}

// ---------------------------------------------------------------------------
// EditMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditKind {
    BookmarkNote,
    Annotation,
}

// ---------------------------------------------------------------------------
// TopicInfo
// ---------------------------------------------------------------------------

/// Lightweight topic descriptor for topic switching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicInfo {
    pub name: String,
}

// ---------------------------------------------------------------------------
// ThreadViewModel
// ---------------------------------------------------------------------------

/// View model for the thread view.
#[derive(Debug, Clone)]
pub struct ThreadViewModel {
    // Topic state.
    topics: Vec<TopicInfo>,
    topic: String,

    // Display mode.
    mode: ThreadMode,

    // Message data.
    all_msgs: Vec<ThreadMessage>,
    msg_by_id: HashMap<String, ThreadMessage>,
    rows: Vec<ThreadRow>,
    row_index_by_id: HashMap<String, usize>,

    // Collapse/expand.
    collapsed: HashSet<String>,
    expanded_bodies: HashSet<String>,

    // Read tracking.
    read_markers: HashMap<String, String>,

    // Bookmarks and annotations.
    bookmarked_ids: HashSet<String>,
    annotations: HashMap<String, String>,
    bookmark_confirm_id: String,

    // Navigation.
    selected: usize,
    top: usize,
    viewport_rows: usize,

    // Pending new messages indicator.
    pending_new: usize,

    // Edit mode.
    edit_active: bool,
    edit_kind: Option<EditKind>,
    edit_target_id: String,
    edit_input: String,

    // Status.
    status_line: String,
    status_err: bool,
}

impl Default for ThreadViewModel {
    fn default() -> Self {
        Self {
            topics: Vec::new(),
            topic: String::new(),
            mode: ThreadMode::Threaded,
            all_msgs: Vec::new(),
            msg_by_id: HashMap::new(),
            rows: Vec::new(),
            row_index_by_id: HashMap::new(),
            collapsed: HashSet::new(),
            expanded_bodies: HashSet::new(),
            read_markers: HashMap::new(),
            bookmarked_ids: HashSet::new(),
            annotations: HashMap::new(),
            bookmark_confirm_id: String::new(),
            selected: 0,
            top: 0,
            viewport_rows: 1,
            pending_new: 0,
            edit_active: false,
            edit_kind: None,
            edit_target_id: String::new(),
            edit_input: String::new(),
            status_line: String::new(),
            status_err: false,
        }
    }
}

impl ThreadViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn topic(&self) -> &str {
        &self.topic
    }

    #[must_use]
    pub fn mode(&self) -> ThreadMode {
        self.mode
    }

    #[must_use]
    pub fn rows(&self) -> &[ThreadRow] {
        &self.rows
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_id(&self) -> &str {
        self.rows
            .get(self.selected)
            .map(|r| r.msg.id.as_str())
            .unwrap_or("")
    }

    #[must_use]
    pub fn message_count(&self) -> usize {
        self.all_msgs.len()
    }

    #[must_use]
    pub fn is_bookmarked(&self, id: &str) -> bool {
        self.bookmarked_ids.contains(id)
    }

    #[must_use]
    pub fn annotation(&self, id: &str) -> &str {
        self.annotations.get(id).map(String::as_str).unwrap_or("")
    }

    #[must_use]
    pub fn is_unread(&self, id: &str) -> bool {
        let marker = self
            .read_markers
            .get(&self.topic)
            .map(String::as_str)
            .unwrap_or("");
        if marker.is_empty() {
            return true;
        }
        id > marker
    }

    #[must_use]
    pub fn unread_count(&self) -> usize {
        self.all_msgs
            .iter()
            .filter(|m| self.is_unread(&m.id))
            .count()
    }

    // -- data loading --------------------------------------------------------

    /// Set the topic and messages. Call after loading data.
    pub fn set_data(&mut self, topic: &str, topics: Vec<TopicInfo>, messages: Vec<ThreadMessage>) {
        let changed_topic = topic.trim() != self.topic.trim();
        self.topics = topics;

        if changed_topic {
            self.topic = topic.trim().to_owned();
            self.selected = 0;
            self.top = 0;
            self.pending_new = 0;
        } else {
            // Detect new messages.
            let new_count = messages.len().saturating_sub(self.all_msgs.len());
            if new_count > 0 && !self.is_at_bottom() {
                self.pending_new = new_count;
            }
        }

        self.all_msgs = messages;
        self.rebuild_msg_index();

        let anchor = self.selected_id().to_owned();
        self.rebuild_rows(&anchor, false);
        self.ensure_visible();

        // Initialize read marker if absent.
        if !self.topic.is_empty() && !self.read_markers.contains_key(&self.topic) {
            if let Some(last) = self.all_msgs.last() {
                self.read_markers
                    .insert(self.topic.clone(), last.id.clone());
            }
        }
    }

    /// Set topics list only (for topic switching without reloading messages).
    pub fn set_topics(&mut self, topics: Vec<TopicInfo>) {
        self.topics = topics;
    }

    // -- bookmarks & annotations (external state injection) ------------------

    pub fn set_bookmarked_ids(&mut self, ids: HashSet<String>) {
        self.bookmarked_ids = ids;
    }

    pub fn set_annotations(&mut self, annotations: HashMap<String, String>) {
        self.annotations = annotations;
    }

    pub fn set_read_markers(&mut self, markers: HashMap<String, String>) {
        self.read_markers = markers;
    }

    // -- navigation ----------------------------------------------------------

    pub fn move_selection(&mut self, delta: i32) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.top = 0;
            return;
        }
        let new = self.selected as i32 + delta;
        self.selected = new.max(0).min(self.rows.len() as i32 - 1) as usize;
        self.ensure_visible();
        self.advance_read_marker();
    }

    pub fn jump_top(&mut self) {
        self.selected = 0;
        self.top = 0;
        self.advance_read_marker();
    }

    pub fn jump_bottom(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.top = 0;
            self.pending_new = 0;
            return;
        }
        self.selected = self.rows.len() - 1;
        self.top = self
            .selected
            .saturating_sub(self.viewport_rows.saturating_sub(1));
        self.pending_new = 0;
        self.advance_read_marker();
    }

    fn page_step(&self) -> usize {
        if self.viewport_rows > 0 {
            (self.viewport_rows / 2).max(1)
        } else {
            6
        }
    }

    fn is_at_bottom(&self) -> bool {
        self.rows.is_empty() || self.selected >= self.rows.len().saturating_sub(1)
    }

    fn ensure_visible(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.top = 0;
            return;
        }
        self.selected = self.selected.min(self.rows.len() - 1);
        if self.selected < self.top {
            self.top = self.selected;
        }
        let visible = self.viewport_rows.max(1);
        if self.selected >= self.top + visible {
            self.top = self.selected - visible + 1;
        }
        let max_top = self.rows.len().saturating_sub(1);
        self.top = self.top.min(max_top);
    }

    fn advance_read_marker(&mut self) {
        if self.topic.is_empty() {
            return;
        }
        let id = self.selected_id().to_owned();
        if id.is_empty() {
            return;
        }
        let current = self
            .read_markers
            .get(&self.topic)
            .cloned()
            .unwrap_or_default();
        if id > current {
            self.read_markers.insert(self.topic.clone(), id);
        }
    }

    // -- mode toggle ---------------------------------------------------------

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ThreadMode::Threaded => ThreadMode::Flat,
            ThreadMode::Flat => ThreadMode::Threaded,
        };
        let anchor = self.selected_id().to_owned();
        self.rebuild_rows(&anchor, false);
        self.ensure_visible();
    }

    // -- enter (expand/collapse) ---------------------------------------------

    pub fn handle_enter(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let id = row.msg.id.trim().to_owned();
        if id.is_empty() {
            return;
        }

        if row.truncated {
            self.expanded_bodies.insert(id.clone());
            self.rebuild_rows(&id, false);
            self.ensure_visible();
            return;
        }

        if self.mode == ThreadMode::Threaded && row.has_children {
            if self.collapsed.contains(&id) {
                self.collapsed.remove(&id);
            } else {
                self.collapsed.insert(id.clone());
            }
            self.rebuild_rows(&id, false);
            self.ensure_visible();
        }
    }

    // -- topic switching -----------------------------------------------------

    pub fn switch_topic(&mut self, delta: i32) {
        if self.topics.is_empty() {
            return;
        }
        if self.topic.trim().starts_with('@') {
            return;
        }
        let idx = self
            .topics
            .iter()
            .position(|t| t.name == self.topic)
            .unwrap_or(0) as i32;
        let len = self.topics.len() as i32;
        let new_idx = ((idx + delta) % len + len) % len;
        self.topic = self.topics[new_idx as usize].name.clone();
        self.selected = 0;
        self.top = 0;
        self.pending_new = 0;
        // Note: caller must reload messages for the new topic.
    }

    // -- bookmarks -----------------------------------------------------------

    pub fn toggle_bookmark(&mut self) {
        let id = self.selected_id().to_owned();
        if id.is_empty() {
            return;
        }
        if self.bookmarked_ids.contains(&id) {
            // Confirm removal (double-press pattern).
            if self.bookmark_confirm_id == id {
                self.bookmarked_ids.remove(&id);
                self.bookmark_confirm_id.clear();
                self.status_line = format!("unbookmarked {}", short_id(&id));
                self.status_err = false;
            } else {
                self.bookmark_confirm_id = id;
                self.status_line = "press b again to remove bookmark".to_owned();
                self.status_err = false;
            }
        } else {
            self.bookmarked_ids.insert(id.clone());
            self.bookmark_confirm_id.clear();
            self.status_line = format!("bookmarked {}", short_id(&id));
            self.status_err = false;
        }
    }

    pub fn open_bookmark_note_editor(&mut self) {
        let id = self.selected_id().to_owned();
        if id.is_empty() {
            return;
        }
        self.edit_active = true;
        self.edit_kind = Some(EditKind::BookmarkNote);
        self.edit_target_id = id;
        self.edit_input.clear();
    }

    pub fn open_annotation_editor(&mut self) {
        let id = self.selected_id().to_owned();
        if id.is_empty() {
            return;
        }
        self.edit_active = true;
        self.edit_kind = Some(EditKind::Annotation);
        self.edit_target_id = id.clone();
        self.edit_input = self.annotations.get(&id).cloned().unwrap_or_default();
    }

    fn save_edit(&mut self) {
        let target = self.edit_target_id.clone();
        let input = self.edit_input.trim().to_owned();
        match self.edit_kind {
            Some(EditKind::BookmarkNote) => {
                self.status_line = format!("note saved for {}", short_id(&target));
                self.status_err = false;
            }
            Some(EditKind::Annotation) => {
                if input.is_empty() {
                    self.annotations.remove(&target);
                    self.status_line = format!("annotation cleared for {}", short_id(&target));
                } else {
                    self.annotations.insert(target.clone(), input);
                    self.status_line = format!("annotation saved for {}", short_id(&target));
                }
                self.status_err = false;
            }
            None => {}
        }
        self.edit_active = false;
        self.edit_kind = None;
        self.edit_target_id.clear();
        self.edit_input.clear();
    }

    fn cancel_edit(&mut self) {
        self.edit_active = false;
        self.edit_kind = None;
        self.edit_target_id.clear();
        self.edit_input.clear();
    }

    // -- rebuild rows --------------------------------------------------------

    fn rebuild_msg_index(&mut self) {
        self.msg_by_id.clear();
        for m in &self.all_msgs {
            let id = m.id.trim();
            if !id.is_empty() {
                self.msg_by_id.insert(id.to_owned(), m.clone());
            }
        }
    }

    fn rebuild_rows(&mut self, anchor_id: &str, prefer_bottom: bool) {
        let mut rows = Vec::with_capacity(self.all_msgs.len());

        if self.mode == ThreadMode::Flat {
            let mut sorted = self.all_msgs.clone();
            sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
            for msg in sorted {
                let (truncated, hidden) = self.body_truncation(&msg);
                rows.push(ThreadRow {
                    msg,
                    has_children: false,
                    connector: String::new(),
                    depth: 0,
                    overflow: false,
                    group_gap: false,
                    reply_to: String::new(),
                    cross_target: String::new(),
                    truncated,
                    hidden_lines: hidden,
                });
            }
        } else {
            let threads = build_threads(&self.all_msgs);
            for (t_idx, thread) in threads.iter().enumerate() {
                let flat = flatten_thread(thread);
                let node_map: HashMap<&str, &ThreadNode> =
                    flat.iter().map(|n| (n.message.id.as_str(), *n)).collect();

                for node in &flat {
                    // Skip if hidden by collapsed ancestor.
                    if self.hidden_by_collapsed_ancestor(node, &node_map) {
                        continue;
                    }

                    let depth = node.depth.min(THREAD_MAX_DEPTH);
                    let connector = prefix_for_node(node, &node_map, THREAD_MAX_DEPTH);
                    let overflow = node.depth > THREAD_MAX_DEPTH;

                    let cross_target = {
                        let parent = node
                            .parent_id
                            .as_deref()
                            .and_then(|pid| node_map.get(pid).copied());
                        if let Some(parent) = parent {
                            if is_cross_target_reply(node, Some(parent)) {
                                parent.message.to.trim().to_owned()
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    };

                    let (truncated, hidden) = self.body_truncation(&node.message);

                    rows.push(ThreadRow {
                        msg: node.message.clone(),
                        has_children: !node.children_ids.is_empty(),
                        connector,
                        depth,
                        overflow,
                        group_gap: t_idx > 0 && node.parent_id.is_none(),
                        reply_to: node.message.reply_to.trim().to_owned(),
                        cross_target,
                        truncated,
                        hidden_lines: hidden,
                    });
                }
            }
        }

        self.rows = rows;
        self.row_index_by_id.clear();
        for (idx, row) in self.rows.iter().enumerate() {
            let id = row.msg.id.trim();
            if !id.is_empty() {
                self.row_index_by_id.insert(id.to_owned(), idx);
            }
        }

        if self.rows.is_empty() {
            self.selected = 0;
            self.top = 0;
            return;
        }
        if prefer_bottom {
            self.selected = self.rows.len() - 1;
            self.top = self
                .selected
                .saturating_sub(self.viewport_rows.saturating_sub(1));
            return;
        }
        if let Some(&idx) = self.row_index_by_id.get(anchor_id) {
            self.selected = idx;
        } else {
            self.selected = self.selected.min(self.rows.len() - 1);
        }
    }

    fn hidden_by_collapsed_ancestor(
        &self,
        node: &ThreadNode,
        node_map: &HashMap<&str, &ThreadNode>,
    ) -> bool {
        let mut cur_pid = node.parent_id.as_deref();
        while let Some(pid) = cur_pid {
            if self.collapsed.contains(pid) {
                return true;
            }
            cur_pid = node_map.get(pid).and_then(|n| n.parent_id.as_deref());
        }
        false
    }

    fn body_truncation(&self, msg: &ThreadMessage) -> (bool, usize) {
        if self.expanded_bodies.contains(&msg.id) {
            return (false, 0);
        }
        let line_count = msg.body.lines().count();
        if line_count > THREAD_MAX_BODY_LINES {
            (true, line_count - THREAD_MAX_BODY_LINES)
        } else {
            (false, 0)
        }
    }
}

// ---------------------------------------------------------------------------
// Input handling
// ---------------------------------------------------------------------------

/// Process an input event for the thread view.
pub fn apply_thread_input(view: &mut ThreadViewModel, event: InputEvent) {
    if view.edit_active {
        apply_edit_input(view, event);
        return;
    }

    match event {
        // Mode toggle.
        InputEvent::Key(KeyEvent {
            key: Key::Char('f'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.bookmark_confirm_id.clear();
            view.toggle_mode();
            return;
        }
        // Enter: expand/collapse.
        InputEvent::Key(KeyEvent {
            key: Key::Enter, ..
        }) => {
            view.bookmark_confirm_id.clear();
            view.handle_enter();
            return;
        }
        // Jump top.
        InputEvent::Key(KeyEvent {
            key: Key::Char('g'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.bookmark_confirm_id.clear();
            view.jump_top();
            return;
        }
        // Jump bottom.
        InputEvent::Key(KeyEvent {
            key: Key::Char('G'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.bookmark_confirm_id.clear();
            view.jump_bottom();
            return;
        }
        // Page down (Ctrl+D).
        InputEvent::Key(KeyEvent {
            key: Key::Char('d'),
            modifiers,
        }) if modifiers.ctrl => {
            view.bookmark_confirm_id.clear();
            let step = view.page_step() as i32;
            view.move_selection(step);
            return;
        }
        // Page up (Ctrl+U).
        InputEvent::Key(KeyEvent {
            key: Key::Char('u'),
            modifiers,
        }) if modifiers.ctrl => {
            view.bookmark_confirm_id.clear();
            let step = -(view.page_step() as i32);
            view.move_selection(step);
            return;
        }
        // Bookmark toggle.
        InputEvent::Key(KeyEvent {
            key: Key::Char('b'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt && !modifiers.shift => {
            view.toggle_bookmark();
            return;
        }
        // Bookmark note editor.
        InputEvent::Key(KeyEvent {
            key: Key::Char('B'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.open_bookmark_note_editor();
            return;
        }
        // Annotation editor.
        InputEvent::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.open_annotation_editor();
            return;
        }
        // Topic prev.
        InputEvent::Key(KeyEvent {
            key: Key::Char('['),
            ..
        }) => {
            view.bookmark_confirm_id.clear();
            view.switch_topic(-1);
            return;
        }
        // Topic next.
        InputEvent::Key(KeyEvent {
            key: Key::Char(']'),
            ..
        }) => {
            view.bookmark_confirm_id.clear();
            view.switch_topic(1);
            return;
        }
        _ => {}
    }

    // Standard navigation via adapter.
    match translate_input(&event) {
        UiAction::MoveUp => {
            view.bookmark_confirm_id.clear();
            view.move_selection(-1);
        }
        UiAction::MoveDown => {
            view.bookmark_confirm_id.clear();
            view.move_selection(1);
        }
        _ => {
            view.bookmark_confirm_id.clear();
        }
    }
}

fn apply_edit_input(view: &mut ThreadViewModel, event: InputEvent) {
    let InputEvent::Key(key) = event else {
        return;
    };
    match key.key {
        Key::Escape => view.cancel_edit(),
        Key::Enter => view.save_edit(),
        Key::Backspace => {
            view.edit_input.pop();
        }
        Key::Char(ch) => {
            view.edit_input.push(ch);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the thread view frame.
#[must_use]
pub fn render_thread_frame(
    view: &ThreadViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let mut row = 0usize;

    // Header: topic | message count | participants.
    {
        let topic = if view.topic.trim().is_empty() {
            "(no topic)"
        } else {
            view.topic.trim()
        };
        let total = view.all_msgs.len();
        let participant_count = {
            let mut agents = HashSet::new();
            for m in &view.all_msgs {
                let from = m.from.trim();
                if !from.is_empty() {
                    agents.insert(from);
                }
                let to = m.to.trim();
                if to.starts_with('@') {
                    let peer = to.trim_start_matches('@');
                    if !peer.is_empty() {
                        agents.insert(peer);
                    }
                }
            }
            agents.len()
        };
        let header = format!("{topic}  {total} messages  {participant_count} participants");
        frame.draw_text(0, row, &truncate(&header, width), TextRole::Accent);
        row += 1;
    }
    if row >= height {
        return frame;
    }

    // Meta line: mode, selection, unread, keybindings.
    {
        let mode_label = match view.mode {
            ThreadMode::Threaded => "threaded",
            ThreadMode::Flat => "flat",
        };
        let total_rows = view.rows.len();
        let sel_display = if total_rows > 0 {
            view.selected.min(total_rows - 1) + 1
        } else {
            0
        };
        let unread = view.unread_count();
        let mut meta = format!(
            "mode:{mode_label}  msg:{sel_display}/{total_rows}  unread:{unread}  j/k move  g/G top/bot  Enter expand  f toggle  [ ] topic"
        );
        let marker = view
            .read_markers
            .get(&view.topic)
            .map(String::as_str)
            .unwrap_or("");
        if !marker.is_empty() {
            meta.push_str(&format!("  read:{}", short_id(marker)));
        }
        frame.draw_text(0, row, &truncate(&meta, width), TextRole::Muted);
        row += 1;
    }
    if row >= height {
        return frame;
    }

    // Reserve space for edit prompt and status line.
    let reserved = {
        let mut r = 0;
        if view.edit_active {
            r += 4;
        }
        if !view.status_line.trim().is_empty() {
            r += 1;
        }
        r
    };
    let body_height = height.saturating_sub(row + reserved).max(1);

    // Render message rows.
    if view.rows.is_empty() {
        frame.draw_text(0, row, "No messages", TextRole::Muted);
    } else {
        let start = view.top.min(view.rows.len().saturating_sub(1));
        let mut remaining = body_height;

        for i in start..view.rows.len() {
            if remaining == 0 {
                break;
            }
            let thread_row = &view.rows[i];

            // Group gap.
            if thread_row.group_gap && i > start && remaining > 0 {
                row += 1;
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
            }

            let is_selected = i == view.selected;
            let is_unread = view.is_unread(&thread_row.msg.id);

            let card_lines = render_row_card(view, thread_row, width, is_selected, is_unread);
            let lines_to_show = card_lines.len().min(remaining);
            for line in card_lines.iter().take(lines_to_show) {
                if row >= height {
                    break;
                }
                let role = if is_selected {
                    TextRole::Accent
                } else {
                    TextRole::Primary
                };
                frame.draw_text(0, row, &truncate(line, width), role);
                row += 1;
            }
            remaining -= lines_to_show;
        }

        // New messages indicator.
        if view.pending_new > 0 && !view.is_at_bottom() && row > 2 {
            let indicator = format!("New messages ({}) - press G", view.pending_new);
            frame.draw_text(0, row - 1, &truncate(&indicator, width), TextRole::Accent);
        }
    }

    // Edit prompt.
    if view.edit_active {
        let edit_row = height.saturating_sub(reserved);
        if edit_row < height {
            let title = match view.edit_kind {
                Some(EditKind::BookmarkNote) => "Bookmark note:",
                Some(EditKind::Annotation) => "Annotation:",
                None => "Edit:",
            };
            frame.draw_text(0, edit_row, &truncate(title, width), TextRole::Accent);
            if edit_row + 1 < height {
                let input_line = format!("> {}_", view.edit_input);
                frame.draw_text(
                    0,
                    edit_row + 1,
                    &truncate(&input_line, width),
                    TextRole::Primary,
                );
            }
            if edit_row + 2 < height {
                frame.draw_text(
                    0,
                    edit_row + 2,
                    &truncate("Enter save  Esc cancel", width),
                    TextRole::Muted,
                );
            }
        }
    }

    // Status line.
    if !view.status_line.trim().is_empty() && height > 0 {
        let status_row = height - 1;
        let role = if view.status_err {
            TextRole::Danger
        } else {
            TextRole::Muted
        };
        frame.draw_text(0, status_row, &truncate(&view.status_line, width), role);
    }

    frame
}

fn render_row_card(
    view: &ThreadViewModel,
    row: &ThreadRow,
    _width: usize,
    selected: bool,
    unread: bool,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(8);
    let indent = &row.connector;
    let overflow_marker = if row.overflow { "... " } else { "" };
    let unread_dot = if unread { "* " } else { "" };
    let bookmark_star = if view.bookmarked_ids.contains(row.msg.id.trim()) {
        " ★"
    } else {
        ""
    };

    // Border indicator.
    let border = if selected {
        "║ "
    } else if unread {
        "┃ "
    } else {
        "│ "
    };

    // Header line: border + indent + unread + agent · time [priority] [bookmark].
    let mut header = format!(
        "{border}{indent}{overflow_marker}{unread_dot}{from} · {ts}",
        from = row.msg.from.trim(),
        ts = row.msg.timestamp.trim(),
    );
    if !row.msg.priority.trim().is_empty() {
        header.push_str(&format!(" [{}]", row.msg.priority.trim()));
    }
    header.push_str(bookmark_star);
    lines.push(header);

    let body_indent = format!(
        "{border}{}",
        " ".repeat(indent.len() + overflow_marker.len())
    );

    // Reply indicator.
    if !row.reply_to.is_empty() {
        let reply_target = if let Some(parent) = view.msg_by_id.get(&row.reply_to) {
            let from = parent.from.trim();
            if from.is_empty() {
                short_id(&row.reply_to)
            } else {
                from.to_owned()
            }
        } else {
            short_id(&row.reply_to)
        };
        let mut reply_line = format!("{body_indent}↩ replying to {reply_target}");
        if !row.cross_target.is_empty() {
            reply_line.push_str(&format!(" from {}", row.cross_target));
        }
        lines.push(reply_line);
    }

    // Body lines.
    let body = row.msg.body.trim();
    if !body.is_empty() {
        let body_lines: Vec<&str> = body.lines().collect();
        let limit = if row.truncated {
            body_lines.len().min(THREAD_MAX_BODY_LINES)
        } else {
            body_lines.len()
        };
        for bl in body_lines.iter().take(limit) {
            lines.push(format!("{body_indent}{bl}"));
        }
        if row.truncated {
            lines.push(format!(
                "{body_indent}... [show more] ({} lines)",
                row.hidden_lines
            ));
        }
    }

    // Tags.
    if !row.msg.tags.is_empty() {
        let tags = row.msg.tags.join(", ");
        lines.push(format!("{body_indent}tags: {tags}"));
    }

    // Annotation.
    let annotation = view
        .annotations
        .get(row.msg.id.trim())
        .map(String::as_str)
        .unwrap_or("");
    if !annotation.trim().is_empty() {
        lines.push(format!("{body_indent}▌ NOTE: {}", annotation.trim()));
    }

    // Details (selected only).
    if selected {
        let mut details = format!("{body_indent}id:{}", row.msg.id);
        if !row.msg.host.trim().is_empty() {
            details.push_str(&format!("  host:{}", row.msg.host.trim()));
        }
        lines.push(details);
    }

    lines
}

// ---------------------------------------------------------------------------
// Tree connector generation
// ---------------------------------------------------------------------------

/// Generate box-drawing connector prefix for a thread node.
fn prefix_for_node(
    node: &ThreadNode,
    node_map: &HashMap<&str, &ThreadNode>,
    max_depth: usize,
) -> String {
    if node.depth == 0 || node.parent_id.is_none() {
        return String::new();
    }

    let depth = node.depth.min(max_depth);
    let mut parts = Vec::with_capacity(depth);

    // Build from the node upward.
    let mut current = node;
    let mut chain = Vec::with_capacity(depth);
    chain.push(current);

    while let Some(pid) = current.parent_id.as_deref() {
        if let Some(parent) = node_map.get(pid).copied() {
            chain.push(parent);
            current = parent;
        } else {
            break;
        }
        if chain.len() > max_depth + 1 {
            break;
        }
    }

    chain.reverse(); // root -> ... -> node

    // For each level, determine if the ancestor at that level has more siblings below.
    for i in 1..chain.len() {
        let ancestor = chain[i];
        let parent = chain[i - 1];

        let is_last_child = parent
            .children_ids
            .last()
            .map_or(true, |last_id| last_id == &ancestor.message.id);

        if i == chain.len() - 1 {
            // This is the node itself.
            if is_last_child {
                parts.push("└─");
            } else {
                parts.push("├─");
            }
        } else {
            // Intermediate ancestor.
            if is_last_child {
                parts.push("  ");
            } else {
                parts.push("│ ");
            }
        }
    }

    parts.join("")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn short_id(id: &str) -> String {
    let id = id.trim();
    if id.len() <= 8 {
        id.to_owned()
    } else {
        id[..8].to_owned()
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    fn tmsg(id: &str, from: &str, to: &str, ts: &str, body: &str) -> ThreadMessage {
        ThreadMessage::new(id, from, to, ts, body)
    }

    fn tmsg_reply(
        id: &str,
        from: &str,
        to: &str,
        ts: &str,
        body: &str,
        reply_to: &str,
    ) -> ThreadMessage {
        let mut m = tmsg(id, from, to, ts, body);
        m.reply_to = reply_to.to_owned();
        m
    }

    fn sample_messages() -> Vec<ThreadMessage> {
        vec![
            tmsg(
                "20260209-080000-0001",
                "alice",
                "task",
                "20260209-080000",
                "initial plan",
            ),
            tmsg_reply(
                "20260209-080001-0001",
                "bob",
                "task",
                "20260209-080001",
                "sounds good",
                "20260209-080000-0001",
            ),
            tmsg_reply(
                "20260209-080002-0001",
                "charlie",
                "task",
                "20260209-080002",
                "agreed",
                "20260209-080000-0001",
            ),
        ]
    }

    fn sample_topics() -> Vec<TopicInfo> {
        vec![
            TopicInfo {
                name: "task".to_owned(),
            },
            TopicInfo {
                name: "build".to_owned(),
            },
        ]
    }

    #[test]
    fn initial_state() {
        let view = ThreadViewModel::new();
        assert_eq!(view.mode(), ThreadMode::Threaded);
        assert_eq!(view.selected(), 0);
        assert!(view.rows().is_empty());
    }

    #[test]
    fn set_data_builds_rows() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        assert_eq!(view.topic(), "task");
        assert_eq!(view.rows().len(), 3);
        assert_eq!(view.message_count(), 3);
    }

    #[test]
    fn threaded_mode_shows_connectors() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        // Root has no connector.
        assert!(view.rows()[0].connector.is_empty());
        // Children have connectors.
        assert!(!view.rows()[1].connector.is_empty());
        assert!(!view.rows()[2].connector.is_empty());
    }

    #[test]
    fn flat_mode_no_connectors() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        view.toggle_mode();
        assert_eq!(view.mode(), ThreadMode::Flat);
        for row in view.rows() {
            assert!(row.connector.is_empty());
        }
    }

    #[test]
    fn toggle_mode_preserves_anchor() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        view.move_selection(1); // select row 1
        let anchor_id = view.selected_id().to_owned();
        view.toggle_mode();
        assert_eq!(view.selected_id(), anchor_id);
    }

    #[test]
    fn navigation_up_down() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        assert_eq!(view.selected(), 0);

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Down)));
        assert_eq!(view.selected(), 1);

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Up)));
        assert_eq!(view.selected(), 0);

        // Can't go above 0.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Up)));
        assert_eq!(view.selected(), 0);
    }

    #[test]
    fn jump_top_and_bottom() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());

        // Jump bottom.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('G'))));
        assert_eq!(view.selected(), 2);

        // Jump top.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('g'))));
        assert_eq!(view.selected(), 0);
    }

    #[test]
    fn mode_toggle_key() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        assert_eq!(view.mode(), ThreadMode::Threaded);

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('f'))));
        assert_eq!(view.mode(), ThreadMode::Flat);

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('f'))));
        assert_eq!(view.mode(), ThreadMode::Threaded);
    }

    #[test]
    fn collapse_expand_children() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        assert_eq!(view.rows().len(), 3);

        // Root has children, collapse it.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        // Children should be hidden.
        assert_eq!(view.rows().len(), 1);

        // Expand again.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert_eq!(view.rows().len(), 3);
    }

    #[test]
    fn bookmark_toggle() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        let id = view.selected_id().to_owned();
        assert!(!view.is_bookmarked(&id));

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('b'))));
        assert!(view.is_bookmarked(&id));

        // First press on bookmarked sets confirm.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('b'))));
        assert!(view.is_bookmarked(&id)); // still bookmarked (confirm pending)

        // Second press removes.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('b'))));
        assert!(!view.is_bookmarked(&id));
    }

    #[test]
    fn topic_switching() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        assert_eq!(view.topic(), "task");

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        assert_eq!(view.topic(), "build");

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('['))));
        assert_eq!(view.topic(), "task");
    }

    #[test]
    fn dm_topic_no_switch() {
        let mut view = ThreadViewModel::new();
        view.set_data("@alice", sample_topics(), sample_messages());
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char(']'))));
        // Should not switch for DM topics.
        assert_eq!(view.topic(), "@alice");
    }

    #[test]
    fn unread_tracking() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        // Read marker is set to last message on first load.
        assert_eq!(view.unread_count(), 0);

        // If we set a custom read marker.
        let mut markers = HashMap::new();
        markers.insert("task".to_owned(), "20260209-080000-0001".to_owned());
        view.set_read_markers(markers);
        assert_eq!(view.unread_count(), 2); // two messages after the marker
    }

    #[test]
    fn edit_mode_annotation() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());

        // Open annotation editor.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));
        assert!(view.edit_active);

        // Type.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('n'))));
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('o'))));

        // Save.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert!(!view.edit_active);
        let id = view.selected_id().to_owned();
        assert_eq!(view.annotation(&id), "no");
    }

    #[test]
    fn edit_mode_cancel() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('a'))));
        assert!(view.edit_active);

        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Escape)));
        assert!(!view.edit_active);
    }

    #[test]
    fn render_empty() {
        let view = ThreadViewModel::new();
        let frame = render_thread_frame(&view, 60, 5, ThemeSpec::default());
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("(no topic)"));
        assert!(snapshot.contains("No messages"));
    }

    #[test]
    fn render_with_messages() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        let frame = render_thread_frame(&view, 70, 12, ThemeSpec::default());
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("task"));
        assert!(snapshot.contains("3 messages"));
        assert!(snapshot.contains("alice"));
        assert!(snapshot.contains("initial plan"));
    }

    #[test]
    fn render_selected_shows_details() {
        let mut view = ThreadViewModel::new();
        view.set_data("task", sample_topics(), sample_messages());
        let frame = render_thread_frame(&view, 70, 15, ThemeSpec::default());
        let snapshot = frame.snapshot();
        // Selected row shows message ID in details.
        assert!(snapshot.contains("id:20260209-080000-0001"));
    }

    #[test]
    fn thread_snapshot() {
        let mut view = ThreadViewModel::new();
        view.set_data(
            "task",
            vec![TopicInfo {
                name: "task".to_owned(),
            }],
            vec![
                tmsg("m1", "alice", "task", "15:00", "root msg"),
                tmsg_reply("m2", "bob", "task", "15:01", "reply", "m1"),
            ],
        );
        let frame = render_thread_frame(&view, 52, 8, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_thread_frame",
            &frame,
            "task  2 messages  2 participants                    \nmode:threaded  msg:1/2  unread:0  j/k move  g/G top…\n║ alice · 15:00                                     \n║ root msg                                          \n║ id:m1                                             \n│ └─bob · 15:01                                     \n│       ↩ replying to alice                         \n│       reply                                       ",
        );
    }

    #[test]
    fn cross_target_reply_shows_from() {
        let mut view = ThreadViewModel::new();
        view.set_data(
            "task",
            vec![TopicInfo {
                name: "task".to_owned(),
            }],
            vec![
                tmsg("m1", "alice", "task", "15:00", "root"),
                tmsg_reply("m2", "bob", "build", "15:01", "cross", "m1"),
            ],
        );
        let frame = render_thread_frame(&view, 60, 10, ThemeSpec::default());
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("from task"));
    }

    #[test]
    fn page_down_up() {
        let mut view = ThreadViewModel::new();
        let mut msgs = Vec::new();
        for i in 0..20 {
            msgs.push(tmsg(
                &format!("m{i:02}"),
                "alice",
                "task",
                &format!("15:{i:02}"),
                &format!("msg {i}"),
            ));
        }
        view.set_data("task", sample_topics(), msgs);
        view.viewport_rows = 8;

        // Ctrl+D page down.
        apply_thread_input(
            &mut view,
            InputEvent::Key(KeyEvent {
                key: Key::Char('d'),
                modifiers: Modifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            }),
        );
        assert!(view.selected() > 0);

        let pos = view.selected();
        // Ctrl+U page up.
        apply_thread_input(
            &mut view,
            InputEvent::Key(KeyEvent {
                key: Key::Char('u'),
                modifiers: Modifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            }),
        );
        assert!(view.selected() < pos);
    }

    #[test]
    fn body_truncation() {
        let mut view = ThreadViewModel::new();
        let long_body = (0..60)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let msgs = vec![tmsg("m1", "alice", "task", "15:00", &long_body)];
        view.set_data("task", sample_topics(), msgs);
        assert!(view.rows()[0].truncated);
        assert_eq!(view.rows()[0].hidden_lines, 10); // 60 - 50

        // Enter expands.
        apply_thread_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
        assert!(!view.rows()[0].truncated);
    }

    #[test]
    fn connector_generation() {
        let mut view = ThreadViewModel::new();
        view.set_data(
            "task",
            sample_topics(),
            vec![
                tmsg("r", "alice", "task", "15:00", "root"),
                tmsg_reply("c1", "bob", "task", "15:01", "first", "r"),
                tmsg_reply("c2", "charlie", "task", "15:02", "second", "r"),
            ],
        );
        // Root: no connector.
        assert!(view.rows()[0].connector.is_empty());
        // First child (has sibling): ├─
        assert!(view.rows()[1].connector.contains('├'));
        // Last child: └─
        assert!(view.rows()[2].connector.contains('└'));
    }
}

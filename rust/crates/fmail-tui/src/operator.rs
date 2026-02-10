//! Operator view for the fmail TUI, ported from Go `operatorView`.
//!
//! Full-screen operator console with sidebar conversation list, message
//! display, compose panel, quick-target bar, agent status ticker, and
//! slash-command palette.

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum messages retained per conversation.
pub const OPERATOR_MESSAGE_LIMIT: usize = 250;

/// Agent activity threshold in seconds (10 minutes).
const ACTIVE_WINDOW_SECS: i64 = 600;

// ---------------------------------------------------------------------------
// OperatorConversation
// ---------------------------------------------------------------------------

/// A conversation entry in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorConversation {
    pub target: String,
    pub last_activity_secs: i64,
    pub unread: usize,
    pub last_message_preview: String,
}

impl OperatorConversation {
    #[must_use]
    pub fn new(target: &str) -> Self {
        Self {
            target: target.to_owned(),
            last_activity_secs: 0,
            unread: 0,
            last_message_preview: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// OperatorMessage
// ---------------------------------------------------------------------------

/// A message displayed in the conversation panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub time_label: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub reply_to: String,
    pub reply_preview: String,
    pub is_mine: bool,
}

// ---------------------------------------------------------------------------
// OperatorAgent
// ---------------------------------------------------------------------------

/// An agent displayed in the status ticker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorAgent {
    pub name: String,
    pub status: String,
    pub last_seen_secs: i64,
}

impl OperatorAgent {
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
// OperatorViewModel
// ---------------------------------------------------------------------------

/// View-model for the operator console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorViewModel {
    /// Operator's own agent name.
    pub self_name: String,

    /// Conversations in the sidebar.
    conversations: Vec<OperatorConversation>,
    selected: usize,

    /// Currently viewed target.
    pub target: String,

    /// Messages in the current conversation.
    messages: Vec<OperatorMessage>,
    scroll: usize,
    follow: bool,

    /// Quick targets (top 9).
    quick_targets: Vec<String>,

    /// Active agents.
    agents: Vec<OperatorAgent>,

    /// Compose state.
    pub compose: String,
    pub compose_priority: String,
    pub compose_tags: Vec<String>,
    pub compose_multiline: bool,

    /// Sidebar collapsed.
    pub sidebar_collapsed: bool,

    /// Command palette visible.
    pub show_palette: bool,

    /// Pending approval message ID.
    pub pending_approve: String,

    /// Total unread count.
    pub unread_total: usize,

    /// Current time in seconds since epoch.
    pub now_secs: i64,

    /// Status messages.
    pub status_line: String,
    pub status_err: String,
}

impl Default for OperatorViewModel {
    fn default() -> Self {
        Self::new("")
    }
}

impl OperatorViewModel {
    #[must_use]
    pub fn new(self_name: &str) -> Self {
        Self {
            self_name: self_name.to_owned(),
            conversations: Vec::new(),
            selected: 0,
            target: String::new(),
            messages: Vec::new(),
            scroll: 0,
            follow: true,
            quick_targets: Vec::new(),
            agents: Vec::new(),
            compose: String::new(),
            compose_priority: "normal".to_owned(),
            compose_tags: Vec::new(),
            compose_multiline: false,
            sidebar_collapsed: false,
            show_palette: false,
            pending_approve: String::new(),
            unread_total: 0,
            now_secs: 0,
            status_line: String::new(),
            status_err: String::new(),
        }
    }

    // -- data population -----------------------------------------------------

    /// Set conversation list. Sorted by last_activity descending.
    pub fn set_conversations(&mut self, mut convs: Vec<OperatorConversation>) {
        convs.sort_by(|a, b| b.last_activity_secs.cmp(&a.last_activity_secs));
        self.unread_total = convs.iter().map(|c| c.unread).sum();
        // Build quick targets (top 9).
        self.quick_targets = convs.iter().take(9).map(|c| c.target.clone()).collect();
        self.conversations = convs;
        self.clamp_selection();
    }

    /// Set messages for the current conversation.
    pub fn set_messages(&mut self, mut messages: Vec<OperatorMessage>) {
        if messages.len() > OPERATOR_MESSAGE_LIMIT {
            let drop_n = messages.len() - OPERATOR_MESSAGE_LIMIT;
            messages.drain(0..drop_n);
        }
        self.messages = messages;
        if self.follow {
            self.scroll = 0;
        }
    }

    /// Set agent list.
    pub fn set_agents(&mut self, agents: Vec<OperatorAgent>) {
        self.agents = agents;
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn conversations(&self) -> &[OperatorConversation] {
        &self.conversations
    }

    #[must_use]
    pub fn messages(&self) -> &[OperatorMessage] {
        &self.messages
    }

    #[must_use]
    pub fn agents(&self) -> &[OperatorAgent] {
        &self.agents
    }

    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    #[must_use]
    pub fn is_following(&self) -> bool {
        self.follow
    }

    #[must_use]
    pub fn quick_targets(&self) -> &[String] {
        &self.quick_targets
    }

    // -- navigation ----------------------------------------------------------

    /// Select next conversation.
    pub fn next_conversation(&mut self) {
        if self.conversations.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.conversations.len();
        self.update_target_from_selection();
    }

    /// Select previous conversation.
    pub fn prev_conversation(&mut self) {
        if self.conversations.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.conversations.len() - 1;
        } else {
            self.selected -= 1;
        }
        self.update_target_from_selection();
    }

    /// Jump to a quick target by 1-based index.
    pub fn select_quick_target(&mut self, idx: usize) {
        if idx == 0 || idx > self.quick_targets.len() {
            return;
        }
        let target = self.quick_targets[idx - 1].clone();
        self.target = target.clone();
        // Find in conversations.
        if let Some(pos) = self.conversations.iter().position(|c| c.target == target) {
            self.selected = pos;
        }
    }

    /// Scroll messages up (older).
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll = self.scroll.saturating_add(lines);
        self.follow = false;
    }

    /// Scroll messages down (newer).
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll = self.scroll.saturating_sub(lines);
        if self.scroll == 0 {
            self.follow = true;
        }
    }

    /// Jump to newest messages.
    pub fn scroll_to_end(&mut self) {
        self.scroll = 0;
        self.follow = true;
    }

    /// Jump to oldest messages.
    pub fn scroll_to_start(&mut self) {
        self.scroll = self.messages.len().saturating_sub(1);
        self.follow = false;
    }

    // -- compose -------------------------------------------------------------

    /// Append a character to compose.
    pub fn compose_push(&mut self, ch: char) {
        self.compose.push(ch);
    }

    /// Remove last character from compose.
    pub fn compose_pop(&mut self) {
        self.compose.pop();
    }

    /// Clear compose.
    pub fn compose_clear(&mut self) {
        self.compose.clear();
    }

    /// Toggle multiline compose mode.
    pub fn toggle_multiline(&mut self) {
        self.compose_multiline = !self.compose_multiline;
    }

    /// Toggle command palette.
    pub fn toggle_palette(&mut self) {
        self.show_palette = !self.show_palette;
    }

    /// Toggle sidebar collapsed.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    /// Set compose priority from string.
    pub fn set_priority(&mut self, priority: &str) {
        let normalized = match priority.trim().to_ascii_lowercase().as_str() {
            "high" | "h" => "high",
            "low" | "l" => "low",
            _ => "normal",
        };
        self.compose_priority = normalized.to_owned();
    }

    /// Set compose tags.
    pub fn set_tags(&mut self, tags: Vec<String>) {
        self.compose_tags = tags;
    }

    // -- internal ------------------------------------------------------------

    fn clamp_selection(&mut self) {
        let max = self.conversations.len().saturating_sub(1);
        self.selected = self.selected.min(max);
    }

    fn update_target_from_selection(&mut self) {
        if let Some(conv) = self.conversations.get(self.selected) {
            self.target = conv.target.clone();
        }
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

/// Process an input event on the operator view model.
/// Returns `true` if the event was consumed.
pub fn apply_operator_input(view: &mut OperatorViewModel, event: InputEvent) -> bool {
    // If compose is non-empty, most keys go to compose.
    let InputEvent::Key(KeyEvent { key, modifiers }) = event else {
        return false;
    };

    // Global modifiers first.
    if modifiers.ctrl {
        match key {
            Key::Char('m') => {
                view.toggle_multiline();
                return true;
            }
            Key::Char('p') => {
                view.toggle_palette();
                return true;
            }
            Key::Char('b') => {
                view.toggle_sidebar();
                return true;
            }
            _ => {}
        }
    }

    // Tab cycles conversations.
    if key == Key::Tab && !modifiers.ctrl && !modifiers.alt {
        if modifiers.shift {
            view.prev_conversation();
        } else {
            view.next_conversation();
        }
        return true;
    }

    // Number keys for quick targets (only when not composing).
    if view.compose.is_empty() && !modifiers.ctrl && !modifiers.alt {
        if let Key::Char(ch @ '1'..='9') = key {
            let idx = (ch as u8 - b'0') as usize;
            view.select_quick_target(idx);
            return true;
        }
    }

    // Escape: clear compose or close palette.
    if key == Key::Escape {
        if view.show_palette {
            view.show_palette = false;
            return true;
        }
        if !view.compose.is_empty() {
            view.compose_clear();
            return true;
        }
        return false; // let global handler pop view
    }

    // Compose input.
    if !view.compose.is_empty() || matches!(key, Key::Char('/')) {
        match key {
            Key::Backspace => {
                view.compose_pop();
                return true;
            }
            Key::Char(ch) => {
                view.compose_push(ch);
                return true;
            }
            Key::Enter => {
                if view.compose_multiline {
                    view.compose_push('\n');
                } else {
                    // Submit is handled by caller; we just signal readiness.
                    // The compose text is available in view.compose.
                }
                return true;
            }
            _ => {}
        }
    }

    // Navigation when compose is empty.
    if view.compose.is_empty() {
        match key {
            Key::Char('n') if !modifiers.ctrl => {
                view.target.clear();
                return true;
            }
            Key::Char('y') if !modifiers.ctrl && !view.pending_approve.is_empty() => {
                view.status_line = format!("approved {}", view.pending_approve);
                view.pending_approve.clear();
                return true;
            }
            Key::Char('x') if !modifiers.ctrl && !view.pending_approve.is_empty() => {
                view.compose = "/reject ".to_owned();
                return true;
            }
            _ => {}
        }
    }

    // Standard scroll actions.
    match translate_input(&event) {
        UiAction::MoveUp | UiAction::ScrollUp => {
            view.scroll_up(3);
            true
        }
        UiAction::MoveDown | UiAction::ScrollDown => {
            view.scroll_down(3);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the operator view into a frame.
#[must_use]
pub fn render_operator_frame(
    view: &OperatorViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    // Layout: conversation area, quick bar, status ticker, compose, [palette], [status]
    // Reserve heights from bottom up.
    let compose_h = if view.compose_multiline { 4 } else { 2 };
    let quick_h = 1;
    let ticker_h = 1;
    let palette_h = if view.show_palette { 5 } else { 0 };
    let status_h = if !view.status_line.is_empty() || !view.status_err.is_empty() {
        1
    } else {
        0
    };
    let reserved = compose_h + quick_h + ticker_h + palette_h + status_h;
    let conv_h = height.saturating_sub(reserved).max(4);

    let mut y = 0;

    // Conversation area.
    render_conversation_area(view, &mut frame, 0, y, width, conv_h);
    y += conv_h;

    // Quick actions bar.
    if y < height {
        render_quick_bar(view, &mut frame, 0, y, width);
        y += quick_h;
    }

    // Status ticker.
    if y < height {
        render_status_ticker(view, &mut frame, 0, y, width);
        y += ticker_h;
    }

    // Compose panel.
    if y < height {
        render_compose_panel(view, &mut frame, 0, y, width, compose_h);
        y += compose_h;
    }

    // Command palette.
    if view.show_palette && y < height {
        render_command_palette(&mut frame, 0, y, width, palette_h);
        y += palette_h;
    }

    // Status line.
    if y < height && !view.status_err.is_empty() {
        frame.draw_text(
            0,
            y,
            &truncate(&format!("\u{2717} {}", view.status_err), width),
            TextRole::Danger,
        );
    } else if y < height && !view.status_line.is_empty() {
        frame.draw_text(
            0,
            y,
            &truncate(&format!("\u{2713} {}", view.status_line), width),
            TextRole::Success,
        );
    }

    frame
}

fn render_conversation_area(
    view: &OperatorViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y_off: usize,
    width: usize,
    height: usize,
) {
    if width == 0 || height == 0 {
        return;
    }

    if view.sidebar_collapsed || width < 60 {
        // Full-width conversation panel.
        render_conversation_panel(view, frame, x_off, y_off, width, height);
    } else {
        // Sidebar + conversation.
        let sidebar_w = (width / 4).clamp(24, 34).min(width.saturating_sub(16));
        let main_w = width.saturating_sub(sidebar_w + 1);
        render_sidebar(view, frame, x_off, y_off, sidebar_w, height);
        render_conversation_panel(view, frame, x_off + sidebar_w + 1, y_off, main_w, height);
    }
}

fn render_sidebar(
    view: &OperatorViewModel,
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

    frame.draw_text(
        x_off,
        y,
        &truncate("Conversations", width),
        TextRole::Accent,
    );
    y += 1;

    if view.conversations.is_empty() {
        if y < y_off + height {
            frame.draw_text(x_off, y, "(none)", TextRole::Muted);
        }
        return;
    }

    for (idx, conv) in view.conversations.iter().enumerate() {
        if y >= y_off + height {
            break;
        }
        let marker = if idx == view.selected {
            "\u{25b8} "
        } else {
            "  "
        };
        let unread = if conv.unread > 0 {
            format!(" [{}]", conv.unread)
        } else {
            String::new()
        };
        let line = format!("{marker}{}{unread}", conv.target);
        let role = if idx == view.selected {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        frame.draw_text(x_off, y, &truncate(&line, width), role);
        y += 1;
    }
}

fn render_conversation_panel(
    view: &OperatorViewModel,
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
    let title = if view.target.is_empty() {
        "Operator Console".to_owned()
    } else {
        format!("Conversation with {}", view.target)
    };
    frame.draw_text(x_off, y, &truncate(&title, width), TextRole::Accent);
    y += 1;

    if view.messages.is_empty() {
        if y < y_off + height {
            frame.draw_text(x_off, y, "(no messages)", TextRole::Muted);
        }
        return;
    }

    // Render messages with scroll.
    let body_h = (y_off + height).saturating_sub(y);
    let n = view.messages.len();
    let start = if view.scroll > 0 {
        n.saturating_sub(body_h + view.scroll)
    } else {
        n.saturating_sub(body_h)
    };
    let end = n.min(start + body_h);

    for msg in view.messages.iter().skip(start).take(end - start) {
        if y >= y_off + height {
            break;
        }

        // Reply preview.
        if !msg.reply_preview.is_empty() {
            let reply_line = format!("  \u{21aa} {}", first_line(&msg.reply_preview));
            frame.draw_text(x_off, y, &truncate(&reply_line, width), TextRole::Muted);
            y += 1;
            if y >= y_off + height {
                break;
            }
        }

        // Author + timestamp.
        let author = if msg.is_mine { "you" } else { &msg.from };
        let tag_hint = if msg
            .tags
            .iter()
            .any(|t| t == "question" || t == "needs-approval")
        {
            " ?"
        } else {
            ""
        };
        let priority_hint = match msg.priority.trim().to_ascii_lowercase().as_str() {
            "high" => " [HIGH]",
            "low" => " [low]",
            _ => "",
        };
        let header = format!("{} {}{}{priority_hint}", msg.time_label, author, tag_hint);
        let header_role = if msg.is_mine {
            TextRole::Accent
        } else {
            TextRole::Primary
        };
        frame.draw_text(x_off, y, &truncate(&header, width), header_role);
        y += 1;
        if y >= y_off + height {
            break;
        }

        // Body (first line, indented).
        let body = first_line(&msg.body);
        let body = if body.is_empty() { "(empty)" } else { body };
        frame.draw_text(
            x_off,
            y,
            &truncate(&format!("  {body}"), width),
            TextRole::Primary,
        );
        y += 1;
    }
}

fn render_quick_bar(
    view: &OperatorViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y: usize,
    width: usize,
) {
    let mut parts: Vec<String> = Vec::new();
    parts.push("quick:".to_owned());
    for (idx, target) in view.quick_targets.iter().enumerate().take(9) {
        parts.push(format!("[{}] {}", idx + 1, target));
    }
    if view.unread_total > 0 {
        parts.push(format!("[N:{}]", view.unread_total));
    }
    let line = parts.join("  ");
    frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Muted);
}

fn render_status_ticker(
    view: &OperatorViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y: usize,
    width: usize,
) {
    if view.agents.is_empty() {
        frame.draw_text(x_off, y, "(no agents online)", TextRole::Muted);
        return;
    }
    let mut parts: Vec<String> = Vec::new();
    for agent in view.agents.iter().take(6) {
        let active = agent.last_seen_secs > 0
            && (view.now_secs - agent.last_seen_secs) <= ACTIVE_WINDOW_SECS;
        let indicator = if active { "\u{25cf}" } else { "\u{25cb}" };
        let status = agent.status.trim();
        if status.is_empty() {
            parts.push(format!("{indicator} {}", agent.name));
        } else {
            parts.push(format!("{indicator} {} {}", agent.name, status));
        }
    }
    let line = parts.join("  ");
    frame.draw_text(x_off, y, &truncate(&line, width), TextRole::Muted);
}

fn render_compose_panel(
    view: &OperatorViewModel,
    frame: &mut RenderFrame,
    x_off: usize,
    y_off: usize,
    width: usize,
    height: usize,
) {
    if height == 0 || width == 0 {
        return;
    }
    let mut y = y_off;

    // Metadata line.
    let target_label = if view.target.is_empty() {
        "(none)".to_owned()
    } else {
        view.target.clone()
    };
    let mut meta_parts = vec![format!("To: {target_label}")];
    if view.compose_priority != "normal" {
        meta_parts.push(format!("Priority: {}", view.compose_priority));
    }
    if !view.compose_tags.is_empty() {
        meta_parts.push(format!("Tags: {}", view.compose_tags.join(",")));
    }
    let mode = if view.compose_multiline {
        "multi"
    } else {
        "single"
    };
    meta_parts.push(format!("Mode: {mode}"));
    let meta = meta_parts.join("  ");
    frame.draw_text(x_off, y, &truncate(&meta, width), TextRole::Muted);
    y += 1;

    if y < y_off + height {
        // Compose input.
        let cursor = if view.compose.is_empty() { "" } else { "_" };
        let input_line = format!("> {}{cursor}", view.compose);
        frame.draw_text(x_off, y, &truncate(&input_line, width), TextRole::Primary);
        y += 1;
    }

    // Hints (if space).
    if y < y_off + height {
        let hints = "Ctrl+Enter: send | Ctrl+M: multiline | Ctrl+P: cmds | Ctrl+B: sidebar";
        frame.draw_text(x_off, y, &truncate(hints, width), TextRole::Muted);
    }
}

fn render_command_palette(
    frame: &mut RenderFrame,
    x_off: usize,
    y_off: usize,
    width: usize,
    height: usize,
) {
    if height == 0 || width == 0 {
        return;
    }
    let commands = [
        "/dm <agent> [msg]     - Direct message",
        "/topic <name> [msg]   - Topic message",
        "/broadcast <msg>      - Send to all active",
        "/priority high|low    - Set priority",
        "/tag <tags>           - Set tags",
    ];
    frame.draw_text(
        x_off,
        y_off,
        &truncate("=== Command Palette ===", width),
        TextRole::Accent,
    );
    for (i, cmd) in commands.iter().enumerate() {
        if i + 1 >= height {
            break;
        }
        frame.draw_text(x_off, y_off + i + 1, &truncate(cmd, width), TextRole::Muted);
    }
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

    fn shift_tab() -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: true,
                ctrl: false,
                alt: false,
            },
        })
    }

    fn sample_conversations() -> Vec<OperatorConversation> {
        vec![
            {
                let mut c = OperatorConversation::new("@architect");
                c.last_activity_secs = 1000;
                c.unread = 2;
                c.last_message_preview = "review pending".into();
                c
            },
            {
                let mut c = OperatorConversation::new("#task");
                c.last_activity_secs = 900;
                c.unread = 0;
                c
            },
            {
                let mut c = OperatorConversation::new("@coder");
                c.last_activity_secs = 800;
                c.unread = 1;
                c
            },
        ]
    }

    fn sample_messages() -> Vec<OperatorMessage> {
        vec![
            OperatorMessage {
                id: "msg-001".into(),
                from: "architect".into(),
                to: "@operator".into(),
                body: "please review the plan".into(),
                time_label: "10:00".into(),
                priority: String::new(),
                tags: vec!["question".into()],
                reply_to: String::new(),
                reply_preview: String::new(),
                is_mine: false,
            },
            OperatorMessage {
                id: "msg-002".into(),
                from: "operator".into(),
                to: "@architect".into(),
                body: "looks good, approved".into(),
                time_label: "10:05".into(),
                priority: "high".into(),
                tags: vec!["approved".into()],
                reply_to: "msg-001".into(),
                reply_preview: "please review the plan".into(),
                is_mine: true,
            },
        ]
    }

    fn sample_agents() -> Vec<OperatorAgent> {
        vec![
            {
                let mut a = OperatorAgent::new("architect");
                a.last_seen_secs = 1000;
                a.status = "working".into();
                a
            },
            {
                let mut a = OperatorAgent::new("coder");
                a.last_seen_secs = 500;
                a
            },
        ]
    }

    // -- ViewModel basics ----------------------------------------------------

    #[test]
    fn new_viewmodel_defaults() {
        let vm = OperatorViewModel::new("operator");
        assert_eq!(vm.self_name, "operator");
        assert!(vm.conversations().is_empty());
        assert!(vm.messages().is_empty());
        assert!(vm.is_following());
        assert_eq!(vm.compose_priority, "normal");
        assert!(!vm.sidebar_collapsed);
        assert!(!vm.show_palette);
    }

    #[test]
    fn set_conversations_sorts_and_builds_quick() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        assert_eq!(vm.conversations().len(), 3);
        assert_eq!(vm.conversations()[0].target, "@architect");
        assert_eq!(vm.quick_targets().len(), 3);
        assert_eq!(vm.unread_total, 3);
    }

    #[test]
    fn select_quick_target() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        vm.select_quick_target(2); // #task
        assert_eq!(vm.target, "#task");
        assert_eq!(vm.selected(), 1);
    }

    #[test]
    fn next_prev_conversation() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        assert_eq!(vm.selected(), 0);
        vm.next_conversation();
        assert_eq!(vm.selected(), 1);
        assert_eq!(vm.target, "#task");
        vm.next_conversation();
        assert_eq!(vm.selected(), 2);
        vm.next_conversation(); // wraps
        assert_eq!(vm.selected(), 0);
        vm.prev_conversation(); // wraps back
        assert_eq!(vm.selected(), 2);
    }

    #[test]
    fn scroll_up_down_and_follow() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_messages(sample_messages());
        assert!(vm.is_following());
        vm.scroll_up(3);
        assert_eq!(vm.scroll(), 3);
        assert!(!vm.is_following());
        vm.scroll_down(2);
        assert_eq!(vm.scroll(), 1);
        vm.scroll_to_end();
        assert!(vm.is_following());
        assert_eq!(vm.scroll(), 0);
    }

    #[test]
    fn set_messages_enforces_limit() {
        let mut vm = OperatorViewModel::new("operator");
        let msgs: Vec<OperatorMessage> = (0..(OPERATOR_MESSAGE_LIMIT + 5))
            .map(|i| OperatorMessage {
                id: format!("msg-{i}"),
                from: "a".into(),
                to: "@operator".into(),
                body: "x".into(),
                time_label: "10:00".into(),
                priority: String::new(),
                tags: Vec::new(),
                reply_to: String::new(),
                reply_preview: String::new(),
                is_mine: false,
            })
            .collect();

        vm.set_messages(msgs);

        assert_eq!(vm.messages().len(), OPERATOR_MESSAGE_LIMIT);
        assert_eq!(vm.messages()[0].id, "msg-5");
        assert_eq!(
            vm.messages()[OPERATOR_MESSAGE_LIMIT - 1].id,
            format!("msg-{}", OPERATOR_MESSAGE_LIMIT + 4)
        );
    }

    #[test]
    fn compose_push_pop_clear() {
        let mut vm = OperatorViewModel::new("operator");
        vm.compose_push('/');
        vm.compose_push('d');
        vm.compose_push('m');
        assert_eq!(vm.compose, "/dm");
        vm.compose_pop();
        assert_eq!(vm.compose, "/d");
        vm.compose_clear();
        assert!(vm.compose.is_empty());
    }

    #[test]
    fn set_priority() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_priority("high");
        assert_eq!(vm.compose_priority, "high");
        vm.set_priority("l");
        assert_eq!(vm.compose_priority, "low");
        vm.set_priority("normal");
        assert_eq!(vm.compose_priority, "normal");
        vm.set_priority("invalid");
        assert_eq!(vm.compose_priority, "normal");
    }

    #[test]
    fn toggle_multiline() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(!vm.compose_multiline);
        vm.toggle_multiline();
        assert!(vm.compose_multiline);
    }

    #[test]
    fn toggle_palette() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(!vm.show_palette);
        vm.toggle_palette();
        assert!(vm.show_palette);
    }

    #[test]
    fn toggle_sidebar() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(!vm.sidebar_collapsed);
        vm.toggle_sidebar();
        assert!(vm.sidebar_collapsed);
    }

    // -- Input handling ------------------------------------------------------

    #[test]
    fn input_tab_cycles_conversations() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        assert!(apply_operator_input(&mut vm, key(Key::Tab)));
        assert_eq!(vm.selected(), 1);
    }

    #[test]
    fn input_shift_tab_goes_back() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        vm.next_conversation();
        assert!(apply_operator_input(&mut vm, shift_tab()));
        assert_eq!(vm.selected(), 0);
    }

    #[test]
    fn input_number_selects_quick() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        assert!(apply_operator_input(&mut vm, key(Key::Char('2'))));
        assert_eq!(vm.target, "#task");
    }

    #[test]
    fn input_ctrl_m_toggles_multiline() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(apply_operator_input(&mut vm, ctrl_key('m')));
        assert!(vm.compose_multiline);
    }

    #[test]
    fn input_ctrl_p_toggles_palette() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(apply_operator_input(&mut vm, ctrl_key('p')));
        assert!(vm.show_palette);
    }

    #[test]
    fn input_ctrl_b_toggles_sidebar() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(apply_operator_input(&mut vm, ctrl_key('b')));
        assert!(vm.sidebar_collapsed);
    }

    #[test]
    fn input_slash_starts_compose() {
        let mut vm = OperatorViewModel::new("operator");
        assert!(apply_operator_input(&mut vm, key(Key::Char('/'))));
        assert_eq!(vm.compose, "/");
    }

    #[test]
    fn input_escape_clears_compose() {
        let mut vm = OperatorViewModel::new("operator");
        vm.compose = "/dm test".into();
        assert!(apply_operator_input(&mut vm, key(Key::Escape)));
        assert!(vm.compose.is_empty());
    }

    #[test]
    fn input_escape_closes_palette_first() {
        let mut vm = OperatorViewModel::new("operator");
        vm.show_palette = true;
        assert!(apply_operator_input(&mut vm, key(Key::Escape)));
        assert!(!vm.show_palette);
    }

    #[test]
    fn input_n_clears_target() {
        let mut vm = OperatorViewModel::new("operator");
        vm.target = "@someone".into();
        assert!(apply_operator_input(&mut vm, key(Key::Char('n'))));
        assert!(vm.target.is_empty());
    }

    #[test]
    fn input_y_approves_pending() {
        let mut vm = OperatorViewModel::new("operator");
        vm.pending_approve = "msg-001".into();
        assert!(apply_operator_input(&mut vm, key(Key::Char('y'))));
        assert!(vm.pending_approve.is_empty());
        assert!(vm.status_line.contains("approved"));
    }

    #[test]
    fn input_x_starts_reject() {
        let mut vm = OperatorViewModel::new("operator");
        vm.pending_approve = "msg-001".into();
        assert!(apply_operator_input(&mut vm, key(Key::Char('x'))));
        assert!(vm.compose.starts_with("/reject"));
    }

    // -- Rendering -----------------------------------------------------------

    #[test]
    fn render_empty_operator() {
        let vm = OperatorViewModel::new("operator");
        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("Operator Console"),
            "should show title: {all_text}"
        );
    }

    #[test]
    fn render_with_conversations_and_messages() {
        let mut vm = OperatorViewModel::new("operator");
        vm.now_secs = 1050;
        vm.set_conversations(sample_conversations());
        vm.target = "@architect".into();
        vm.set_messages(sample_messages());
        vm.set_agents(sample_agents());

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("Conversations"), "should show sidebar");
        assert!(all_text.contains("@architect"), "should show conversation");
        assert!(
            all_text.contains("please review"),
            "should show message body"
        );
        assert!(all_text.contains("quick:"), "should show quick bar");
    }

    #[test]
    fn render_sidebar_collapsed() {
        let mut vm = OperatorViewModel::new("operator");
        vm.sidebar_collapsed = true;
        vm.set_conversations(sample_conversations());
        vm.target = "@architect".into();
        vm.set_messages(sample_messages());

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !all_text.contains("Conversations"),
            "sidebar should be hidden"
        );
        assert!(
            all_text.contains("Conversation with @architect"),
            "should show main panel"
        );
    }

    #[test]
    fn render_narrow_hides_sidebar() {
        let mut vm = OperatorViewModel::new("operator");
        vm.set_conversations(sample_conversations());
        vm.target = "@architect".into();

        // width < 60 hides sidebar
        let frame = render_operator_frame(&vm, 50, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!all_text.contains("Conversations"), "narrow: no sidebar");
    }

    #[test]
    fn render_compose_panel_shows_metadata() {
        let mut vm = OperatorViewModel::new("operator");
        vm.target = "@architect".into();
        vm.compose_priority = "high".to_owned();
        vm.compose_tags = vec!["urgent".into()];
        vm.compose = "hello".into();

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("To: @architect"), "compose: target");
        assert!(all_text.contains("Priority: high"), "compose: priority");
        assert!(all_text.contains("Tags: urgent"), "compose: tags");
        assert!(all_text.contains("> hello"), "compose: input");
    }

    #[test]
    fn render_command_palette_when_visible() {
        let mut vm = OperatorViewModel::new("operator");
        vm.show_palette = true;

        let frame = render_operator_frame(&vm, 100, 25, ThemeSpec::default());
        let all_text: String = (0..25)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("Command Palette"),
            "should show palette: {all_text}"
        );
        assert!(all_text.contains("/dm"), "palette: should show commands");
    }

    #[test]
    fn render_status_line_success() {
        let mut vm = OperatorViewModel::new("operator");
        vm.status_line = "sent 1 message(s)".into();

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("\u{2713}"), "should show success marker");
    }

    #[test]
    fn render_status_line_error() {
        let mut vm = OperatorViewModel::new("operator");
        vm.status_err = "send failed".into();

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("\u{2717}"), "should show error marker");
    }

    #[test]
    fn render_reply_preview() {
        let mut vm = OperatorViewModel::new("operator");
        vm.target = "@architect".into();
        vm.set_messages(sample_messages());

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("\u{21aa}"),
            "should show reply hook: {all_text}"
        );
    }

    #[test]
    fn render_priority_and_question_indicators() {
        let mut vm = OperatorViewModel::new("operator");
        vm.target = "@architect".into();
        vm.set_messages(sample_messages());

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all_text.contains("[HIGH]"),
            "should show HIGH priority: {all_text}"
        );
        assert!(all_text.contains("?"), "should show question indicator");
    }

    #[test]
    fn render_agent_status_ticker() {
        let mut vm = OperatorViewModel::new("operator");
        vm.now_secs = 1050;
        vm.set_agents(sample_agents());

        let frame = render_operator_frame(&vm, 100, 20, ThemeSpec::default());
        let all_text: String = (0..20)
            .map(|r| frame.row_text(r))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("architect"), "ticker: should show agent");
        assert!(
            all_text.contains("\u{25cf}"),
            "ticker: should show active indicator"
        );
    }

    // -- Snapshot test -------------------------------------------------------

    #[test]
    fn operator_snapshot_render() {
        let mut vm = OperatorViewModel::new("operator");
        vm.now_secs = 1050;
        vm.set_conversations(sample_conversations());
        vm.target = "@architect".into();
        vm.set_messages(sample_messages());
        vm.set_agents(sample_agents());

        let frame = render_operator_frame(&vm, 100, 16, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_operator_view",
            &frame,
            &(0..16)
                .map(|r| {
                    let text = frame.row_text(r);
                    format!("{:<100}", text)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

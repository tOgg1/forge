//! Compose overlay and quick-send bar for the fmail TUI, ported from Go
//! `compose.go`.
//!
//! Provides two messaging modes:
//! - **Compose overlay**: modal overlay with To, Priority, Tags, Body fields,
//!   tab completion, draft persistence, and reply context.
//! - **Quick-send bar**: command bar for rapid `:target message` messaging
//!   with history and tab completion.

use forge_ftui_adapter::input::{InputEvent, Key};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum quick-send history entries.
pub const QUICK_HISTORY_LIMIT: usize = 100;

/// Compose priority values (Go parity: low, normal, high).
pub const PRIORITIES: &[&str] = &["low", "normal", "high"];

/// Default priority for new messages.
pub const DEFAULT_PRIORITY: &str = "normal";

/// Spinner animation frames.
const SPINNER_FRAMES: &[&str] = &["|", "/", "-", "\\"];

/// Toast display duration (seconds).
const TOAST_DURATION_SECS: u64 = 2;

// ---------------------------------------------------------------------------
// ComposeField
// ---------------------------------------------------------------------------

/// Which field has focus in the compose overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    To = 0,
    Priority = 1,
    Tags = 2,
    Body = 3,
}

impl ComposeField {
    /// Advance to next field (wrapping).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::To => Self::Priority,
            Self::Priority => Self::Tags,
            Self::Tags => Self::Body,
            Self::Body => Self::To,
        }
    }

    /// Go to previous field (wrapping).
    #[must_use]
    pub fn prev(self) -> Self {
        match self {
            Self::To => Self::Body,
            Self::Priority => Self::To,
            Self::Tags => Self::Priority,
            Self::Body => Self::Tags,
        }
    }
}

// ---------------------------------------------------------------------------
// SendSource
// ---------------------------------------------------------------------------

/// Whether the send originated from compose overlay or quick-send bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendSource {
    Compose,
    Quick,
}

// ---------------------------------------------------------------------------
// ComposeReplySeed
// ---------------------------------------------------------------------------

/// Seed data for opening compose as a reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeReplySeed {
    pub target: String,
    pub reply_to: String,
    pub parent_line: String,
}

// ---------------------------------------------------------------------------
// ComposeDraft
// ---------------------------------------------------------------------------

/// Persistent draft saved to TUI state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeDraft {
    pub target: String,
    pub to: String,
    pub priority: String,
    pub tags: String,
    pub reply_to: String,
    pub body: String,
    pub updated_at_epoch_secs: u64,
}

// ---------------------------------------------------------------------------
// SendRequest
// ---------------------------------------------------------------------------

/// A normalized send request ready for dispatch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendRequest {
    pub from: String,
    pub to: String,
    pub body: String,
    pub reply_to: String,
    pub priority: String,
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// ComposeAction
// ---------------------------------------------------------------------------

/// Commands emitted by compose input handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    /// No action needed.
    None,
    /// Initiate a send with the given source.
    Send(SendSource),
    /// Close the compose overlay.
    Close,
    /// Display a toast message.
    Toast(String),
}

// ---------------------------------------------------------------------------
// ComposeState
// ---------------------------------------------------------------------------

/// Internal compose overlay state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeState {
    pub active: bool,
    pub focus: ComposeField,
    pub to: String,
    pub priority: String,
    pub tags: String,
    pub reply_to: String,
    pub parent_line: String,
    pub body: String,
    pub sending: bool,
    pub err: String,
    pub save_prompt: bool,
    pub restore_ask: bool,
    pub draft_cached: ComposeDraft,

    pub to_completion_prefix: String,
    pub to_completion_index: i32,
    pub tag_completion_prefix: String,
    pub tag_completion_index: i32,
}

impl Default for ComposeState {
    fn default() -> Self {
        Self {
            active: false,
            focus: ComposeField::Body,
            to: String::new(),
            priority: DEFAULT_PRIORITY.to_owned(),
            tags: String::new(),
            reply_to: String::new(),
            parent_line: String::new(),
            body: String::new(),
            sending: false,
            err: String::new(),
            save_prompt: false,
            restore_ask: false,
            draft_cached: ComposeDraft::default(),
            to_completion_prefix: String::new(),
            to_completion_index: -1,
            tag_completion_prefix: String::new(),
            tag_completion_index: -1,
        }
    }
}

// ---------------------------------------------------------------------------
// QuickSendState
// ---------------------------------------------------------------------------

/// Quick-send bar state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickSendState {
    pub active: bool,
    pub input: String,
    pub err: String,
    pub sending: bool,

    pub history: Vec<String>,
    pub history_index: i32,

    pub completion_prefix: String,
    pub completion_index: i32,
}

impl Default for QuickSendState {
    fn default() -> Self {
        Self {
            active: false,
            input: String::new(),
            err: String::new(),
            sending: false,
            history: Vec::new(),
            history_index: -1,
            completion_prefix: String::new(),
            completion_index: -1,
        }
    }
}

// ---------------------------------------------------------------------------
// ComposeViewModel
// ---------------------------------------------------------------------------

/// Unified view-model for compose overlay and quick-send bar.
#[derive(Debug, Clone)]
pub struct ComposeViewModel {
    pub compose: ComposeState,
    pub quick: QuickSendState,
    pub self_agent: String,
    pub spinner_frame: usize,
    pub toast: String,
    pub toast_until_epoch_secs: u64,
}

impl ComposeViewModel {
    #[must_use]
    pub fn new(self_agent: &str) -> Self {
        Self {
            compose: ComposeState::default(),
            quick: QuickSendState::default(),
            self_agent: self_agent.to_owned(),
            spinner_frame: 0,
            toast: String::new(),
            toast_until_epoch_secs: 0,
        }
    }

    /// Open compose overlay.
    /// `draft` is an optional saved draft for the target.
    pub fn open_compose(
        &mut self,
        target: &str,
        seed: &ComposeReplySeed,
        draft: Option<&ComposeDraft>,
    ) {
        self.quick.active = false;
        self.quick.err.clear();
        self.compose = ComposeState {
            active: true,
            focus: if target.trim().is_empty() {
                ComposeField::To
            } else {
                ComposeField::Body
            },
            to: target.trim().to_owned(),
            priority: DEFAULT_PRIORITY.to_owned(),
            tags: String::new(),
            reply_to: seed.reply_to.trim().to_owned(),
            parent_line: seed.parent_line.trim().to_owned(),
            body: String::new(),
            sending: false,
            err: String::new(),
            save_prompt: false,
            restore_ask: false,
            draft_cached: ComposeDraft::default(),
            to_completion_prefix: String::new(),
            to_completion_index: -1,
            tag_completion_prefix: String::new(),
            tag_completion_index: -1,
        };

        if let Some(d) = draft {
            if !d.body.trim().is_empty() {
                self.compose.restore_ask = true;
                self.compose.draft_cached = d.clone();
            }
        }
    }

    /// Open quick-send bar.
    pub fn open_quick_send(&mut self) {
        self.compose.active = false;
        self.compose.err.clear();
        self.quick.active = true;
        self.quick.input = ":".to_owned();
        self.quick.err.clear();
        self.quick.sending = false;
        self.quick.history_index = -1;
    }

    /// Close compose overlay (reset state).
    pub fn close_compose(&mut self) {
        self.compose = ComposeState::default();
    }

    /// Whether compose or quick-send is active (absorbing keys).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.compose.active || self.quick.active
    }

    /// Build a `SendRequest` from the current compose or quick-send state.
    pub fn build_send_request(&self, source: SendSource) -> Result<SendRequest, String> {
        match source {
            SendSource::Quick => {
                let (target, body, ok) = parse_quick_send_input(&self.quick.input);
                if !ok {
                    return Err("expected :<target> <message>".to_owned());
                }
                Ok(SendRequest {
                    from: self.self_agent.clone(),
                    to: target,
                    body,
                    priority: DEFAULT_PRIORITY.to_owned(),
                    ..Default::default()
                })
            }
            SendSource::Compose => {
                let to = self.compose.to.trim().to_owned();
                if to.is_empty() {
                    return Err("missing target".to_owned());
                }
                let body = self.compose.body.trim().to_owned();
                if body.is_empty() {
                    return Err("message body is empty".to_owned());
                }
                Ok(SendRequest {
                    from: self.self_agent.clone(),
                    to,
                    body,
                    reply_to: self.compose.reply_to.trim().to_owned(),
                    priority: normalize_priority(&self.compose.priority),
                    tags: parse_tag_csv(&self.compose.tags),
                })
            }
        }
    }

    /// Mark send as in-progress for the given source.
    pub fn mark_sending(&mut self, source: SendSource) {
        match source {
            SendSource::Compose => {
                self.compose.sending = true;
                self.compose.err.clear();
            }
            SendSource::Quick => {
                self.quick.sending = true;
                self.quick.err.clear();
            }
        }
    }

    /// Handle a send result. Returns a toast message on success.
    pub fn handle_send_result(
        &mut self,
        source: SendSource,
        req: &SendRequest,
        err: Option<&str>,
    ) -> Option<String> {
        match source {
            SendSource::Compose => {
                self.compose.sending = false;
                if let Some(e) = err {
                    self.compose.err = e.to_owned();
                    return None;
                }
                self.close_compose();
                Some("Sent \u{2713}".to_owned())
            }
            SendSource::Quick => {
                self.quick.sending = false;
                if let Some(e) = err {
                    self.quick.err = e.to_owned();
                    return None;
                }
                self.quick.active = false;
                self.quick.err.clear();
                self.quick.input.clear();
                self.quick.history_index = -1;
                self.record_quick_history(req);
                Some("Sent \u{2713}".to_owned())
            }
        }
    }

    /// Set error for the given source.
    pub fn set_error(&mut self, source: SendSource, err: &str) {
        match source {
            SendSource::Compose => self.compose.err = err.to_owned(),
            SendSource::Quick => self.quick.err = err.to_owned(),
        }
    }

    /// Set toast text with a duration.
    pub fn set_toast(&mut self, text: &str, now_epoch_secs: u64) {
        self.toast = text.trim().to_owned();
        self.toast_until_epoch_secs = now_epoch_secs + TOAST_DURATION_SECS;
    }

    /// Build a draft from compose state.
    #[must_use]
    pub fn build_draft(&self, now_epoch_secs: u64) -> ComposeDraft {
        let target = self.compose.to.trim().to_owned();
        ComposeDraft {
            target: target.clone(),
            to: target,
            priority: normalize_priority(&self.compose.priority),
            tags: self.compose.tags.trim().to_owned(),
            reply_to: self.compose.reply_to.trim().to_owned(),
            body: self.compose.body.trim().to_owned(),
            updated_at_epoch_secs: now_epoch_secs,
        }
    }

    /// Restore cached draft into compose fields.
    pub fn restore_draft(&mut self) {
        let d = &self.compose.draft_cached;
        self.compose.to = d.to.clone();
        self.compose.priority = d.priority.clone();
        self.compose.tags = d.tags.clone();
        self.compose.reply_to = d.reply_to.clone();
        self.compose.body = d.body.clone();
        self.compose.restore_ask = false;
    }

    /// Record quick-send history.
    fn record_quick_history(&mut self, req: &SendRequest) {
        let line = format!(":{} {}", req.to.trim(), req.body.trim())
            .trim()
            .to_owned();
        if line.is_empty() {
            return;
        }
        if !self.quick.history.last().is_some_and(|last| last == &line) {
            self.quick.history.push(line);
            if self.quick.history.len() > QUICK_HISTORY_LIMIT {
                let excess = self.quick.history.len() - QUICK_HISTORY_LIMIT;
                self.quick.history.drain(..excess);
            }
        }
    }

    /// Navigate quick-send history.
    pub fn quick_history_step(&mut self, delta: i32) {
        if self.quick.history.is_empty() {
            return;
        }
        if self.quick.history_index < 0 {
            if delta > 0 {
                return;
            }
            self.quick.history_index = (self.quick.history.len() as i32) - 1;
            self.quick.input = self.quick.history[self.quick.history_index as usize].clone();
            return;
        }
        let next = self.quick.history_index + delta;
        if next < 0 {
            self.quick.history_index = 0;
            self.quick.input = self.quick.history[0].clone();
            return;
        }
        if next >= self.quick.history.len() as i32 {
            self.quick.history_index = -1;
            self.quick.input = ":".to_owned();
            return;
        }
        self.quick.history_index = next;
        self.quick.input = self.quick.history[next as usize].clone();
    }

    /// Cycle compose priority up/down.
    pub fn cycle_priority(&mut self, delta: i32) {
        let current = normalize_priority(&self.compose.priority);
        let idx = PRIORITIES.iter().position(|&p| p == current).unwrap_or(1) as i32;
        let len = PRIORITIES.len() as i32;
        let next = ((idx + delta) % len + len) % len;
        self.compose.priority = PRIORITIES[next as usize].to_owned();
    }

    /// Delete last rune from the focused compose field.
    pub fn compose_delete_rune(&mut self) {
        let field = match self.compose.focus {
            ComposeField::To => &mut self.compose.to,
            ComposeField::Priority => &mut self.compose.priority,
            ComposeField::Tags => &mut self.compose.tags,
            ComposeField::Body => &mut self.compose.body,
        };
        if !field.is_empty() {
            field.pop();
        }
        self.reset_compose_completion();
    }

    /// Insert a character into the focused compose field.
    pub fn compose_insert_char(&mut self, ch: &str) {
        match self.compose.focus {
            ComposeField::To => self.compose.to.push_str(ch),
            ComposeField::Priority => self.compose.priority.push_str(&ch.to_lowercase()),
            ComposeField::Tags => self.compose.tags.push_str(&ch.to_lowercase()),
            ComposeField::Body => self.compose.body.push_str(ch),
        }
        self.reset_compose_completion();
    }

    /// Tab-complete target in compose To field.
    pub fn complete_compose_target(&mut self, known_targets: &[String]) {
        let prefix = self.compose.to.trim().to_owned();
        let choices = filter_prefix(known_targets, &prefix);
        if choices.is_empty() {
            return;
        }
        if self.compose.to_completion_prefix != prefix {
            self.compose.to_completion_prefix = prefix;
            self.compose.to_completion_index = 0;
        } else {
            self.compose.to_completion_index =
                (self.compose.to_completion_index + 1) % choices.len() as i32;
        }
        self.compose.to = choices[self.compose.to_completion_index as usize].clone();
    }

    /// Tab-complete tag in compose Tags field.
    pub fn complete_compose_tag(&mut self, known_tags: &[String]) {
        if known_tags.is_empty() {
            return;
        }
        let parts: Vec<&str> = self.compose.tags.split(',').collect();
        let prefix = parts.last().map_or("", |s| s.trim()).to_owned();
        let choices = filter_prefix(known_tags, &prefix);
        if choices.is_empty() {
            return;
        }
        if self.compose.tag_completion_prefix != prefix {
            self.compose.tag_completion_prefix = prefix;
            self.compose.tag_completion_index = 0;
        } else {
            self.compose.tag_completion_index =
                (self.compose.tag_completion_index + 1) % choices.len() as i32;
        }
        let mut parts: Vec<String> = self.compose.tags.split(',').map(|s| s.to_owned()).collect();
        if let Some(last) = parts.last_mut() {
            *last = format!(" {}", choices[self.compose.tag_completion_index as usize]);
        }
        let updated = parts.join(",");
        self.compose.tags = updated.trim_start().to_owned();
    }

    /// Tab-complete target in quick-send bar.
    pub fn complete_quick_target(&mut self, known_targets: &[String]) {
        let mut input = self.quick.input.trim().to_owned();
        if input.is_empty() {
            input = ":".to_owned();
        }
        if !input.starts_with(':') {
            input = format!(":{input}");
        }
        let rest = &input[1..];
        if rest.contains(' ') {
            return;
        }
        let prefix = rest.trim().to_owned();
        let choices = filter_prefix(known_targets, &prefix);
        if choices.is_empty() {
            return;
        }
        if self.quick.completion_prefix != prefix {
            self.quick.completion_prefix = prefix;
            self.quick.completion_index = 0;
        } else {
            self.quick.completion_index = (self.quick.completion_index + 1) % choices.len() as i32;
        }
        self.quick.input = format!(":{} ", choices[self.quick.completion_index as usize]);
    }

    /// Reset compose completion state.
    pub fn reset_compose_completion(&mut self) {
        self.compose.to_completion_prefix.clear();
        self.compose.to_completion_index = -1;
        self.compose.tag_completion_prefix.clear();
        self.compose.tag_completion_index = -1;
    }

    /// Reset quick-send completion state.
    pub fn reset_quick_completion(&mut self) {
        self.quick.completion_prefix.clear();
        self.quick.completion_index = -1;
    }

    /// Advance spinner frame.
    pub fn tick_spinner(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
    }
}

// ---------------------------------------------------------------------------
// Input handling
// ---------------------------------------------------------------------------

/// Process an input event for compose overlay. Returns a `ComposeAction`.
///
/// `known_targets` and `known_tags` are used for tab-completion.
#[must_use]
pub fn apply_compose_input(
    vm: &mut ComposeViewModel,
    event: InputEvent,
    known_targets: &[String],
    known_tags: &[String],
) -> ComposeAction {
    if vm.compose.active {
        return handle_compose_overlay_key(vm, event, known_targets, known_tags);
    }
    if vm.quick.active {
        return handle_quick_send_key(vm, event, known_targets);
    }
    ComposeAction::None
}

fn handle_compose_overlay_key(
    vm: &mut ComposeViewModel,
    event: InputEvent,
    known_targets: &[String],
    known_tags: &[String],
) -> ComposeAction {
    let key_event = match event {
        InputEvent::Key(k) => k,
        _ => return ComposeAction::None,
    };

    // Restore-ask prompt.
    if vm.compose.restore_ask {
        match key_event.key {
            Key::Char('y') | Key::Char('Y') => {
                vm.restore_draft();
                return ComposeAction::None;
            }
            Key::Char('n') | Key::Char('N') | Key::Escape | Key::Backspace => {
                vm.compose.restore_ask = false;
                return ComposeAction::None;
            }
            _ => return ComposeAction::None,
        }
    }

    // Save-prompt on close.
    if vm.compose.save_prompt {
        match key_event.key {
            Key::Char('y') | Key::Char('Y') => {
                // Discard draft, close.
                vm.close_compose();
                return ComposeAction::Close;
            }
            Key::Char('s') | Key::Char('S') => {
                // Save draft, close.
                vm.compose.save_prompt = false;
                vm.close_compose();
                return ComposeAction::Toast("Draft saved".to_owned());
            }
            Key::Char('n') | Key::Char('N') | Key::Escape | Key::Backspace => {
                vm.compose.save_prompt = false;
                return ComposeAction::None;
            }
            _ => return ComposeAction::None,
        }
    }

    // Block input while sending.
    if vm.compose.sending {
        return ComposeAction::None;
    }

    match key_event.key {
        Key::Tab if !key_event.modifiers.shift => {
            // Try target/tag completion first; if unchanged, advance field.
            if vm.compose.focus == ComposeField::To {
                let before = vm.compose.to.clone();
                vm.complete_compose_target(known_targets);
                if vm.compose.to != before {
                    return ComposeAction::None;
                }
            }
            if vm.compose.focus == ComposeField::Tags {
                let before = vm.compose.tags.clone();
                vm.complete_compose_tag(known_tags);
                if vm.compose.tags != before {
                    return ComposeAction::None;
                }
            }
            vm.compose.focus = vm.compose.focus.next();
            ComposeAction::None
        }
        Key::Tab if key_event.modifiers.shift => {
            vm.compose.focus = vm.compose.focus.prev();
            ComposeAction::None
        }
        Key::Escape => {
            if !vm.compose.body.trim().is_empty() {
                vm.compose.save_prompt = true;
                return ComposeAction::None;
            }
            vm.close_compose();
            ComposeAction::Close
        }
        Key::Enter if key_event.modifiers.ctrl => ComposeAction::Send(SendSource::Compose),
        Key::Enter => {
            match vm.compose.focus {
                ComposeField::To | ComposeField::Priority | ComposeField::Tags => {
                    vm.compose.focus = vm.compose.focus.next();
                }
                ComposeField::Body => {
                    vm.compose.body.push('\n');
                }
            }
            ComposeAction::None
        }
        Key::Up => {
            if vm.compose.focus == ComposeField::Priority {
                vm.cycle_priority(-1);
            }
            ComposeAction::None
        }
        Key::Down => {
            if vm.compose.focus == ComposeField::Priority {
                vm.cycle_priority(1);
            }
            ComposeAction::None
        }
        Key::Backspace => {
            vm.compose_delete_rune();
            ComposeAction::None
        }
        Key::Char('h') if key_event.modifiers.ctrl => {
            vm.compose_delete_rune();
            ComposeAction::None
        }
        Key::Char('j') if key_event.modifiers.ctrl => ComposeAction::Send(SendSource::Compose),
        Key::Char('\n') if key_event.modifiers.alt => {
            vm.compose.body.push('\n');
            ComposeAction::None
        }
        Key::Char(c) => {
            vm.compose_insert_char(&c.to_string());
            ComposeAction::None
        }
        _ => ComposeAction::None,
    }
}

fn handle_quick_send_key(
    vm: &mut ComposeViewModel,
    event: InputEvent,
    known_targets: &[String],
) -> ComposeAction {
    let key_event = match event {
        InputEvent::Key(k) => k,
        _ => return ComposeAction::None,
    };

    if vm.quick.sending {
        return ComposeAction::None;
    }

    match key_event.key {
        Key::Escape => {
            vm.quick.active = false;
            vm.quick.err.clear();
            vm.quick.input.clear();
            vm.quick.history_index = -1;
            ComposeAction::Close
        }
        Key::Backspace => {
            let trimmed = vm.quick.input.trim();
            if trimmed.is_empty() || trimmed == ":" {
                vm.quick.active = false;
                vm.quick.err.clear();
                vm.quick.input.clear();
                vm.quick.history_index = -1;
                return ComposeAction::Close;
            }
            if !vm.quick.input.is_empty() {
                vm.quick.input.pop();
            }
            vm.reset_quick_completion();
            ComposeAction::None
        }
        Key::Char('h') if key_event.modifiers.ctrl => {
            if !vm.quick.input.is_empty() {
                vm.quick.input.pop();
            }
            vm.reset_quick_completion();
            ComposeAction::None
        }
        Key::Up => {
            vm.quick_history_step(-1);
            ComposeAction::None
        }
        Key::Down => {
            vm.quick_history_step(1);
            ComposeAction::None
        }
        Key::Tab => {
            vm.complete_quick_target(known_targets);
            ComposeAction::None
        }
        Key::Enter => ComposeAction::Send(SendSource::Quick),
        Key::Char(c) => {
            vm.quick.input.push(c);
            vm.quick.err.clear();
            vm.reset_quick_completion();
            ComposeAction::None
        }
        _ => ComposeAction::None,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the compose overlay as a centered modal panel.
#[must_use]
pub fn render_compose_frame(
    vm: &ComposeViewModel,
    size: FrameSize,
    theme: &ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(size, *theme);
    let width = size.width;
    let height = size.height;

    let panel_width = width.saturating_sub(8).clamp(50, 96);
    let panel_height = height.saturating_sub(4).clamp(14, height);

    let c = &vm.compose;
    let mut to = c.to.clone();
    let priority = normalize_priority(&c.priority);
    let mut tags = c.tags.clone();
    let mut body = c.body.clone();

    let cursor = if c.sending { "" } else { "_" };

    if c.focus == ComposeField::To && !c.sending {
        to.push_str(cursor);
    }
    let mut priority_display = priority.clone();
    if c.focus == ComposeField::Priority && !c.sending {
        priority_display.push_str(cursor);
    }
    if c.focus == ComposeField::Tags && !c.sending {
        tags.push_str(cursor);
    }
    if c.focus == ComposeField::Body && !c.sending {
        body.push_str(cursor);
    }

    // Build header.
    let mut head = "Compose".to_owned();
    if !c.reply_to.trim().is_empty() {
        head.push_str(&format!("  reply {}", short_id(&c.reply_to)));
    }

    // Build status line.
    let status = if c.save_prompt {
        "Discard draft? [y] discard  [s] save  [Esc] cancel".to_owned()
    } else if c.restore_ask {
        "Restore saved draft? [y/N]".to_owned()
    } else if !c.err.trim().is_empty() {
        format!("Send failed: {}", c.err)
    } else if c.sending {
        format!("Sending... {}", spinner_frame(vm.spinner_frame))
    } else {
        "[Ctrl+Enter: Send] [Esc: Close] [Tab: Next]".to_owned()
    };

    // Build lines.
    let mut lines: Vec<String> = Vec::new();
    lines.push(head);
    lines.push(String::new());

    if !c.reply_to.trim().is_empty() && !c.parent_line.trim().is_empty() {
        lines.push(format!("Replying to: {}", truncate(&c.parent_line, 60)));
        lines.push(String::new());
    }

    lines.push(format!("To: {to}"));
    lines.push(format!("Priority: {priority_display}"));
    lines.push(format!("Tags: {tags}"));

    if !c.reply_to.trim().is_empty() {
        let mut reply_line = format!("Reply to: {}", c.reply_to);
        if !c.parent_line.trim().is_empty() {
            reply_line.push_str(&format!(" ({})", truncate(&c.parent_line, 40)));
        }
        lines.push(reply_line);
    }

    lines.push(String::new());
    lines.push("Body:".to_owned());

    let max_body = (panel_height.saturating_sub(lines.len()).saturating_sub(4)).max(3);
    let body_lines: Vec<&str> = body.split('\n').collect();
    let start = if body_lines.len() > max_body {
        body_lines.len() - max_body
    } else {
        0
    };
    for bl in &body_lines[start..] {
        if bl.trim().is_empty() {
            lines.push("  ".to_owned());
        } else {
            for wrapped in wrap_lines(bl, panel_width.saturating_sub(6).max(8)) {
                lines.push(format!("  {wrapped}"));
            }
        }
    }

    lines.push(String::new());
    lines.push(status);

    // Center the panel.
    let start_x = width.saturating_sub(panel_width) / 2;
    let start_y = height.saturating_sub(panel_height) / 2;

    // Draw border.
    frame.draw_text(start_x, start_y, "\u{256d}", TextRole::Accent);
    let top_border = "\u{2500}".repeat(panel_width.saturating_sub(2));
    frame.draw_text(start_x + 1, start_y, &top_border, TextRole::Accent);
    frame.draw_text(
        start_x + panel_width.saturating_sub(1),
        start_y,
        "\u{256e}",
        TextRole::Accent,
    );

    for (i, line) in lines.iter().enumerate() {
        let y = start_y + 1 + i;
        if y >= start_y + panel_height.saturating_sub(1) {
            break;
        }
        frame.draw_text(start_x, y, "\u{2502}", TextRole::Accent);
        let padded = format!("  {:<width$}", line, width = panel_width.saturating_sub(4));
        let display = &padded[..padded.len().min(panel_width.saturating_sub(2))];
        frame.draw_text(start_x + 1, y, display, TextRole::Primary);
        frame.draw_text(
            start_x + panel_width.saturating_sub(1),
            y,
            "\u{2502}",
            TextRole::Accent,
        );
    }

    let bottom_y = start_y + panel_height.saturating_sub(1);
    frame.draw_text(start_x, bottom_y, "\u{2570}", TextRole::Accent);
    frame.draw_text(start_x + 1, bottom_y, &top_border, TextRole::Accent);
    frame.draw_text(
        start_x + panel_width.saturating_sub(1),
        bottom_y,
        "\u{256f}",
        TextRole::Accent,
    );

    frame
}

/// Render the quick-send bar (single line at bottom).
#[must_use]
pub fn render_quick_send_bar(vm: &ComposeViewModel, width: usize, _theme: &ThemeSpec) -> String {
    let mut input = vm.quick.input.clone();
    if input.is_empty() {
        input = ":".to_owned();
    }
    if !vm.quick.sending {
        input.push('_');
    }
    let mut status = String::new();
    if vm.quick.sending {
        status = format!("  Sending... {}", spinner_frame(vm.spinner_frame));
    }
    if !vm.quick.err.trim().is_empty() {
        status = format!("  error: {}", vm.quick.err);
    }
    let line = format!("quick-send  {input}{status}");
    truncate(&line, width)
}

/// Render toast message (returns empty string if no active toast).
#[must_use]
pub fn render_toast(vm: &ComposeViewModel, now_epoch_secs: u64) -> String {
    if vm.toast.trim().is_empty() {
        return String::new();
    }
    if vm.toast_until_epoch_secs > 0 && now_epoch_secs > vm.toast_until_epoch_secs {
        return String::new();
    }
    vm.toast.clone()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse quick-send input `:target message`.
#[must_use]
pub fn parse_quick_send_input(input: &str) -> (String, String, bool) {
    let trimmed = input.trim().trim_start_matches(':').trim();
    if trimmed.is_empty() {
        return (String::new(), String::new(), false);
    }
    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return (String::new(), String::new(), false);
    }
    let target = parts[0].trim().to_owned();
    let body = parts[1].trim().to_owned();
    if target.is_empty() || body.is_empty() {
        return (String::new(), String::new(), false);
    }
    (target, body, true)
}

/// Normalize a priority string to one of the valid values.
#[must_use]
pub fn normalize_priority(value: &str) -> String {
    let v = value.trim().to_lowercase();
    match v.as_str() {
        "low" | "normal" | "high" => v,
        _ => DEFAULT_PRIORITY.to_owned(),
    }
}

/// Parse a comma-separated tag string into deduplicated lowercase tags.
#[must_use]
pub fn parse_tag_csv(csv: &str) -> Vec<String> {
    if csv.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in csv.split(',') {
        let tag = part.trim().to_lowercase();
        if tag.is_empty() {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}

/// Filter values by prefix (case-insensitive).
#[must_use]
pub fn filter_prefix(values: &[String], prefix: &str) -> Vec<String> {
    let prefix_lower = prefix.trim().to_lowercase();
    if values.is_empty() {
        return Vec::new();
    }
    if prefix_lower.is_empty() {
        return values.to_vec();
    }
    values
        .iter()
        .filter(|v| v.to_lowercase().starts_with(&prefix_lower))
        .cloned()
        .collect()
}

/// Spinner frame character.
fn spinner_frame(frame: usize) -> &'static str {
    SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
}

/// Short message ID (first 8 chars).
fn short_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.len() <= 8 {
        trimmed.to_owned()
    } else {
        trimmed[..8].to_owned()
    }
}

/// Truncate string to max visible width.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_owned()
    }
}

/// Wrap a single line into multiple lines at the given width.
fn wrap_lines(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![s.to_owned()];
    }
    let mut out = Vec::new();
    let mut remaining = s;
    while remaining.len() > max_width {
        out.push(remaining[..max_width].to_owned());
        remaining = &remaining[max_width..];
    }
    if !remaining.is_empty() || out.is_empty() {
        out.push(remaining.to_owned());
    }
    out
}

/// Get the first non-empty line from a multiline string.
#[must_use]
pub fn first_non_empty_line(text: &str) -> String {
    for line in text.replace("\r\n", "\n").split('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers};
    use forge_ftui_adapter::render::FrameSize;
    use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

    fn theme() -> ThemeSpec {
        ThemeSpec::for_kind(ThemeKind::HighContrast)
    }

    fn key(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent::plain(k))
    }

    fn key_shift(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: k,
            modifiers: Modifiers {
                shift: true,
                ctrl: false,
                alt: false,
            },
        })
    }

    fn key_ctrl(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: k,
            modifiers: Modifiers {
                shift: false,
                ctrl: true,
                alt: false,
            },
        })
    }

    fn char_key(c: char) -> InputEvent {
        key(Key::Char(c))
    }

    // --- parse_quick_send_input ---

    #[test]
    fn parse_quick_send_valid() {
        let (target, body, ok) = parse_quick_send_input(":task implement JWT auth");
        assert!(ok);
        assert_eq!(target, "task");
        assert_eq!(body, "implement JWT auth");
    }

    #[test]
    fn parse_quick_send_missing_body() {
        let (_, _, ok) = parse_quick_send_input(":task");
        assert!(!ok);
    }

    #[test]
    fn parse_quick_send_empty() {
        let (_, _, ok) = parse_quick_send_input(":");
        assert!(!ok);
    }

    #[test]
    fn parse_quick_send_no_colon() {
        let (target, body, ok) = parse_quick_send_input("task hello world");
        assert!(ok);
        assert_eq!(target, "task");
        assert_eq!(body, "hello world");
    }

    #[test]
    fn parse_quick_send_whitespace_only() {
        let (_, _, ok) = parse_quick_send_input("   ");
        assert!(!ok);
    }

    // --- normalize_priority ---

    #[test]
    fn normalize_priority_valid() {
        assert_eq!(normalize_priority("low"), "low");
        assert_eq!(normalize_priority("HIGH"), "high");
        assert_eq!(normalize_priority("Normal"), "normal");
    }

    #[test]
    fn normalize_priority_invalid() {
        assert_eq!(normalize_priority("urgent"), "normal");
        assert_eq!(normalize_priority(""), "normal");
    }

    // --- parse_tag_csv ---

    #[test]
    fn parse_tag_csv_basic() {
        let tags = parse_tag_csv("Auth, Urgent, auth");
        assert_eq!(tags, vec!["auth", "urgent"]);
    }

    #[test]
    fn parse_tag_csv_empty() {
        assert!(parse_tag_csv("").is_empty());
        assert!(parse_tag_csv("  ").is_empty());
    }

    // --- filter_prefix ---

    #[test]
    fn filter_prefix_basic() {
        let values: Vec<String> = vec!["@alice".into(), "@bob".into(), "task".into()];
        let result = filter_prefix(&values, "@");
        assert_eq!(result, vec!["@alice", "@bob"]);
    }

    #[test]
    fn filter_prefix_empty_prefix() {
        let values: Vec<String> = vec!["a".into(), "b".into()];
        assert_eq!(filter_prefix(&values, ""), values);
    }

    #[test]
    fn filter_prefix_no_match() {
        let values: Vec<String> = vec!["x".into()];
        assert!(filter_prefix(&values, "z").is_empty());
    }

    // --- first_non_empty_line ---

    #[test]
    fn first_non_empty_line_basic() {
        assert_eq!(first_non_empty_line("\n\n  hello\nworld"), "hello");
        assert_eq!(first_non_empty_line(""), "");
    }

    // --- ComposeField navigation ---

    #[test]
    fn compose_field_next_wraps() {
        assert_eq!(ComposeField::To.next(), ComposeField::Priority);
        assert_eq!(ComposeField::Body.next(), ComposeField::To);
    }

    #[test]
    fn compose_field_prev_wraps() {
        assert_eq!(ComposeField::To.prev(), ComposeField::Body);
        assert_eq!(ComposeField::Body.prev(), ComposeField::Tags);
    }

    // --- ComposeViewModel basics ---

    #[test]
    fn open_compose_sets_focus_to_body_when_target_given() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        assert!(vm.compose.active);
        assert_eq!(vm.compose.focus, ComposeField::Body);
        assert_eq!(vm.compose.to, "task");
    }

    #[test]
    fn open_compose_sets_focus_to_to_when_no_target() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("", &ComposeReplySeed::default(), None);
        assert_eq!(vm.compose.focus, ComposeField::To);
    }

    #[test]
    fn open_compose_with_draft_triggers_restore_ask() {
        let mut vm = ComposeViewModel::new("me");
        let draft = ComposeDraft {
            body: "saved text".into(),
            to: "task".into(),
            ..Default::default()
        };
        vm.open_compose("task", &ComposeReplySeed::default(), Some(&draft));
        assert!(vm.compose.restore_ask);
        assert_eq!(vm.compose.draft_cached.body, "saved text");
    }

    #[test]
    fn open_quick_send() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        assert!(vm.quick.active);
        assert_eq!(vm.quick.input, ":");
    }

    // --- Cycle priority ---

    #[test]
    fn cycle_priority_forward() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.priority = "normal".into();
        vm.cycle_priority(1);
        assert_eq!(vm.compose.priority, "high");
        vm.cycle_priority(1);
        assert_eq!(vm.compose.priority, "low");
    }

    #[test]
    fn cycle_priority_backward() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.priority = "normal".into();
        vm.cycle_priority(-1);
        assert_eq!(vm.compose.priority, "low");
    }

    // --- Delete rune ---

    #[test]
    fn compose_delete_rune_body() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.focus = ComposeField::Body;
        vm.compose.body = "hello".into();
        vm.compose_delete_rune();
        assert_eq!(vm.compose.body, "hell");
    }

    #[test]
    fn compose_delete_rune_empty() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.focus = ComposeField::To;
        vm.compose.to.clear();
        vm.compose_delete_rune();
        assert!(vm.compose.to.is_empty());
    }

    // --- Insert char ---

    #[test]
    fn compose_insert_char_to_field() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.focus = ComposeField::To;
        vm.compose_insert_char("@");
        vm.compose_insert_char("a");
        assert_eq!(vm.compose.to, "@a");
    }

    #[test]
    fn compose_insert_char_priority_lowercases() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.focus = ComposeField::Priority;
        vm.compose.priority.clear();
        vm.compose_insert_char("H");
        assert_eq!(vm.compose.priority, "h");
    }

    // --- Tab completion ---

    #[test]
    fn complete_compose_target_first_match() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.to = "@a".into();
        let targets: Vec<String> = vec!["@alice".into(), "@anna".into(), "@bob".into()];
        vm.complete_compose_target(&targets);
        assert_eq!(vm.compose.to, "@alice");
        // Second call sees "@alice" as prefix — only matches "@alice" now.
        vm.complete_compose_target(&targets);
        assert_eq!(vm.compose.to, "@alice");
    }

    #[test]
    fn complete_compose_target_no_match() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.to = "@z".into();
        let targets: Vec<String> = vec!["@alice".into()];
        vm.complete_compose_target(&targets);
        assert_eq!(vm.compose.to, "@z"); // no match, unchanged
    }

    #[test]
    fn complete_quick_target_first_match() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.input = ":ta".into();
        let targets: Vec<String> = vec!["task".into(), "team".into()];
        vm.complete_quick_target(&targets);
        assert_eq!(vm.quick.input, ":task ");
    }

    #[test]
    fn complete_quick_target_after_space_noop() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.input = ":task hello".into(); // has space → already past target
        let targets: Vec<String> = vec!["task".into(), "team".into()];
        vm.complete_quick_target(&targets);
        assert_eq!(vm.quick.input, ":task hello"); // unchanged
    }

    #[test]
    fn complete_compose_tag_first_match() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.tags = "auth, u".into();
        let tags: Vec<String> = vec!["urgent".into(), "update".into()];
        vm.complete_compose_tag(&tags);
        assert_eq!(vm.compose.tags, "auth, urgent");
    }

    #[test]
    fn complete_compose_tag_empty_tags() {
        let mut vm = ComposeViewModel::new("me");
        vm.complete_compose_tag(&[]);
        assert!(vm.compose.tags.is_empty());
    }

    // --- Quick history ---

    #[test]
    fn quick_history_navigation() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.history = vec![":task hello".into(), ":task world".into()];
        vm.quick.history_index = -1;
        vm.quick_history_step(-1);
        assert_eq!(vm.quick.input, ":task world");
        vm.quick_history_step(-1);
        assert_eq!(vm.quick.input, ":task hello");
        vm.quick_history_step(-1);
        assert_eq!(vm.quick.input, ":task hello"); // at start, stays
        vm.quick_history_step(1);
        assert_eq!(vm.quick.input, ":task world");
        vm.quick_history_step(1);
        assert_eq!(vm.quick.input, ":"); // past end resets
    }

    #[test]
    fn quick_history_limit() {
        let mut vm = ComposeViewModel::new("me");
        for i in 0..150 {
            let req = SendRequest {
                to: "task".into(),
                body: format!("msg{i}"),
                ..Default::default()
            };
            vm.record_quick_history(&req);
        }
        assert_eq!(vm.quick.history.len(), QUICK_HISTORY_LIMIT);
    }

    // --- Build send request ---

    #[test]
    fn build_send_request_compose_ok() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.to = "task".into();
        vm.compose.body = "hello world".into();
        vm.compose.priority = "high".into();
        vm.compose.tags = "auth, urgent".into();
        let result = vm.build_send_request(SendSource::Compose);
        assert!(result.is_ok());
        if let Ok(req) = result {
            assert_eq!(req.from, "me");
            assert_eq!(req.to, "task");
            assert_eq!(req.body, "hello world");
            assert_eq!(req.priority, "high");
            assert_eq!(req.tags, vec!["auth", "urgent"]);
        }
    }

    #[test]
    fn build_send_request_compose_missing_target() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.body = "hello".into();
        assert!(vm.build_send_request(SendSource::Compose).is_err());
    }

    #[test]
    fn build_send_request_compose_missing_body() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.to = "task".into();
        assert!(vm.build_send_request(SendSource::Compose).is_err());
    }

    #[test]
    fn build_send_request_quick_ok() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.input = ":task hello world".into();
        let result = vm.build_send_request(SendSource::Quick);
        assert!(result.is_ok());
        if let Ok(req) = result {
            assert_eq!(req.to, "task");
            assert_eq!(req.body, "hello world");
        }
    }

    #[test]
    fn build_send_request_quick_malformed() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.input = ":task".into();
        assert!(vm.build_send_request(SendSource::Quick).is_err());
    }

    // --- Send result handling ---

    #[test]
    fn handle_send_result_compose_success() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.active = true;
        vm.compose.sending = true;
        let req = SendRequest::default();
        let toast = vm.handle_send_result(SendSource::Compose, &req, None);
        assert!(!vm.compose.active);
        assert!(toast.is_some());
        assert!(toast.as_deref().is_some_and(|t| t.contains('\u{2713}')));
    }

    #[test]
    fn handle_send_result_compose_error() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.active = true;
        vm.compose.sending = true;
        let req = SendRequest::default();
        let toast = vm.handle_send_result(SendSource::Compose, &req, Some("network error"));
        assert!(toast.is_none());
        assert_eq!(vm.compose.err, "network error");
    }

    #[test]
    fn handle_send_result_quick_success() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.active = true;
        vm.quick.sending = true;
        let req = SendRequest {
            to: "task".into(),
            body: "test".into(),
            ..Default::default()
        };
        let toast = vm.handle_send_result(SendSource::Quick, &req, None);
        assert!(!vm.quick.active);
        assert!(toast.is_some());
        assert_eq!(vm.quick.history.len(), 1);
    }

    // --- Draft lifecycle ---

    #[test]
    fn build_and_restore_draft() {
        let mut vm = ComposeViewModel::new("me");
        vm.compose.to = "task".into();
        vm.compose.body = "draft body".into();
        vm.compose.priority = "high".into();
        vm.compose.tags = "auth".into();
        vm.compose.reply_to = "msg-123".into();

        let draft = vm.build_draft(1000);
        assert_eq!(draft.target, "task");
        assert_eq!(draft.body, "draft body");
        assert_eq!(draft.priority, "high");

        // Reset and restore.
        vm.compose.body.clear();
        vm.compose.draft_cached = draft;
        vm.compose.restore_ask = true;
        vm.restore_draft();
        assert_eq!(vm.compose.body, "draft body");
        assert!(!vm.compose.restore_ask);
    }

    // --- Input handling: compose overlay ---

    #[test]
    fn compose_tab_advances_field() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        assert_eq!(vm.compose.focus, ComposeField::Body);

        let action = apply_compose_input(&mut vm, key(Key::Tab), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert_eq!(vm.compose.focus, ComposeField::To);
    }

    #[test]
    fn compose_shift_tab_goes_back() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        let action = apply_compose_input(&mut vm, key_shift(Key::Tab), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert_eq!(vm.compose.focus, ComposeField::Tags);
    }

    #[test]
    fn compose_ctrl_enter_sends() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        let action = apply_compose_input(&mut vm, key_ctrl(Key::Enter), &[], &[]);
        assert_eq!(action, ComposeAction::Send(SendSource::Compose));
    }

    #[test]
    fn compose_ctrl_j_sends() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        let action = apply_compose_input(&mut vm, key_ctrl(Key::Char('j')), &[], &[]);
        assert_eq!(action, ComposeAction::Send(SendSource::Compose));
    }

    #[test]
    fn compose_esc_with_empty_body_closes() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        let action = apply_compose_input(&mut vm, key(Key::Escape), &[], &[]);
        assert_eq!(action, ComposeAction::Close);
        assert!(!vm.compose.active);
    }

    #[test]
    fn compose_esc_with_body_triggers_save_prompt() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.body = "something".into();
        let action = apply_compose_input(&mut vm, key(Key::Escape), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert!(vm.compose.save_prompt);
    }

    #[test]
    fn compose_save_prompt_y_discards() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.save_prompt = true;
        let action = apply_compose_input(&mut vm, char_key('y'), &[], &[]);
        assert_eq!(action, ComposeAction::Close);
    }

    #[test]
    fn compose_save_prompt_s_saves() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.save_prompt = true;
        let action = apply_compose_input(&mut vm, char_key('s'), &[], &[]);
        assert!(matches!(action, ComposeAction::Toast(_)));
    }

    #[test]
    fn compose_save_prompt_n_cancels() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.save_prompt = true;
        let action = apply_compose_input(&mut vm, char_key('n'), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert!(!vm.compose.save_prompt);
    }

    #[test]
    fn compose_restore_ask_y_restores() {
        let mut vm = ComposeViewModel::new("me");
        let draft = ComposeDraft {
            body: "saved".into(),
            to: "task".into(),
            priority: "high".into(),
            ..Default::default()
        };
        vm.open_compose("task", &ComposeReplySeed::default(), Some(&draft));
        assert!(vm.compose.restore_ask);

        let action = apply_compose_input(&mut vm, char_key('y'), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert!(!vm.compose.restore_ask);
        assert_eq!(vm.compose.body, "saved");
        assert_eq!(vm.compose.priority, "high");
    }

    #[test]
    fn compose_restore_ask_n_skips() {
        let mut vm = ComposeViewModel::new("me");
        let draft = ComposeDraft {
            body: "saved".into(),
            to: "task".into(),
            ..Default::default()
        };
        vm.open_compose("task", &ComposeReplySeed::default(), Some(&draft));
        let action = apply_compose_input(&mut vm, char_key('n'), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert!(!vm.compose.restore_ask);
        assert!(vm.compose.body.is_empty());
    }

    #[test]
    fn compose_enter_in_body_inserts_newline() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.focus = ComposeField::Body;
        vm.compose.body = "line1".into();
        let _ = apply_compose_input(&mut vm, key(Key::Enter), &[], &[]);
        assert_eq!(vm.compose.body, "line1\n");
    }

    #[test]
    fn compose_enter_on_non_body_advances_field() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.focus = ComposeField::To;
        let _ = apply_compose_input(&mut vm, key(Key::Enter), &[], &[]);
        assert_eq!(vm.compose.focus, ComposeField::Priority);
    }

    #[test]
    fn compose_up_down_cycles_priority() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.focus = ComposeField::Priority;
        let _ = apply_compose_input(&mut vm, key(Key::Up), &[], &[]);
        assert_eq!(vm.compose.priority, "low");
        let _ = apply_compose_input(&mut vm, key(Key::Down), &[], &[]);
        assert_eq!(vm.compose.priority, "normal");
    }

    #[test]
    fn compose_char_input() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("", &ComposeReplySeed::default(), None);
        assert_eq!(vm.compose.focus, ComposeField::To);
        let _ = apply_compose_input(&mut vm, char_key('@'), &[], &[]);
        let _ = apply_compose_input(&mut vm, char_key('b'), &[], &[]);
        assert_eq!(vm.compose.to, "@b");
    }

    #[test]
    fn compose_blocks_input_while_sending() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.sending = true;
        let action = apply_compose_input(&mut vm, char_key('x'), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert!(vm.compose.body.is_empty());
    }

    // --- Input handling: quick-send ---

    #[test]
    fn quick_send_enter_sends() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.input = ":task hello".into();
        let action = apply_compose_input(&mut vm, key(Key::Enter), &[], &[]);
        assert_eq!(action, ComposeAction::Send(SendSource::Quick));
    }

    #[test]
    fn quick_send_esc_closes() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        let action = apply_compose_input(&mut vm, key(Key::Escape), &[], &[]);
        assert_eq!(action, ComposeAction::Close);
        assert!(!vm.quick.active);
    }

    #[test]
    fn quick_send_backspace_on_empty_closes() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        // input is ":" which counts as empty
        let action = apply_compose_input(&mut vm, key(Key::Backspace), &[], &[]);
        assert_eq!(action, ComposeAction::Close);
    }

    #[test]
    fn quick_send_backspace_on_content_deletes() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.input = ":task".into();
        let action = apply_compose_input(&mut vm, key(Key::Backspace), &[], &[]);
        assert_eq!(action, ComposeAction::None);
        assert_eq!(vm.quick.input, ":tas");
    }

    #[test]
    fn quick_send_char_input() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        let _ = apply_compose_input(&mut vm, char_key('t'), &[], &[]);
        assert_eq!(vm.quick.input, ":t");
    }

    #[test]
    fn quick_send_up_down_history() {
        let mut vm = ComposeViewModel::new("me");
        vm.quick.history = vec![":task hello".into()];
        vm.open_quick_send();
        let _ = apply_compose_input(&mut vm, key(Key::Up), &[], &[]);
        assert_eq!(vm.quick.input, ":task hello");
        let _ = apply_compose_input(&mut vm, key(Key::Down), &[], &[]);
        assert_eq!(vm.quick.input, ":");
    }

    #[test]
    fn quick_send_tab_completes() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.input = ":ta".into();
        let targets: Vec<String> = vec!["task".into(), "team".into()];
        let _ = apply_compose_input(&mut vm, key(Key::Tab), &targets, &[]);
        assert_eq!(vm.quick.input, ":task ");
    }

    #[test]
    fn quick_send_blocks_input_while_sending() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.sending = true;
        let before = vm.quick.input.clone();
        let _ = apply_compose_input(&mut vm, char_key('x'), &[], &[]);
        assert_eq!(vm.quick.input, before);
    }

    // --- Rendering ---

    #[test]
    fn render_compose_frame_basic() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_compose("task", &ComposeReplySeed::default(), None);
        vm.compose.body = "test message".into();
        let frame = render_compose_frame(
            &vm,
            FrameSize {
                width: 80,
                height: 24,
            },
            &theme(),
        );
        let text = frame.snapshot();
        assert!(text.contains("Compose"));
        assert!(text.contains("To: task"));
        assert!(text.contains("test message"));
    }

    #[test]
    fn render_compose_frame_with_reply() {
        let mut vm = ComposeViewModel::new("me");
        let seed = ComposeReplySeed {
            target: "task".into(),
            reply_to: "msg-abc12345".into(),
            parent_line: "original message here".into(),
        };
        vm.open_compose("task", &seed, None);
        let frame = render_compose_frame(
            &vm,
            FrameSize {
                width: 80,
                height: 24,
            },
            &theme(),
        );
        let text = frame.snapshot();
        assert!(text.contains("reply msg-abc1"));
        assert!(text.contains("Replying to:"));
    }

    #[test]
    fn render_quick_send_bar_basic() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.input = ":task hello".into();
        let bar = render_quick_send_bar(&vm, 80, &theme());
        assert!(bar.contains("quick-send"));
        assert!(bar.contains(":task hello"));
    }

    #[test]
    fn render_quick_send_bar_sending() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.sending = true;
        let bar = render_quick_send_bar(&vm, 80, &theme());
        assert!(bar.contains("Sending..."));
    }

    #[test]
    fn render_quick_send_bar_error() {
        let mut vm = ComposeViewModel::new("me");
        vm.open_quick_send();
        vm.quick.err = "network fail".into();
        let bar = render_quick_send_bar(&vm, 80, &theme());
        assert!(bar.contains("error: network fail"));
    }

    #[test]
    fn render_toast_active() {
        let mut vm = ComposeViewModel::new("me");
        vm.set_toast("Sent \u{2713}", 100);
        assert_eq!(render_toast(&vm, 101), "Sent \u{2713}");
    }

    #[test]
    fn render_toast_expired() {
        let mut vm = ComposeViewModel::new("me");
        vm.set_toast("Sent \u{2713}", 100);
        assert_eq!(render_toast(&vm, 200), "");
    }

    #[test]
    fn render_toast_empty() {
        let vm = ComposeViewModel::new("me");
        assert_eq!(render_toast(&vm, 100), "");
    }

    // --- Spinner ---

    #[test]
    fn spinner_tick_cycles() {
        let mut vm = ComposeViewModel::new("me");
        assert_eq!(spinner_frame(vm.spinner_frame), "|");
        vm.tick_spinner();
        assert_eq!(spinner_frame(vm.spinner_frame), "/");
        vm.tick_spinner();
        assert_eq!(spinner_frame(vm.spinner_frame), "-");
        vm.tick_spinner();
        assert_eq!(spinner_frame(vm.spinner_frame), "\\");
        vm.tick_spinner();
        assert_eq!(spinner_frame(vm.spinner_frame), "|");
    }

    // --- truncate ---

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    // --- wrap_lines ---

    #[test]
    fn wrap_lines_basic() {
        let lines = wrap_lines("abcdefghij", 5);
        assert_eq!(lines, vec!["abcde", "fghij"]);
    }

    #[test]
    fn wrap_lines_short() {
        let lines = wrap_lines("abc", 10);
        assert_eq!(lines, vec!["abc"]);
    }
}

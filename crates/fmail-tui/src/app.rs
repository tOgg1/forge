//! App shell and navigation stack for the fmail TUI.
//!
//! Ports the Go `fmailtui.Model` app-shell semantics: a stack-based navigation
//! system, global key routing, view lifecycle, overlay management, and layout
//! modes. Individual views implement the [`View`] trait.

use std::collections::HashMap;

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

// ---------------------------------------------------------------------------
// ViewId
// ---------------------------------------------------------------------------

/// Stable view identifiers matching the Go `ViewID` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewId {
    Dashboard,
    Topics,
    Thread,
    Agents,
    Operator,
    Search,
    LiveTail,
    Timeline,
    Stats,
    Heatmap,
    Graph,
    Replay,
    Bookmarks,
    Notifications,
}

impl ViewId {
    /// Display label for the view — used in headers and help overlays.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Topics => "Topics",
            Self::Thread => "Thread",
            Self::Agents => "Agents",
            Self::Operator => "Operator",
            Self::Search => "Search",
            Self::LiveTail => "Live Tail",
            Self::Timeline => "Timeline",
            Self::Stats => "Stats",
            Self::Heatmap => "Heatmap",
            Self::Graph => "Graph",
            Self::Replay => "Replay",
            Self::Bookmarks => "Bookmarks",
            Self::Notifications => "Notifications",
        }
    }

    /// The canonical string ID matching Go's `ViewID` type (used in layout
    /// persistence and view lookups).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Topics => "topics",
            Self::Thread => "thread",
            Self::Agents => "agents",
            Self::Operator => "operator",
            Self::Search => "search",
            Self::LiveTail => "live-tail",
            Self::Timeline => "timeline",
            Self::Stats => "stats",
            Self::Heatmap => "heatmap",
            Self::Graph => "graph",
            Self::Replay => "replay",
            Self::Bookmarks => "bookmarks",
            Self::Notifications => "notifications",
        }
    }

    /// All registered view IDs.
    pub const ALL: [ViewId; 14] = [
        Self::Dashboard,
        Self::Topics,
        Self::Thread,
        Self::Agents,
        Self::Operator,
        Self::Search,
        Self::LiveTail,
        Self::Timeline,
        Self::Stats,
        Self::Heatmap,
        Self::Graph,
        Self::Replay,
        Self::Bookmarks,
        Self::Notifications,
    ];
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// Commands returned by views and the app shell to drive the event loop.
/// Mirrors BubbleTea's `tea.Cmd` pattern but as a simple enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    None,
    PushView(ViewId),
    PopView,
    Quit,
    Batch(Vec<Command>),
}

impl Command {
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

// ---------------------------------------------------------------------------
// View trait
// ---------------------------------------------------------------------------

/// View-model interface matching Go's `viewModel` interface.
///
/// Views are owned by the `App` and dispatched through the navigation stack.
pub trait View {
    /// Called when the view becomes the active (topmost) view.
    fn init(&mut self) -> Command;

    /// Handle an input event routed to this view.
    fn update(&mut self, event: InputEvent) -> Command;

    /// Render the view into a frame of the given dimensions.
    fn view(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame;
}

// ---------------------------------------------------------------------------
// LayoutMode
// ---------------------------------------------------------------------------

/// Layout modes matching Go `layout.Mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    #[default]
    Single,
    Split,
    Dashboard,
    Zen,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// The global fmail TUI application state.
///
/// Owns the view stack, registered views, theme, layout, and overlay states.
pub struct App {
    view_stack: Vec<ViewId>,
    views: HashMap<ViewId, Box<dyn View>>,

    width: usize,
    height: usize,
    theme: ThemeSpec,
    layout_mode: LayoutMode,

    show_help: bool,
    toast: String,
}

/// View-switch shortcut keys matching Go's `viewSwitchKeys` map.
const VIEW_SWITCH_KEYS: &[(char, ViewId)] = &[
    ('o', ViewId::Operator),
    ('t', ViewId::Topics),
    ('a', ViewId::Agents),
    ('l', ViewId::LiveTail),
    ('m', ViewId::Timeline),
    ('p', ViewId::Stats),
    ('H', ViewId::Heatmap),
    ('v', ViewId::Graph),
    ('R', ViewId::Replay),
    ('N', ViewId::Notifications),
    ('D', ViewId::Dashboard),
    ('S', ViewId::Search),
];

/// Default enter-key drill-down routes matching Go's `defaultEnterRoute`.
const DEFAULT_ENTER_ROUTE: &[(ViewId, ViewId)] = &[
    (ViewId::Dashboard, ViewId::Topics),
    (ViewId::Topics, ViewId::Thread),
];

impl App {
    /// Create a new app shell with the given initial view and theme.
    #[must_use]
    pub fn new(initial_view: ViewId, theme: ThemeSpec) -> Self {
        Self {
            view_stack: vec![initial_view],
            views: HashMap::new(),
            width: 120,
            height: 40,
            theme,
            layout_mode: LayoutMode::default(),
            show_help: false,
            toast: String::new(),
        }
    }

    // -- view registration --------------------------------------------------

    /// Register a view implementation for a given view ID.
    pub fn register_view(&mut self, id: ViewId, view: Box<dyn View>) {
        self.views.insert(id, view);
    }

    /// Returns `true` if a view is registered for `id`.
    #[must_use]
    pub fn has_view(&self, id: ViewId) -> bool {
        self.views.contains_key(&id)
    }

    // -- navigation stack ----------------------------------------------------

    /// Push a view onto the navigation stack.
    /// Silently ignored if the view is already active or not registered.
    pub fn push_view(&mut self, id: ViewId) -> Command {
        if !self.views.contains_key(&id) {
            return Command::None;
        }
        if self.active_view_id() == id {
            return Command::None;
        }
        self.view_stack.push(id);
        if let Some(view) = self.views.get_mut(&id) {
            return view.init();
        }
        Command::None
    }

    /// Pop the topmost view. The stack always keeps at least one entry.
    pub fn pop_view(&mut self) -> Command {
        if self.view_stack.len() <= 1 {
            return Command::None;
        }
        self.view_stack.pop();
        let id = self.active_view_id();
        if let Some(view) = self.views.get_mut(&id) {
            return view.init();
        }
        Command::None
    }

    /// The currently active (topmost) view ID.
    #[must_use]
    pub fn active_view_id(&self) -> ViewId {
        self.view_stack.last().copied().unwrap_or(ViewId::Dashboard)
    }

    /// Returns the full navigation stack (bottom to top).
    #[must_use]
    pub fn view_stack(&self) -> &[ViewId] {
        &self.view_stack
    }

    /// Stack depth.
    #[must_use]
    pub fn stack_depth(&self) -> usize {
        self.view_stack.len()
    }

    // -- dimensions ----------------------------------------------------------

    /// Update terminal dimensions.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    // -- theme ---------------------------------------------------------------

    #[must_use]
    pub fn theme(&self) -> ThemeSpec {
        self.theme
    }

    /// Cycle to the next theme (matching Go's `nextTheme`).
    pub fn cycle_theme(&mut self) {
        self.theme = match self.theme.kind {
            ThemeKind::Dark | ThemeKind::Light => ThemeSpec::for_kind(ThemeKind::HighContrast),
            ThemeKind::HighContrast => ThemeSpec::for_kind(ThemeKind::Dark),
        };
    }

    // -- layout --------------------------------------------------------------

    #[must_use]
    pub fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    pub fn set_layout_mode(&mut self, mode: LayoutMode) {
        self.layout_mode = mode;
    }

    // -- overlays ------------------------------------------------------------

    #[must_use]
    pub fn show_help(&self) -> bool {
        self.show_help
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    #[must_use]
    pub fn toast(&self) -> &str {
        &self.toast
    }

    pub fn set_toast(&mut self, msg: &str) {
        self.toast = msg.to_owned();
    }

    pub fn clear_toast(&mut self) {
        self.toast.clear();
    }

    // -- main update loop ----------------------------------------------------

    /// Process an input event, routing through global keys first, then to the
    /// active view.
    pub fn update(&mut self, event: InputEvent) -> Command {
        // Resize events bypass key routing.
        if let InputEvent::Resize(resize) = event {
            self.width = resize.width;
            self.height = resize.height;
            return Command::None;
        }

        // Global key handling.
        if let InputEvent::Key(key_event) = event {
            let (cmd, handled) = self.handle_global_key(key_event);
            if handled {
                return self.resolve_command(cmd);
            }
        }

        // Delegate to active view.
        let id = self.active_view_id();
        if let Some(view) = self.views.get_mut(&id) {
            let cmd = view.update(event);
            return self.resolve_command(cmd);
        }
        Command::None
    }

    /// Render the full UI frame.
    #[must_use]
    pub fn render(&self) -> RenderFrame {
        let is_zen = self.layout_mode == LayoutMode::Zen;

        let header_height = if is_zen { 0 } else { 1 };
        let footer_height = if is_zen { 0 } else { 1 };
        let content_height = self
            .height
            .saturating_sub(header_height + footer_height)
            .max(1);

        let total_width = self.width.max(1);
        let mut frame = RenderFrame::new(
            FrameSize {
                width: total_width,
                height: self.height.max(1),
            },
            self.theme,
        );

        if !is_zen {
            // Header row.
            self.render_header(&mut frame, total_width);
        }

        // Help overlay takes priority over view content.
        if self.show_help {
            self.render_help_overlay(&mut frame, total_width, content_height, header_height);
        } else if let Some(view) = self.views.get(&self.active_view_id()) {
            let view_frame = view.view(
                FrameSize {
                    width: total_width,
                    height: content_height,
                },
                self.theme,
            );
            self.blit_frame(&mut frame, &view_frame, 0, header_height);
        }

        if !is_zen {
            // Footer row.
            self.render_footer(&mut frame, total_width, self.height.saturating_sub(1));
        }

        frame
    }

    // -- internal key dispatch -----------------------------------------------

    fn handle_global_key(&mut self, key: KeyEvent) -> (Command, bool) {
        // Help overlay: Esc or ? closes; any other key closes then continues.
        if self.show_help {
            match key.key {
                Key::Char('?') | Key::Escape => {
                    self.show_help = false;
                    return (Command::None, true);
                }
                _ => {
                    self.show_help = false;
                    // fall through to normal handling
                }
            }
        }

        // Operator view: limited keys.
        if self.active_view_id() == ViewId::Operator {
            match key.key {
                Key::Char('q') => return (Command::Quit, true),
                Key::Char('c') if key.modifiers.ctrl => return (Command::Quit, true),
                Key::Char('?') => {
                    self.show_help = !self.show_help;
                    return (Command::None, true);
                }
                _ => return (Command::None, false),
            }
        }

        // Standard global keys.
        match key.key {
            Key::Escape => return (Command::PopView, true),

            Key::Char('q') => return (Command::Quit, true),
            Key::Char('c') if key.modifiers.ctrl => return (Command::Quit, true),

            Key::Char('?') => {
                self.show_help = !self.show_help;
                return (Command::None, true);
            }

            // Ctrl+T: cycle theme.
            Key::Char('t') if key.modifiers.ctrl => {
                self.cycle_theme();
                self.toast = format!("theme: {:?}", self.theme.kind);
                return (Command::None, true);
            }

            // Ctrl+Z: toggle zen mode.
            Key::Char('z') if key.modifiers.ctrl => {
                self.layout_mode = match self.layout_mode {
                    LayoutMode::Zen => LayoutMode::Split,
                    _ => LayoutMode::Zen,
                };
                return (Command::None, true);
            }

            // Ctrl+B: bookmarks.
            Key::Char('b') if key.modifiers.ctrl => {
                return (Command::PushView(ViewId::Bookmarks), true);
            }

            // Ctrl+N: notifications.
            Key::Char('n') if key.modifiers.ctrl => {
                return (Command::PushView(ViewId::Notifications), true);
            }

            // Ctrl+R: refresh active view.
            Key::Char('r') if key.modifiers.ctrl => {
                let id = self.active_view_id();
                if let Some(view) = self.views.get_mut(&id) {
                    return (view.init(), true);
                }
                return (Command::None, true);
            }

            // Number keys for quick view switch.
            Key::Char('1') => return (Command::PushView(ViewId::Dashboard), true),
            Key::Char('2') => return (Command::PushView(ViewId::Topics), true),
            Key::Char('3') => return (Command::PushView(ViewId::Agents), true),

            // / for search (unless already on topics/agents which own it).
            Key::Char('/') => {
                let active = self.active_view_id();
                if active != ViewId::Topics && active != ViewId::Agents {
                    return (Command::PushView(ViewId::Search), true);
                }
            }

            _ => {}
        }

        // View switch shortcut keys (single letter, no modifiers).
        if key.modifiers == Modifiers::none() {
            if let Key::Char(ch) = key.key {
                for &(switch_key, target) in VIEW_SWITCH_KEYS {
                    if ch == switch_key {
                        return (Command::PushView(target), true);
                    }
                }
            }
        }

        (Command::None, false)
    }

    /// Resolve meta-commands (PushView/PopView) by mutating the stack.
    fn resolve_command(&mut self, cmd: Command) -> Command {
        match cmd {
            Command::PushView(id) => self.push_view(id),
            Command::PopView => self.pop_view(),
            Command::Batch(cmds) => {
                let mut resolved = Vec::with_capacity(cmds.len());
                for c in cmds {
                    let r = self.resolve_command(c);
                    if !r.is_none() {
                        resolved.push(r);
                    }
                }
                if resolved.is_empty() {
                    Command::None
                } else if resolved.len() == 1 {
                    resolved.into_iter().next().unwrap_or(Command::None)
                } else {
                    Command::Batch(resolved)
                }
            }
            other => other,
        }
    }

    // -- rendering helpers ---------------------------------------------------

    fn render_header(&self, frame: &mut RenderFrame, width: usize) {
        let active = self.active_view_id();
        let label = format!(
            " fmail  [{active}]  depth:{depth}",
            active = active.label(),
            depth = self.view_stack.len(),
        );
        let truncated = if label.len() > width {
            &label[..width]
        } else {
            &label
        };
        frame.draw_text(0, 0, truncated, TextRole::Accent);
    }

    fn render_footer(&self, frame: &mut RenderFrame, width: usize, y: usize) {
        let hint = if self.show_help {
            "? close help"
        } else {
            "? help  q quit  esc back"
        };
        let truncated = if hint.len() > width {
            &hint[..width]
        } else {
            hint
        };
        frame.draw_text(0, y, truncated, TextRole::Muted);
    }

    fn render_help_overlay(
        &self,
        frame: &mut RenderFrame,
        width: usize,
        height: usize,
        y_offset: usize,
    ) {
        let lines = [
            "=== fmail TUI Help ===",
            "",
            "Navigation:",
            "  esc       back / pop view",
            "  1         dashboard",
            "  2         topics",
            "  3         agents",
            "  /         search",
            "",
            "Views:",
            "  o  operator    t  topics     a  agents",
            "  l  live-tail   m  timeline   p  stats",
            "  H  heatmap     v  graph      R  replay",
            "  N  notify      D  dashboard  S  search",
            "",
            "Global:",
            "  ?         toggle help",
            "  q         quit",
            "  ctrl+t    cycle theme",
            "  ctrl+z    zen mode",
            "  ctrl+b    bookmarks",
            "  ctrl+n    notifications",
            "  ctrl+r    refresh view",
        ];
        for (i, line) in lines.iter().enumerate() {
            if i >= height {
                break;
            }
            let truncated = if line.len() > width {
                &line[..width]
            } else {
                line
            };
            frame.draw_text(0, y_offset + i, truncated, TextRole::Primary);
        }
    }

    fn blit_frame(
        &self,
        dest: &mut RenderFrame,
        src: &RenderFrame,
        x_offset: usize,
        y_offset: usize,
    ) {
        let src_size = src.size();
        for sy in 0..src_size.height {
            for sx in 0..src_size.width {
                if let Some(cell) = src.cell(sx, sy) {
                    let dx = x_offset + sx;
                    let dy = y_offset + sy;
                    dest.set_cell(dx, dy, cell);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PlaceholderView
// ---------------------------------------------------------------------------

/// A minimal view used for views that haven't been ported yet.
/// Matches Go's `placeholderView`.
pub struct PlaceholderView {
    id: ViewId,
    last_key: String,
}

impl PlaceholderView {
    #[must_use]
    pub fn new(id: ViewId) -> Self {
        Self {
            id,
            last_key: String::new(),
        }
    }
}

impl View for PlaceholderView {
    fn init(&mut self) -> Command {
        Command::None
    }

    fn update(&mut self, event: InputEvent) -> Command {
        if let InputEvent::Key(key_event) = event {
            let action = forge_ftui_adapter::input::translate_input(&InputEvent::Key(key_event));
            self.last_key = format!("{action:?}");

            if action == UiAction::Confirm {
                for &(from, to) in DEFAULT_ENTER_ROUTE {
                    if from == self.id {
                        return Command::PushView(to);
                    }
                }
            }
            if action == UiAction::Cancel {
                return Command::PopView;
            }
        }
        Command::None
    }

    fn view(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame {
        let mut frame = RenderFrame::new(size, theme);
        let label = format!("{} view", self.id.label());
        frame.draw_text(0, 0, &label, TextRole::Accent);
        frame.draw_text(
            0,
            1,
            "press enter for drill-down where available",
            TextRole::Muted,
        );
        if !self.last_key.is_empty() {
            let status = format!("last: {}", self.last_key);
            frame.draw_text(0, 2, &status, TextRole::Primary);
        }
        frame
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, ResizeEvent};
    use forge_ftui_adapter::style::ThemeSpec;

    fn test_theme() -> ThemeSpec {
        ThemeSpec::for_kind(ThemeKind::HighContrast)
    }

    fn app_with_placeholders() -> App {
        let mut app = App::new(ViewId::Dashboard, test_theme());
        for id in ViewId::ALL {
            app.register_view(id, Box::new(PlaceholderView::new(id)));
        }
        app
    }

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

    // -- ViewId --------------------------------------------------------------

    #[test]
    fn view_id_label_snapshot() {
        let labels: Vec<&str> = ViewId::ALL.iter().map(|id| id.label()).collect();
        assert_eq!(
            labels.join("|"),
            "Dashboard|Topics|Thread|Agents|Operator|Search|Live Tail|Timeline|Stats|Heatmap|Graph|Replay|Bookmarks|Notifications"
        );
    }

    #[test]
    fn view_id_as_str_snapshot() {
        let ids: Vec<&str> = ViewId::ALL.iter().map(|id| id.as_str()).collect();
        assert_eq!(
            ids.join("|"),
            "dashboard|topics|thread|agents|operator|search|live-tail|timeline|stats|heatmap|graph|replay|bookmarks|notifications"
        );
    }

    #[test]
    fn all_view_ids_count() {
        assert_eq!(ViewId::ALL.len(), 14);
    }

    // -- navigation ----------------------------------------------------------

    #[test]
    fn initial_view_is_on_stack() {
        let app = app_with_placeholders();
        assert_eq!(app.active_view_id(), ViewId::Dashboard);
        assert_eq!(app.stack_depth(), 1);
    }

    #[test]
    fn push_view_adds_to_stack() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        assert_eq!(app.active_view_id(), ViewId::Topics);
        assert_eq!(app.stack_depth(), 2);
    }

    #[test]
    fn push_duplicate_active_is_noop() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        app.push_view(ViewId::Topics);
        assert_eq!(app.stack_depth(), 2);
    }

    #[test]
    fn push_unregistered_view_is_noop() {
        let mut app = App::new(ViewId::Dashboard, test_theme());
        app.register_view(
            ViewId::Dashboard,
            Box::new(PlaceholderView::new(ViewId::Dashboard)),
        );
        app.push_view(ViewId::Topics);
        assert_eq!(app.active_view_id(), ViewId::Dashboard);
        assert_eq!(app.stack_depth(), 1);
    }

    #[test]
    fn pop_view_removes_top() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        app.push_view(ViewId::Thread);
        assert_eq!(app.stack_depth(), 3);
        app.pop_view();
        assert_eq!(app.active_view_id(), ViewId::Topics);
        assert_eq!(app.stack_depth(), 2);
    }

    #[test]
    fn pop_view_at_root_is_noop() {
        let mut app = app_with_placeholders();
        app.pop_view();
        assert_eq!(app.stack_depth(), 1);
        assert_eq!(app.active_view_id(), ViewId::Dashboard);
    }

    #[test]
    fn view_stack_returns_full_stack() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        app.push_view(ViewId::Thread);
        assert_eq!(
            app.view_stack(),
            &[ViewId::Dashboard, ViewId::Topics, ViewId::Thread]
        );
    }

    // -- global keys ---------------------------------------------------------

    #[test]
    fn escape_pops_view() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        let cmd = app.update(key(Key::Escape));
        assert_eq!(app.active_view_id(), ViewId::Dashboard);
        assert!(cmd.is_none() || matches!(cmd, Command::None));
    }

    #[test]
    fn q_quits() {
        let mut app = app_with_placeholders();
        let cmd = app.update(key(Key::Char('q')));
        assert_eq!(cmd, Command::Quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = app_with_placeholders();
        let cmd = app.update(ctrl_key('c'));
        assert_eq!(cmd, Command::Quit);
    }

    #[test]
    fn question_mark_toggles_help() {
        let mut app = app_with_placeholders();
        assert!(!app.show_help());

        app.update(key(Key::Char('?')));
        assert!(app.show_help());

        app.update(key(Key::Char('?')));
        assert!(!app.show_help());
    }

    #[test]
    fn help_closes_on_escape() {
        let mut app = app_with_placeholders();
        app.update(key(Key::Char('?')));
        assert!(app.show_help());
        app.update(key(Key::Escape));
        assert!(!app.show_help());
    }

    #[test]
    fn help_closes_on_any_key_then_processes() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        app.update(key(Key::Char('?')));
        assert!(app.show_help());
        // pressing 'q' should close help AND trigger quit
        let cmd = app.update(key(Key::Char('q')));
        assert!(!app.show_help());
        assert_eq!(cmd, Command::Quit);
    }

    #[test]
    fn number_keys_push_views() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Topics);
        app.update(key(Key::Char('1')));
        assert_eq!(app.active_view_id(), ViewId::Dashboard);

        app.update(key(Key::Char('2')));
        assert_eq!(app.active_view_id(), ViewId::Topics);

        app.update(key(Key::Char('3')));
        assert_eq!(app.active_view_id(), ViewId::Agents);
    }

    #[test]
    fn view_switch_keys_push_views() {
        let mut app = app_with_placeholders();
        app.update(key(Key::Char('o')));
        assert_eq!(app.active_view_id(), ViewId::Operator);
    }

    #[test]
    fn slash_pushes_search_unless_on_topics_or_agents() {
        let mut app = app_with_placeholders();
        // from dashboard, / should push search
        app.update(key(Key::Char('/')));
        assert_eq!(app.active_view_id(), ViewId::Search);

        // from topics, / should NOT push search
        let mut app2 = app_with_placeholders();
        app2.push_view(ViewId::Topics);
        app2.update(key(Key::Char('/')));
        assert_eq!(app2.active_view_id(), ViewId::Topics);
    }

    #[test]
    fn ctrl_t_cycles_theme() {
        let mut app = app_with_placeholders();
        assert_eq!(app.theme().kind, ThemeKind::HighContrast);
        app.update(ctrl_key('t'));
        assert_eq!(app.theme().kind, ThemeKind::Dark);
        app.update(ctrl_key('t'));
        assert_eq!(app.theme().kind, ThemeKind::HighContrast);
    }

    #[test]
    fn ctrl_z_toggles_zen() {
        let mut app = app_with_placeholders();
        assert_eq!(app.layout_mode(), LayoutMode::Single);
        app.update(ctrl_key('z'));
        assert_eq!(app.layout_mode(), LayoutMode::Zen);
        app.update(ctrl_key('z'));
        assert_eq!(app.layout_mode(), LayoutMode::Split);
    }

    #[test]
    fn ctrl_b_pushes_bookmarks() {
        let mut app = app_with_placeholders();
        app.update(ctrl_key('b'));
        assert_eq!(app.active_view_id(), ViewId::Bookmarks);
    }

    #[test]
    fn ctrl_n_pushes_notifications() {
        let mut app = app_with_placeholders();
        app.update(ctrl_key('n'));
        assert_eq!(app.active_view_id(), ViewId::Notifications);
    }

    // -- resize --------------------------------------------------------------

    #[test]
    fn resize_updates_dimensions() {
        let mut app = app_with_placeholders();
        app.update(InputEvent::Resize(ResizeEvent {
            width: 200,
            height: 50,
        }));
        assert_eq!(app.width(), 200);
        assert_eq!(app.height(), 50);
    }

    // -- placeholder view ----------------------------------------------------

    #[test]
    fn placeholder_enter_drills_down() {
        let mut app = app_with_placeholders();
        // Dashboard -> enter should push Topics
        app.update(key(Key::Enter));
        assert_eq!(app.active_view_id(), ViewId::Topics);

        // Topics -> enter should push Thread
        app.update(key(Key::Enter));
        assert_eq!(app.active_view_id(), ViewId::Thread);
    }

    #[test]
    fn placeholder_renders_label() {
        let view = PlaceholderView::new(ViewId::Dashboard);
        let frame = view.view(
            FrameSize {
                width: 40,
                height: 3,
            },
            test_theme(),
        );
        assert!(frame.row_text(0).contains("Dashboard view"));
    }

    // -- render --------------------------------------------------------------

    #[test]
    fn render_produces_non_empty_frame() {
        let app = app_with_placeholders();
        let frame = app.render();
        assert_eq!(frame.size().width, 120);
        assert_eq!(frame.size().height, 40);
        // Header should contain the active view label.
        assert!(frame.row_text(0).contains("Dashboard"));
    }

    #[test]
    fn render_zen_mode_no_header_footer() {
        let mut app = app_with_placeholders();
        app.set_layout_mode(LayoutMode::Zen);
        let frame = app.render();
        // First row should be view content, not header.
        assert!(frame.row_text(0).contains("Dashboard view"));
    }

    #[test]
    fn render_help_overlay() {
        let mut app = app_with_placeholders();
        app.toggle_help();
        let frame = app.render();
        // First content row (after header) should contain help text.
        assert!(frame.row_text(1).contains("fmail TUI Help"));
    }

    // -- operator view keys --------------------------------------------------

    #[test]
    fn operator_view_only_accepts_q_and_help() {
        let mut app = app_with_placeholders();
        app.push_view(ViewId::Operator);

        // 't' key should NOT switch views from operator
        app.update(key(Key::Char('t')));
        assert_eq!(app.active_view_id(), ViewId::Operator);

        // 'q' should quit
        let cmd = app.update(key(Key::Char('q')));
        assert_eq!(cmd, Command::Quit);
    }

    // -- command resolution --------------------------------------------------

    #[test]
    fn command_none_is_none() {
        assert!(Command::None.is_none());
        assert!(!Command::Quit.is_none());
    }

    #[test]
    fn command_batch_empty_is_none() {
        let cmd = Command::Batch(vec![]);
        assert!(!cmd.is_none()); // Batch itself is not None
    }
}

//! App shell and state model for the Forge loop TUI.
//!
//! Ports the Go `internal/looptui/looptui.go` model: tab-based navigation,
//! modal UI modes (filter/confirm/wizard/help/expanded-logs), loop selection,
//! log source/layer cycling, multi-log pagination, and pinned loops.

use std::collections::{HashMap, HashSet};

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

use crate::layouts::{
    fit_pane_layout, layout_index_for, normalize_layout_index, PaneLayout, PANE_LAYOUTS,
};
use crate::theme::{cycle_palette, resolve_palette, Palette};

// ---------------------------------------------------------------------------
// Constants – matching Go defaults
// ---------------------------------------------------------------------------

pub const DEFAULT_LOG_LINES: usize = 12;
pub const LOG_SCROLL_STEP: usize = 20;
pub const MAX_LOG_BACKFILL: usize = 8000;

pub const MULTI_HEADER_ROWS: i32 = 2;
pub const MULTI_CELL_GAP: i32 = 1;
pub const MULTI_MIN_CELL_WIDTH: i32 = 38;
pub const MULTI_MIN_CELL_HEIGHT: i32 = 8;

pub const FILTER_STATUS_OPTIONS: &[&str] =
    &["all", "running", "sleeping", "waiting", "stopped", "error"];

// ---------------------------------------------------------------------------
// MainTab
// ---------------------------------------------------------------------------

/// The four main tabs, matching Go's `mainTab` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainTab {
    Overview,
    Logs,
    Runs,
    MultiLogs,
}

impl MainTab {
    pub const ORDER: [MainTab; 4] = [
        MainTab::Overview,
        MainTab::Logs,
        MainTab::Runs,
        MainTab::MultiLogs,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Runs => "Runs",
            Self::MultiLogs => "Multi Logs",
        }
    }

    #[must_use]
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Overview => "ov",
            Self::Logs => "logs",
            Self::Runs => "runs",
            Self::MultiLogs => "multi",
        }
    }
}

// ---------------------------------------------------------------------------
// UiMode
// ---------------------------------------------------------------------------

/// UI mode – which interaction mode is active.
/// Matches Go's `uiMode` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Main,
    Filter,
    ExpandedLogs,
    Confirm,
    Wizard,
    Help,
}

// ---------------------------------------------------------------------------
// StatusKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Ok,
    Err,
}

// ---------------------------------------------------------------------------
// FilterFocus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFocus {
    Text,
    Status,
}

// ---------------------------------------------------------------------------
// ActionType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    None,
    Stop,
    Kill,
    Delete,
    Resume,
    Create,
}

// ---------------------------------------------------------------------------
// LogSource / LogLayer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Live,
    LatestRun,
    RunSelection,
}

impl LogSource {
    pub const ORDER: [LogSource; 3] = [
        LogSource::Live,
        LogSource::LatestRun,
        LogSource::RunSelection,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::LatestRun => "latest-run",
            Self::RunSelection => "selected-run",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLayer {
    Raw,
    Events,
    Errors,
    Tools,
    Diff,
}

impl LogLayer {
    pub const ORDER: [LogLayer; 5] = [
        LogLayer::Raw,
        LogLayer::Events,
        LogLayer::Errors,
        LogLayer::Tools,
        LogLayer::Diff,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Events => "events",
            Self::Errors => "errors",
            Self::Tools => "tools",
            Self::Diff => "diff",
        }
    }
}

// ---------------------------------------------------------------------------
// LoopView / RunView / LogTailView – view-model data
// ---------------------------------------------------------------------------

/// Minimal loop data shown in the loop list. Matches Go's `loopView`.
#[derive(Debug, Clone, Default)]
pub struct LoopView {
    pub id: String,
    pub name: String,
    pub state: String,
    pub repo_path: String,
    pub runs: usize,
    pub queue_depth: usize,
    pub profile_name: String,
    pub profile_harness: String,
    pub profile_auth: String,
    pub pool_name: String,
}

/// A single run entry. Matches Go's `runView`.
#[derive(Debug, Clone, Default)]
pub struct RunView {
    pub id: String,
    pub status: String,
    pub profile_name: String,
    pub harness: String,
    pub auth_kind: String,
}

/// Tail view of log content. Matches Go's `logTailView`.
#[derive(Debug, Clone, Default)]
pub struct LogTailView {
    pub lines: Vec<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// ConfirmState / WizardValues / WizardState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub action: ActionType,
    pub loop_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Default)]
pub struct WizardValues {
    pub name: String,
    pub name_prefix: String,
    pub count: String,
    pub pool: String,
    pub profile: String,
    pub prompt: String,
    pub prompt_msg: String,
    pub interval: String,
    pub max_runtime: String,
    pub max_iterations: String,
    pub tags: String,
}

#[derive(Debug, Clone, Default)]
pub struct WizardState {
    pub step: usize,
    pub field: usize,
    pub values: WizardValues,
    pub error: String,
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// Commands returned from update handlers. Matches BubbleTea's `tea.Cmd` pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    None,
    Quit,
    Fetch,
    Batch(Vec<Command>),
    RunAction(ActionKind),
}

impl Command {
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    Resume { loop_id: String },
    Stop { loop_id: String },
    Kill { loop_id: String },
    Delete { loop_id: String, force: bool },
    Create { wizard: Vec<(String, String)> },
}

// ---------------------------------------------------------------------------
// View trait
// ---------------------------------------------------------------------------

/// View-model interface for tab content panes.
pub trait View {
    fn init(&mut self) -> Command;
    fn update(&mut self, event: InputEvent) -> Command;
    fn view(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame;
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// The loop TUI application state, matching Go's `model` struct.
pub struct App {
    // -- tab/mode state --
    pub tab: MainTab,
    pub mode: UiMode,
    pub help_return: UiMode,

    // -- loop selection --
    loops: Vec<LoopView>,
    filtered: Vec<LoopView>,
    selected_id: String,
    selected_idx: usize,
    selected_log: LogTailView,

    // -- run selection --
    run_history: Vec<RunView>,
    selected_run: usize,

    // -- log display --
    log_source: LogSource,
    log_layer: LogLayer,
    log_scroll: usize,
    pub log_lines: usize,

    // -- focus/layout --
    focus_right: bool,
    layout_idx: usize,
    multi_page: usize,
    multi_logs: HashMap<String, LogTailView>,
    pinned: HashSet<String>,

    // -- filter --
    filter_text: String,
    filter_state: String,
    filter_focus: FilterFocus,

    // -- confirm/wizard --
    confirm: Option<ConfirmState>,
    wizard: WizardState,

    // -- status bar --
    status_text: String,
    status_kind: StatusKind,
    action_busy: bool,

    // -- display --
    width: usize,
    height: usize,
    palette: Palette,
    quitting: bool,

    // -- view registry (for tab content) --
    views: HashMap<MainTab, Box<dyn View>>,
}

impl App {
    /// Create a new loop TUI app with the given palette name.
    #[must_use]
    pub fn new(palette_name: &str, log_lines: usize) -> Self {
        let palette = resolve_palette(palette_name);
        let log_lines = if log_lines == 0 {
            DEFAULT_LOG_LINES
        } else {
            log_lines
        };

        Self {
            tab: MainTab::Overview,
            mode: UiMode::Main,
            help_return: UiMode::Main,

            loops: Vec::new(),
            filtered: Vec::new(),
            selected_id: String::new(),
            selected_idx: 0,
            selected_log: LogTailView::default(),

            run_history: Vec::new(),
            selected_run: 0,

            log_source: LogSource::Live,
            log_layer: LogLayer::Raw,
            log_scroll: 0,
            log_lines,

            focus_right: false,
            layout_idx: layout_index_for(2, 2),
            multi_page: 0,
            multi_logs: HashMap::new(),
            pinned: HashSet::new(),

            filter_text: String::new(),
            filter_state: "all".to_owned(),
            filter_focus: FilterFocus::Text,

            confirm: None,
            wizard: WizardState::default(),

            status_text: String::new(),
            status_kind: StatusKind::Info,
            action_busy: false,

            width: 120,
            height: 40,
            palette,
            quitting: false,

            views: HashMap::new(),
        }
    }

    // -- view registration ---------------------------------------------------

    pub fn register_view(&mut self, tab: MainTab, view: Box<dyn View>) {
        self.views.insert(tab, view);
    }

    // -- accessors -----------------------------------------------------------

    #[must_use]
    pub fn tab(&self) -> MainTab {
        self.tab
    }

    #[must_use]
    pub fn mode(&self) -> UiMode {
        self.mode
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    #[must_use]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    #[must_use]
    pub fn selected_id(&self) -> &str {
        &self.selected_id
    }

    #[must_use]
    pub fn selected_idx(&self) -> usize {
        self.selected_idx
    }

    #[must_use]
    pub fn loops(&self) -> &[LoopView] {
        &self.loops
    }

    #[must_use]
    pub fn filtered(&self) -> &[LoopView] {
        &self.filtered
    }

    #[must_use]
    pub fn run_history(&self) -> &[RunView] {
        &self.run_history
    }

    #[must_use]
    pub fn log_source(&self) -> LogSource {
        self.log_source
    }

    #[must_use]
    pub fn log_layer(&self) -> LogLayer {
        self.log_layer
    }

    #[must_use]
    pub fn log_scroll(&self) -> usize {
        self.log_scroll
    }

    #[must_use]
    pub fn focus_right(&self) -> bool {
        self.focus_right
    }

    #[must_use]
    pub fn is_pinned(&self, loop_id: &str) -> bool {
        !loop_id.trim().is_empty() && self.pinned.contains(loop_id)
    }

    #[must_use]
    pub fn pinned_count(&self) -> usize {
        self.pinned.len()
    }

    #[must_use]
    pub fn filter_text(&self) -> &str {
        &self.filter_text
    }

    #[must_use]
    pub fn filter_state(&self) -> &str {
        &self.filter_state
    }

    #[must_use]
    pub fn filter_focus(&self) -> FilterFocus {
        self.filter_focus
    }

    #[must_use]
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    #[must_use]
    pub fn status_kind(&self) -> StatusKind {
        self.status_kind
    }

    #[must_use]
    pub fn confirm(&self) -> Option<&ConfirmState> {
        self.confirm.as_ref()
    }

    #[must_use]
    pub fn wizard(&self) -> &WizardState {
        &self.wizard
    }

    #[must_use]
    pub fn quitting(&self) -> bool {
        self.quitting
    }

    #[must_use]
    pub fn action_busy(&self) -> bool {
        self.action_busy
    }

    #[must_use]
    pub fn selected_log(&self) -> &LogTailView {
        &self.selected_log
    }

    #[must_use]
    pub fn multi_logs(&self) -> &HashMap<String, LogTailView> {
        &self.multi_logs
    }

    // -- data setters (called from refresh/tick) -----------------------------

    pub fn set_loops(&mut self, loops: Vec<LoopView>) {
        let old_id = self.selected_id.clone();
        let old_idx = self.selected_idx;
        self.loops = loops;
        self.apply_filters(&old_id, old_idx);
    }

    pub fn set_selected_log(&mut self, log: LogTailView) {
        self.selected_log = log;
    }

    pub fn set_run_history(&mut self, runs: Vec<RunView>) {
        self.run_history = runs;
        if self.run_history.is_empty() {
            self.selected_run = 0;
            self.log_source = LogSource::Live;
        } else if self.selected_run >= self.run_history.len() {
            self.selected_run = self.run_history.len() - 1;
        }
    }

    pub fn set_multi_logs(&mut self, logs: HashMap<String, LogTailView>) {
        self.multi_logs = logs;
    }

    pub fn set_action_busy(&mut self, busy: bool) {
        self.action_busy = busy;
    }

    pub fn clear_status(&mut self) {
        self.status_text.clear();
    }

    // -- tab management (matching Go) ----------------------------------------

    pub fn set_tab(&mut self, tab: MainTab) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.log_scroll = 0;
        if tab == MainTab::MultiLogs {
            self.focus_right = true;
            self.clamp_multi_page();
        } else if self.focus_right {
            self.focus_right = false;
        }
    }

    pub fn cycle_tab(&mut self, delta: i32) {
        let order = &MainTab::ORDER;
        let mut idx = 0i32;
        for (i, &t) in order.iter().enumerate() {
            if t == self.tab {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        while idx < 0 {
            idx += order.len() as i32;
        }
        self.set_tab(order[(idx as usize) % order.len()]);
    }

    // -- theme ---------------------------------------------------------------

    pub fn cycle_theme(&mut self) {
        self.palette = cycle_palette(self.palette.name, 1);
        self.set_status(StatusKind::Info, &format!("Theme: {}", self.palette.name));
    }

    // -- selection -----------------------------------------------------------

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            self.selected_idx = 0;
            self.selected_id.clear();
            self.log_scroll = 0;
            return;
        }
        let mut idx = self.selected_idx as i32 + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= self.filtered.len() as i32 {
            idx = (self.filtered.len() as i32) - 1;
        }
        self.selected_idx = idx as usize;
        self.selected_id = self.filtered[self.selected_idx].id.clone();
        self.log_scroll = 0;
    }

    #[must_use]
    pub fn selected_view(&self) -> Option<&LoopView> {
        if self.filtered.is_empty() {
            return None;
        }
        let idx = self.selected_idx.min(self.filtered.len().saturating_sub(1));
        Some(&self.filtered[idx])
    }

    // -- pinning -------------------------------------------------------------

    pub fn toggle_pinned(&mut self, loop_id: &str) {
        if loop_id.trim().is_empty() {
            return;
        }
        if self.pinned.contains(loop_id) {
            self.pinned.remove(loop_id);
            self.set_status(StatusKind::Info, &format!("Unpinned {loop_id}"));
        } else {
            self.pinned.insert(loop_id.to_owned());
            self.set_status(StatusKind::Info, &format!("Pinned {loop_id}"));
        }
    }

    pub fn clear_pinned(&mut self) {
        self.pinned.clear();
        self.set_status(StatusKind::Info, "Cleared pinned loops");
    }

    // -- filters -------------------------------------------------------------

    pub fn apply_filters(&mut self, previous_id: &str, previous_idx: usize) {
        let query = self.filter_text.trim().to_ascii_lowercase();
        let state = self.filter_state.trim().to_ascii_lowercase();

        let mut filtered = Vec::with_capacity(self.loops.len());
        for lv in &self.loops {
            let loop_state = lv.state.to_ascii_lowercase();
            if !state.is_empty() && state != "all" && loop_state != state {
                continue;
            }
            if !query.is_empty() {
                let id_lower = lv.id.to_ascii_lowercase();
                let name_lower = lv.name.to_ascii_lowercase();
                let repo_lower = lv.repo_path.to_ascii_lowercase();
                if !id_lower.contains(&query)
                    && !name_lower.contains(&query)
                    && !repo_lower.contains(&query)
                {
                    continue;
                }
            }
            filtered.push(lv.clone());
        }

        self.filtered = filtered;
        if self.filtered.is_empty() {
            self.selected_idx = 0;
            self.selected_id.clear();
            self.multi_page = 0;
            return;
        }

        if !previous_id.is_empty() {
            for (i, lv) in self.filtered.iter().enumerate() {
                if lv.id == previous_id {
                    self.selected_idx = i;
                    self.selected_id = previous_id.to_owned();
                    return;
                }
            }
        }

        let clamped = previous_idx.min(self.filtered.len().saturating_sub(1));
        self.selected_idx = clamped;
        self.selected_id = self.filtered[clamped].id.clone();
        self.clamp_multi_page();
    }

    pub fn cycle_filter_status(&mut self, delta: i32) {
        let mut idx = 0i32;
        for (i, &opt) in FILTER_STATUS_OPTIONS.iter().enumerate() {
            if opt == self.filter_state {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        if idx < 0 {
            idx = FILTER_STATUS_OPTIONS.len() as i32 - 1;
        }
        if idx >= FILTER_STATUS_OPTIONS.len() as i32 {
            idx = 0;
        }
        self.filter_state = FILTER_STATUS_OPTIONS[idx as usize].to_owned();
        let old_id = self.selected_id.clone();
        let old_idx = self.selected_idx;
        self.apply_filters(&old_id, old_idx);
    }

    // -- log source/layer cycling --------------------------------------------

    pub fn cycle_log_source(&mut self, delta: i32) {
        let options = &LogSource::ORDER;
        let mut idx = 0i32;
        for (i, &opt) in options.iter().enumerate() {
            if opt == self.log_source {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        while idx < 0 {
            idx += options.len() as i32;
        }
        self.log_source = options[(idx as usize) % options.len()];
        self.log_scroll = 0;
        self.set_status(
            StatusKind::Info,
            &format!("Log source: {}", self.log_source.label()),
        );
    }

    pub fn cycle_log_layer(&mut self, delta: i32) {
        let options = &LogLayer::ORDER;
        let mut idx = 0i32;
        for (i, &opt) in options.iter().enumerate() {
            if opt == self.log_layer {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        while idx < 0 {
            idx += options.len() as i32;
        }
        self.log_layer = options[(idx as usize) % options.len()];
        self.set_status(
            StatusKind::Info,
            &format!("Log layer: {}", self.log_layer.label()),
        );
    }

    // -- log scrolling -------------------------------------------------------

    pub fn scroll_logs(&mut self, delta: i32) {
        if delta >= 0 {
            self.log_scroll = self.log_scroll.saturating_add(delta as usize);
        } else {
            self.log_scroll = self.log_scroll.saturating_sub((-delta) as usize);
        }
    }

    pub fn scroll_logs_to_top(&mut self) {
        self.log_scroll = MAX_LOG_BACKFILL;
    }

    pub fn scroll_logs_to_bottom(&mut self) {
        self.log_scroll = 0;
    }

    #[must_use]
    pub fn log_scroll_page_size(&self) -> usize {
        // Approximate page based on configured viewport, not full terminal height.
        let estimate = (self.log_lines / 2).saturating_add(LOG_SCROLL_STEP);
        estimate.max(LOG_SCROLL_STEP)
    }

    // -- run selection -------------------------------------------------------

    pub fn move_run_selection(&mut self, delta: i32) {
        if self.run_history.is_empty() {
            self.selected_run = 0;
            return;
        }
        let mut idx = self.selected_run as i32 + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= self.run_history.len() as i32 {
            idx = (self.run_history.len() as i32) - 1;
        }
        self.selected_run = idx as usize;
        self.log_scroll = 0;
    }

    #[must_use]
    pub fn selected_run_view(&self) -> Option<&RunView> {
        if self.run_history.is_empty() {
            return None;
        }
        let idx = self
            .selected_run
            .min(self.run_history.len().saturating_sub(1));
        Some(&self.run_history[idx])
    }

    // -- multi-log / layout helpers ------------------------------------------

    #[must_use]
    pub fn current_layout(&self) -> PaneLayout {
        if PANE_LAYOUTS.is_empty() {
            return PaneLayout { rows: 1, cols: 1 };
        }
        PANE_LAYOUTS[normalize_layout_index(self.layout_idx as i32)]
    }

    pub fn cycle_layout(&mut self, delta: i32) {
        self.layout_idx = normalize_layout_index(self.layout_idx as i32 + delta);
        self.clamp_multi_page();
        self.set_status(
            StatusKind::Info,
            &format!("Layout: {}", self.current_layout().label()),
        );
    }

    pub fn ordered_multi_target_views(&self) -> Vec<&LoopView> {
        let mut ordered: Vec<&LoopView> = Vec::with_capacity(self.filtered.len());
        let mut added = HashSet::new();

        // Pinned first.
        for lv in &self.filtered {
            if self.pinned.contains(&lv.id) {
                ordered.push(lv);
                added.insert(&lv.id);
            }
        }
        // Then unpinned.
        for lv in &self.filtered {
            if !added.contains(&lv.id) {
                ordered.push(lv);
            }
        }
        ordered
    }

    #[must_use]
    pub fn effective_multi_layout(&self) -> PaneLayout {
        let (width, height) = self.multi_viewport_size();
        let grid_height = (height - MULTI_HEADER_ROWS).max(MULTI_MIN_CELL_HEIGHT);
        fit_pane_layout(
            self.current_layout(),
            width,
            grid_height,
            MULTI_CELL_GAP,
            MULTI_MIN_CELL_WIDTH,
            MULTI_MIN_CELL_HEIGHT,
        )
    }

    #[must_use]
    pub fn multi_page_size(&self) -> usize {
        self.effective_multi_layout().capacity().max(1) as usize
    }

    pub fn clamp_multi_page(&mut self) {
        let total = self.ordered_multi_target_views().len();
        let (page, _, _, _) = multi_page_bounds(total, self.multi_page_size(), self.multi_page);
        self.multi_page = page;
    }

    pub fn move_multi_page(&mut self, delta: i32) {
        let total = self.ordered_multi_target_views().len();
        let new_page = if delta >= 0 {
            self.multi_page.saturating_add(delta as usize)
        } else {
            self.multi_page.saturating_sub((-delta) as usize)
        };
        let (page, total_pages, _, _) = multi_page_bounds(total, self.multi_page_size(), new_page);
        self.multi_page = page;
        self.set_status(
            StatusKind::Info,
            &format!("Matrix page {}/{}", page + 1, total_pages),
        );
    }

    pub fn move_multi_page_to_start(&mut self) {
        self.multi_page = 0;
        self.clamp_multi_page();
    }

    pub fn move_multi_page_to_end(&mut self) {
        let total = self.ordered_multi_target_views().len();
        let (page, _, _, _) = multi_page_bounds(total, self.multi_page_size(), usize::MAX / 2);
        self.multi_page = page;
    }

    fn multi_viewport_size(&self) -> (i32, i32) {
        let width = self.width as i32;
        let height = self.height as i32;
        let overhead: i32 =
            4 + match self.mode {
                UiMode::Filter | UiMode::Confirm | UiMode::Wizard | UiMode::Help => 3,
                _ => 0,
            } + if self.status_text.is_empty() { 0 } else { 1 };
        let pane_height = (height - overhead).max(10);
        let right_width = if self.focus_right {
            width
        } else {
            // Approximate right pane width: 60% of total for non-overview tabs.
            (width * 6 / 10).max(1)
        };
        ((right_width - 2).max(1), (pane_height - 2).max(1))
    }

    // -- status bar ----------------------------------------------------------

    pub fn set_status(&mut self, kind: StatusKind, text: &str) {
        self.status_kind = kind;
        self.status_text = text.to_owned();
    }

    // -- wizard helpers ------------------------------------------------------

    pub fn set_wizard(&mut self, wizard: WizardState) {
        self.wizard = wizard;
    }

    pub fn wizard_next_field(&mut self) {
        self.wizard.field += 1;
    }

    pub fn wizard_prev_field(&mut self) {
        if self.wizard.field > 0 {
            self.wizard.field -= 1;
        }
    }

    // -- confirm helpers -----------------------------------------------------

    pub fn enter_confirm(&mut self, action: ActionType) -> Command {
        let view = match self.selected_view() {
            Some(v) => v.clone(),
            None => {
                self.set_status(StatusKind::Info, "No loop selected");
                return Command::None;
            }
        };

        let loop_id = &view.id;
        let prompt = match action {
            ActionType::Stop => {
                format!("Stop loop {loop_id} after current iteration? [y/N]")
            }
            ActionType::Kill => {
                format!("Kill loop {loop_id} immediately? [y/N]")
            }
            ActionType::Delete => {
                if view.state == "stopped" {
                    format!("Delete loop record {loop_id}? [y/N]")
                } else {
                    format!("Loop is still running. Force delete record {loop_id}? [y/N]")
                }
            }
            _ => {
                self.set_status(StatusKind::Err, "Unsupported destructive action");
                return Command::None;
            }
        };

        self.confirm = Some(ConfirmState {
            action,
            loop_id: loop_id.to_owned(),
            prompt,
        });
        self.mode = UiMode::Confirm;
        Command::None
    }

    // -- main update loop ----------------------------------------------------

    /// Process an input event. Returns a command for the host to execute.
    pub fn update(&mut self, event: InputEvent) -> Command {
        if let InputEvent::Resize(r) = event {
            self.width = r.width;
            self.height = r.height;
            if self.tab == MainTab::MultiLogs {
                self.clamp_multi_page();
            }
            return Command::Fetch;
        }

        if let InputEvent::Key(key_event) = event {
            // Ctrl+C is always quit.
            if key_event.key == Key::Char('c') && key_event.modifiers.ctrl {
                self.quitting = true;
                return Command::Quit;
            }

            match self.mode {
                UiMode::Filter => self.update_filter_mode(key_event),
                UiMode::ExpandedLogs => self.update_expanded_logs_mode(key_event),
                UiMode::Confirm => self.update_confirm_mode(key_event),
                UiMode::Wizard => self.update_wizard_mode(key_event),
                UiMode::Help => self.update_help_mode(key_event),
                UiMode::Main => self.update_main_mode(key_event),
            }
        } else {
            Command::None
        }
    }

    fn update_main_mode(&mut self, key: KeyEvent) -> Command {
        match key.key {
            Key::Char('q') => {
                self.quitting = true;
                Command::Quit
            }
            Key::Char('?') => {
                self.help_return = UiMode::Main;
                self.mode = UiMode::Help;
                Command::None
            }
            Key::Char('1') => {
                self.set_tab(MainTab::Overview);
                Command::Fetch
            }
            Key::Char('2') => {
                self.set_tab(MainTab::Logs);
                Command::Fetch
            }
            Key::Char('3') => {
                self.set_tab(MainTab::Runs);
                Command::Fetch
            }
            Key::Char('4') => {
                self.set_tab(MainTab::MultiLogs);
                Command::Fetch
            }
            Key::Char(']') => {
                self.cycle_tab(1);
                Command::Fetch
            }
            Key::Char('[') => {
                self.cycle_tab(-1);
                Command::Fetch
            }
            Key::Char('t') => {
                self.cycle_theme();
                Command::None
            }
            Key::Char('z') => {
                self.focus_right = !self.focus_right;
                if self.tab == MainTab::MultiLogs {
                    self.clamp_multi_page();
                }
                if self.focus_right {
                    self.set_status(StatusKind::Info, "Zen mode: right pane focus");
                } else {
                    self.set_status(StatusKind::Info, "Zen mode: split view");
                }
                if self.tab == MainTab::MultiLogs {
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('/') => {
                self.mode = UiMode::Filter;
                self.filter_focus = FilterFocus::Text;
                Command::None
            }
            Key::Char('j') | Key::Down => {
                self.move_selection(1);
                Command::Fetch
            }
            Key::Char('k') | Key::Up => {
                self.move_selection(-1);
                Command::Fetch
            }
            Key::Char('u') if key.modifiers.ctrl => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    let page = self.log_scroll_page_size() as i32;
                    self.scroll_logs(page);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('d') if key.modifiers.ctrl => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    let page = self.log_scroll_page_size() as i32;
                    self.scroll_logs(-page);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('u') => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    let page = self.log_scroll_page_size() as i32;
                    self.scroll_logs(page);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('d') => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    let page = self.log_scroll_page_size() as i32;
                    self.scroll_logs(-page);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char(' ') => {
                if let Some(view) = self.selected_view().cloned() {
                    self.toggle_pinned(&view.id);
                }
                Command::Fetch
            }
            Key::Char('c') => {
                self.clear_pinned();
                Command::Fetch
            }
            Key::Char('m') => {
                if self.tab == MainTab::MultiLogs {
                    self.cycle_layout(1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('v') => {
                if self.tab == MainTab::Logs {
                    self.cycle_log_source(1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('x') => {
                if self.tab == MainTab::Logs
                    || self.tab == MainTab::Runs
                    || self.tab == MainTab::MultiLogs
                {
                    self.cycle_log_layer(1);
                }
                Command::None
            }
            Key::Char(',') => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    self.move_run_selection(-1);
                    Command::Fetch
                } else if self.tab == MainTab::MultiLogs {
                    self.move_multi_page(-1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('.') => {
                if self.tab == MainTab::Logs || self.tab == MainTab::Runs {
                    self.move_run_selection(1);
                    Command::Fetch
                } else if self.tab == MainTab::MultiLogs {
                    self.move_multi_page(1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('l') => {
                if self.selected_view().is_none() {
                    self.set_status(StatusKind::Info, "No loop selected");
                    Command::None
                } else {
                    self.mode = UiMode::ExpandedLogs;
                    Command::Fetch
                }
            }
            Key::Char('n') => {
                self.mode = UiMode::Wizard;
                self.wizard = WizardState::default();
                Command::None
            }
            Key::Char('r') => {
                let loop_id = match self.selected_view() {
                    Some(v) => v.id.clone(),
                    None => {
                        self.set_status(StatusKind::Info, "No loop selected");
                        return Command::None;
                    }
                };
                self.run_action(ActionType::Resume, &loop_id)
            }
            Key::Char('S') => self.enter_confirm(ActionType::Stop),
            Key::Char('K') => self.enter_confirm(ActionType::Kill),
            Key::Char('D') => self.enter_confirm(ActionType::Delete),
            _ => Command::None,
        }
    }

    fn update_filter_mode(&mut self, key: KeyEvent) -> Command {
        match key.key {
            Key::Char('q') | Key::Escape => {
                self.mode = UiMode::Main;
                self.filter_focus = FilterFocus::Text;
                Command::None
            }
            Key::Char('?') => {
                self.help_return = UiMode::Filter;
                self.mode = UiMode::Help;
                Command::None
            }
            Key::Tab => {
                self.filter_focus = match self.filter_focus {
                    FilterFocus::Text => FilterFocus::Status,
                    FilterFocus::Status => FilterFocus::Text,
                };
                Command::None
            }
            _ => {
                if self.filter_focus == FilterFocus::Status {
                    match key.key {
                        Key::Left | Key::Up | Key::Char('k') => {
                            self.cycle_filter_status(-1);
                            Command::None
                        }
                        Key::Right | Key::Down | Key::Char('j') | Key::Enter => {
                            self.cycle_filter_status(1);
                            Command::None
                        }
                        _ => Command::None,
                    }
                } else {
                    match key.key {
                        Key::Backspace => {
                            if !self.filter_text.is_empty() {
                                self.filter_text.pop();
                                let old_id = self.selected_id.clone();
                                let old_idx = self.selected_idx;
                                self.apply_filters(&old_id, old_idx);
                                Command::Fetch
                            } else {
                                Command::None
                            }
                        }
                        Key::Char(' ') => {
                            self.filter_text.push(' ');
                            let old_id = self.selected_id.clone();
                            let old_idx = self.selected_idx;
                            self.apply_filters(&old_id, old_idx);
                            Command::Fetch
                        }
                        Key::Char(ch) => {
                            self.filter_text.push(ch);
                            let old_id = self.selected_id.clone();
                            let old_idx = self.selected_idx;
                            self.apply_filters(&old_id, old_idx);
                            Command::Fetch
                        }
                        _ => Command::None,
                    }
                }
            }
        }
    }

    fn update_expanded_logs_mode(&mut self, key: KeyEvent) -> Command {
        match key.key {
            Key::Char('q') | Key::Escape => {
                self.mode = UiMode::Main;
                Command::Fetch
            }
            Key::Char('?') => {
                self.help_return = UiMode::ExpandedLogs;
                self.mode = UiMode::Help;
                Command::None
            }
            Key::Char(']') => {
                self.cycle_tab(1);
                Command::Fetch
            }
            Key::Char('[') => {
                self.cycle_tab(-1);
                Command::Fetch
            }
            Key::Char('t') => {
                self.cycle_theme();
                Command::None
            }
            Key::Char('z') => {
                self.focus_right = !self.focus_right;
                Command::None
            }
            Key::Char('j') | Key::Down => {
                self.move_selection(1);
                Command::Fetch
            }
            Key::Char('k') | Key::Up => {
                self.move_selection(-1);
                Command::Fetch
            }
            Key::Char('v') => {
                if self.tab == MainTab::Logs {
                    self.cycle_log_source(1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('x') => {
                self.cycle_log_layer(1);
                Command::None
            }
            Key::Char(',') => {
                self.move_run_selection(-1);
                Command::Fetch
            }
            Key::Char('.') => {
                self.move_run_selection(1);
                Command::Fetch
            }
            Key::Char('/') => {
                self.mode = UiMode::Filter;
                self.filter_focus = FilterFocus::Text;
                Command::None
            }
            Key::Char('S') => {
                self.mode = UiMode::Main;
                self.enter_confirm(ActionType::Stop)
            }
            Key::Char('K') => {
                self.mode = UiMode::Main;
                self.enter_confirm(ActionType::Kill)
            }
            Key::Char('D') => {
                self.mode = UiMode::Main;
                self.enter_confirm(ActionType::Delete)
            }
            Key::Char('r') => {
                let loop_id = match self.selected_view() {
                    Some(v) => v.id.clone(),
                    None => {
                        self.set_status(StatusKind::Info, "No loop selected");
                        return Command::None;
                    }
                };
                self.mode = UiMode::Main;
                self.run_action(ActionType::Resume, &loop_id)
            }
            _ => Command::None,
        }
    }

    fn update_confirm_mode(&mut self, key: KeyEvent) -> Command {
        if self.confirm.is_none() {
            self.mode = UiMode::Main;
            return Command::None;
        }

        match key.key {
            Key::Char('q') | Key::Escape | Key::Char('n') | Key::Char('N') | Key::Enter => {
                self.mode = UiMode::Main;
                self.confirm = None;
                self.set_status(StatusKind::Info, "Action cancelled");
                Command::None
            }
            Key::Char('?') => {
                self.help_return = UiMode::Confirm;
                self.mode = UiMode::Help;
                Command::None
            }
            Key::Char('y') | Key::Char('Y') => {
                let confirm = self.confirm.take();
                self.mode = UiMode::Main;
                if let Some(confirm) = confirm {
                    let force = confirm.action == ActionType::Delete
                        && confirm.prompt.contains("Force delete");
                    let action = match confirm.action {
                        ActionType::Stop => ActionKind::Stop {
                            loop_id: confirm.loop_id,
                        },
                        ActionType::Kill => ActionKind::Kill {
                            loop_id: confirm.loop_id,
                        },
                        ActionType::Delete => ActionKind::Delete {
                            loop_id: confirm.loop_id,
                            force,
                        },
                        _ => return Command::None,
                    };
                    Command::RunAction(action)
                } else {
                    Command::None
                }
            }
            _ => Command::None,
        }
    }

    fn update_wizard_mode(&mut self, key: KeyEvent) -> Command {
        match key.key {
            Key::Char('q') | Key::Escape => {
                self.mode = UiMode::Main;
                self.wizard.error.clear();
                Command::None
            }
            Key::Char('?') => {
                self.help_return = UiMode::Wizard;
                self.mode = UiMode::Help;
                Command::None
            }
            Key::Tab | Key::Down | Key::Char('j') => {
                self.wizard_next_field();
                Command::None
            }
            Key::Char('k') => {
                if !key.modifiers.shift {
                    self.wizard_prev_field();
                }
                Command::None
            }
            Key::Up => {
                self.wizard_prev_field();
                Command::None
            }
            Key::Enter => {
                // Step-through or submit — handled by host.
                Command::None
            }
            Key::Backspace => {
                // Text editing in wizard fields — handled by host.
                Command::None
            }
            Key::Char(_) => {
                // Text entry — handled by host.
                Command::None
            }
            _ => Command::None,
        }
    }

    fn update_help_mode(&mut self, key: KeyEvent) -> Command {
        match key.key {
            Key::Char('q') | Key::Escape | Key::Char('?') => {
                if self.help_return == UiMode::Help {
                    self.mode = UiMode::Main;
                } else {
                    self.mode = self.help_return;
                }
                Command::None
            }
            _ => Command::None,
        }
    }

    fn run_action(&mut self, action: ActionType, loop_id: &str) -> Command {
        if self.action_busy {
            self.set_status(StatusKind::Info, "Another action is still running");
            return Command::None;
        }

        self.action_busy = true;
        let msg = match action {
            ActionType::Create => "Creating loop(s)...",
            ActionType::Resume => "Resuming loop...",
            ActionType::Stop => "Requesting graceful stop...",
            ActionType::Kill => "Killing loop...",
            ActionType::Delete => "Deleting loop record...",
            _ => "Running action...",
        };
        self.set_status(StatusKind::Info, msg);

        match action {
            ActionType::Resume => Command::RunAction(ActionKind::Resume {
                loop_id: loop_id.to_owned(),
            }),
            ActionType::Stop => Command::RunAction(ActionKind::Stop {
                loop_id: loop_id.to_owned(),
            }),
            ActionType::Kill => Command::RunAction(ActionKind::Kill {
                loop_id: loop_id.to_owned(),
            }),
            ActionType::Delete => Command::RunAction(ActionKind::Delete {
                loop_id: loop_id.to_owned(),
                force: false,
            }),
            _ => Command::None,
        }
    }

    // -- render --------------------------------------------------------------

    /// Render the full TUI frame.
    #[must_use]
    pub fn render(&self) -> RenderFrame {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let theme = crate::default_theme();

        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);

        if self.quitting {
            return frame;
        }

        // Header line.
        let header = self.render_header_text(width);
        frame.draw_text(0, 0, &header, TextRole::Accent);

        // Tab bar line.
        let tab_bar = self.render_tab_bar(width);
        frame.draw_text(0, 1, &tab_bar, TextRole::Primary);

        // Content area.
        let content_start = 2;
        let footer_lines = if self.status_text.is_empty() { 1 } else { 2 };
        let content_height = height.saturating_sub(content_start + footer_lines).max(1);

        match self.mode {
            UiMode::Help => {
                self.render_help_content(&mut frame, width, content_height, content_start);
            }
            UiMode::Confirm => {
                if let Some(ref confirm) = self.confirm {
                    let prompt = &confirm.prompt;
                    let truncated = if prompt.len() > width {
                        &prompt[..width]
                    } else {
                        prompt
                    };
                    frame.draw_text(0, content_start, truncated, TextRole::Danger);
                    frame.draw_text(0, content_start + 1, "  y/n", TextRole::Muted);
                }
            }
            UiMode::Filter => {
                let filter_line = format!(
                    "Filter: {} [status: {}]",
                    self.filter_text, self.filter_state
                );
                let truncated = if filter_line.len() > width {
                    &filter_line[..width]
                } else {
                    &filter_line
                };
                frame.draw_text(0, content_start, truncated, TextRole::Accent);
            }
            UiMode::Wizard => {
                let wizard_line = format!(
                    "Create Loop (step {}/4): {}",
                    self.wizard.step + 1,
                    if self.wizard.error.is_empty() {
                        ""
                    } else {
                        &self.wizard.error
                    }
                );
                frame.draw_text(0, content_start, &wizard_line, TextRole::Accent);
            }
            _ => {
                // Delegate to registered view if available.
                if let Some(view) = self.views.get(&self.tab) {
                    let view_frame = view.view(
                        FrameSize {
                            width,
                            height: content_height,
                        },
                        theme,
                    );
                    blit_frame(&mut frame, &view_frame, 0, content_start);
                } else if self.tab == MainTab::Overview {
                    for (idx, line) in self.render_overview_lines(width).iter().enumerate() {
                        if idx >= content_height {
                            break;
                        }
                        frame.draw_text(0, content_start + idx, line, TextRole::Primary);
                    }
                } else {
                    // Placeholder: show tab label + selection info.
                    let info = format!(
                        "{} tab  |  {} loops  |  selected: {}",
                        self.tab.label(),
                        self.filtered.len(),
                        if self.selected_id.is_empty() {
                            "none"
                        } else {
                            &self.selected_id
                        }
                    );
                    frame.draw_text(0, content_start, &info, TextRole::Primary);
                }
            }
        }

        // Status line.
        if !self.status_text.is_empty() {
            let status_y = height.saturating_sub(2);
            let role = match self.status_kind {
                StatusKind::Ok => TextRole::Success,
                StatusKind::Err => TextRole::Danger,
                StatusKind::Info => TextRole::Muted,
            };
            let status_text = self.status_display_text();
            let truncated = if status_text.len() > width {
                &status_text[..width]
            } else {
                &status_text
            };
            frame.draw_text(0, status_y, truncated, role);
        }

        // Footer hint line.
        let footer_y = height.saturating_sub(1);
        let hint = "? help  q quit  / filter  1-4 tabs  j/k sel  S stop  K kill  D del  n new";
        let truncated = if hint.len() > width {
            &hint[..width]
        } else {
            hint
        };
        frame.draw_text(0, footer_y, truncated, TextRole::Muted);

        frame
    }

    fn render_header_text(&self, width: usize) -> String {
        let count_label = format!("{}/{} loops", self.filtered.len(), self.loops.len());
        let header = format!(
            " Forge Loops  [{tab}]  {count}  theme:{theme}",
            tab = self.tab.label(),
            count = count_label,
            theme = self.palette.name,
        );
        if header.len() > width {
            header[..width].to_owned()
        } else {
            header
        }
    }

    fn render_tab_bar(&self, width: usize) -> String {
        let tabs: Vec<String> = MainTab::ORDER
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if *t == self.tab {
                    format!("[{}:{}]", i + 1, t.label())
                } else {
                    format!(" {}:{} ", i + 1, t.label())
                }
            })
            .collect();
        let bar = tabs.join("  ");
        if bar.len() > width {
            bar[..width].to_owned()
        } else {
            bar
        }
    }

    fn render_overview_lines(&self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        if self.filtered.is_empty() {
            lines.push("No loops found.".to_owned());
            lines.push("Start one: forge up --count 1".to_owned());
        } else {
            lines.push(format!(
                "Overview  |  visible {}/{}",
                self.filtered.len(),
                self.loops.len()
            ));
            if let Some(selected) = self.selected_view() {
                lines.push(format!(
                    "Selected: {}  state={}  repo={}",
                    selected.id, selected.state, selected.repo_path
                ));
            }
        }

        lines
            .into_iter()
            .map(|line| {
                if line.len() > width {
                    line[..width].to_owned()
                } else {
                    line
                }
            })
            .collect()
    }

    fn status_display_text(&self) -> String {
        if self.status_kind == StatusKind::Err {
            let trimmed = self.status_text.trim();
            if trimmed.starts_with("Error:") {
                return trimmed.to_owned();
            }
            return format!("Error: {trimmed}");
        }
        self.status_text.clone()
    }

    fn render_help_content(
        &self,
        frame: &mut RenderFrame,
        width: usize,
        height: usize,
        y_offset: usize,
    ) {
        let lines = [
            "=== Forge Loop TUI Help ===",
            "",
            "Navigation:",
            "  1/2/3/4   switch tabs (Overview/Logs/Runs/MultiLogs)",
            "  ]/[       cycle tabs",
            "  j/k       move loop selection",
            "  ,/.       move run selection / multi page",
            "",
            "Actions:",
            "  S         stop selected loop",
            "  K         kill selected loop",
            "  D         delete selected loop",
            "  r         resume selected loop",
            "  n         new loop wizard",
            "",
            "Logs:",
            "  v         cycle log source",
            "  x         cycle log layer",
            "  u/d       scroll logs",
            "  l         expand logs fullscreen",
            "",
            "Multi Logs:",
            "  m         cycle layout",
            "  space     toggle pin",
            "  c         clear pinned",
            "  ,/.       page left/right",
            "",
            "Global:",
            "  ?         toggle help",
            "  q         quit",
            "  t         cycle theme",
            "  z         zen mode (focus right pane)",
            "  /         filter mode",
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
}

/// Blit a source frame onto a destination frame at the given offset.
fn blit_frame(dest: &mut RenderFrame, src: &RenderFrame, x_offset: usize, y_offset: usize) {
    let src_size = src.size();
    for sy in 0..src_size.height {
        for sx in 0..src_size.width {
            if let Some(cell) = src.cell(sx, sy) {
                dest.set_cell(x_offset + sx, y_offset + sy, cell);
            }
        }
    }
}

/// Multi-page pagination bounds, matching Go's `multiPageBounds`.
#[must_use]
pub fn multi_page_bounds(
    total: usize,
    page_size: usize,
    page: usize,
) -> (usize, usize, usize, usize) {
    let page_size = page_size.max(1);
    let total_pages = if total == 0 {
        1
    } else {
        total.div_ceil(page_size)
    };
    let page = page.min(total_pages.saturating_sub(1));

    let start = (page * page_size).min(total);
    let end = (start + page_size).min(total);
    (page, total_pages, start, end)
}

// ---------------------------------------------------------------------------
// PlaceholderView
// ---------------------------------------------------------------------------

/// A minimal view used for tabs that haven't been ported yet.
pub struct PlaceholderView {
    tab: MainTab,
    last_action: String,
}

impl PlaceholderView {
    #[must_use]
    pub fn new(tab: MainTab) -> Self {
        Self {
            tab,
            last_action: String::new(),
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
            self.last_action = format!("{action:?}");
        }
        Command::None
    }

    fn view(&self, size: FrameSize, theme: ThemeSpec) -> RenderFrame {
        let mut frame = RenderFrame::new(size, theme);
        let label = format!("{} tab", self.tab.label());
        frame.draw_text(0, 0, &label, TextRole::Accent);
        frame.draw_text(0, 1, "placeholder content", TextRole::Muted);
        if !self.last_action.is_empty() {
            let status = format!("last: {}", self.last_action);
            frame.draw_text(0, 2, &status, TextRole::Primary);
        }
        frame
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers, ResizeEvent};

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

    fn sample_loops(n: usize) -> Vec<LoopView> {
        (0..n)
            .map(|i| LoopView {
                id: format!("loop-{i}"),
                name: format!("test-loop-{i}"),
                state: if i % 2 == 0 {
                    "running".to_owned()
                } else {
                    "stopped".to_owned()
                },
                repo_path: format!("/repo/{i}"),
                ..Default::default()
            })
            .collect()
    }

    fn app_with_loops(n: usize) -> App {
        let mut app = App::new("default", 12);
        app.set_loops(sample_loops(n));
        app
    }

    // -- MainTab labels --

    #[test]
    fn tab_label_snapshot() {
        let labels: Vec<&str> = MainTab::ORDER.iter().map(|t| t.label()).collect();
        assert_eq!(labels.join("|"), "Overview|Logs|Runs|Multi Logs");
    }

    #[test]
    fn tab_short_label_snapshot() {
        let labels: Vec<&str> = MainTab::ORDER.iter().map(|t| t.short_label()).collect();
        assert_eq!(labels.join("|"), "ov|logs|runs|multi");
    }

    // -- LogSource / LogLayer labels --

    #[test]
    fn log_source_labels() {
        let labels: Vec<&str> = LogSource::ORDER.iter().map(|s| s.label()).collect();
        assert_eq!(labels.join("|"), "live|latest-run|selected-run");
    }

    #[test]
    fn log_layer_labels() {
        let labels: Vec<&str> = LogLayer::ORDER.iter().map(|l| l.label()).collect();
        assert_eq!(labels.join("|"), "raw|events|errors|tools|diff");
    }

    // -- App construction --

    #[test]
    fn new_app_defaults() {
        let app = App::new("default", 0);
        assert_eq!(app.tab(), MainTab::Overview);
        assert_eq!(app.mode(), UiMode::Main);
        assert_eq!(app.log_source(), LogSource::Live);
        assert_eq!(app.log_layer(), LogLayer::Raw);
        assert_eq!(app.palette().name, "default");
        assert_eq!(app.log_lines, DEFAULT_LOG_LINES);
        assert!(app.filtered().is_empty());
        assert!(app.selected_id().is_empty());
    }

    // -- tab switching --

    #[test]
    fn number_keys_switch_tabs() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('2')));
        assert_eq!(app.tab(), MainTab::Logs);
        app.update(key(Key::Char('3')));
        assert_eq!(app.tab(), MainTab::Runs);
        app.update(key(Key::Char('4')));
        assert_eq!(app.tab(), MainTab::MultiLogs);
        app.update(key(Key::Char('1')));
        assert_eq!(app.tab(), MainTab::Overview);
    }

    #[test]
    fn bracket_keys_cycle_tabs() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char(']')));
        assert_eq!(app.tab(), MainTab::Logs);
        app.update(key(Key::Char(']')));
        assert_eq!(app.tab(), MainTab::Runs);
        app.update(key(Key::Char('[')));
        assert_eq!(app.tab(), MainTab::Logs);
    }

    #[test]
    fn multi_logs_tab_sets_focus_right() {
        let mut app = App::new("default", 12);
        assert!(!app.focus_right());
        app.update(key(Key::Char('4')));
        assert!(app.focus_right());
        app.update(key(Key::Char('1')));
        assert!(!app.focus_right());
    }

    // -- quit --

    #[test]
    fn q_quits() {
        let mut app = App::new("default", 12);
        let cmd = app.update(key(Key::Char('q')));
        assert_eq!(cmd, Command::Quit);
        assert!(app.quitting());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new("default", 12);
        let cmd = app.update(ctrl_key('c'));
        assert_eq!(cmd, Command::Quit);
        assert!(app.quitting());
    }

    // -- help mode --

    #[test]
    fn question_mark_enters_help() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('?')));
        assert_eq!(app.mode(), UiMode::Help);
    }

    #[test]
    fn help_exits_on_q() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('?')));
        assert_eq!(app.mode(), UiMode::Help);
        app.update(key(Key::Char('q')));
        assert_eq!(app.mode(), UiMode::Main);
    }

    #[test]
    fn help_returns_to_previous_mode() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('/')));
        assert_eq!(app.mode(), UiMode::Filter);
        app.update(key(Key::Char('?')));
        assert_eq!(app.mode(), UiMode::Help);
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Filter);
    }

    // -- selection --

    #[test]
    fn selection_moves_with_jk() {
        let mut app = app_with_loops(5);
        assert_eq!(app.selected_idx(), 0);
        app.update(key(Key::Char('j')));
        assert_eq!(app.selected_idx(), 1);
        assert_eq!(app.selected_id(), "loop-1");
        app.update(key(Key::Char('k')));
        assert_eq!(app.selected_idx(), 0);
    }

    #[test]
    fn selection_clamps_at_bounds() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('k')));
        assert_eq!(app.selected_idx(), 0);
        app.update(key(Key::Char('j')));
        app.update(key(Key::Char('j')));
        app.update(key(Key::Char('j')));
        app.update(key(Key::Char('j')));
        assert_eq!(app.selected_idx(), 2);
    }

    #[test]
    fn selected_view_returns_none_for_empty() {
        let app = App::new("default", 12);
        assert!(app.selected_view().is_none());
    }

    // -- pinning --

    #[test]
    fn toggle_pin_and_clear() {
        let mut app = app_with_loops(3);
        assert!(!app.is_pinned("loop-0"));
        app.toggle_pinned("loop-0");
        assert!(app.is_pinned("loop-0"));
        assert_eq!(app.pinned_count(), 1);
        app.toggle_pinned("loop-0");
        assert!(!app.is_pinned("loop-0"));
        app.toggle_pinned("loop-1");
        app.clear_pinned();
        assert_eq!(app.pinned_count(), 0);
    }

    #[test]
    fn pin_via_space_key() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char(' ')));
        assert!(app.is_pinned("loop-0"));
    }

    // -- filter mode --

    #[test]
    fn slash_enters_filter_mode() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('/')));
        assert_eq!(app.mode(), UiMode::Filter);
        assert_eq!(app.filter_focus(), FilterFocus::Text);
    }

    #[test]
    fn filter_text_narrows_results() {
        let mut app = app_with_loops(5);
        assert_eq!(app.filtered().len(), 5);
        app.update(key(Key::Char('/')));

        // Type "loop-1"
        for ch in "loop-1".chars() {
            app.update(key(Key::Char(ch)));
        }
        assert_eq!(app.filtered().len(), 1);
        assert_eq!(app.filtered()[0].id, "loop-1");
    }

    #[test]
    fn filter_backspace_removes_char() {
        let mut app = app_with_loops(5);
        app.update(key(Key::Char('/')));
        app.update(key(Key::Char('x')));
        assert_eq!(app.filter_text(), "x");
        app.update(key(Key::Backspace));
        assert_eq!(app.filter_text(), "");
    }

    #[test]
    fn filter_tab_switches_focus() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('/')));
        assert_eq!(app.filter_focus(), FilterFocus::Text);
        app.update(key(Key::Tab));
        assert_eq!(app.filter_focus(), FilterFocus::Status);
        app.update(key(Key::Tab));
        assert_eq!(app.filter_focus(), FilterFocus::Text);
    }

    #[test]
    fn filter_status_cycling() {
        let mut app = app_with_loops(5);
        app.update(key(Key::Char('/')));
        app.update(key(Key::Tab));
        // Cycle status filter.
        app.update(key(Key::Char('j')));
        assert_eq!(app.filter_state(), "running");
        // Should filter to only running loops.
        assert!(app.filtered().iter().all(|l| l.state == "running"));
    }

    #[test]
    fn filter_escape_exits() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('/')));
        assert_eq!(app.mode(), UiMode::Filter);
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Main);
    }

    // -- theme cycling --

    #[test]
    fn t_cycles_theme() {
        let mut app = App::new("default", 12);
        assert_eq!(app.palette().name, "default");
        app.update(key(Key::Char('t')));
        assert_eq!(app.palette().name, "high-contrast");
        app.update(key(Key::Char('t')));
        assert_eq!(app.palette().name, "ocean");
    }

    // -- zen mode --

    #[test]
    fn z_toggles_zen() {
        let mut app = App::new("default", 12);
        assert!(!app.focus_right());
        app.update(key(Key::Char('z')));
        assert!(app.focus_right());
        app.update(key(Key::Char('z')));
        assert!(!app.focus_right());
    }

    // -- log source/layer cycling --

    #[test]
    fn v_cycles_log_source_in_logs_tab() {
        let mut app = App::new("default", 12);
        app.set_tab(MainTab::Logs);
        assert_eq!(app.log_source(), LogSource::Live);
        app.update(key(Key::Char('v')));
        assert_eq!(app.log_source(), LogSource::LatestRun);
        app.update(key(Key::Char('v')));
        assert_eq!(app.log_source(), LogSource::RunSelection);
    }

    #[test]
    fn v_noop_in_overview_tab() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('v')));
        assert_eq!(app.log_source(), LogSource::Live);
    }

    #[test]
    fn x_cycles_log_layer() {
        let mut app = App::new("default", 12);
        app.set_tab(MainTab::Logs);
        assert_eq!(app.log_layer(), LogLayer::Raw);
        app.update(key(Key::Char('x')));
        assert_eq!(app.log_layer(), LogLayer::Events);
    }

    // -- log scrolling --

    #[test]
    fn u_d_scroll_in_logs_tab() {
        let mut app = App::new("default", 12);
        app.set_tab(MainTab::Logs);
        assert_eq!(app.log_scroll(), 0);
        app.update(key(Key::Char('u')));
        assert!(app.log_scroll() > 0);
    }

    // -- run selection --

    #[test]
    fn comma_dot_move_run_selection() {
        let mut app = App::new("default", 12);
        app.set_tab(MainTab::Logs);
        app.set_run_history(vec![
            RunView {
                id: "run-0".into(),
                ..Default::default()
            },
            RunView {
                id: "run-1".into(),
                ..Default::default()
            },
        ]);
        assert_eq!(
            app.selected_run_view().map(|r| r.id.as_str()),
            Some("run-0")
        );
        app.update(key(Key::Char('.')));
        assert_eq!(
            app.selected_run_view().map(|r| r.id.as_str()),
            Some("run-1")
        );
        app.update(key(Key::Char(',')));
        assert_eq!(
            app.selected_run_view().map(|r| r.id.as_str()),
            Some("run-0")
        );
    }

    // -- expanded logs mode --

    #[test]
    fn l_enters_expanded_logs() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('l')));
        assert_eq!(app.mode(), UiMode::ExpandedLogs);
    }

    #[test]
    fn l_noop_without_selection() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('l')));
        assert_eq!(app.mode(), UiMode::Main);
    }

    #[test]
    fn expanded_logs_escape_returns_to_main() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('l')));
        assert_eq!(app.mode(), UiMode::ExpandedLogs);
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Main);
    }

    // -- wizard mode --

    #[test]
    fn n_enters_wizard() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.mode(), UiMode::Wizard);
    }

    #[test]
    fn wizard_escape_exits() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Main);
    }

    // -- confirm mode --

    #[test]
    fn s_enters_confirm_for_stop() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('S')));
        assert_eq!(app.mode(), UiMode::Confirm);
        let confirm = app.confirm();
        assert!(confirm.is_some());
        let confirm = match confirm {
            Some(v) => v,
            None => panic!("expected confirm state"),
        };
        assert_eq!(confirm.action, ActionType::Stop);
        assert!(confirm.prompt.contains("Stop loop"));
    }

    #[test]
    fn confirm_n_cancels() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('S')));
        assert_eq!(app.mode(), UiMode::Confirm);
        app.update(key(Key::Char('n')));
        assert_eq!(app.mode(), UiMode::Main);
        assert!(app.confirm().is_none());
    }

    #[test]
    fn confirm_y_returns_action() {
        let mut app = app_with_loops(3);
        app.update(key(Key::Char('S')));
        let cmd = app.update(key(Key::Char('y')));
        assert_eq!(app.mode(), UiMode::Main);
        match cmd {
            Command::RunAction(ActionKind::Stop { loop_id }) => {
                assert_eq!(loop_id, "loop-0");
            }
            other => panic!("Expected RunAction(Stop), got {other:?}"),
        }
    }

    // -- confirm no selection --

    #[test]
    fn confirm_noop_without_selection() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('S')));
        assert_eq!(app.mode(), UiMode::Main);
        assert!(app.confirm().is_none());
    }

    // -- resize --

    #[test]
    fn resize_updates_dimensions() {
        let mut app = App::new("default", 12);
        app.update(InputEvent::Resize(ResizeEvent {
            width: 200,
            height: 50,
        }));
        assert_eq!(app.width(), 200);
        assert_eq!(app.height(), 50);
    }

    // -- render smoke test --

    #[test]
    fn render_produces_non_empty_frame() {
        let app = app_with_loops(3);
        let frame = app.render();
        assert_eq!(frame.size().width, 120);
        assert_eq!(frame.size().height, 40);
        assert!(frame.row_text(0).contains("Forge Loops"));
        assert!(frame.row_text(1).contains("Overview"));
    }

    #[test]
    fn overview_empty_state_guides_loop_creation() {
        let app = App::new("default", 12);
        let frame = app.render();

        let all_rows = frame.snapshot();
        assert!(
            all_rows.contains("Start one: forge up --count 1"),
            "rendered rows:\\n{all_rows}"
        );
    }

    #[test]
    fn render_error_state_shows_prefixed_error_text() {
        let mut app = App::new("default", 12);
        app.set_status(StatusKind::Err, "boom");

        let frame = app.render();
        let all_rows = frame.snapshot();
        assert!(
            all_rows.contains("Error: boom"),
            "rendered rows:\\n{all_rows}"
        );
    }

    #[test]
    fn render_help_mode_shows_help() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('?')));
        let frame = app.render();
        assert!(frame.row_text(2).contains("Forge Loop TUI Help"));
    }

    #[test]
    fn render_quitting_returns_blank() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('q')));
        let frame = app.render();
        // All rows should be blank.
        assert!(frame.row_text(0).trim().is_empty());
    }

    // -- multi page bounds --

    #[test]
    fn multi_page_bounds_basic() {
        let (page, total_pages, start, end) = multi_page_bounds(10, 4, 0);
        assert_eq!(page, 0);
        assert_eq!(total_pages, 3);
        assert_eq!(start, 0);
        assert_eq!(end, 4);
    }

    #[test]
    fn multi_page_bounds_clamps() {
        let (page, total_pages, start, end) = multi_page_bounds(10, 4, 999);
        assert_eq!(page, 2);
        assert_eq!(total_pages, 3);
        assert_eq!(start, 8);
        assert_eq!(end, 10);
    }

    #[test]
    fn multi_page_bounds_empty() {
        let (page, total_pages, start, end) = multi_page_bounds(0, 4, 0);
        assert_eq!(page, 0);
        assert_eq!(total_pages, 1);
        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    // -- ordered multi target views --

    #[test]
    fn ordered_multi_targets_pinned_first() {
        let mut app = app_with_loops(4);
        app.toggle_pinned("loop-2");
        let ordered = app.ordered_multi_target_views();
        assert_eq!(ordered[0].id, "loop-2");
        assert_eq!(ordered.len(), 4);
    }

    // -- placeholder view --

    #[test]
    fn placeholder_view_renders() {
        let view = PlaceholderView::new(MainTab::Overview);
        let frame = view.view(
            FrameSize {
                width: 40,
                height: 3,
            },
            crate::default_theme(),
        );
        assert!(frame.row_text(0).contains("Overview tab"));
    }

    // -- resume action --

    #[test]
    fn r_resumes_selected() {
        let mut app = app_with_loops(3);
        let cmd = app.update(key(Key::Char('r')));
        match cmd {
            Command::RunAction(ActionKind::Resume { loop_id }) => {
                assert_eq!(loop_id, "loop-0");
            }
            other => panic!("Expected RunAction(Resume), got {other:?}"),
        }
    }

    #[test]
    fn r_noop_without_selection() {
        let mut app = App::new("default", 12);
        let cmd = app.update(key(Key::Char('r')));
        assert_eq!(cmd, Command::None);
    }

    // -- action busy --

    #[test]
    fn action_busy_blocks_new_action() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        let cmd = app.update(key(Key::Char('r')));
        assert_eq!(cmd, Command::None);
        assert!(app.status_text().contains("Another action"));
    }

    // -- set_loops preserves selection --

    #[test]
    fn set_loops_preserves_selected_id() {
        let mut app = app_with_loops(5);
        app.move_selection(2);
        assert_eq!(app.selected_id(), "loop-2");
        // Re-set loops with same data.
        app.set_loops(sample_loops(5));
        assert_eq!(app.selected_id(), "loop-2");
        assert_eq!(app.selected_idx(), 2);
    }

    // -- delete confirm force --

    #[test]
    fn delete_running_loop_shows_force() {
        let mut app = app_with_loops(3);
        // loop-0 is "running"
        app.update(key(Key::Char('D')));
        let confirm = match app.confirm() {
            Some(v) => v,
            None => panic!("expected confirm state"),
        };
        assert!(confirm.prompt.contains("Force delete"));
    }

    #[test]
    fn delete_stopped_loop_normal() {
        let mut app = app_with_loops(3);
        app.move_selection(1); // loop-1 is "stopped"
        app.update(key(Key::Char('D')));
        let confirm = match app.confirm() {
            Some(v) => v,
            None => panic!("expected confirm state"),
        };
        assert!(confirm.prompt.contains("Delete loop record"));
        assert!(!confirm.prompt.contains("Force"));
    }
}

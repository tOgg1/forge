//! App shell and state model for the Forge loop TUI.
//!
//! Ports the Go `internal/looptui/looptui.go` model: tab-based navigation,
//! modal UI modes (filter/confirm/wizard/help/expanded-logs), loop selection,
//! log source/layer cycling, multi-log pagination, and pinned loops.

use std::collections::{HashMap, HashSet};

use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

use crate::command_palette::{
    CommandPalette, PaletteActionId, PaletteContext, DEFAULT_SEARCH_BUDGET,
};
use crate::keymap::{KeyChord, KeyCommand, KeyScope, Keymap, ModeScope};
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

/// Main tabs for the Forge operator shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainTab {
    Overview,
    Logs,
    Runs,
    MultiLogs,
    Inbox,
}

impl MainTab {
    pub const ORDER: [MainTab; 5] = [
        MainTab::Overview,
        MainTab::Logs,
        MainTab::Runs,
        MainTab::MultiLogs,
        MainTab::Inbox,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Runs => "Runs",
            Self::MultiLogs => "Multi Logs",
            Self::Inbox => "Inbox",
        }
    }

    #[must_use]
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Overview => "ov",
            Self::Logs => "logs",
            Self::Runs => "runs",
            Self::MultiLogs => "multi",
            Self::Inbox => "inbox",
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
    Palette,
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
// DensityMode / FocusMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityMode {
    Comfortable,
    Compact,
}

impl DensityMode {
    pub const ORDER: [DensityMode; 2] = [DensityMode::Comfortable, DensityMode::Compact];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    Standard,
    DeepDebug,
}

impl FocusMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::DeepDebug => "deep",
        }
    }
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
    /// Preferred short id for display; falls back to `id` truncated to 8.
    pub short_id: String,
    pub name: String,
    pub state: String,
    pub repo_path: String,
    pub runs: usize,
    pub queue_depth: usize,
    /// RFC3339 UTC timestamp (already formatted), or `None`.
    pub last_run_at: Option<String>,
    pub interval_seconds: i64,
    pub max_runtime_seconds: i64,
    pub max_iterations: i64,
    pub last_error: String,
    pub profile_name: String,
    pub profile_harness: String,
    pub profile_auth: String,
    pub profile_id: String,
    pub pool_name: String,
    pub pool_id: String,
}

/// A single run entry. Matches Go's `runView`.
#[derive(Debug, Clone, Default)]
pub struct RunView {
    pub id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    /// Preformatted duration (e.g. "12s", "1m0s", "running", "-").
    pub duration: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxFilter {
    All,
    Unread,
    AckRequired,
}

impl InboxFilter {
    const ORDER: [InboxFilter; 3] = [
        InboxFilter::All,
        InboxFilter::Unread,
        InboxFilter::AckRequired,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unread => "unread",
            Self::AckRequired => "ack-required",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InboxMessageView {
    pub id: i64,
    pub thread_id: Option<String>,
    pub from: String,
    pub subject: String,
    pub body: String,
    pub created_at: String,
    pub ack_required: bool,
    pub read_at: Option<String>,
    pub acked_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct InboxThreadView {
    thread_key: String,
    message_indices: Vec<usize>,
    subject: String,
    latest_created_at: String,
    latest_message_id: i64,
    unread_count: usize,
    pending_ack_count: usize,
    participant_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ClaimEventView {
    pub task_id: String,
    pub claimed_by: String,
    pub claimed_at: String,
}

#[derive(Debug, Clone, Default)]
struct ClaimConflictView {
    task_id: String,
    latest_by: String,
    previous_by: String,
    latest_at: String,
}

#[derive(Debug, Clone, Default)]
struct HandoffSnapshotView {
    thread_key: String,
    task_id: String,
    loop_id: String,
    status: String,
    context: String,
    links: String,
    pending_risks: String,
}

impl HandoffSnapshotView {
    fn lines(&self) -> [String; 5] {
        [
            format!("task={} loop={}", self.task_id, self.loop_id),
            format!("status: {}", self.status),
            format!("context: {}", self.context),
            format!("links: {}", self.links),
            format!("pending-risks: {}", self.pending_risks),
        ]
    }
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

#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: usize,
    pub field: usize,
    pub values: WizardValues,
    pub error: String,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            step: 1,
            field: 0,
            values: WizardValues {
                count: "1".to_owned(),
                ..WizardValues::default()
            },
            error: String::new(),
        }
    }
}

impl WizardState {
    /// Create a wizard state pre-populated with config defaults.
    /// Matches Go `newWizardState(defaultInterval, defaultPrompt, defaultPromptMsg)`.
    #[must_use]
    pub fn with_defaults(interval: &str, prompt: &str, prompt_msg: &str) -> Self {
        Self {
            step: 1,
            field: 0,
            values: WizardValues {
                count: "1".to_owned(),
                prompt: prompt.trim().to_owned(),
                prompt_msg: prompt_msg.trim().to_owned(),
                interval: interval.trim().to_owned(),
                ..WizardValues::default()
            },
            error: String::new(),
        }
    }
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

/// Result of an asynchronous action execution. Matches Go's `actionResultMsg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionResult {
    pub kind: ActionType,
    pub loop_id: String,
    /// For create actions, the ID of the newly selected loop.
    pub selected_loop_id: String,
    /// Human-readable success message.
    pub message: String,
    /// Error message, if the action failed.
    pub error: Option<String>,
}

fn wizard_field_count(step: usize) -> usize {
    match step {
        1 => 3,
        2 => 2,
        3 => 6,
        _ => 0,
    }
}

fn wizard_field_key(step: usize, field: usize) -> Option<&'static str> {
    match step {
        1 => match field {
            0 => Some("name"),
            1 => Some("name_prefix"),
            2 => Some("count"),
            _ => None,
        },
        2 => match field {
            0 => Some("pool"),
            1 => Some("profile"),
            _ => None,
        },
        3 => match field {
            0 => Some("prompt"),
            1 => Some("prompt_msg"),
            2 => Some("interval"),
            3 => Some("max_runtime"),
            4 => Some("max_iterations"),
            5 => Some("tags"),
            _ => None,
        },
        _ => None,
    }
}

fn wizard_get<'a>(values: &'a WizardValues, key: &str) -> &'a str {
    match key {
        "name" => values.name.as_str(),
        "name_prefix" => values.name_prefix.as_str(),
        "count" => values.count.as_str(),
        "pool" => values.pool.as_str(),
        "profile" => values.profile.as_str(),
        "prompt" => values.prompt.as_str(),
        "prompt_msg" => values.prompt_msg.as_str(),
        "interval" => values.interval.as_str(),
        "max_runtime" => values.max_runtime.as_str(),
        "max_iterations" => values.max_iterations.as_str(),
        "tags" => values.tags.as_str(),
        _ => "",
    }
}

fn wizard_set(values: &mut WizardValues, key: &str, value: String) {
    match key {
        "name" => values.name = value,
        "name_prefix" => values.name_prefix = value,
        "count" => values.count = value,
        "pool" => values.pool = value,
        "profile" => values.profile = value,
        "prompt" => values.prompt = value,
        "prompt_msg" => values.prompt_msg = value,
        "interval" => values.interval = value,
        "max_runtime" => values.max_runtime = value,
        "max_iterations" => values.max_iterations = value,
        "tags" => values.tags = value,
        _ => {}
    }
}

fn parse_duration_value(raw: &str, field: &str) -> Result<(), String> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(());
    }
    if value.starts_with('-') {
        return Err(format!("{field} must be >= 0"));
    }

    let (number, unit) = if let Some(stripped) = value.strip_suffix("ms") {
        (stripped, "ms")
    } else if let Some(stripped) = value.strip_suffix('s') {
        (stripped, "s")
    } else if let Some(stripped) = value.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = value.strip_suffix('h') {
        (stripped, "h")
    } else {
        (value, "s")
    };

    if number.trim().is_empty() {
        return Err(format!("invalid {field} {raw:?}"));
    }
    if number.trim().parse::<f64>().is_err() {
        return Err(format!("invalid {field} {raw:?}"));
    }
    if unit.is_empty() {
        return Err(format!("invalid {field} {raw:?}"));
    }
    Ok(())
}

fn validate_wizard_step(step: usize, values: &WizardValues) -> Result<(), String> {
    match step {
        1 => {
            let count_raw = if values.count.trim().is_empty() {
                "1"
            } else {
                values.count.trim()
            };
            let count = count_raw
                .parse::<i64>()
                .map_err(|_| format!("invalid count {:?}", values.count))?;
            if count < 1 {
                return Err("count must be >= 1".to_owned());
            }
            if !values.name.trim().is_empty() && !values.count.trim().is_empty() && count > 1 {
                return Err("name requires count=1".to_owned());
            }
        }
        2 => {
            if !values.pool.trim().is_empty() && !values.profile.trim().is_empty() {
                return Err("use either pool or profile, not both".to_owned());
            }
        }
        3 => {
            parse_duration_value(&values.interval, "interval")?;
            parse_duration_value(&values.max_runtime, "max runtime")?;
            if !values.max_iterations.trim().is_empty() {
                let parsed =
                    values.max_iterations.trim().parse::<i64>().map_err(|_| {
                        format!("invalid max-iterations {:?}", values.max_iterations)
                    })?;
                if parsed < 0 {
                    return Err("max-iterations must be >= 0".to_owned());
                }
            }
        }
        _ => {}
    }
    Ok(())
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
    density_mode: DensityMode,
    focus_mode: FocusMode,
    layout_idx: usize,
    multi_page: usize,
    multi_logs: HashMap<String, LogTailView>,
    pinned: HashSet<String>,
    inbox_messages: Vec<InboxMessageView>,
    inbox_filter: InboxFilter,
    inbox_selected_thread: usize,
    claim_events: Vec<ClaimEventView>,
    selected_claim_conflict: usize,
    handoff_snapshot: Option<HandoffSnapshotView>,

    // -- filter --
    filter_text: String,
    filter_state: String,
    filter_focus: FilterFocus,

    // -- confirm/wizard --
    confirm: Option<ConfirmState>,
    wizard: WizardState,

    // -- wizard defaults (from config) --
    default_interval: String,
    default_prompt: String,
    default_prompt_msg: String,

    // -- status bar --
    status_text: String,
    status_kind: StatusKind,
    action_busy: bool,

    // -- display --
    width: usize,
    height: usize,
    palette: Palette,
    keymap: Keymap,
    command_palette: CommandPalette,
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
            density_mode: DensityMode::Comfortable,
            focus_mode: FocusMode::Standard,
            layout_idx: layout_index_for(2, 2),
            multi_page: 0,
            multi_logs: HashMap::new(),
            pinned: HashSet::new(),
            inbox_messages: Vec::new(),
            inbox_filter: InboxFilter::All,
            inbox_selected_thread: 0,
            claim_events: Vec::new(),
            selected_claim_conflict: 0,
            handoff_snapshot: None,

            filter_text: String::new(),
            filter_state: "all".to_owned(),
            filter_focus: FilterFocus::Text,

            confirm: None,
            wizard: WizardState::default(),

            default_interval: String::new(),
            default_prompt: String::new(),
            default_prompt_msg: String::new(),

            status_text: String::new(),
            status_kind: StatusKind::Info,
            action_busy: false,

            width: 120,
            height: 40,
            palette,
            keymap: Keymap::default_forge_tui(),
            command_palette: CommandPalette::new_default(),
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
    pub fn density_mode(&self) -> DensityMode {
        self.density_mode
    }

    #[must_use]
    pub fn focus_mode(&self) -> FocusMode {
        self.focus_mode
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
    pub fn palette_query(&self) -> &str {
        self.command_palette.query()
    }

    #[must_use]
    pub fn palette_match_count(&self) -> usize {
        self.command_palette.matches().len()
    }

    fn palette_context(&self) -> PaletteContext {
        PaletteContext {
            tab: self.tab,
            has_selection: self.selected_view().is_some(),
        }
    }

    fn key_scope_chain(&self) -> [KeyScope; 3] {
        let mode_scope = match self.mode {
            UiMode::Main => ModeScope::Main,
            UiMode::Filter => ModeScope::Filter,
            UiMode::ExpandedLogs => ModeScope::ExpandedLogs,
            UiMode::Confirm => ModeScope::Confirm,
            UiMode::Wizard => ModeScope::Wizard,
            UiMode::Help => ModeScope::Help,
            UiMode::Palette => ModeScope::Palette,
        };
        [
            KeyScope::View(self.tab),
            KeyScope::Mode(mode_scope),
            KeyScope::Global,
        ]
    }

    fn resolve_key_command(&self, key: KeyEvent) -> Option<KeyCommand> {
        self.keymap
            .resolve(&self.key_scope_chain(), KeyChord::from_event(key))
    }

    #[must_use]
    pub fn multi_logs(&self) -> &HashMap<String, LogTailView> {
        &self.multi_logs
    }

    #[must_use]
    pub fn multi_page(&self) -> usize {
        self.multi_page
    }

    #[must_use]
    pub fn inbox_filter(&self) -> InboxFilter {
        self.inbox_filter
    }

    #[must_use]
    pub fn inbox_messages(&self) -> &[InboxMessageView] {
        &self.inbox_messages
    }

    pub fn set_inbox_messages(&mut self, messages: Vec<InboxMessageView>) {
        self.inbox_messages = messages;
        self.handoff_snapshot = None;
        self.clamp_inbox_selection();
    }

    #[must_use]
    pub fn claim_events(&self) -> &[ClaimEventView] {
        &self.claim_events
    }

    pub fn set_claim_events(&mut self, events: Vec<ClaimEventView>) {
        self.claim_events = events;
        self.claim_events.sort_by(|a, b| {
            b.claimed_at
                .cmp(&a.claimed_at)
                .then(a.task_id.cmp(&b.task_id))
        });
        let conflicts = self.claim_conflicts();
        if conflicts.is_empty() {
            self.selected_claim_conflict = 0;
        } else {
            self.selected_claim_conflict = self
                .selected_claim_conflict
                .min(conflicts.len().saturating_sub(1));
        }
    }

    pub fn set_layout_idx(&mut self, idx: usize) {
        self.layout_idx = idx;
    }

    fn inbox_threads(&self) -> Vec<InboxThreadView> {
        let mut grouped: HashMap<String, InboxThreadView> = HashMap::new();
        for (idx, message) in self.inbox_messages.iter().enumerate() {
            let thread_key = inbox_thread_key(message);
            let entry = grouped
                .entry(thread_key.clone())
                .or_insert_with(|| InboxThreadView {
                    thread_key: thread_key.clone(),
                    ..InboxThreadView::default()
                });

            entry.message_indices.push(idx);
            if entry.subject.is_empty() {
                entry.subject = if message.subject.trim().is_empty() {
                    "(no subject)".to_owned()
                } else {
                    message.subject.trim().to_owned()
                };
            }
            if message.created_at >= entry.latest_created_at {
                entry.latest_created_at = message.created_at.clone();
                entry.latest_message_id = message.id;
            }
            if message.read_at.is_none() {
                entry.unread_count += 1;
            }
            if message.ack_required && message.acked_at.is_none() {
                entry.pending_ack_count += 1;
            }
        }

        let mut threads: Vec<InboxThreadView> = grouped
            .into_values()
            .map(|mut thread| {
                thread.message_indices.sort_by(|a, b| {
                    self.inbox_messages[*a]
                        .created_at
                        .cmp(&self.inbox_messages[*b].created_at)
                });
                let mut participants = HashSet::new();
                for index in &thread.message_indices {
                    participants.insert(self.inbox_messages[*index].from.trim().to_owned());
                }
                thread.participant_count = participants.len();
                thread
            })
            .collect();

        threads.retain(|thread| match self.inbox_filter {
            InboxFilter::All => true,
            InboxFilter::Unread => thread.unread_count > 0,
            InboxFilter::AckRequired => thread.pending_ack_count > 0,
        });
        threads.sort_by(|a, b| {
            b.latest_created_at
                .cmp(&a.latest_created_at)
                .then(b.latest_message_id.cmp(&a.latest_message_id))
        });
        threads
    }

    fn clamp_inbox_selection(&mut self) {
        let total = self.inbox_threads().len();
        if total == 0 {
            self.inbox_selected_thread = 0;
            return;
        }
        self.inbox_selected_thread = self.inbox_selected_thread.min(total.saturating_sub(1));
    }

    fn move_inbox_selection(&mut self, delta: i32) {
        let total = self.inbox_threads().len();
        if total == 0 {
            self.inbox_selected_thread = 0;
            return;
        }
        let mut idx = self.inbox_selected_thread as i32 + delta;
        if idx < 0 {
            idx = 0;
        }
        if idx >= total as i32 {
            idx = total as i32 - 1;
        }
        self.inbox_selected_thread = idx as usize;
    }

    fn cycle_inbox_filter(&mut self, delta: i32) {
        let mut idx = 0i32;
        for (i, option) in InboxFilter::ORDER.iter().enumerate() {
            if *option == self.inbox_filter {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        while idx < 0 {
            idx += InboxFilter::ORDER.len() as i32;
        }
        self.inbox_filter = InboxFilter::ORDER[(idx as usize) % InboxFilter::ORDER.len()];
        self.clamp_inbox_selection();
        self.set_status(
            StatusKind::Info,
            &format!("Inbox filter: {}", self.inbox_filter.label()),
        );
    }

    fn mark_selected_inbox_thread_read(&mut self) {
        let threads = self.inbox_threads();
        let Some(thread) = threads.get(self.inbox_selected_thread) else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };

        let mut marked = 0usize;
        for index in &thread.message_indices {
            if let Some(message) = self.inbox_messages.get_mut(*index) {
                if message.read_at.is_none() {
                    message.read_at = Some("now".to_owned());
                    marked += 1;
                }
            }
        }
        if marked == 0 {
            self.set_status(StatusKind::Info, "Thread already read");
        } else {
            self.set_status(StatusKind::Ok, &format!("Marked {marked} message(s) read"));
        }
        self.clamp_inbox_selection();
    }

    fn acknowledge_selected_inbox_message(&mut self) {
        let threads = self.inbox_threads();
        let Some(thread) = threads.get(self.inbox_selected_thread) else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };

        let mut acked_id = None;
        for index in thread.message_indices.iter().rev() {
            if let Some(message) = self.inbox_messages.get_mut(*index) {
                if message.ack_required && message.acked_at.is_none() {
                    message.acked_at = Some("now".to_owned());
                    acked_id = Some(message.id);
                    break;
                }
            }
        }

        if let Some(id) = acked_id {
            self.set_status(
                StatusKind::Ok,
                &format!("Acknowledged {}", format_mail_id(id)),
            );
        } else {
            self.set_status(StatusKind::Info, "No pending ack in selected thread");
        }
        self.clamp_inbox_selection();
    }

    fn quick_reply_selected_inbox_thread(&mut self) {
        let threads = self.inbox_threads();
        let Some(thread) = threads.get(self.inbox_selected_thread) else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };
        let Some(latest_index) = thread.message_indices.last().copied() else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };
        let message = &self.inbox_messages[latest_index];
        let target = message.from.trim();
        let target = if target.is_empty() { "unknown" } else { target };
        self.set_status(
            StatusKind::Info,
            &format!(
                "Reply shortcut: to {target}, thread {}, reply-to {}",
                thread.thread_key,
                format_mail_id(message.id)
            ),
        );
    }

    fn claim_conflicts(&self) -> Vec<ClaimConflictView> {
        let mut by_task: HashMap<String, Vec<&ClaimEventView>> = HashMap::new();
        for event in &self.claim_events {
            if event.task_id.trim().is_empty() {
                continue;
            }
            by_task
                .entry(event.task_id.clone())
                .or_default()
                .push(event);
        }

        let mut conflicts = Vec::new();
        for (task_id, events) in by_task {
            if events.len() < 2 {
                continue;
            }
            let latest = events[0];
            let previous = events.iter().skip(1).find(|event| {
                !event.claimed_by.trim().is_empty() && event.claimed_by != latest.claimed_by
            });
            let Some(previous) = previous else {
                continue;
            };
            conflicts.push(ClaimConflictView {
                task_id,
                latest_by: latest.claimed_by.clone(),
                previous_by: previous.claimed_by.clone(),
                latest_at: latest.claimed_at.clone(),
            });
        }

        conflicts.sort_by(|a, b| {
            b.latest_at
                .cmp(&a.latest_at)
                .then(a.task_id.cmp(&b.task_id))
        });
        conflicts
    }

    fn cycle_claim_conflict(&mut self, delta: i32) {
        let conflicts = self.claim_conflicts();
        if conflicts.is_empty() {
            self.selected_claim_conflict = 0;
            self.set_status(StatusKind::Info, "No claim ownership conflicts");
            return;
        }
        let len = conflicts.len() as i32;
        let mut idx = self.selected_claim_conflict as i32 + delta;
        while idx < 0 {
            idx += len;
        }
        self.selected_claim_conflict = (idx as usize) % conflicts.len();
        let conflict = &conflicts[self.selected_claim_conflict];
        self.set_status(
            StatusKind::Err,
            &format!(
                "Claim conflict {}: {} vs {}",
                conflict.task_id, conflict.latest_by, conflict.previous_by
            ),
        );
    }

    fn show_claim_resolution_hint(&mut self) {
        let conflicts = self.claim_conflicts();
        let Some(conflict) = conflicts.get(self.selected_claim_conflict) else {
            self.set_status(StatusKind::Info, "No claim conflicts to resolve");
            return;
        };
        self.set_status(
            StatusKind::Info,
            &format!(
                "Resolve {}: confirm owner, then post `fmail send task \"takeover claim: {} by <agent>\"`",
                conflict.task_id, conflict.task_id
            ),
        );
    }

    fn extract_task_id_from_thread(&self, thread: &InboxThreadView) -> Option<String> {
        for index in thread.message_indices.iter().rev() {
            let Some(message) = self.inbox_messages.get(*index) else {
                continue;
            };
            if let Some(task_id) = extract_prefixed_token(&message.subject, "forge-")
                .or_else(|| extract_prefixed_token(&message.body, "forge-"))
            {
                return Some(task_id);
            }
        }
        None
    }

    fn extract_loop_id_from_thread(&self, thread: &InboxThreadView) -> Option<String> {
        for index in thread.message_indices.iter().rev() {
            let Some(message) = self.inbox_messages.get(*index) else {
                continue;
            };
            if let Some(loop_id) = extract_prefixed_token(&message.subject, "loop-")
                .or_else(|| extract_prefixed_token(&message.body, "loop-"))
            {
                return Some(loop_id);
            }
        }
        None
    }

    fn loop_state_for_handoff(&self, loop_id: &str) -> Option<String> {
        if loop_id.trim().is_empty() {
            return None;
        }
        self.loops
            .iter()
            .find(|view| view.id == loop_id)
            .map(|view| view.state.trim().to_owned())
            .filter(|state| !state.is_empty())
    }

    fn generate_handoff_snapshot(&mut self) {
        let threads = self.inbox_threads();
        let Some(thread) = threads.get(self.inbox_selected_thread) else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };
        let Some(latest_index) = thread.message_indices.last().copied() else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };
        let Some(latest_message) = self.inbox_messages.get(latest_index) else {
            self.set_status(StatusKind::Info, "Inbox is empty");
            return;
        };

        let claim_conflicts = self.claim_conflicts();
        let task_id = self
            .extract_task_id_from_thread(thread)
            .or_else(|| {
                claim_conflicts
                    .get(self.selected_claim_conflict)
                    .map(|conflict| conflict.task_id.clone())
            })
            .or_else(|| self.claim_events.first().map(|event| event.task_id.clone()))
            .unwrap_or_else(|| "unknown-task".to_owned());

        let loop_id = self
            .extract_loop_id_from_thread(thread)
            .or_else(|| self.selected_view().map(|view| view.id.clone()))
            .unwrap_or_else(|| "unknown-loop".to_owned());

        let loop_state = self
            .loop_state_for_handoff(&loop_id)
            .unwrap_or_else(|| "unknown".to_owned());
        let from = latest_message.from.trim();
        let from = if from.is_empty() { "unknown" } else { from };
        let status = format!(
            "loop={loop_state} unread={} pending-ack={}",
            thread.unread_count, thread.pending_ack_count
        );
        let context = format!(
            "thread={} latest={} from={} messages={} participants={}",
            thread.thread_key,
            format_mail_id(latest_message.id),
            from,
            thread.message_indices.len(),
            thread.participant_count
        );
        let links = format!(
            "task:sv task show {} | loop:forge logs {} | mail:fmail log task -n 200 | rg {}",
            task_id, loop_id, thread.thread_key
        );

        let mut risks = Vec::new();
        if thread.unread_count > 0 {
            risks.push(format!("unread messages={}", thread.unread_count));
        }
        if thread.pending_ack_count > 0 {
            risks.push(format!("ack pending={}", thread.pending_ack_count));
        }
        if loop_id == "unknown-loop" {
            risks.push("loop mapping missing".to_owned());
        }
        if loop_state.eq_ignore_ascii_case("error") {
            risks.push("loop state=error".to_owned());
        }
        if let Some(conflict) = claim_conflicts
            .iter()
            .find(|conflict| conflict.task_id == task_id)
        {
            risks.push(format!(
                "ownership conflict: {} vs {}",
                conflict.latest_by, conflict.previous_by
            ));
        }
        let pending_risks = if risks.is_empty() {
            "none".to_owned()
        } else {
            risks.join("; ")
        };

        self.handoff_snapshot = Some(HandoffSnapshotView {
            thread_key: thread.thread_key.clone(),
            task_id: task_id.clone(),
            loop_id: loop_id.clone(),
            status,
            context,
            links,
            pending_risks,
        });
        self.set_status(
            StatusKind::Ok,
            &format!("Handoff snapshot ready: task {task_id}, loop {loop_id}"),
        );
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
        } else if tab == MainTab::Inbox {
            self.clamp_inbox_selection();
        } else if self.focus_mode == FocusMode::DeepDebug {
            self.focus_right = true;
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

    pub fn cycle_density_mode(&mut self, delta: i32) {
        let options = &DensityMode::ORDER;
        let mut idx = 0i32;
        for (i, mode) in options.iter().enumerate() {
            if *mode == self.density_mode {
                idx = i as i32;
                break;
            }
        }
        idx += delta;
        while idx < 0 {
            idx += options.len() as i32;
        }
        self.density_mode = options[(idx as usize) % options.len()];
        if self.tab == MainTab::MultiLogs {
            self.clamp_multi_page();
        }
        self.set_status(
            StatusKind::Info,
            &format!("Density: {}", self.density_mode.label()),
        );
    }

    #[allow(dead_code)]
    fn toggle_zen_mode(&mut self) {
        self.focus_right = !self.focus_right;
        if self.tab == MainTab::MultiLogs {
            self.clamp_multi_page();
        }
        if self.focus_right {
            self.set_status(StatusKind::Info, "Zen mode: right pane focus");
        } else {
            self.set_status(StatusKind::Info, "Zen mode: split view");
        }
    }

    #[allow(dead_code)]
    fn toggle_deep_focus_mode(&mut self) {
        self.focus_mode = if self.focus_mode == FocusMode::Standard {
            FocusMode::DeepDebug
        } else {
            FocusMode::Standard
        };
        if self.focus_mode == FocusMode::DeepDebug {
            self.focus_right = true;
        } else if self.tab != MainTab::MultiLogs {
            self.focus_right = false;
        }
        if self.tab == MainTab::MultiLogs {
            self.clamp_multi_page();
        }
        self.set_status(
            StatusKind::Info,
            &format!("Focus mode: {}", self.focus_mode.label()),
        );
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
        let grid_height = (height - self.multi_header_rows()).max(self.multi_min_cell_height());
        fit_pane_layout(
            self.current_layout(),
            width,
            grid_height,
            self.multi_cell_gap(),
            self.multi_min_cell_width(),
            self.multi_min_cell_height(),
        )
    }

    #[must_use]
    pub(crate) fn multi_header_rows(&self) -> i32 {
        if self.focus_mode == FocusMode::DeepDebug || self.density_mode == DensityMode::Compact {
            1
        } else {
            MULTI_HEADER_ROWS
        }
    }

    #[must_use]
    pub(crate) fn multi_cell_gap(&self) -> i32 {
        if self.density_mode == DensityMode::Compact {
            0
        } else {
            MULTI_CELL_GAP
        }
    }

    #[must_use]
    pub(crate) fn multi_min_cell_width(&self) -> i32 {
        if self.density_mode == DensityMode::Compact {
            32
        } else {
            MULTI_MIN_CELL_WIDTH
        }
    }

    #[must_use]
    pub(crate) fn multi_min_cell_height(&self) -> i32 {
        if self.density_mode == DensityMode::Compact {
            6
        } else {
            MULTI_MIN_CELL_HEIGHT
        }
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
        let base_overhead = match self.focus_mode {
            FocusMode::Standard => 4,
            FocusMode::DeepDebug => 2,
        };
        let mode_overhead = match self.mode {
            UiMode::Palette | UiMode::Filter | UiMode::Confirm | UiMode::Wizard | UiMode::Help => 3,
            _ => 0,
        };
        let density_adjust = if self.density_mode == DensityMode::Compact {
            1
        } else {
            0
        };
        let overhead: i32 = (base_overhead + mode_overhead - density_adjust).max(1)
            + if self.status_text.is_empty() { 0 } else { 1 };
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

    pub fn set_wizard_defaults(&mut self, interval: &str, prompt: &str, prompt_msg: &str) {
        self.default_interval = interval.to_owned();
        self.default_prompt = prompt.to_owned();
        self.default_prompt_msg = prompt_msg.to_owned();
    }

    pub fn set_wizard(&mut self, wizard: WizardState) {
        self.wizard = wizard;
    }

    pub fn wizard_next_field(&mut self) {
        let field_count = wizard_field_count(self.wizard.step);
        if field_count == 0 {
            return;
        }
        self.wizard.field = (self.wizard.field + 1) % field_count;
    }

    pub fn wizard_prev_field(&mut self) {
        let field_count = wizard_field_count(self.wizard.step);
        if field_count == 0 {
            return;
        }
        if self.wizard.field == 0 {
            self.wizard.field = field_count - 1;
        } else {
            self.wizard.field -= 1;
        }
    }

    fn wizard_pairs(&self) -> Vec<(String, String)> {
        vec![
            ("name".to_owned(), self.wizard.values.name.clone()),
            (
                "name_prefix".to_owned(),
                self.wizard.values.name_prefix.clone(),
            ),
            ("count".to_owned(), self.wizard.values.count.clone()),
            ("pool".to_owned(), self.wizard.values.pool.clone()),
            ("profile".to_owned(), self.wizard.values.profile.clone()),
            ("prompt".to_owned(), self.wizard.values.prompt.clone()),
            (
                "prompt_msg".to_owned(),
                self.wizard.values.prompt_msg.clone(),
            ),
            ("interval".to_owned(), self.wizard.values.interval.clone()),
            (
                "max_runtime".to_owned(),
                self.wizard.values.max_runtime.clone(),
            ),
            (
                "max_iterations".to_owned(),
                self.wizard.values.max_iterations.clone(),
            ),
            ("tags".to_owned(), self.wizard.values.tags.clone()),
        ]
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
            if matches!(self.resolve_key_command(key_event), Some(KeyCommand::Quit)) {
                self.quitting = true;
                return Command::Quit;
            }

            match self.mode {
                UiMode::Palette => self.update_palette_mode(key_event),
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
        if matches!(self.resolve_key_command(key), Some(KeyCommand::OpenPalette)) {
            self.command_palette
                .open(self.palette_context(), DEFAULT_SEARCH_BUDGET);
            self.mode = UiMode::Palette;
            return Command::None;
        }

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
            Key::Char('5') => {
                self.set_tab(MainTab::Inbox);
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
                self.toggle_zen_mode();
                Command::Fetch
            }
            Key::Char('Z') => {
                self.toggle_deep_focus_mode();
                Command::Fetch
            }
            Key::Char('M') => {
                self.cycle_density_mode(1);
                Command::Fetch
            }
            Key::Char('/') => {
                self.mode = UiMode::Filter;
                self.filter_focus = FilterFocus::Text;
                Command::None
            }
            Key::Char('j') | Key::Down => {
                if self.tab == MainTab::Inbox {
                    self.move_inbox_selection(1);
                } else {
                    self.move_selection(1);
                }
                Command::Fetch
            }
            Key::Char('k') | Key::Up => {
                if self.tab == MainTab::Inbox {
                    self.move_inbox_selection(-1);
                } else {
                    self.move_selection(-1);
                }
                Command::Fetch
            }
            Key::Enter => {
                if self.tab == MainTab::Inbox {
                    self.mark_selected_inbox_thread_read();
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('f') => {
                if self.tab == MainTab::Inbox {
                    self.cycle_inbox_filter(1);
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('a') => {
                if self.tab == MainTab::Inbox {
                    self.acknowledge_selected_inbox_message();
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('h') => {
                if self.tab == MainTab::Inbox {
                    self.generate_handoff_snapshot();
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('R') => {
                if self.tab == MainTab::Inbox {
                    self.quick_reply_selected_inbox_thread();
                    Command::None
                } else {
                    Command::None
                }
            }
            Key::Char('o') => {
                if self.tab == MainTab::Inbox {
                    self.cycle_claim_conflict(1);
                    Command::None
                } else {
                    Command::None
                }
            }
            Key::Char('O') => {
                if self.tab == MainTab::Inbox {
                    self.show_claim_resolution_hint();
                    Command::None
                } else {
                    Command::None
                }
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
            Key::Char('g') => {
                if self.tab == MainTab::MultiLogs {
                    self.move_multi_page_to_start();
                    Command::Fetch
                } else {
                    Command::None
                }
            }
            Key::Char('G') => {
                if self.tab == MainTab::MultiLogs {
                    self.move_multi_page_to_end();
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
                self.wizard = WizardState::with_defaults(
                    &self.default_interval,
                    &self.default_prompt,
                    &self.default_prompt_msg,
                );
                Command::None
            }
            Key::Char('r') => {
                if self.tab == MainTab::Inbox {
                    self.quick_reply_selected_inbox_thread();
                    return Command::None;
                }
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

    fn update_palette_mode(&mut self, key: KeyEvent) -> Command {
        match self.resolve_key_command(key) {
            Some(KeyCommand::PaletteClose) => {
                self.mode = UiMode::Main;
                Command::None
            }
            Some(KeyCommand::ToggleHelp) => {
                self.help_return = UiMode::Palette;
                self.mode = UiMode::Help;
                Command::None
            }
            Some(KeyCommand::PaletteMoveNext) => {
                self.command_palette.move_selection(1);
                Command::None
            }
            Some(KeyCommand::PaletteMovePrev) => {
                self.command_palette.move_selection(-1);
                Command::None
            }
            Some(KeyCommand::PaletteQueryBackspace) => {
                self.command_palette
                    .pop_char(self.palette_context(), DEFAULT_SEARCH_BUDGET);
                Command::None
            }
            Some(KeyCommand::PaletteExecute) => {
                let context = self.palette_context();
                let Some(action) = self.command_palette.accept(context, DEFAULT_SEARCH_BUDGET)
                else {
                    return Command::None;
                };
                self.execute_palette_action(action)
            }
            _ => match key.key {
                Key::Char(ch) if !key.modifiers.ctrl && !key.modifiers.alt => {
                    self.command_palette.push_char(
                        ch,
                        self.palette_context(),
                        DEFAULT_SEARCH_BUDGET,
                    );
                    Command::None
                }
                _ => Command::None,
            },
        }
    }

    fn execute_palette_action(&mut self, action: PaletteActionId) -> Command {
        self.mode = UiMode::Main;
        match action {
            PaletteActionId::SwitchOverview => {
                self.set_tab(MainTab::Overview);
                Command::Fetch
            }
            PaletteActionId::SwitchLogs => {
                self.set_tab(MainTab::Logs);
                Command::Fetch
            }
            PaletteActionId::SwitchRuns => {
                self.set_tab(MainTab::Runs);
                Command::Fetch
            }
            PaletteActionId::SwitchMultiLogs => {
                self.set_tab(MainTab::MultiLogs);
                Command::Fetch
            }
            PaletteActionId::SwitchInbox => {
                self.set_tab(MainTab::Inbox);
                Command::Fetch
            }
            PaletteActionId::OpenFilter => {
                self.mode = UiMode::Filter;
                self.filter_focus = FilterFocus::Text;
                Command::None
            }
            PaletteActionId::NewLoopWizard => {
                self.mode = UiMode::Wizard;
                self.wizard = WizardState::with_defaults(
                    &self.default_interval,
                    &self.default_prompt,
                    &self.default_prompt_msg,
                );
                Command::None
            }
            PaletteActionId::ResumeSelectedLoop => {
                let loop_id = match self.selected_view() {
                    Some(v) => v.id.clone(),
                    None => {
                        self.set_status(StatusKind::Info, "No loop selected");
                        return Command::None;
                    }
                };
                self.run_action(ActionType::Resume, &loop_id)
            }
            PaletteActionId::StopSelectedLoop => self.enter_confirm(ActionType::Stop),
            PaletteActionId::KillSelectedLoop => self.enter_confirm(ActionType::Kill),
            PaletteActionId::DeleteSelectedLoop => self.enter_confirm(ActionType::Delete),
            PaletteActionId::CycleTheme => {
                self.cycle_theme();
                Command::None
            }
            PaletteActionId::ToggleZenMode => {
                self.toggle_zen_mode();
                Command::Fetch
            }
            PaletteActionId::CycleDensityMode => {
                self.cycle_density_mode(1);
                Command::Fetch
            }
            PaletteActionId::ToggleFocusMode => {
                self.toggle_deep_focus_mode();
                Command::Fetch
            }
            PaletteActionId::Custom(_) => Command::None,
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
                self.toggle_zen_mode();
                Command::Fetch
            }
            Key::Char('Z') => {
                self.toggle_deep_focus_mode();
                Command::Fetch
            }
            Key::Char('M') => {
                self.cycle_density_mode(1);
                Command::Fetch
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
            Key::Tab if key.modifiers.shift => {
                self.wizard_prev_field();
                Command::None
            }
            Key::Tab | Key::Down | Key::Char('j') => {
                self.wizard_next_field();
                Command::None
            }
            Key::Up | Key::Char('k') => {
                self.wizard_prev_field();
                Command::None
            }
            Key::Enter => {
                if self.wizard.step < 4 {
                    if let Err(err) = validate_wizard_step(self.wizard.step, &self.wizard.values) {
                        self.wizard.error = err;
                        return Command::None;
                    }
                    self.wizard.step += 1;
                    self.wizard.field = 0;
                    self.wizard.error.clear();
                    return Command::None;
                }
                self.run_action(ActionType::Create, "")
            }
            Key::Char('b') | Key::Left => {
                if self.wizard.step > 1 {
                    self.wizard.step -= 1;
                    self.wizard.field = 0;
                    self.wizard.error.clear();
                }
                Command::None
            }
            Key::Backspace => {
                if self.wizard.step > 3 {
                    return Command::None;
                }
                let Some(field_key) = wizard_field_key(self.wizard.step, self.wizard.field) else {
                    return Command::None;
                };
                let mut value = wizard_get(&self.wizard.values, field_key).to_owned();
                value.pop();
                wizard_set(&mut self.wizard.values, field_key, value);
                Command::None
            }
            Key::Char('h') if key.modifiers.ctrl => {
                if self.wizard.step > 3 {
                    return Command::None;
                }
                let Some(field_key) = wizard_field_key(self.wizard.step, self.wizard.field) else {
                    return Command::None;
                };
                let mut value = wizard_get(&self.wizard.values, field_key).to_owned();
                value.pop();
                wizard_set(&mut self.wizard.values, field_key, value);
                Command::None
            }
            Key::Char(' ') => {
                if self.wizard.step > 3 {
                    return Command::None;
                }
                let Some(field_key) = wizard_field_key(self.wizard.step, self.wizard.field) else {
                    return Command::None;
                };
                let mut value = wizard_get(&self.wizard.values, field_key).to_owned();
                value.push(' ');
                wizard_set(&mut self.wizard.values, field_key, value);
                Command::None
            }
            Key::Char(ch) => {
                if self.wizard.step > 3 {
                    return Command::None;
                }
                let Some(field_key) = wizard_field_key(self.wizard.step, self.wizard.field) else {
                    return Command::None;
                };
                let mut value = wizard_get(&self.wizard.values, field_key).to_owned();
                value.push(ch);
                wizard_set(&mut self.wizard.values, field_key, value);
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
            ActionType::Create => Command::RunAction(ActionKind::Create {
                wizard: self.wizard_pairs(),
            }),
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

    /// Handle the result of an asynchronous action.
    ///
    /// Matches Go `actionResultMsg` handling: clears busy flag, shows
    /// success/error status, resets wizard on create success, and
    /// triggers a data refresh.
    pub fn handle_action_result(&mut self, result: ActionResult) -> Command {
        self.action_busy = false;

        if let Some(ref err) = result.error {
            self.set_status(StatusKind::Err, err);
            if result.kind == ActionType::Create {
                self.mode = UiMode::Wizard;
                self.wizard.error = err.clone();
            }
            return Command::None;
        }

        if result.kind == ActionType::Create {
            self.mode = UiMode::Main;
            self.wizard.error.clear();
            if !result.selected_loop_id.is_empty() {
                self.selected_id = result.selected_loop_id;
            }
        }

        if !result.message.is_empty() {
            self.set_status(StatusKind::Ok, &result.message);
        }

        Command::Fetch
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

        // Content area.
        let content_start = if self.focus_mode == FocusMode::DeepDebug {
            1
        } else {
            let tab_bar = self.render_tab_bar(width);
            frame.draw_text(0, 1, &tab_bar, TextRole::Primary);
            2
        };
        let footer_lines = if self.status_text.is_empty() { 1 } else { 2 };
        let content_height = height.saturating_sub(content_start + footer_lines).max(1);

        match self.mode {
            UiMode::Help => {
                self.render_help_content(&mut frame, width, content_height, content_start);
            }
            UiMode::Palette => {
                let lines = self.command_palette.render_lines(width, content_height);
                for (idx, line) in lines.iter().enumerate() {
                    if idx >= content_height {
                        break;
                    }
                    let role = if idx == 0 {
                        TextRole::Accent
                    } else if idx == 1 {
                        TextRole::Muted
                    } else if line.starts_with(">") {
                        TextRole::Primary
                    } else {
                        TextRole::Muted
                    };
                    frame.draw_text(0, content_start + idx, line, role);
                }
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
                for (idx, line) in self.render_wizard_lines(width).iter().enumerate() {
                    if idx >= content_height {
                        break;
                    }
                    let role = if idx == 0 {
                        TextRole::Accent
                    } else if line.starts_with("Error:") {
                        TextRole::Danger
                    } else {
                        TextRole::Primary
                    };
                    frame.draw_text(0, content_start + idx, line, role);
                }
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
                } else if self.mode == UiMode::Main && self.tab == MainTab::Overview {
                    let lines = crate::overview_tab::overview_pane_lines(
                        self.selected_view(),
                        &self.run_history,
                        self.selected_run,
                        width,
                        content_height,
                    );
                    for (idx, line) in lines.iter().enumerate() {
                        if idx >= content_height {
                            break;
                        }
                        frame.draw_text(0, content_start + idx, &line.text, line.role);
                    }
                } else if self.mode == UiMode::Main && self.tab == MainTab::MultiLogs {
                    let multi_frame = self.render_multi_logs_pane(width, content_height);
                    blit_frame(&mut frame, &multi_frame, 0, content_start);
                } else if self.mode == UiMode::Main && self.tab == MainTab::Inbox {
                    let inbox_frame = self.render_inbox_pane(width, content_height);
                    blit_frame(&mut frame, &inbox_frame, 0, content_start);
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
        let hint = if self.focus_mode == FocusMode::DeepDebug {
            "deep focus  Z toggle  M density  z zen  q quit  ? help"
        } else if self.density_mode == DensityMode::Compact {
            "? q ctrl+p / 1-5 j/k z Z M"
        } else {
            "? help  q quit  ctrl+p palette  / filter  1-5 tabs  j/k sel  S stop  K kill  M density  Z focus"
        };
        let truncated = if hint.len() > width {
            &hint[..width]
        } else {
            hint
        };
        frame.draw_text(0, footer_y, truncated, TextRole::Muted);

        frame
    }

    fn render_header_text(&self, width: usize) -> String {
        let count_label = if self.tab == MainTab::Inbox {
            let threads = self.inbox_threads();
            let unread = threads
                .iter()
                .map(|thread| thread.unread_count)
                .sum::<usize>();
            format!("{} threads, {} unread", threads.len(), unread)
        } else {
            format!("{}/{} loops", self.filtered.len(), self.loops.len())
        };
        let mode_label = match self.mode {
            UiMode::Wizard => "  mode:New Loop Wizard",
            UiMode::Palette => "  mode:Command Palette",
            UiMode::Filter => "  mode:Filter",
            UiMode::Help => "  mode:Help",
            UiMode::Confirm => "  mode:Confirm",
            UiMode::ExpandedLogs => "  mode:Expanded Logs",
            UiMode::Main => "",
        };
        let header = format!(
            " Forge Loops  [{tab}]  {count}  theme:{theme}  density:{density}  focus:{focus}{mode}",
            tab = self.tab.label(),
            count = count_label,
            theme = self.palette.name,
            density = self.density_mode.label(),
            focus = self.focus_mode.label(),
            mode = mode_label,
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
                let label = if self.density_mode == DensityMode::Compact {
                    t.short_label()
                } else {
                    t.label()
                };
                if *t == self.tab {
                    format!("[{}:{}]", i + 1, label)
                } else {
                    format!(" {}:{} ", i + 1, label)
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

    fn render_wizard_lines(&self, width: usize) -> Vec<String> {
        let step = self.wizard.step.clamp(1, 4);
        let mut lines = vec![
            format!("New loop wizard (step {step}/4)"),
            "1) Identity+Count  2) Pool/Profile  3) Prompt+Runtime  4) Review+Submit".to_owned(),
            String::new(),
        ];

        match step {
            1 => {
                lines.push(self.render_wizard_field("name", &self.wizard.values.name, 0));
                lines.push(self.render_wizard_field(
                    "name-prefix",
                    &self.wizard.values.name_prefix,
                    1,
                ));
                lines.push(self.render_wizard_field("count", &self.wizard.values.count, 2));
            }
            2 => {
                lines.push(self.render_wizard_field("pool", &self.wizard.values.pool, 0));
                lines.push(self.render_wizard_field("profile", &self.wizard.values.profile, 1));
            }
            3 => {
                lines.push(self.render_wizard_field("prompt", &self.wizard.values.prompt, 0));
                lines.push(self.render_wizard_field(
                    "prompt-msg",
                    &self.wizard.values.prompt_msg,
                    1,
                ));
                lines.push(self.render_wizard_field("interval", &self.wizard.values.interval, 2));
                lines.push(self.render_wizard_field(
                    "max-runtime",
                    &self.wizard.values.max_runtime,
                    3,
                ));
                lines.push(self.render_wizard_field(
                    "max-iterations",
                    &self.wizard.values.max_iterations,
                    4,
                ));
                lines.push(self.render_wizard_field("tags", &self.wizard.values.tags, 5));
            }
            4 => {
                lines.push("Review:".to_owned());
                lines.push(format!("  name={:?}", self.wizard.values.name));
                lines.push(format!(
                    "  name-prefix={:?}",
                    self.wizard.values.name_prefix
                ));
                lines.push(format!("  count={:?}", self.wizard.values.count));
                lines.push(format!("  pool={:?}", self.wizard.values.pool));
                lines.push(format!("  profile={:?}", self.wizard.values.profile));
                lines.push(format!("  prompt={:?}", self.wizard.values.prompt));
                lines.push(format!("  prompt-msg={:?}", self.wizard.values.prompt_msg));
                lines.push(format!("  interval={:?}", self.wizard.values.interval));
                lines.push(format!(
                    "  max-runtime={:?}",
                    self.wizard.values.max_runtime
                ));
                lines.push(format!(
                    "  max-iterations={:?}",
                    self.wizard.values.max_iterations
                ));
                lines.push(format!("  tags={:?}", self.wizard.values.tags));
            }
            _ => {}
        }

        lines.push(String::new());
        lines.push("tab/down/up navigate fields, enter next/submit, b back, esc cancel".to_owned());
        if !self.wizard.error.is_empty() {
            lines.push(format!("Error: {}", self.wizard.error));
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

    fn render_wizard_field(&self, label: &str, value: &str, field: usize) -> String {
        let display = if value.trim().is_empty() {
            "<empty>"
        } else {
            value
        };
        if self.wizard.field == field {
            format!("{label}: {display}_")
        } else {
            format!("{label}: {display}")
        }
    }

    fn render_inbox_pane(&self, width: usize, height: usize) -> RenderFrame {
        let theme = crate::default_theme();
        let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
        if width == 0 || height == 0 {
            return frame;
        }

        let threads = self.inbox_threads();
        let claim_conflicts = self.claim_conflicts();
        let unread_total = threads
            .iter()
            .map(|thread| thread.unread_count)
            .sum::<usize>();
        let pending_ack_total = threads
            .iter()
            .map(|thread| thread.pending_ack_count)
            .sum::<usize>();
        let header = format!(
            "Inbox filter:{}  threads:{}  unread:{}  pending-ack:{}  claims:{}  conflicts:{}",
            self.inbox_filter.label(),
            threads.len(),
            unread_total,
            pending_ack_total,
            self.claim_events.len(),
            claim_conflicts.len()
        );
        frame.draw_text(0, 0, &trim_to_width(&header, width), TextRole::Accent);

        if height <= 1 {
            return frame;
        }

        let timeline_reserved = if self.claim_events.is_empty() {
            0usize
        } else {
            4usize.min(height.saturating_sub(1))
        };
        let timeline_start = height.saturating_sub(timeline_reserved);

        if threads.is_empty() {
            frame.draw_text(0, 1, "No messages for selected filter", TextRole::Muted);
            frame.draw_text(
                0,
                2.min(timeline_start.saturating_sub(1)),
                "keys: f filter  j/k select  enter read  a ack  h handoff  r reply",
                TextRole::Muted,
            );
            if timeline_reserved == 0 {
                return frame;
            }
        } else {
            let min_detail_width = 30usize;
            let list_width = if width > min_detail_width + 22 {
                (width * 2 / 5).clamp(22, width - min_detail_width - 1)
            } else {
                width
            };
            let detail_x = if list_width + 2 < width {
                list_width + 2
            } else {
                width
            };

            if detail_x < width {
                for y in 1..timeline_start {
                    frame.draw_text(list_width, y, "|", TextRole::Muted);
                }
            }

            let list_height = timeline_start.saturating_sub(2);
            for row in 0..list_height {
                let Some(thread) = threads.get(row) else {
                    break;
                };
                let selected = row == self.inbox_selected_thread;
                let prefix = if selected { ">" } else { " " };
                let line = format!(
                    "{prefix} {} u:{} a:{} {}",
                    format_mail_id(thread.latest_message_id),
                    thread.unread_count,
                    thread.pending_ack_count,
                    thread.subject
                );
                let role = if selected {
                    TextRole::Primary
                } else {
                    TextRole::Muted
                };
                frame.draw_text(0, row + 1, &trim_to_width(&line, list_width), role);
            }

            if let Some(selected_thread) = threads.get(self.inbox_selected_thread) {
                if detail_x < width {
                    let detail_width = width.saturating_sub(detail_x);
                    let detail_header = format!(
                        "thread:{}  msgs:{}  participants:{}",
                        selected_thread.thread_key,
                        selected_thread.message_indices.len(),
                        selected_thread.participant_count
                    );
                    frame.draw_text(
                        detail_x,
                        1,
                        &trim_to_width(&detail_header, detail_width),
                        TextRole::Primary,
                    );

                    let detail_hint =
                        "enter=read  a=ack  h=handoff  r=reply  o=next-conflict  O=resolution";
                    if timeline_start > 2 {
                        frame.draw_text(
                            detail_x,
                            2,
                            &trim_to_width(detail_hint, detail_width),
                            TextRole::Muted,
                        );
                    }

                    let mut row = 3usize;
                    if let Some(snapshot) = self
                        .handoff_snapshot
                        .as_ref()
                        .filter(|snapshot| snapshot.thread_key == selected_thread.thread_key)
                    {
                        if row < timeline_start {
                            frame.draw_text(
                                detail_x,
                                row,
                                &trim_to_width("handoff snapshot (h regenerate)", detail_width),
                                TextRole::Accent,
                            );
                            row += 1;
                        }
                        for line in snapshot.lines() {
                            if row >= timeline_start {
                                break;
                            }
                            frame.draw_text(
                                detail_x,
                                row,
                                &trim_to_width(&line, detail_width),
                                TextRole::Primary,
                            );
                            row += 1;
                        }
                        if row < timeline_start {
                            frame.draw_text(
                                detail_x,
                                row,
                                "recent thread messages",
                                TextRole::Muted,
                            );
                            row += 1;
                        }
                    }
                    for idx in selected_thread.message_indices.iter().rev() {
                        if row >= timeline_start {
                            break;
                        }
                        let Some(message) = self.inbox_messages.get(*idx) else {
                            continue;
                        };
                        let unread_mark = if message.read_at.is_none() { "*" } else { " " };
                        let ack_mark = if message.ack_required && message.acked_at.is_none() {
                            "!"
                        } else if message.acked_at.is_some() {
                            "a"
                        } else {
                            "-"
                        };
                        let preview = if !message.subject.trim().is_empty() {
                            message.subject.trim()
                        } else if !message.body.trim().is_empty() {
                            message.body.trim()
                        } else {
                            "(empty)"
                        };
                        let line = format!(
                            "{unread_mark}{ack_mark} {} {} {}",
                            format_mail_id(message.id),
                            message.from.trim(),
                            preview
                        );
                        frame.draw_text(
                            detail_x,
                            row,
                            &trim_to_width(&line, detail_width),
                            if message.read_at.is_none() {
                                TextRole::Primary
                            } else {
                                TextRole::Muted
                            },
                        );
                        row += 1;
                    }
                }
            }
        }
        if timeline_reserved > 0 {
            frame.draw_text(
                0,
                timeline_start,
                "claim timeline (latest)",
                TextRole::Accent,
            );
            let conflict_task_ids: HashSet<&str> = claim_conflicts
                .iter()
                .map(|conflict| conflict.task_id.as_str())
                .collect();
            let highlight_task = claim_conflicts
                .get(self.selected_claim_conflict)
                .map(|conflict| conflict.task_id.as_str());
            for row in 1..timeline_reserved {
                let Some(event) = self.claim_events.get(row - 1) else {
                    break;
                };
                let flag = if conflict_task_ids.contains(event.task_id.as_str()) {
                    "!"
                } else {
                    " "
                };
                let line = format!(
                    "{flag} {} {} <- {}",
                    event.claimed_at, event.task_id, event.claimed_by
                );
                let role = if Some(event.task_id.as_str()) == highlight_task {
                    TextRole::Danger
                } else if flag == "!" {
                    TextRole::Primary
                } else {
                    TextRole::Muted
                };
                frame.draw_text(0, timeline_start + row, &trim_to_width(&line, width), role);
            }
        }

        frame
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
        let mut lines: Vec<String> = vec![
            "=== Forge Loop TUI Help ===".to_owned(),
            "".to_owned(),
            "Navigation:".to_owned(),
            "  1/2/3/4/5 switch tabs (Overview/Logs/Runs/MultiLogs/Inbox)".to_owned(),
            "  ]/[       cycle tabs".to_owned(),
            "  j/k       move loop selection".to_owned(),
            "  ,/.       move run selection / multi page".to_owned(),
            "".to_owned(),
            "Actions:".to_owned(),
            "  S         stop selected loop".to_owned(),
            "  K         kill selected loop".to_owned(),
            "  D         delete selected loop".to_owned(),
            "  r         resume selected loop".to_owned(),
            "  n         new loop wizard".to_owned(),
            "".to_owned(),
            "Command Palette:".to_owned(),
            "  Ctrl+P    open command palette".to_owned(),
            "  type      fuzzy search action registry".to_owned(),
            "  tab/j/k   move result selection".to_owned(),
            "  enter     run selected action".to_owned(),
            "".to_owned(),
            "Logs:".to_owned(),
            "  v         cycle log source".to_owned(),
            "  x         cycle log layer".to_owned(),
            "  u/d       scroll logs".to_owned(),
            "  l         expand logs fullscreen".to_owned(),
            "".to_owned(),
            "Multi Logs:".to_owned(),
            "  m         cycle layout".to_owned(),
            "  space     toggle pin".to_owned(),
            "  c         clear pinned".to_owned(),
            "  ,/.       page left/right".to_owned(),
            "  g/G       first/last page".to_owned(),
            "".to_owned(),
            "Inbox:".to_owned(),
            "  f         cycle inbox filter (all/unread/ack-required)".to_owned(),
            "  enter     mark selected thread read".to_owned(),
            "  a         ack latest pending message in thread".to_owned(),
            "  h         generate handoff snapshot package".to_owned(),
            "  r         quick reply shortcut (thread + reply-to id)".to_owned(),
            "  o         next claim conflict".to_owned(),
            "  O         show conflict resolution hint".to_owned(),
            "".to_owned(),
            "Global:".to_owned(),
            "  ?         toggle help".to_owned(),
            "  q         quit".to_owned(),
            "  t         cycle theme".to_owned(),
            "  z         zen mode (focus right pane)".to_owned(),
            "  Z         deep focus mode (distraction-minimized)".to_owned(),
            "  M         cycle density (comfortable/compact)".to_owned(),
            "  /         filter mode".to_owned(),
            "".to_owned(),
        ];
        lines.extend(
            self.keymap
                .conflict_diagnostics_lines(width, height.saturating_sub(lines.len())),
        );
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

fn inbox_thread_key(message: &InboxMessageView) -> String {
    if let Some(thread_id) = &message.thread_id {
        let trimmed = thread_id.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    format_mail_id(message.id)
}

fn extract_prefixed_token(text: &str, prefix: &str) -> Option<String> {
    let normalized_prefix = prefix.to_ascii_lowercase();
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
        .find_map(|token| {
            if token.is_empty() {
                return None;
            }
            let normalized = token.to_ascii_lowercase();
            if normalized.starts_with(&normalized_prefix) {
                Some(normalized)
            } else {
                None
            }
        })
}

fn format_mail_id(id: i64) -> String {
    format!("m-{id}")
}

fn trim_to_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.len() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
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

    fn sample_inbox_messages() -> Vec<InboxMessageView> {
        vec![
            InboxMessageView {
                id: 1,
                thread_id: Some("thread-a".to_owned()),
                from: "agent-a".to_owned(),
                subject: "handoff ready".to_owned(),
                body: "please review".to_owned(),
                created_at: "2026-02-12T08:10:00Z".to_owned(),
                ack_required: true,
                read_at: None,
                acked_at: None,
            },
            InboxMessageView {
                id: 2,
                thread_id: Some("thread-a".to_owned()),
                from: "agent-b".to_owned(),
                subject: "re: handoff ready".to_owned(),
                body: "reviewing now".to_owned(),
                created_at: "2026-02-12T08:11:00Z".to_owned(),
                ack_required: false,
                read_at: None,
                acked_at: None,
            },
            InboxMessageView {
                id: 3,
                thread_id: Some("thread-b".to_owned()),
                from: "agent-c".to_owned(),
                subject: "incident escalated".to_owned(),
                body: "needs ack".to_owned(),
                created_at: "2026-02-12T08:12:00Z".to_owned(),
                ack_required: true,
                read_at: Some("2026-02-12T08:12:30Z".to_owned()),
                acked_at: None,
            },
        ]
    }

    fn sample_handoff_messages() -> Vec<InboxMessageView> {
        vec![InboxMessageView {
            id: 11,
            thread_id: Some("thread-handoff".to_owned()),
            from: "agent-h".to_owned(),
            subject: "handoff forge-jws loop-2".to_owned(),
            body: "status update and pending risk summary".to_owned(),
            created_at: "2026-02-12T08:13:00Z".to_owned(),
            ack_required: true,
            read_at: None,
            acked_at: None,
        }]
    }

    fn sample_claim_events() -> Vec<ClaimEventView> {
        vec![
            ClaimEventView {
                task_id: "forge-jws".to_owned(),
                claimed_by: "agent-a".to_owned(),
                claimed_at: "2026-02-12T08:10:00Z".to_owned(),
            },
            ClaimEventView {
                task_id: "forge-jws".to_owned(),
                claimed_by: "agent-b".to_owned(),
                claimed_at: "2026-02-12T08:12:00Z".to_owned(),
            },
            ClaimEventView {
                task_id: "forge-73b".to_owned(),
                claimed_by: "agent-c".to_owned(),
                claimed_at: "2026-02-12T08:11:00Z".to_owned(),
            },
        ]
    }

    // -- MainTab labels --

    #[test]
    fn tab_label_snapshot() {
        let labels: Vec<&str> = MainTab::ORDER.iter().map(|t| t.label()).collect();
        assert_eq!(labels.join("|"), "Overview|Logs|Runs|Multi Logs|Inbox");
    }

    #[test]
    fn tab_short_label_snapshot() {
        let labels: Vec<&str> = MainTab::ORDER.iter().map(|t| t.short_label()).collect();
        assert_eq!(labels.join("|"), "ov|logs|runs|multi|inbox");
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
        app.update(key(Key::Char('5')));
        assert_eq!(app.tab(), MainTab::Inbox);
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

    #[test]
    fn inbox_filter_cycles_and_clamps_selection() {
        let mut app = App::new("default", 12);
        app.set_inbox_messages(sample_inbox_messages());
        app.update(key(Key::Char('5')));
        assert_eq!(app.tab(), MainTab::Inbox);
        assert_eq!(app.inbox_filter(), InboxFilter::All);

        app.update(key(Key::Char('f')));
        assert_eq!(app.inbox_filter(), InboxFilter::Unread);
        app.update(key(Key::Char('f')));
        assert_eq!(app.inbox_filter(), InboxFilter::AckRequired);
        app.update(key(Key::Char('f')));
        assert_eq!(app.inbox_filter(), InboxFilter::All);
    }

    #[test]
    fn inbox_enter_marks_selected_thread_read() {
        let mut app = App::new("default", 12);
        app.set_inbox_messages(sample_inbox_messages());
        app.update(key(Key::Char('5')));
        app.update(key(Key::Char('j')));
        app.update(key(Key::Enter));
        let unread = app
            .inbox_messages()
            .iter()
            .filter(|message| message.read_at.is_none())
            .count();
        assert_eq!(unread, 0);
    }

    #[test]
    fn inbox_acknowledges_latest_pending_message() {
        let mut app = App::new("default", 12);
        app.set_inbox_messages(sample_inbox_messages());
        app.update(key(Key::Char('5')));
        app.update(key(Key::Char('a')));
        let acked = app
            .inbox_messages()
            .iter()
            .find(|message| message.id == 3)
            .and_then(|message| message.acked_at.clone());
        assert_eq!(acked.as_deref(), Some("now"));
    }

    #[test]
    fn inbox_render_uses_cli_mail_ids_and_threads() {
        let mut app = App::new("default", 12);
        app.set_inbox_messages(sample_inbox_messages());
        app.set_claim_events(sample_claim_events());
        app.update(key(Key::Char('5')));
        let frame = app.render();
        let snapshot = frame.snapshot();
        assert!(snapshot.contains("Inbox filter:all"));
        assert!(snapshot.contains("m-3"));
        assert!(snapshot.contains("thread:thread-b"));
        assert!(snapshot.contains("claim timeline (latest)"));
        assert!(snapshot.contains("! 2026-02-12T08:12:00Z forge-jws <- agent-b"));
    }

    #[test]
    fn inbox_claim_conflict_shortcuts_show_status() {
        let mut app = App::new("default", 12);
        app.set_inbox_messages(sample_inbox_messages());
        app.set_claim_events(sample_claim_events());
        app.update(key(Key::Char('5')));
        app.update(key(Key::Char('o')));
        assert!(app.status_text().contains("Claim conflict forge-jws"));
        app.update(key(Key::Char('O')));
        assert!(app
            .status_text()
            .contains("takeover claim: forge-jws by <agent>"));
    }

    #[test]
    fn inbox_handoff_snapshot_generates_compact_package() {
        let mut app = app_with_loops(3);
        app.set_inbox_messages(sample_handoff_messages());
        app.set_claim_events(sample_claim_events());
        app.update(key(Key::Char('5')));
        app.update(key(Key::Char('h')));
        assert!(app.status_text().contains("Handoff snapshot ready"));
        let snapshot = app.render().snapshot();
        assert!(snapshot.contains("handoff snapshot"));
        assert!(snapshot.contains("task=forge-jws loop=loop-2"));
        assert!(snapshot.contains("status: loop=running"));
        assert!(snapshot.contains("context: thread=thread-handoff"));
        assert!(snapshot.contains("links: task:sv task show forge-jws"));
        assert!(snapshot.contains("pending-risks:"));
    }

    #[test]
    fn inbox_handoff_snapshot_uses_claim_fallback_when_task_not_in_thread() {
        let mut app = app_with_loops(2);
        app.set_inbox_messages(sample_inbox_messages());
        app.set_claim_events(sample_claim_events());
        app.update(key(Key::Char('5')));
        app.update(key(Key::Char('h')));
        let snapshot = app.render().snapshot();
        assert!(snapshot.contains("task=forge-jws"));
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

    #[test]
    fn help_includes_keymap_diagnostics_panel() {
        let mut app = App::new("default", 12);
        app.height = 80;
        app.update(key(Key::Char('?')));
        let frame = app.render();
        let all_text = (0..app.height())
            .map(|row| frame.row_text(row))
            .collect::<Vec<String>>()
            .join("\n");
        assert!(all_text.contains("Keymap diagnostics"));
        assert!(all_text.contains("no conflicts detected"));
    }

    #[test]
    fn ctrl_p_enters_palette_mode() {
        let mut app = App::new("default", 12);
        let cmd = app.update(ctrl_key('p'));
        assert_eq!(cmd, Command::None);
        assert_eq!(app.mode(), UiMode::Palette);
        assert!(app.palette_match_count() > 0);
        assert!(app.palette_query().is_empty());
    }

    #[test]
    fn palette_typing_and_backspace_updates_query() {
        let mut app = App::new("default", 12);
        app.update(ctrl_key('p'));
        app.update(key(Key::Char('l')));
        app.update(key(Key::Char('o')));
        assert_eq!(app.palette_query(), "lo");
        app.update(key(Key::Backspace));
        assert_eq!(app.palette_query(), "l");
    }

    #[test]
    fn palette_enter_executes_navigation_action() {
        let mut app = App::new("default", 12);
        app.update(ctrl_key('p'));
        for ch in ['l', 'o', 'g', 's'] {
            app.update(key(Key::Char(ch)));
        }
        let cmd = app.update(key(Key::Enter));
        assert_eq!(cmd, Command::Fetch);
        assert_eq!(app.mode(), UiMode::Main);
        assert_eq!(app.tab(), MainTab::Logs);
    }

    #[test]
    fn palette_enter_executes_selected_loop_action() {
        let mut app = app_with_loops(2);
        app.update(ctrl_key('p'));
        for ch in ['s', 't', 'o', 'p'] {
            app.update(key(Key::Char(ch)));
        }
        let cmd = app.update(key(Key::Enter));
        assert_eq!(cmd, Command::None);
        assert_eq!(app.mode(), UiMode::Confirm);
        assert!(app.confirm().is_some());
    }

    #[test]
    fn palette_help_round_trips_back_to_palette() {
        let mut app = App::new("default", 12);
        app.update(ctrl_key('p'));
        assert_eq!(app.mode(), UiMode::Palette);
        app.update(key(Key::Char('?')));
        assert_eq!(app.mode(), UiMode::Help);
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Palette);
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

    #[test]
    fn m_cycles_density_modes() {
        let mut app = App::new("default", 12);
        assert_eq!(app.density_mode(), DensityMode::Comfortable);
        app.update(key(Key::Char('M')));
        assert_eq!(app.density_mode(), DensityMode::Compact);
        assert!(app.status_text().contains("Density: compact"));
        app.update(key(Key::Char('M')));
        assert_eq!(app.density_mode(), DensityMode::Comfortable);
    }

    #[test]
    fn shift_z_toggles_deep_focus_and_collapses_tab_bar() {
        let mut app = app_with_loops(3);
        app.update(InputEvent::Resize(ResizeEvent {
            width: 120,
            height: 36,
        }));
        let baseline = app.render();
        assert!(baseline.row_text(1).contains("[1:Overview]"));

        app.update(key(Key::Char('Z')));
        assert_eq!(app.focus_mode(), FocusMode::DeepDebug);
        assert!(app.focus_right());
        let focused = app.render();
        assert!(!focused.row_text(1).contains("[1:Overview]"));
        assert!(focused.row_text(0).contains("focus:deep"));

        app.update(key(Key::Char('Z')));
        assert_eq!(app.focus_mode(), FocusMode::Standard);
        assert!(!app.focus_right());
    }

    #[test]
    fn compact_density_increases_multi_page_capacity() {
        let mut app = app_with_loops(24);
        app.set_tab(MainTab::MultiLogs);
        app.update(InputEvent::Resize(ResizeEvent {
            width: 120,
            height: 30,
        }));
        let comfortable = app.multi_page_size();
        app.update(key(Key::Char('M')));
        let compact = app.multi_page_size();
        assert!(compact >= comfortable);
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
        assert_eq!(app.wizard().step, 1);
        assert_eq!(app.wizard().field, 0);
        assert_eq!(app.wizard().values.count, "1");
    }

    #[test]
    fn wizard_escape_exits() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Main);
    }

    #[test]
    fn wizard_tab_wraps_fields_by_step() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.wizard().field, 0);
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 1);
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 2);
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 0);
    }

    #[test]
    fn wizard_shift_tab_and_up_move_previous_field() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 1);

        app.update(InputEvent::Key(KeyEvent {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: true,
                ctrl: false,
                alt: false,
            },
        }));
        assert_eq!(app.wizard().field, 0);

        app.update(key(Key::Up));
        assert_eq!(app.wizard().field, 2);
    }

    #[test]
    fn wizard_enter_validates_count_and_stays_on_step() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.count = "0".to_owned();

        let cmd = app.update(key(Key::Enter));
        assert_eq!(cmd, Command::None);
        assert_eq!(app.wizard().step, 1);
        assert!(app.wizard().error.contains("count"));
    }

    #[test]
    fn wizard_enter_advances_steps_and_back_goes_previous() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.wizard().step, 1);

        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 2);

        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);

        app.update(key(Key::Char('b')));
        assert_eq!(app.wizard().step, 2);

        app.update(key(Key::Left));
        assert_eq!(app.wizard().step, 1);
    }

    #[test]
    fn wizard_text_editing_updates_focused_field() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));

        app.update(key(Key::Char('x')));
        app.update(key(Key::Char('y')));
        assert_eq!(app.wizard().values.name, "xy");

        app.update(key(Key::Backspace));
        assert_eq!(app.wizard().values.name, "x");

        app.update(key(Key::Char(' ')));
        assert_eq!(app.wizard().values.name, "x ");
    }

    #[test]
    fn wizard_enter_on_review_submits_create_action() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));

        app.wizard.values.name = "wizard-loop".to_owned();
        app.wizard.values.count = "1".to_owned();
        app.wizard.values.interval = "30s".to_owned();

        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 4);

        let cmd = app.update(key(Key::Enter));
        match cmd {
            Command::RunAction(ActionKind::Create { wizard }) => {
                assert!(wizard
                    .iter()
                    .any(|(k, v)| k == "name" && v == "wizard-loop"));
                assert!(wizard.iter().any(|(k, v)| k == "count" && v == "1"));
                assert!(wizard.iter().any(|(k, v)| k == "interval" && v == "30s"));
            }
            other => panic!("Expected RunAction(Create), got {other:?}"),
        }
        assert!(app.action_busy());
    }

    #[test]
    fn wizard_with_defaults_populates_fields() {
        let mut app = App::new("default", 12);
        app.set_wizard_defaults("30s", "default.md", "run tests");
        app.update(key(Key::Char('n')));
        assert_eq!(app.wizard().values.interval, "30s");
        assert_eq!(app.wizard().values.prompt, "default.md");
        assert_eq!(app.wizard().values.prompt_msg, "run tests");
        assert_eq!(app.wizard().values.count, "1");
        assert!(app.wizard().values.name.is_empty());
    }

    #[test]
    fn wizard_q_exits() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.mode(), UiMode::Wizard);
        app.update(key(Key::Char('q')));
        assert_eq!(app.mode(), UiMode::Main);
    }

    #[test]
    fn wizard_question_mark_opens_help() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Char('?')));
        assert_eq!(app.mode(), UiMode::Help);
        assert_eq!(app.help_return, UiMode::Wizard);
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Wizard);
    }

    #[test]
    fn wizard_j_k_cycle_fields() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.wizard().field, 0);
        app.update(key(Key::Char('j')));
        assert_eq!(app.wizard().field, 1);
        app.update(key(Key::Char('k')));
        assert_eq!(app.wizard().field, 0);
        app.update(key(Key::Char('k')));
        assert_eq!(app.wizard().field, 2);
    }

    #[test]
    fn wizard_down_up_cycle_fields() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Down));
        assert_eq!(app.wizard().field, 1);
        app.update(key(Key::Up));
        assert_eq!(app.wizard().field, 0);
    }

    #[test]
    fn wizard_ctrl_h_backspaces() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Char('a')));
        app.update(key(Key::Char('c')));
        assert_eq!(app.wizard().values.name, "ac");
        app.update(ctrl_key('h'));
        assert_eq!(app.wizard().values.name, "a");
    }

    #[test]
    fn wizard_back_noop_on_step_1() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        assert_eq!(app.wizard().step, 1);
        app.update(key(Key::Char('b')));
        assert_eq!(app.wizard().step, 1);
        app.update(key(Key::Left));
        assert_eq!(app.wizard().step, 1);
    }

    #[test]
    fn wizard_step4_ignores_text_input() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 4);
        app.update(key(Key::Char('x')));
        app.update(key(Key::Char(' ')));
        app.update(key(Key::Backspace));
        app.update(ctrl_key('h'));
        assert!(app.wizard().values.name.is_empty());
    }

    #[test]
    fn wizard_enter_clears_error_on_success() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.count = "0".to_owned();
        app.update(key(Key::Enter));
        assert!(!app.wizard().error.is_empty());
        app.wizard.values.count = "1".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 2);
        assert!(app.wizard().error.is_empty());
    }

    #[test]
    fn wizard_escape_clears_error() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.count = "bad".to_owned();
        app.update(key(Key::Enter));
        assert!(!app.wizard().error.is_empty());
        app.update(key(Key::Escape));
        assert_eq!(app.mode(), UiMode::Main);
    }

    #[test]
    fn wizard_validates_name_requires_count_1() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.name = "my-loop".to_owned();
        app.wizard.values.count = "3".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 1);
        assert!(app.wizard().error.contains("name requires count=1"));
    }

    #[test]
    fn wizard_validates_pool_profile_conflict() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 2);
        app.wizard.values.pool = "my-pool".to_owned();
        app.wizard.values.profile = "my-profile".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 2);
        assert!(app
            .wizard()
            .error
            .contains("use either pool or profile, not both"));
    }

    #[test]
    fn wizard_validates_interval_duration() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        app.wizard.values.interval = "not-a-duration".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        assert!(app.wizard().error.contains("interval"));
    }

    #[test]
    fn wizard_validates_max_runtime_duration() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.wizard.values.max_runtime = "xyz".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        assert!(app.wizard().error.contains("max runtime"));
    }

    #[test]
    fn wizard_validates_max_iterations_integer() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.wizard.values.max_iterations = "abc".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        assert!(app.wizard().error.contains("max-iterations"));
    }

    #[test]
    fn wizard_validates_negative_max_iterations() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.wizard.values.max_iterations = "-5".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        assert!(app.wizard().error.contains("max-iterations"));
    }

    #[test]
    fn wizard_empty_limits_are_valid() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.wizard.values.interval = String::new();
        app.wizard.values.max_runtime = String::new();
        app.wizard.values.max_iterations = String::new();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 4);
        assert!(app.wizard().error.is_empty());
    }

    #[test]
    fn wizard_valid_durations_pass() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.wizard.values.interval = "30s".to_owned();
        app.wizard.values.max_runtime = "1h".to_owned();
        app.wizard.values.max_iterations = "100".to_owned();
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 4);
    }

    #[test]
    fn wizard_step2_has_2_fields() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 2);
        assert_eq!(app.wizard().field, 0);
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 1);
        app.update(key(Key::Tab));
        assert_eq!(app.wizard().field, 0);
    }

    #[test]
    fn wizard_step3_has_6_fields() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 3);
        for i in 0..6 {
            assert_eq!(app.wizard().field, i);
            app.update(key(Key::Tab));
        }
        assert_eq!(app.wizard().field, 0);
    }

    #[test]
    fn wizard_typing_on_step2_edits_pool() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Char('p')));
        app.update(key(Key::Char('1')));
        assert_eq!(app.wizard().values.pool, "p1");
        app.update(key(Key::Tab));
        app.update(key(Key::Char('x')));
        assert_eq!(app.wizard().values.profile, "x");
    }

    #[test]
    fn wizard_render_step1_snapshot() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.name = "my-loop".to_owned();
        let lines = app.render_wizard_lines(80);
        assert!(lines[0].contains("step 1/4"));
        assert!(lines[1].contains("1) Identity+Count"));
        let body = lines.join("\n");
        assert!(body.contains("name: my-loop_"));
        assert!(body.contains("name-prefix: <empty>"));
        assert!(body.contains("count: 1"));
        assert!(body.contains("tab/down/up navigate"));
    }

    #[test]
    fn wizard_render_step2_snapshot() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        let lines = app.render_wizard_lines(80);
        assert!(lines[0].contains("step 2/4"));
        let body = lines.join("\n");
        assert!(body.contains("pool: <empty>_"));
        assert!(body.contains("profile: <empty>"));
    }

    #[test]
    fn wizard_render_step3_snapshot() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        let lines = app.render_wizard_lines(80);
        assert!(lines[0].contains("step 3/4"));
        let body = lines.join("\n");
        assert!(body.contains("prompt:"));
        assert!(body.contains("interval:"));
        assert!(body.contains("max-runtime:"));
        assert!(body.contains("max-iterations:"));
        assert!(body.contains("tags:"));
    }

    #[test]
    fn wizard_render_step4_review_snapshot() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.name = "test".to_owned();
        app.wizard.values.interval = "10s".to_owned();
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        app.update(key(Key::Enter));
        assert_eq!(app.wizard().step, 4);
        let lines = app.render_wizard_lines(80);
        assert!(lines[0].contains("step 4/4"));
        let body = lines.join("\n");
        assert!(body.contains("Review:"));
        assert!(body.contains("name=\"test\""));
        assert!(body.contains("count=\"1\""));
        assert!(body.contains("interval=\"10s\""));
    }

    #[test]
    fn wizard_render_shows_error() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.count = "0".to_owned();
        app.update(key(Key::Enter));
        let lines = app.render_wizard_lines(80);
        let body = lines.join("\n");
        assert!(body.contains("Error:"));
        assert!(body.contains("count"));
    }

    #[test]
    fn wizard_render_truncates_long_lines() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.name = "a".repeat(100);
        let lines = app.render_wizard_lines(50);
        for line in &lines {
            assert!(line.len() <= 50, "line too long: {}", line.len());
        }
    }

    #[test]
    fn wizard_render_frame_smoke() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        app.wizard.values.count = "bad".to_owned();
        app.update(key(Key::Enter));
        let frame = app.render();
        let snap = frame.snapshot();
        assert!(snap.contains("New loop wizard"));
    }

    #[test]
    fn wizard_header_shows_mode() {
        let mut app = App::new("default", 12);
        app.update(key(Key::Char('n')));
        let frame = app.render();
        let header = frame.row_text(0);
        assert!(
            header.contains("New Loop Wizard") || header.contains("Wizard"),
            "header: {header}"
        );
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

    // -- handle_action_result --

    #[test]
    fn action_result_success_clears_busy_and_sets_ok_status() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        let cmd = app.handle_action_result(ActionResult {
            kind: ActionType::Stop,
            loop_id: "loop-0".into(),
            selected_loop_id: String::new(),
            message: "Stop requested for loop loop-0".into(),
            error: None,
        });
        assert!(!app.action_busy());
        assert!(app.status_text().contains("Stop requested"));
        assert_eq!(cmd, Command::Fetch);
    }

    #[test]
    fn action_result_error_sets_err_status() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        let cmd = app.handle_action_result(ActionResult {
            kind: ActionType::Kill,
            loop_id: "loop-0".into(),
            selected_loop_id: String::new(),
            message: String::new(),
            error: Some("loop not found".into()),
        });
        assert!(!app.action_busy());
        assert!(app.status_text().contains("loop not found"));
        assert_eq!(cmd, Command::None);
    }

    #[test]
    fn action_result_create_error_returns_to_wizard() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        app.mode = UiMode::Main;
        app.handle_action_result(ActionResult {
            kind: ActionType::Create,
            loop_id: String::new(),
            selected_loop_id: String::new(),
            message: String::new(),
            error: Some("invalid count".into()),
        });
        assert_eq!(app.mode(), UiMode::Wizard);
        assert_eq!(app.wizard().error, "invalid count");
    }

    #[test]
    fn action_result_create_success_selects_new_loop() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        app.mode = UiMode::Wizard;
        let cmd = app.handle_action_result(ActionResult {
            kind: ActionType::Create,
            loop_id: String::new(),
            selected_loop_id: "new-loop-42".into(),
            message: "Created 1 loop".into(),
            error: None,
        });
        assert_eq!(app.mode(), UiMode::Main);
        assert_eq!(app.selected_id(), "new-loop-42");
        assert!(app.wizard().error.is_empty());
        assert!(app.status_text().contains("Created 1 loop"));
        assert_eq!(cmd, Command::Fetch);
    }

    #[test]
    fn action_result_resume_success() {
        let mut app = app_with_loops(3);
        app.set_action_busy(true);
        let cmd = app.handle_action_result(ActionResult {
            kind: ActionType::Resume,
            loop_id: "loop-1".into(),
            selected_loop_id: String::new(),
            message: "Loop \"my-loop\" resumed (loop-1)".into(),
            error: None,
        });
        assert!(!app.action_busy());
        assert!(app.status_text().contains("resumed"));
        assert_eq!(cmd, Command::Fetch);
    }
}

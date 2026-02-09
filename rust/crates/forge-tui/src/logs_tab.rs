//! Loop TUI logs-tab state/math helpers.
//!
//! Parity-oriented port of selected behavior from `internal/looptui/looptui.go`.

const DEFAULT_LOG_LINES: i32 = 12;
const DEFAULT_LOG_BACKFILL: i32 = 1200;
const MAX_LOG_BACKFILL: i32 = 8000;
const LOG_SCROLL_STEP: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Overview,
    Logs,
    Runs,
    MultiLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Main,
    ExpandedLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Live,
    LatestRun,
    SelectedRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLayer {
    Raw,
    Events,
    Errors,
    Tools,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogsTabState {
    pub tab: MainTab,
    pub mode: UiMode,
    pub source: LogSource,
    pub layer: LogLayer,
    pub log_scroll: i32,
    pub log_lines: i32,
}

impl Default for LogsTabState {
    fn default() -> Self {
        Self {
            tab: MainTab::Overview,
            mode: UiMode::Main,
            source: LogSource::Live,
            layer: LogLayer::Raw,
            log_scroll: 0,
            log_lines: DEFAULT_LOG_LINES,
        }
    }
}

impl LogsTabState {
    /// Cycle log source (live -> latest-run -> selected-run).
    pub fn cycle_source(&mut self, delta: i32) {
        let options = [
            LogSource::Live,
            LogSource::LatestRun,
            LogSource::SelectedRun,
        ];
        let mut idx = options
            .iter()
            .position(|candidate| *candidate == self.source);
        let idx = idx.get_or_insert(0);

        let mut next = *idx as i32 + delta;
        while next < 0 {
            next += options.len() as i32;
        }

        self.source = options[(next as usize) % options.len()];
        self.log_scroll = 0;
    }

    /// Cycle semantic log layer (raw/events/errors/tools/diff).
    pub fn cycle_layer(&mut self, delta: i32) {
        let options = [
            LogLayer::Raw,
            LogLayer::Events,
            LogLayer::Errors,
            LogLayer::Tools,
            LogLayer::Diff,
        ];

        let mut idx = options
            .iter()
            .position(|candidate| *candidate == self.layer);
        let idx = idx.get_or_insert(0);

        let mut next = *idx as i32 + delta;
        while next < 0 {
            next += options.len() as i32;
        }

        self.layer = options[(next as usize) % options.len()];
    }

    /// Scroll log window by `delta` lines, clamped at 0.
    pub fn scroll_logs(&mut self, delta: i32) {
        self.log_scroll += delta;
        if self.log_scroll < 0 {
            self.log_scroll = 0;
        }
    }

    /// Equivalent to PgUp behavior in main mode.
    pub fn scroll_page_up(&mut self, effective_height: i32) {
        self.scroll_logs(self.log_scroll_page_size(effective_height));
    }

    /// Equivalent to PgDn behavior in main mode.
    pub fn scroll_page_down(&mut self, effective_height: i32) {
        self.scroll_logs(-self.log_scroll_page_size(effective_height));
    }

    #[must_use]
    pub fn log_scroll_page_size(&self, effective_height: i32) -> i32 {
        let estimate = effective_height / 2 + LOG_SCROLL_STEP;
        if estimate < LOG_SCROLL_STEP {
            return LOG_SCROLL_STEP;
        }
        estimate
    }

    #[must_use]
    pub fn source_label(&self) -> &'static str {
        match self.source {
            LogSource::Live => "live",
            LogSource::LatestRun => "latest-run",
            LogSource::SelectedRun => "selected-run",
        }
    }

    #[must_use]
    pub fn layer_label(&self) -> &'static str {
        match self.layer {
            LogLayer::Raw => "raw",
            LogLayer::Events => "events",
            LogLayer::Errors => "errors",
            LogLayer::Tools => "tools",
            LogLayer::Diff => "diff",
        }
    }

    #[must_use]
    pub fn desired_selected_log_lines(&self, effective_height: i32) -> i32 {
        let mut lines = self.log_lines;
        if lines <= 0 {
            lines = DEFAULT_LOG_LINES;
        }

        let mut backfill = DEFAULT_LOG_BACKFILL
            + self.log_scroll
            + self.log_scroll_page_size(effective_height) * 2;
        if backfill > MAX_LOG_BACKFILL {
            backfill = MAX_LOG_BACKFILL;
        }

        match (self.mode, self.tab, self.source) {
            (UiMode::ExpandedLogs, MainTab::Runs, _) => max_i32(lines, 180),
            (UiMode::ExpandedLogs, _, _) => max_i32(backfill, 600),
            (UiMode::Main, MainTab::Logs, LogSource::Live) => max_i32(backfill, 400),
            (UiMode::Main, MainTab::Logs, _) => max_i32(lines, 200),
            (UiMode::Main, MainTab::Runs, _) => max_i32(lines, 180),
            _ => max_i32(lines, 80),
        }
    }
}

/// Compute start/end window from total lines, viewport height, and scroll offset.
#[must_use]
pub fn log_window_bounds(total_lines: i32, available: i32, mut scroll: i32) -> (i32, i32, i32) {
    if total_lines <= 0 {
        return (0, 0, 0);
    }

    let mut available = available;
    if available < 1 {
        available = 1;
    }

    let max_scroll = max_i32(total_lines - 1, 0);
    if scroll < 0 {
        scroll = 0;
    }
    if scroll > max_scroll {
        scroll = max_scroll;
    }

    let mut end = total_lines - scroll;
    if end < 0 {
        end = 0;
    }
    if end > total_lines {
        end = total_lines;
    }

    let mut start = end - available;
    if start < 0 {
        start = 0;
    }

    (start, end, scroll)
}

fn max_i32(a: i32, b: i32) -> i32 {
    if a >= b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::{log_window_bounds, LogLayer, LogSource, LogsTabState, MainTab, UiMode};

    #[test]
    fn cycle_source_matches_go_order() {
        let mut state = LogsTabState {
            tab: MainTab::Logs,
            ..LogsTabState::default()
        };

        state.cycle_source(1);
        assert_eq!(state.source, LogSource::LatestRun);

        state.cycle_source(1);
        assert_eq!(state.source, LogSource::SelectedRun);

        state.cycle_source(1);
        assert_eq!(state.source, LogSource::Live);
    }

    #[test]
    fn cycle_layer_matches_go_order() {
        let mut state = LogsTabState::default();

        state.cycle_layer(1);
        assert_eq!(state.layer, LogLayer::Events);

        state.cycle_layer(1);
        assert_eq!(state.layer, LogLayer::Errors);

        state.cycle_layer(-1);
        assert_eq!(state.layer, LogLayer::Events);
    }

    #[test]
    fn pgup_scrolls_logs() {
        let mut state = LogsTabState {
            tab: MainTab::Logs,
            ..LogsTabState::default()
        };

        let before = state.log_scroll;
        state.scroll_page_up(34);
        assert!(state.log_scroll > before);
    }

    #[test]
    fn log_window_bounds_supports_scroll() {
        let (start, end, clamped) = log_window_bounds(100, 10, 15);
        assert_eq!((start, end, clamped), (75, 85, 15));
    }

    #[test]
    fn desired_selected_log_lines_grows_with_scroll() {
        let mut state = LogsTabState {
            tab: MainTab::Logs,
            source: LogSource::Live,
            log_lines: 20,
            ..LogsTabState::default()
        };

        let base = state.desired_selected_log_lines(34);
        state.log_scroll = 500;
        let scrolled = state.desired_selected_log_lines(34);

        assert!(scrolled > base, "{scrolled} <= {base}");
    }

    #[test]
    fn desired_selected_log_lines_respects_mode_tab_rules() {
        let mut state = LogsTabState {
            tab: MainTab::Runs,
            mode: UiMode::Main,
            log_lines: 12,
            ..LogsTabState::default()
        };
        assert_eq!(state.desired_selected_log_lines(34), 180);

        state.mode = UiMode::ExpandedLogs;
        assert_eq!(state.desired_selected_log_lines(34), 180);

        state.tab = MainTab::Logs;
        assert!(state.desired_selected_log_lines(34) >= 600);
    }

    #[test]
    fn labels_match_expected_tokens() {
        let state = LogsTabState {
            source: LogSource::LatestRun,
            layer: LogLayer::Tools,
            ..LogsTabState::default()
        };
        assert_eq!(state.source_label(), "latest-run");
        assert_eq!(state.layer_label(), "tools");
    }
}

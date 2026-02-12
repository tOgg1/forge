//! Command palette core for next-gen Forge TUI navigation/action flows.
//!
//! Includes:
//! - typed action registry
//! - context-aware fuzzy ranking
//! - recency bias
//! - explicit latency budget guard

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::app::MainTab;

/// Default latency budget for each palette search pass.
pub const DEFAULT_SEARCH_BUDGET: Duration = Duration::from_millis(4);

const MAX_RESULTS: usize = 8;

/// Typed palette action identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteActionId {
    SwitchOverview,
    SwitchLogs,
    SwitchRuns,
    SwitchMultiLogs,
    SwitchInbox,
    OpenFilter,
    NewLoopWizard,
    ResumeSelectedLoop,
    StopSelectedLoop,
    KillSelectedLoop,
    DeleteSelectedLoop,
    CycleTheme,
    ToggleZenMode,
    Custom(u16),
}

/// One action entry in the command palette registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteAction {
    pub id: PaletteActionId,
    pub title: String,
    pub command: String,
    pub keywords: Vec<String>,
    pub preferred_tab: Option<MainTab>,
    pub requires_selection: bool,
}

impl PaletteAction {
    #[must_use]
    pub fn new(
        id: PaletteActionId,
        title: &str,
        command: &str,
        keywords: &[&str],
        preferred_tab: Option<MainTab>,
        requires_selection: bool,
    ) -> Self {
        Self {
            id,
            title: title.to_owned(),
            command: command.to_owned(),
            keywords: keywords.iter().map(|v| (*v).to_owned()).collect(),
            preferred_tab,
            requires_selection,
        }
    }
}

#[must_use]
pub fn default_action_registry() -> Vec<PaletteAction> {
    vec![
        PaletteAction::new(
            PaletteActionId::SwitchOverview,
            "Go to Overview",
            "view overview",
            &["tab", "dashboard", "home"],
            Some(MainTab::Overview),
            false,
        ),
        PaletteAction::new(
            PaletteActionId::SwitchLogs,
            "Go to Logs",
            "view logs",
            &["tab", "events", "output"],
            Some(MainTab::Logs),
            false,
        ),
        PaletteAction::new(
            PaletteActionId::SwitchRuns,
            "Go to Runs",
            "view runs",
            &["tab", "history", "executions"],
            Some(MainTab::Runs),
            false,
        ),
        PaletteAction::new(
            PaletteActionId::SwitchMultiLogs,
            "Go to Multi Logs",
            "view multi-logs",
            &["tab", "grid", "compare"],
            Some(MainTab::MultiLogs),
            false,
        ),
        PaletteAction::new(
            PaletteActionId::SwitchInbox,
            "Go to Inbox",
            "view inbox",
            &["tab", "mail", "thread", "messages", "fmail"],
            Some(MainTab::Inbox),
            false,
        ),
        PaletteAction::new(
            PaletteActionId::OpenFilter,
            "Open Filter",
            "filter",
            &["search", "query", "status"],
            None,
            false,
        ),
        PaletteAction::new(
            PaletteActionId::NewLoopWizard,
            "New Loop Wizard",
            "loop new",
            &["create", "wizard", "spawn"],
            None,
            false,
        ),
        PaletteAction::new(
            PaletteActionId::ResumeSelectedLoop,
            "Resume Selected Loop",
            "loop resume",
            &["restart", "continue"],
            None,
            true,
        ),
        PaletteAction::new(
            PaletteActionId::StopSelectedLoop,
            "Stop Selected Loop",
            "loop stop",
            &["graceful", "pause"],
            None,
            true,
        ),
        PaletteAction::new(
            PaletteActionId::KillSelectedLoop,
            "Kill Selected Loop",
            "loop kill",
            &["terminate", "abort"],
            None,
            true,
        ),
        PaletteAction::new(
            PaletteActionId::DeleteSelectedLoop,
            "Delete Selected Loop",
            "loop delete",
            &["remove", "destroy"],
            None,
            true,
        ),
        PaletteAction::new(
            PaletteActionId::CycleTheme,
            "Cycle Theme",
            "theme cycle",
            &["palette", "appearance"],
            None,
            false,
        ),
        PaletteAction::new(
            PaletteActionId::ToggleZenMode,
            "Toggle Zen Mode",
            "view zen",
            &["focus", "split"],
            None,
            false,
        ),
    ]
}

/// Runtime context that influences ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteContext {
    pub tab: MainTab,
    pub has_selection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteMatch {
    pub id: PaletteActionId,
    pub title: String,
    pub command: String,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteSearchResult {
    pub matches: Vec<PaletteMatch>,
    pub timed_out: bool,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct PaletteUsage {
    sequence: u64,
    last_seen: HashMap<PaletteActionId, u64>,
    count: HashMap<PaletteActionId, u64>,
}

impl PaletteUsage {
    pub fn record(&mut self, id: PaletteActionId) {
        self.sequence = self.sequence.saturating_add(1);
        self.last_seen.insert(id, self.sequence);
        let next = self.count.get(&id).copied().unwrap_or(0).saturating_add(1);
        self.count.insert(id, next);
    }

    #[must_use]
    fn score(&self, id: PaletteActionId) -> i64 {
        let seen_bonus = self.last_seen.get(&id).map_or(0_i64, |last| {
            (30_i64 - self.sequence.saturating_sub(*last) as i64).max(0)
        });
        let count_bonus = self.count.get(&id).copied().unwrap_or(0).min(10) as i64 * 2;
        seen_bonus + count_bonus
    }
}

/// Stateful command palette controller.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    registry: Vec<PaletteAction>,
    usage: PaletteUsage,
    query: String,
    selected: usize,
    result: PaletteSearchResult,
}

impl CommandPalette {
    #[must_use]
    pub fn new_default() -> Self {
        Self::with_registry(default_action_registry())
    }

    #[must_use]
    pub fn with_registry(registry: Vec<PaletteAction>) -> Self {
        Self {
            registry,
            usage: PaletteUsage::default(),
            query: String::new(),
            selected: 0,
            result: PaletteSearchResult {
                matches: Vec::new(),
                timed_out: false,
                elapsed: Duration::ZERO,
            },
        }
    }

    pub fn open(&mut self, context: PaletteContext, budget: Duration) {
        self.query.clear();
        self.selected = 0;
        self.refresh(context, budget);
    }

    pub fn set_query(&mut self, query: String, context: PaletteContext, budget: Duration) {
        self.query = query;
        self.selected = 0;
        self.refresh(context, budget);
    }

    pub fn push_char(&mut self, ch: char, context: PaletteContext, budget: Duration) {
        self.query.push(ch);
        self.selected = 0;
        self.refresh(context, budget);
    }

    pub fn pop_char(&mut self, context: PaletteContext, budget: Duration) {
        self.query.pop();
        self.selected = 0;
        self.refresh(context, budget);
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.result.matches.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.result.matches.len() as i32;
        let mut idx = self.selected as i32 + delta;
        while idx < 0 {
            idx += len;
        }
        self.selected = (idx as usize) % self.result.matches.len();
    }

    #[must_use]
    pub fn accept(&mut self, context: PaletteContext, budget: Duration) -> Option<PaletteActionId> {
        let action = self.current_action_id()?;
        self.usage.record(action);
        self.refresh(context, budget);
        Some(action)
    }

    pub fn refresh(&mut self, context: PaletteContext, budget: Duration) {
        self.result = search_actions(&self.registry, &self.query, context, &self.usage, budget);
        if self.selected >= self.result.matches.len() {
            self.selected = 0;
        }
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn matches(&self) -> &[PaletteMatch] {
        &self.result.matches
    }

    #[must_use]
    pub fn timed_out(&self) -> bool {
        self.result.timed_out
    }

    #[must_use]
    pub fn current_action_id(&self) -> Option<PaletteActionId> {
        self.result.matches.get(self.selected).map(|m| m.id)
    }

    #[must_use]
    pub fn render_lines(&self, width: usize, max_rows: usize) -> Vec<String> {
        if max_rows == 0 {
            return Vec::new();
        }
        let mut lines = Vec::new();
        lines.push(truncate(
            "Command Palette  (enter run, esc close, tab cycle)",
            width,
        ));
        if lines.len() >= max_rows {
            return lines;
        }
        let query = if self.query.is_empty() {
            "<empty>"
        } else {
            self.query.as_str()
        };
        lines.push(truncate(&format!("query: {query}"), width));
        if lines.len() >= max_rows {
            return lines;
        }
        if self.result.matches.is_empty() {
            lines.push(truncate("  no matching actions", width));
            return lines;
        }
        for (idx, item) in self.result.matches.iter().enumerate() {
            if lines.len() >= max_rows {
                break;
            }
            let marker = if idx == self.selected { ">" } else { " " };
            let row = format!("{marker} {:<18} {}", item.command, item.title);
            lines.push(truncate(&row, width));
        }
        lines
    }
}

fn search_actions(
    registry: &[PaletteAction],
    query: &str,
    context: PaletteContext,
    usage: &PaletteUsage,
    budget: Duration,
) -> PaletteSearchResult {
    let started = Instant::now();
    let normalized_query = query.trim().to_ascii_lowercase();
    let mut scored = Vec::new();
    let mut timed_out = false;

    for action in registry {
        if started.elapsed() > budget {
            timed_out = true;
            break;
        }
        if action.requires_selection && !context.has_selection {
            continue;
        }
        let Some(mut score) = query_score(action, &normalized_query) else {
            continue;
        };
        if action.preferred_tab == Some(context.tab) {
            score += 45;
        }
        if action.requires_selection && context.has_selection {
            score += 20;
        }
        score += usage.score(action.id);
        scored.push(PaletteMatch {
            id: action.id,
            title: action.title.clone(),
            command: action.command.clone(),
            score,
        });
    }

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.title.cmp(&b.title))
            .then(a.command.cmp(&b.command))
    });
    scored.truncate(MAX_RESULTS);

    PaletteSearchResult {
        matches: scored,
        timed_out,
        elapsed: started.elapsed(),
    }
}

fn query_score(action: &PaletteAction, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(100);
    }

    let mut best = fuzzy_score(query, &action.command);
    best = max_option(
        best,
        fuzzy_score(query, &action.title).map(|score| score + 5),
    );
    for keyword in &action.keywords {
        best = max_option(best, fuzzy_score(query, keyword));
    }
    best
}

fn max_option(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let q = query.to_ascii_lowercase();
    let t = text.to_ascii_lowercase();
    if t == q {
        return Some(420);
    }
    if t.starts_with(&q) {
        return Some(360 - (t.len().saturating_sub(q.len()) as i64));
    }
    if let Some(idx) = t.find(&q) {
        return Some(300 - (idx as i64 * 4));
    }

    let mut first: Option<usize> = None;
    let mut qchars = q.chars();
    let mut current = qchars.next()?;
    for (idx, ch) in t.chars().enumerate() {
        if ch != current {
            continue;
        }
        if first.is_none() {
            first = Some(idx);
        }
        if let Some(next) = qchars.next() {
            current = next;
        } else {
            let span = idx.saturating_sub(first.unwrap_or(idx)).saturating_add(1);
            let gaps = span.saturating_sub(q.len());
            return Some(180 - span as i64 - (gaps as i64 * 2));
        }
    }
    None
}

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut iter = value.chars();
    let mut out = String::new();
    for _ in 0..width {
        if let Some(ch) = iter.next() {
            out.push(ch);
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        default_action_registry, CommandPalette, PaletteAction, PaletteActionId, PaletteContext,
        DEFAULT_SEARCH_BUDGET,
    };
    use crate::app::MainTab;
    use std::time::Duration;

    #[test]
    fn default_registry_is_typed_and_stable() {
        let registry = default_action_registry();
        assert_eq!(registry.len(), 13);
        assert_eq!(registry[0].id, PaletteActionId::SwitchOverview);
        assert_eq!(registry[1].id, PaletteActionId::SwitchLogs);
        assert_eq!(registry[4].id, PaletteActionId::SwitchInbox);
        assert_eq!(registry[6].id, PaletteActionId::NewLoopWizard);
        assert_eq!(registry[12].id, PaletteActionId::ToggleZenMode);
    }

    #[test]
    fn context_aware_ranking_prefers_current_tab() {
        let mut palette = CommandPalette::new_default();
        palette.open(
            PaletteContext {
                tab: MainTab::Logs,
                has_selection: false,
            },
            DEFAULT_SEARCH_BUDGET,
        );
        palette.set_query(
            "view".to_owned(),
            PaletteContext {
                tab: MainTab::Logs,
                has_selection: false,
            },
            DEFAULT_SEARCH_BUDGET,
        );
        assert_eq!(palette.matches()[0].id, PaletteActionId::SwitchLogs);
    }

    #[test]
    fn inbox_query_resolves_inbox_navigation_action() {
        let mut palette = CommandPalette::new_default();
        let ctx = PaletteContext {
            tab: MainTab::Overview,
            has_selection: false,
        };
        palette.open(ctx, DEFAULT_SEARCH_BUDGET);
        palette.set_query("inbox".to_owned(), ctx, DEFAULT_SEARCH_BUDGET);
        assert_eq!(palette.matches()[0].id, PaletteActionId::SwitchInbox);
    }

    #[test]
    fn recency_bias_reorders_tied_actions_deterministically() {
        let ctx = PaletteContext {
            tab: MainTab::Overview,
            has_selection: true,
        };
        let mut palette = CommandPalette::new_default();
        palette.open(ctx, DEFAULT_SEARCH_BUDGET);
        palette.set_query("loop".to_owned(), ctx, DEFAULT_SEARCH_BUDGET);
        let before = palette.matches()[0].id;
        palette.set_query("stop".to_owned(), ctx, DEFAULT_SEARCH_BUDGET);
        assert_eq!(palette.matches()[0].id, PaletteActionId::StopSelectedLoop);
        let accepted = palette.accept(ctx, DEFAULT_SEARCH_BUDGET);
        assert_eq!(accepted, Some(PaletteActionId::StopSelectedLoop));
        palette.set_query("loop".to_owned(), ctx, DEFAULT_SEARCH_BUDGET);
        let after = palette.matches()[0].id;
        assert_ne!(before, after);
        assert_eq!(after, PaletteActionId::StopSelectedLoop);
    }

    #[test]
    fn selection_actions_hidden_without_selection_context() {
        let mut palette = CommandPalette::new_default();
        palette.open(
            PaletteContext {
                tab: MainTab::Overview,
                has_selection: false,
            },
            DEFAULT_SEARCH_BUDGET,
        );
        palette.set_query(
            "loop".to_owned(),
            PaletteContext {
                tab: MainTab::Overview,
                has_selection: false,
            },
            DEFAULT_SEARCH_BUDGET,
        );
        assert!(!palette.matches().iter().any(|m| matches!(
            m.id,
            PaletteActionId::ResumeSelectedLoop
                | PaletteActionId::StopSelectedLoop
                | PaletteActionId::KillSelectedLoop
                | PaletteActionId::DeleteSelectedLoop
        )));
    }

    #[test]
    fn latency_budget_is_enforced() {
        let registry: Vec<PaletteAction> = (0..10_000)
            .map(|idx| {
                PaletteAction::new(
                    PaletteActionId::Custom(idx as u16),
                    &format!("Custom Action {idx}"),
                    &format!("custom-{idx}"),
                    &["custom", "action"],
                    None,
                    false,
                )
            })
            .collect();
        let mut palette = CommandPalette::with_registry(registry);
        palette.open(
            PaletteContext {
                tab: MainTab::Overview,
                has_selection: false,
            },
            Duration::ZERO,
        );
        assert!(palette.timed_out());
    }

    #[test]
    fn render_lines_includes_query_and_results() {
        let mut palette = CommandPalette::new_default();
        let ctx = PaletteContext {
            tab: MainTab::Overview,
            has_selection: false,
        };
        palette.open(ctx, DEFAULT_SEARCH_BUDGET);
        palette.set_query("filter".to_owned(), ctx, DEFAULT_SEARCH_BUDGET);
        let lines = palette.render_lines(80, 6);
        assert!(lines[0].contains("Command Palette"));
        assert!(lines[1].contains("query: filter"));
        assert!(lines.iter().any(|line| line.contains("Open Filter")));
    }
}

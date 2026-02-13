//! Universal fuzzy switcher across loops, runs, threads, and actions.

use std::collections::HashMap;

use crate::app::MainTab;
use crate::command_palette::{PaletteAction, PaletteActionId};
use crate::global_search_index::{SearchEntityKind, SearchHit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SwitcherItemKind {
    Action,
    Loop,
    Run,
    Thread,
}

impl SwitcherItemKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Loop => "loop",
            Self::Run => "run",
            Self::Thread => "thread",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SwitcherTarget {
    Action {
        action_id: PaletteActionId,
        command: String,
    },
    Loop {
        loop_id: String,
    },
    Run {
        run_id: String,
    },
    Thread {
        thread_id: String,
    },
}

impl SwitcherTarget {
    #[must_use]
    pub fn usage_key(&self) -> String {
        match self {
            Self::Action { command, .. } => format!("action:{command}"),
            Self::Loop { loop_id } => format!("loop:{loop_id}"),
            Self::Run { run_id } => format!("run:{run_id}"),
            Self::Thread { thread_id } => format!("thread:{thread_id}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherItem {
    pub id: String,
    pub kind: SwitcherItemKind,
    pub title: String,
    pub subtitle: String,
    pub keywords: Vec<String>,
    pub target: SwitcherTarget,
    pub preferred_tab: Option<MainTab>,
    pub requires_selection: bool,
    pub updated_at_epoch_s: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherContext {
    pub tab: MainTab,
    pub has_selection: bool,
    pub now_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherMatch {
    pub kind: SwitcherItemKind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub target: SwitcherTarget,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SwitcherSearchResult {
    pub matches: Vec<SwitcherMatch>,
    pub total_candidates: usize,
}

#[derive(Debug, Clone, Default)]
pub struct UniversalSwitcher {
    items: Vec<SwitcherItem>,
    usage_sequence: u64,
    usage_last_seen: HashMap<String, u64>,
    usage_count: HashMap<String, u64>,
}

impl UniversalSwitcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn upsert_item(&mut self, item: SwitcherItem) {
        let id = normalize_required(&item.id);
        if id.is_empty() {
            return;
        }
        let mut normalized = item;
        normalized.id = id.clone();
        normalized.title = normalize_display(&normalized.title, &id);
        normalized.subtitle = normalized.subtitle.trim().to_owned();
        normalized.keywords = normalized
            .keywords
            .iter()
            .map(|keyword| normalize_required(keyword))
            .filter(|keyword| !keyword.is_empty())
            .collect();
        if let Some(index) = self.items.iter().position(|existing| existing.id == id) {
            self.items[index] = normalized;
        } else {
            self.items.push(normalized);
        }
    }

    pub fn ingest_palette_actions(&mut self, actions: &[PaletteAction]) {
        for action in actions {
            self.upsert_item(SwitcherItem {
                id: format!("action:{}", normalize_required(&action.command)),
                kind: SwitcherItemKind::Action,
                title: action.title.clone(),
                subtitle: action.command.clone(),
                keywords: action.keywords.clone(),
                target: SwitcherTarget::Action {
                    action_id: action.id,
                    command: action.command.clone(),
                },
                preferred_tab: action.preferred_tab,
                requires_selection: action.requires_selection,
                updated_at_epoch_s: 0,
            });
        }
    }

    pub fn ingest_search_hits(&mut self, hits: &[SearchHit]) {
        for hit in hits {
            let kind = map_kind(hit.kind);
            let target = map_target(hit.kind, &hit.id);
            self.upsert_item(SwitcherItem {
                id: format!("{}:{}", kind.label(), normalize_required(&hit.id)),
                kind,
                title: hit.title.clone(),
                subtitle: hit.snippet.clone(),
                keywords: hit.tags.clone(),
                target,
                preferred_tab: kind_default_tab(kind),
                requires_selection: false,
                updated_at_epoch_s: hit.updated_at_epoch_s,
            });
        }
    }

    pub fn upsert_thread(
        &mut self,
        thread_id: &str,
        subject: &str,
        detail: &str,
        updated_at_epoch_s: i64,
    ) {
        let thread_id = normalize_required(thread_id);
        if thread_id.is_empty() {
            return;
        }
        self.upsert_item(SwitcherItem {
            id: format!("thread:{thread_id}"),
            kind: SwitcherItemKind::Thread,
            title: normalize_display(subject, &thread_id),
            subtitle: detail.trim().to_owned(),
            keywords: vec![
                "inbox".to_owned(),
                "message".to_owned(),
                "thread".to_owned(),
            ],
            target: SwitcherTarget::Thread { thread_id },
            preferred_tab: Some(MainTab::Inbox),
            requires_selection: false,
            updated_at_epoch_s,
        });
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn record_use(&mut self, target: &SwitcherTarget) {
        self.usage_sequence = self.usage_sequence.saturating_add(1);
        let key = target.usage_key();
        self.usage_last_seen
            .insert(key.clone(), self.usage_sequence);
        let count = self.usage_count.get(&key).copied().unwrap_or(0);
        self.usage_count.insert(key, count.saturating_add(1));
    }

    #[must_use]
    pub fn search(
        &self,
        query: &str,
        context: SwitcherContext,
        limit: usize,
    ) -> SwitcherSearchResult {
        let query = normalize_required(query);
        let terms = query_terms(&query);
        let limit = if limit == 0 { 12 } else { limit };
        let mut matches = Vec::new();

        for item in &self.items {
            if item.requires_selection && !context.has_selection {
                continue;
            }
            let Some(mut score) = query_score(item, &query, &terms) else {
                continue;
            };
            score += context_bonus(item, context);
            score += recency_score(context.now_epoch_s, item.updated_at_epoch_s);
            score += usage_score(
                &self.usage_last_seen,
                &self.usage_count,
                self.usage_sequence,
                &item.target.usage_key(),
            );
            matches.push(SwitcherMatch {
                kind: item.kind,
                id: item.id.clone(),
                title: item.title.clone(),
                subtitle: item.subtitle.clone(),
                target: item.target.clone(),
                score,
            });
        }

        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.kind.cmp(&b.kind))
                .then(a.title.cmp(&b.title))
                .then(a.id.cmp(&b.id))
        });

        let total_candidates = matches.len();
        matches.truncate(limit);
        SwitcherSearchResult {
            matches,
            total_candidates,
        }
    }
}

fn query_score(item: &SwitcherItem, query: &str, terms: &[String]) -> Option<i64> {
    if query.is_empty() {
        return Some(150);
    }

    let fields = searchable_fields(item);
    let mut total = 0_i64;
    for term in terms {
        let mut best = None;
        for field in &fields {
            best = max_option(best, fuzzy_score(term, field));
        }
        total += best?;
    }
    Some(total)
}

fn searchable_fields(item: &SwitcherItem) -> Vec<String> {
    let mut fields = vec![
        normalize_required(&item.id),
        normalize_required(&item.title),
        normalize_required(&item.subtitle),
        item.kind.label().to_owned(),
    ];
    fields.extend(
        item.keywords
            .iter()
            .map(|keyword| normalize_required(keyword)),
    );
    fields
}

fn context_bonus(item: &SwitcherItem, context: SwitcherContext) -> i64 {
    let mut score = 0_i64;
    if item.preferred_tab == Some(context.tab) {
        score += 40;
    }
    if item.kind == SwitcherItemKind::Action && item.requires_selection && context.has_selection {
        score += 20;
    }
    if item.preferred_tab.is_none() && kind_default_tab(item.kind) == Some(context.tab) {
        score += 16;
    }
    score
}

fn usage_score(
    last_seen: &HashMap<String, u64>,
    count: &HashMap<String, u64>,
    sequence: u64,
    key: &str,
) -> i64 {
    let recency_bonus = last_seen.get(key).map_or(0_i64, |last| {
        (30_i64 - sequence.saturating_sub(*last) as i64).max(0)
    });
    let count_bonus = count.get(key).copied().unwrap_or(0).min(10) as i64 * 2;
    recency_bonus + count_bonus
}

fn recency_score(now_epoch_s: i64, updated_at_epoch_s: i64) -> i64 {
    if updated_at_epoch_s <= 0 || now_epoch_s <= 0 {
        return 0;
    }
    let age = age_seconds(now_epoch_s, updated_at_epoch_s);
    if age <= 60 {
        20
    } else if age <= 300 {
        16
    } else if age <= 3_600 {
        12
    } else if age <= 21_600 {
        8
    } else if age <= 86_400 {
        4
    } else {
        0
    }
}

fn age_seconds(now_epoch_s: i64, then_epoch_s: i64) -> u64 {
    if now_epoch_s <= then_epoch_s {
        0
    } else {
        (now_epoch_s - then_epoch_s) as u64
    }
}

fn kind_default_tab(kind: SwitcherItemKind) -> Option<MainTab> {
    match kind {
        SwitcherItemKind::Action => None,
        SwitcherItemKind::Loop => Some(MainTab::Overview),
        SwitcherItemKind::Run => Some(MainTab::Runs),
        SwitcherItemKind::Thread => Some(MainTab::Inbox),
    }
}

fn map_kind(kind: SearchEntityKind) -> SwitcherItemKind {
    match kind {
        SearchEntityKind::Loop => SwitcherItemKind::Loop,
        SearchEntityKind::Run => SwitcherItemKind::Run,
        SearchEntityKind::Task => SwitcherItemKind::Thread,
        SearchEntityKind::Log => SwitcherItemKind::Loop,
    }
}

fn map_target(kind: SearchEntityKind, id: &str) -> SwitcherTarget {
    match kind {
        SearchEntityKind::Loop => SwitcherTarget::Loop {
            loop_id: id.to_owned(),
        },
        SearchEntityKind::Run => SwitcherTarget::Run {
            run_id: id.to_owned(),
        },
        SearchEntityKind::Task => SwitcherTarget::Thread {
            thread_id: id.to_owned(),
        },
        SearchEntityKind::Log => {
            let loop_id = id
                .strip_prefix("log:")
                .map(str::to_owned)
                .unwrap_or_else(|| id.to_owned());
            SwitcherTarget::Loop { loop_id }
        }
    }
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_display(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(normalize_required)
        .filter(|term| !term.is_empty())
        .collect()
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
    let q = normalize_required(query);
    let t = normalize_required(text);
    if q.is_empty() || t.is_empty() {
        return None;
    }
    if q == t {
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

#[cfg(test)]
mod tests {
    use super::{SwitcherContext, SwitcherItemKind, SwitcherTarget, UniversalSwitcher};
    use crate::app::MainTab;
    use crate::command_palette::{PaletteAction, PaletteActionId};
    use crate::global_search_index::{SearchEntityKind, SearchHit};

    fn context(tab: MainTab, has_selection: bool) -> SwitcherContext {
        SwitcherContext {
            tab,
            has_selection,
            now_epoch_s: 2_000,
        }
    }

    #[test]
    fn search_spans_loop_run_thread_and_action() {
        let mut switcher = UniversalSwitcher::new();
        switcher.ingest_palette_actions(&[PaletteAction::new(
            PaletteActionId::SwitchRuns,
            "Go to Runs",
            "view runs",
            &["history", "executions"],
            Some(MainTab::Runs),
            false,
        )]);
        switcher.ingest_search_hits(&[
            SearchHit {
                id: "loop-1".to_owned(),
                kind: SearchEntityKind::Loop,
                title: "Builder loop".to_owned(),
                snippet: "repo forge".to_owned(),
                repo: None,
                profile: None,
                tags: vec!["builder".to_owned()],
                updated_at_epoch_s: 1_980,
                score: 0,
            },
            SearchHit {
                id: "run-9".to_owned(),
                kind: SearchEntityKind::Run,
                title: "Run run-9 (error)".to_owned(),
                snippet: "panic in test".to_owned(),
                repo: None,
                profile: None,
                tags: vec!["loop-1".to_owned()],
                updated_at_epoch_s: 1_990,
                score: 0,
            },
        ]);
        switcher.upsert_thread("thread-ops", "Ops escalation", "retry queue high", 1_995);

        let result = switcher.search("ops", context(MainTab::Inbox, true), 8);
        assert!(result
            .matches
            .iter()
            .any(|m| m.kind == SwitcherItemKind::Thread));

        let result = switcher.search("view runs", context(MainTab::Runs, true), 8);
        assert_eq!(result.matches[0].kind, SwitcherItemKind::Action);
    }

    #[test]
    fn action_requiring_selection_is_hidden_without_selection() {
        let mut switcher = UniversalSwitcher::new();
        switcher.ingest_palette_actions(&[PaletteAction::new(
            PaletteActionId::StopSelectedLoop,
            "Stop Selected Loop",
            "loop stop",
            &["kill", "halt"],
            None,
            true,
        )]);
        let without_selection = switcher.search("stop", context(MainTab::Overview, false), 8);
        assert!(without_selection.matches.is_empty());

        let with_selection = switcher.search("stop", context(MainTab::Overview, true), 8);
        assert_eq!(with_selection.matches.len(), 1);
        assert_eq!(with_selection.matches[0].kind, SwitcherItemKind::Action);
    }

    #[test]
    fn usage_bias_promotes_frequently_used_target() {
        let mut switcher = UniversalSwitcher::new();
        switcher.ingest_search_hits(&[
            SearchHit {
                id: "loop-alpha".to_owned(),
                kind: SearchEntityKind::Loop,
                title: "Deploy alpha".to_owned(),
                snippet: String::new(),
                repo: None,
                profile: None,
                tags: Vec::new(),
                updated_at_epoch_s: 1_200,
                score: 0,
            },
            SearchHit {
                id: "loop-beta".to_owned(),
                kind: SearchEntityKind::Loop,
                title: "Deploy beta".to_owned(),
                snippet: String::new(),
                repo: None,
                profile: None,
                tags: Vec::new(),
                updated_at_epoch_s: 1_200,
                score: 0,
            },
        ]);

        let baseline = switcher.search("deploy", context(MainTab::Overview, true), 8);
        assert_eq!(baseline.matches[0].title, "Deploy alpha");

        switcher.record_use(&SwitcherTarget::Loop {
            loop_id: "loop-beta".to_owned(),
        });
        switcher.record_use(&SwitcherTarget::Loop {
            loop_id: "loop-beta".to_owned(),
        });

        let boosted = switcher.search("deploy", context(MainTab::Overview, true), 8);
        assert_eq!(boosted.matches[0].title, "Deploy beta");
    }

    #[test]
    fn ingest_search_hit_task_maps_to_thread_target() {
        let mut switcher = UniversalSwitcher::new();
        switcher.ingest_search_hits(&[SearchHit {
            id: "task-thread-1".to_owned(),
            kind: SearchEntityKind::Task,
            title: "Thread: investigate flaky run".to_owned(),
            snippet: "owner=ops".to_owned(),
            repo: None,
            profile: None,
            tags: Vec::new(),
            updated_at_epoch_s: 1_999,
            score: 0,
        }]);

        let results = switcher.search("investigate", context(MainTab::Inbox, true), 8);
        assert_eq!(results.matches.len(), 1);
        match &results.matches[0].target {
            SwitcherTarget::Thread { thread_id } => assert_eq!(thread_id, "task-thread-1"),
            other => panic!("expected thread target, got {other:?}"),
        }
    }
}

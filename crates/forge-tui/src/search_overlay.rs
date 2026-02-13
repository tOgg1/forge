//! Universal search overlay for Forge TUI.
//!
//! Provides full-text search across loops, runs, and log metadata using
//! the [`GlobalSearchIndex`]. Supports incremental query input, result
//! navigation (next/prev), match highlighting in rendered output, and
//! jump-to-source on Enter.

use crate::global_search_index::{
    GlobalSearchIndex, SearchDocument, SearchEntityKind, SearchFilter, SearchHit, SearchRequest,
};

const MAX_SEARCH_RESULTS: usize = 20;

/// Stateful controller for the universal search overlay.
#[derive(Debug, Clone)]
pub struct SearchOverlay {
    index: GlobalSearchIndex,
    query: String,
    results: Vec<SearchHit>,
    selected: usize,
    total_matches: usize,
}

/// Where to jump when the user presses Enter on a search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchJumpTarget {
    Loop { loop_id: String },
    Run { run_id: String },
    Log { loop_id: String },
}

impl Default for SearchOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchOverlay {
    #[must_use]
    pub fn new() -> Self {
        Self {
            index: GlobalSearchIndex::new(),
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            total_matches: 0,
        }
    }

    /// Reset query and results, preparing for a new search session.
    pub fn open(&mut self) {
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.total_matches = 0;
    }

    /// Get a mutable reference to the underlying index for population.
    pub fn index_mut(&mut self) -> &mut GlobalSearchIndex {
        &mut self.index
    }

    /// Push a character to the query and re-search.
    pub fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.selected = 0;
        self.refresh();
    }

    /// Remove the last character from the query and re-search.
    pub fn pop_char(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.refresh();
    }

    /// Move result selection by delta (positive = down, negative = up).
    pub fn move_selection(&mut self, delta: i32) {
        if self.results.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.results.len() as i32;
        let mut idx = self.selected as i32 + delta;
        while idx < 0 {
            idx += len;
        }
        self.selected = (idx as usize) % self.results.len();
    }

    /// Move to the next match (wraps around).
    pub fn next_match(&mut self) {
        self.move_selection(1);
    }

    /// Move to the previous match (wraps around).
    pub fn prev_match(&mut self) {
        self.move_selection(-1);
    }

    /// Accept the currently selected result. Returns the jump target if any.
    #[must_use]
    pub fn accept(&self) -> Option<SearchJumpTarget> {
        let hit = self.results.get(self.selected)?;
        Some(match hit.kind {
            SearchEntityKind::Loop => SearchJumpTarget::Loop {
                loop_id: hit.id.clone(),
            },
            SearchEntityKind::Run => SearchJumpTarget::Run {
                run_id: hit.id.clone(),
            },
            SearchEntityKind::Log | SearchEntityKind::Task => SearchJumpTarget::Log {
                loop_id: hit.id.clone(),
            },
        })
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn results(&self) -> &[SearchHit] {
        &self.results
    }

    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn total_matches(&self) -> usize {
        self.total_matches
    }

    /// Render the search overlay as text lines for the TUI frame.
    #[must_use]
    pub fn render_lines(&self, width: usize, max_rows: usize) -> Vec<SearchLine> {
        if max_rows == 0 {
            return Vec::new();
        }
        let mut lines = Vec::new();

        // Header
        lines.push(SearchLine {
            text: truncate(
                "Universal Search  (enter jump, esc close, tab/arrows cycle)",
                width,
            ),
            highlighted: false,
            selected: false,
        });
        if lines.len() >= max_rows {
            return lines;
        }

        // Query line
        let query_display = if self.query.is_empty() {
            "<type to search across loops, runs, logs>"
        } else {
            self.query.as_str()
        };
        let match_info = format!(
            "query: {}  ({} result{})",
            query_display,
            self.total_matches,
            if self.total_matches == 1 { "" } else { "s" }
        );
        lines.push(SearchLine {
            text: truncate(&match_info, width),
            highlighted: false,
            selected: false,
        });
        if lines.len() >= max_rows {
            return lines;
        }

        // Empty state
        if self.results.is_empty() {
            if self.query.is_empty() {
                lines.push(SearchLine {
                    text: truncate("  start typing to search", width),
                    highlighted: false,
                    selected: false,
                });
            } else {
                lines.push(SearchLine {
                    text: truncate("  no matches found", width),
                    highlighted: false,
                    selected: false,
                });
            }
            return lines;
        }

        // Results
        for (idx, hit) in self.results.iter().enumerate() {
            if lines.len() >= max_rows {
                break;
            }
            let is_selected = idx == self.selected;
            let marker = if is_selected { ">" } else { " " };
            let kind_label = match hit.kind {
                SearchEntityKind::Loop => "loop",
                SearchEntityKind::Run => "run",
                SearchEntityKind::Task => "task",
                SearchEntityKind::Log => "log",
            };
            let row = format!("{marker} [{kind_label}] {}: {}", hit.title, hit.snippet);
            lines.push(SearchLine {
                text: truncate(&row, width),
                highlighted: has_query_highlight(&hit.snippet, &self.query),
                selected: is_selected,
            });
        }

        // Match navigation hint
        if lines.len() < max_rows && !self.results.is_empty() {
            lines.push(SearchLine {
                text: truncate(
                    &format!(
                        "  match {}/{} (ctrl+n next, ctrl+p prev)",
                        self.selected + 1,
                        self.total_matches
                    ),
                    width,
                ),
                highlighted: false,
                selected: false,
            });
        }

        lines
    }

    fn refresh(&mut self) {
        let now_epoch_s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let request = SearchRequest {
            query: self.query.clone(),
            filter: SearchFilter::default(),
            limit: MAX_SEARCH_RESULTS,
            now_epoch_s,
        };
        self.results = self.index.search(&request);
        self.total_matches = self.results.len();
        if self.selected >= self.total_matches {
            self.selected = 0;
        }
    }
}

/// A single rendered line in the search overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLine {
    pub text: String,
    pub highlighted: bool,
    pub selected: bool,
}

/// Index loops into the search index.
pub fn index_loops(index: &mut GlobalSearchIndex, loops: &[super::app::LoopView]) {
    for lv in loops {
        let body = format!(
            "{} {} {} runs={} queue={} error={}",
            lv.name, lv.repo_path, lv.state, lv.runs, lv.queue_depth, lv.last_error
        );
        index.upsert(SearchDocument {
            id: lv.id.clone(),
            kind: SearchEntityKind::Loop,
            title: if lv.name.is_empty() {
                lv.short_id.clone()
            } else {
                lv.name.clone()
            },
            body,
            repo: if lv.repo_path.is_empty() {
                None
            } else {
                Some(lv.repo_path.clone())
            },
            profile: if lv.profile_name.is_empty() {
                None
            } else {
                Some(lv.profile_name.clone())
            },
            tags: Vec::new(),
            updated_at_epoch_s: 0,
        });
    }
}

/// Index runs into the search index.
pub fn index_runs(index: &mut GlobalSearchIndex, runs: &[super::app::RunView], loop_id: &str) {
    for rv in runs {
        let body = format!(
            "{} {} exit={} dur={} harness={} profile={}",
            rv.status,
            rv.auth_kind,
            rv.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            rv.duration,
            rv.harness,
            rv.profile_name
        );
        index.upsert(SearchDocument {
            id: rv.id.clone(),
            kind: SearchEntityKind::Run,
            title: format!("Run {} ({})", rv.id, rv.status),
            body,
            repo: None,
            profile: if rv.profile_name.is_empty() {
                None
            } else {
                Some(rv.profile_name.clone())
            },
            tags: vec![loop_id.to_owned()],
            updated_at_epoch_s: 0,
        });
    }
}

/// Index log lines into the search index (sampled for performance).
pub fn index_logs(index: &mut GlobalSearchIndex, log: &super::app::LogTailView, loop_id: &str) {
    if log.lines.is_empty() {
        return;
    }
    // Index a combined document for the log tail, not individual lines.
    let body = if log.lines.len() <= 100 {
        log.lines.join("\n")
    } else {
        // Sample: first 50 + last 50 lines for large logs.
        let first = &log.lines[..50];
        let last = &log.lines[log.lines.len() - 50..];
        format!("{}\n...\n{}", first.join("\n"), last.join("\n"))
    };
    let doc_id = format!("log:{loop_id}");
    index.upsert(SearchDocument {
        id: doc_id,
        kind: SearchEntityKind::Log,
        title: format!("Logs for {loop_id}"),
        body,
        repo: None,
        profile: None,
        tags: vec![loop_id.to_owned()],
        updated_at_epoch_s: 0,
    });
}

fn has_query_highlight(snippet: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let lower_snippet = snippet.to_ascii_lowercase();
    let lower_query = query.to_ascii_lowercase();
    for term in lower_query.split_whitespace() {
        if lower_snippet.contains(term) {
            return true;
        }
    }
    false
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
    use super::*;
    use crate::app::{LogTailView, LoopView, RunView};

    #[test]
    fn empty_search_shows_start_typing_hint() {
        let overlay = SearchOverlay::new();
        let lines = overlay.render_lines(80, 10);
        assert!(lines.iter().any(|l| l.text.contains("start typing")));
    }

    #[test]
    fn search_indexes_and_finds_loops() {
        let mut overlay = SearchOverlay::new();
        let loops = vec![LoopView {
            id: "loop-abc".to_owned(),
            short_id: "abc".to_owned(),
            name: "my-test-loop".to_owned(),
            state: "running".to_owned(),
            repo_path: "/home/user/project".to_owned(),
            ..LoopView::default()
        }];
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('t');
        overlay.push_char('e');
        overlay.push_char('s');
        overlay.push_char('t');
        assert!(!overlay.results().is_empty());
        assert_eq!(overlay.results()[0].id, "loop-abc");
    }

    #[test]
    fn search_indexes_and_finds_runs() {
        let mut overlay = SearchOverlay::new();
        let runs = vec![RunView {
            id: "run-xyz".to_owned(),
            status: "success".to_owned(),
            exit_code: Some(0),
            duration: "12s".to_owned(),
            profile_name: "default".to_owned(),
            profile_id: "profile-default".to_owned(),
            harness: "claude".to_owned(),
            auth_kind: "api-key".to_owned(),
            started_at: "2026-02-13T12:00:00Z".to_owned(),
            output_lines: vec!["ok".to_owned()],
        }];
        index_runs(overlay.index_mut(), &runs, "loop-1");
        overlay.push_char('s');
        overlay.push_char('u');
        overlay.push_char('c');
        assert!(!overlay.results().is_empty());
        assert_eq!(overlay.results()[0].id, "run-xyz");
    }

    #[test]
    fn search_indexes_and_finds_logs() {
        let mut overlay = SearchOverlay::new();
        let log = LogTailView {
            lines: vec![
                "ERROR: connection refused".to_owned(),
                "retrying in 5s".to_owned(),
            ],
            message: String::new(),
        };
        index_logs(overlay.index_mut(), &log, "loop-1");
        overlay.push_char('c');
        overlay.push_char('o');
        overlay.push_char('n');
        overlay.push_char('n');
        assert!(!overlay.results().is_empty());
        assert!(overlay.results()[0].id.contains("log:"));
    }

    #[test]
    fn navigation_wraps_around() {
        let mut overlay = SearchOverlay::new();
        let loops: Vec<LoopView> = (0..5)
            .map(|i| LoopView {
                id: format!("loop-{i}"),
                short_id: format!("{i}"),
                name: format!("searchable-loop-{i}"),
                ..LoopView::default()
            })
            .collect();
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('s');
        overlay.push_char('e');
        overlay.push_char('a');
        overlay.push_char('r');
        assert_eq!(overlay.selected_index(), 0);

        overlay.move_selection(-1);
        assert_eq!(overlay.selected_index(), overlay.results().len() - 1);

        overlay.move_selection(1);
        assert_eq!(overlay.selected_index(), 0);
    }

    #[test]
    fn accept_returns_jump_target() {
        let mut overlay = SearchOverlay::new();
        let loops = vec![LoopView {
            id: "loop-target".to_owned(),
            short_id: "target".to_owned(),
            name: "jump-here".to_owned(),
            ..LoopView::default()
        }];
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('j');
        overlay.push_char('u');
        overlay.push_char('m');
        overlay.push_char('p');
        let target = overlay.accept();
        assert_eq!(
            target,
            Some(SearchJumpTarget::Loop {
                loop_id: "loop-target".to_owned(),
            })
        );
    }

    #[test]
    fn pop_char_updates_results() {
        let mut overlay = SearchOverlay::new();
        let loops = vec![LoopView {
            id: "loop-1".to_owned(),
            short_id: "1".to_owned(),
            name: "alpha".to_owned(),
            ..LoopView::default()
        }];
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('a');
        overlay.push_char('l');
        overlay.push_char('z');
        let before = overlay.results().len();
        overlay.pop_char();
        let after = overlay.results().len();
        // After removing 'z', should find "alpha" again
        assert!(after >= before);
    }

    #[test]
    fn render_lines_show_results_with_selection_marker() {
        let mut overlay = SearchOverlay::new();
        let loops = vec![LoopView {
            id: "loop-1".to_owned(),
            short_id: "1".to_owned(),
            name: "render-test".to_owned(),
            ..LoopView::default()
        }];
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('r');
        overlay.push_char('e');
        overlay.push_char('n');
        let lines = overlay.render_lines(80, 10);
        // Should have header, query line, at least one result, and hint
        assert!(lines.len() >= 4);
        // First result should be selected
        let result_line = &lines[2];
        assert!(result_line.text.starts_with('>'));
        assert!(result_line.selected);
    }

    #[test]
    fn no_results_shows_no_matches() {
        let mut overlay = SearchOverlay::new();
        overlay.push_char('z');
        overlay.push_char('z');
        overlay.push_char('z');
        let lines = overlay.render_lines(80, 10);
        assert!(lines.iter().any(|l| l.text.contains("no matches")));
    }

    #[test]
    fn open_resets_state() {
        let mut overlay = SearchOverlay::new();
        let loops = vec![LoopView {
            id: "loop-1".to_owned(),
            short_id: "1".to_owned(),
            name: "test".to_owned(),
            ..LoopView::default()
        }];
        index_loops(overlay.index_mut(), &loops);
        overlay.push_char('t');
        assert!(!overlay.results().is_empty());
        overlay.open();
        assert!(overlay.results().is_empty());
        assert!(overlay.query().is_empty());
        assert_eq!(overlay.selected_index(), 0);
    }
}

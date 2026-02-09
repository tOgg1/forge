use forge_ftui_adapter::input::{translate_input, InputEvent, Key, KeyEvent, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResultEntry {
    pub message_id: String,
    pub target: String,
    pub from: String,
    pub preview: String,
    pub score: i32,
}

impl SearchResultEntry {
    #[must_use]
    pub fn new(message_id: &str, from: &str, target: &str, preview: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            target: target.to_owned(),
            from: from.to_owned(),
            preview: preview.to_owned(),
            score: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchViewModel {
    query: String,
    results: Vec<SearchResultEntry>,
    selected: usize,
    case_sensitive: bool,
    status_line: String,
}

impl SearchViewModel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.trim().to_owned();
        self.selected = 0;
        self.status_line.clear();
    }

    pub fn set_results(&mut self, mut results: Vec<SearchResultEntry>) {
        rank_results(&mut results, &self.query, self.case_sensitive);
        self.results = results;
        self.selected = 0;
        self.clamp_selection();
        self.status_line = format!("{} matches", self.filtered_results().len());
    }

    pub fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
        rank_results(&mut self.results, &self.query, self.case_sensitive);
        self.status_line = if self.case_sensitive {
            "case-sensitive on".to_owned()
        } else {
            "case-sensitive off".to_owned()
        };
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.status_line = "search cleared".to_owned();
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max_idx = self.filtered_results().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max_idx);
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn filtered_results(&self) -> Vec<&SearchResultEntry> {
        if self.query.is_empty() {
            return self.results.iter().collect::<Vec<_>>();
        }
        self.results
            .iter()
            .filter(|entry| matches_query(entry, &self.query, self.case_sensitive))
            .collect::<Vec<_>>()
    }

    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.filtered_results().len().saturating_sub(1));
    }
}

pub fn apply_search_input(view: &mut SearchViewModel, event: InputEvent) {
    match event {
        InputEvent::Key(KeyEvent {
            key: Key::Char('c'),
            modifiers,
        }) if !modifiers.ctrl && !modifiers.alt => {
            view.clear();
            return;
        }
        InputEvent::Key(KeyEvent {
            key: Key::Char('s'),
            ..
        }) => {
            view.toggle_case_sensitive();
            return;
        }
        _ => {}
    }
    match translate_input(&event) {
        UiAction::MoveUp => view.move_up(),
        UiAction::MoveDown => view.move_down(),
        _ => {}
    }
}

#[must_use]
pub fn render_search_frame(
    view: &SearchViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    let mode = if view.case_sensitive {
        "case-sensitive"
    } else {
        "case-insensitive"
    };
    frame.draw_text(
        0,
        0,
        &truncate(&format!("SEARCH  {mode}  query:{}", view.query()), width),
        TextRole::Accent,
    );
    if height == 1 {
        return frame;
    }

    let filtered = view.filtered_results();
    if filtered.is_empty() {
        frame.draw_text(0, 1, "(no results)", TextRole::Muted);
    } else {
        let rows = height.saturating_sub(2);
        for (row, entry) in filtered.iter().take(rows).enumerate() {
            let marker = if row == view.selected { ">" } else { " " };
            let line = format!(
                "{} {} {} -> {}  {}",
                marker,
                truncate(&entry.message_id, 16),
                truncate(entry.from.trim(), 12),
                truncate(entry.target.trim(), 12),
                truncate(entry.preview.trim(), 22),
            );
            frame.draw_text(0, row + 1, &truncate(&line, width), TextRole::Primary);
        }
    }

    if !view.status_line.trim().is_empty() && height >= 2 {
        frame.draw_text(
            0,
            height - 1,
            &truncate(view.status_line.trim(), width),
            TextRole::Muted,
        );
    }
    frame
}

fn rank_results(results: &mut [SearchResultEntry], query: &str, case_sensitive: bool) {
    for entry in results.iter_mut() {
        entry.score = score_entry(entry, query, case_sensitive);
    }
    results.sort_by(|left, right| right.score.cmp(&left.score));
}

fn score_entry(entry: &SearchResultEntry, query: &str, case_sensitive: bool) -> i32 {
    if query.trim().is_empty() {
        return 0;
    }
    let haystack = format!("{} {} {}", entry.from, entry.target, entry.preview);
    let haystack = if case_sensitive {
        haystack
    } else {
        haystack.to_ascii_lowercase()
    };
    let needle = if case_sensitive {
        query.trim().to_owned()
    } else {
        query.trim().to_ascii_lowercase()
    };
    if haystack.contains(&needle) {
        100 + (needle.len() as i32)
    } else {
        0
    }
}

fn matches_query(entry: &SearchResultEntry, query: &str, case_sensitive: bool) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    score_entry(entry, query, case_sensitive) > 0
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

#[cfg(test)]
mod tests {
    use super::{apply_search_input, render_search_frame, SearchResultEntry, SearchViewModel};
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::ThemeSpec;

    #[test]
    fn search_toggle_case_and_clear() {
        let mut view = SearchViewModel::new();
        view.set_query("auth");
        view.set_results(vec![SearchResultEntry::new(
            "m1",
            "arch",
            "task",
            "auth refresh",
        )]);
        assert_eq!(view.filtered_results().len(), 1);

        apply_search_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('s'))));
        apply_search_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Char('c'))));
        assert_eq!(view.query(), "");
        assert_eq!(view.filtered_results().len(), 0);
    }

    #[test]
    fn search_snapshot() {
        let mut view = SearchViewModel::new();
        view.set_query("refresh");
        view.set_results(vec![
            SearchResultEntry::new("m1", "architect", "task", "refresh token plan"),
            SearchResultEntry::new("m2", "reviewer", "@architect", "status ping"),
        ]);

        let frame = render_search_frame(&view, 62, 5, ThemeSpec::default());
        assert_render_frame_snapshot(
            "fmail_tui_search_frame",
            &frame,
            "SEARCH  case-insensitive  query:refresh                       \n> m1 architect -> task  refresh token plan                    \n                                                              \n                                                              \n1 matches                                                     ",
        );
    }
}

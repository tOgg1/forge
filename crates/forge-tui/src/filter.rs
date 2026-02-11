//! Loop list filter/search helpers.
//!
//! Parity port of `model.applyFilters` and `cycleFilterStatus` in `internal/looptui/looptui.go`.

pub const FILTER_STATUS_OPTIONS: [&str; 6] =
    ["all", "running", "sleeping", "waiting", "stopped", "error"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFocus {
    Text,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopSummary {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub repo_path: String,
    /// Lowercase label matching Go loop state strings (running/sleeping/waiting/stopped/error).
    pub state: String,
}

impl LoopSummary {
    #[must_use]
    pub fn display_id(&self) -> String {
        loop_display_id(&self.id, &self.short_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopView {
    pub loop_entry: Option<LoopSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopListModel {
    pub loops: Vec<LoopView>,
    pub filtered: Vec<LoopView>,

    pub filter_text: String,
    pub filter_state: String,
    pub filter_focus: FilterFocus,

    pub selected_idx: i32,
    pub selected_id: String,
    pub multi_page: i32,
}

impl Default for LoopListModel {
    fn default() -> Self {
        Self {
            loops: Vec::new(),
            filtered: Vec::new(),
            filter_text: String::new(),
            filter_state: "all".to_string(),
            filter_focus: FilterFocus::Text,
            selected_idx: 0,
            selected_id: String::new(),
            multi_page: 0,
        }
    }
}

impl LoopListModel {
    pub fn cycle_filter_status(&mut self, delta: i32) {
        let mut idx = 0i32;
        for (i, candidate) in FILTER_STATUS_OPTIONS.iter().enumerate() {
            if *candidate == self.filter_state {
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
        self.filter_state = FILTER_STATUS_OPTIONS[idx as usize].to_string();
        let old_id = self.selected_id.clone();
        let old_idx = self.selected_idx;
        self.apply_filters(&old_id, old_idx);
    }

    pub fn apply_filters(&mut self, previous_id: &str, previous_idx: i32) {
        let query = self.filter_text.trim().to_ascii_lowercase();
        let state = self.filter_state.trim().to_ascii_lowercase();

        let mut filtered = Vec::with_capacity(self.loops.len());
        for view in &self.loops {
            let Some(loop_entry) = &view.loop_entry else {
                continue;
            };

            let loop_state = loop_entry.state.trim().to_ascii_lowercase();
            if !state.is_empty() && state != "all" && loop_state != state {
                continue;
            }

            if !query.is_empty() {
                let id_candidate = loop_entry.display_id().to_ascii_lowercase();
                let full_id = loop_entry.id.to_ascii_lowercase();
                let name = loop_entry.name.to_ascii_lowercase();
                let repo_path = loop_entry.repo_path.to_ascii_lowercase();
                if !id_candidate.contains(&query)
                    && !full_id.contains(&query)
                    && !name.contains(&query)
                    && !repo_path.contains(&query)
                {
                    continue;
                }
            }

            filtered.push(view.clone());
        }

        self.filtered = filtered;
        if self.filtered.is_empty() {
            self.selected_idx = 0;
            self.selected_id.clear();
            self.multi_page = 0;
            return;
        }

        if !previous_id.trim().is_empty() {
            for (i, view) in self.filtered.iter().enumerate() {
                if let Some(loop_entry) = &view.loop_entry {
                    if loop_entry.id == previous_id {
                        self.selected_idx = i as i32;
                        self.selected_id = previous_id.to_string();
                        return;
                    }
                }
            }
        }

        let mut idx = previous_idx;
        if idx < 0 {
            idx = 0;
        }
        if idx >= self.filtered.len() as i32 {
            idx = self.filtered.len() as i32 - 1;
        }
        self.selected_idx = idx;
        self.selected_id = self
            .filtered
            .get(idx as usize)
            .and_then(|v| v.loop_entry.as_ref())
            .map(|l| l.id.clone())
            .unwrap_or_default();
    }

    #[must_use]
    pub fn selected_view(&self) -> Option<&LoopView> {
        if self.filtered.is_empty() {
            return None;
        }
        let mut idx = self.selected_idx;
        if idx < 0 {
            idx = 0;
        }
        if idx >= self.filtered.len() as i32 {
            idx = self.filtered.len() as i32 - 1;
        }
        self.filtered.get(idx as usize)
    }
}

#[must_use]
pub fn loop_display_id(loop_id: &str, short_id: &str) -> String {
    if !short_id.trim().is_empty() {
        return short_id.trim().to_string();
    }
    if loop_id.len() <= 8 {
        return loop_id.to_string();
    }
    loop_id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::{loop_display_id, FilterFocus, LoopListModel, LoopSummary, LoopView};

    fn view(id: &str, short_id: &str, name: &str, repo_path: &str, state: &str) -> LoopView {
        LoopView {
            loop_entry: Some(LoopSummary {
                id: id.to_string(),
                short_id: short_id.to_string(),
                name: name.to_string(),
                repo_path: repo_path.to_string(),
                state: state.to_string(),
            }),
        }
    }

    #[test]
    fn display_id_matches_go_precedence() {
        assert_eq!(loop_display_id("abcdefghi", "short"), "short");
        assert_eq!(loop_display_id("abcdefg", ""), "abcdefg");
        assert_eq!(loop_display_id("abcdefghijklmnop", ""), "abcdefgh");
    }

    #[test]
    fn apply_filters_respects_state_and_query_and_retains_selection() {
        let mut m = LoopListModel {
            loops: vec![
                view("aaaaaaaa1111", "a1", "alpha", "/repo/a", "running"),
                view("bbbbbbbb2222", "b2", "beta", "/repo/b", "stopped"),
                LoopView { loop_entry: None },
                view("cccccccc3333", "", "gamma", "/repo/c", "running"),
            ],
            filter_text: "alp".to_string(),
            filter_state: "running".to_string(),
            filter_focus: FilterFocus::Text,
            ..Default::default()
        };

        m.apply_filters("", 0);
        assert_eq!(m.filtered.len(), 1);
        assert_eq!(
            m.filtered[0]
                .loop_entry
                .as_ref()
                .map(|entry| entry.id.as_str()),
            Some("aaaaaaaa1111")
        );

        // Selection retention by previous ID.
        m.filter_text = "".to_string();
        m.apply_filters("cccccccc3333", 0);
        assert_eq!(m.filtered.len(), 2);
        assert_eq!(m.selected_id, "cccccccc3333");
        assert_eq!(m.selected_idx, 1);
    }

    #[test]
    fn cycle_filter_status_wraps_like_go() {
        let mut m = LoopListModel::default();
        assert_eq!(m.filter_state, "all");
        m.cycle_filter_status(-1);
        assert_eq!(m.filter_state, "error");
        m.cycle_filter_status(1);
        assert_eq!(m.filter_state, "all");
    }
}

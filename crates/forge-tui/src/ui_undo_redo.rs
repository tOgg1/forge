//! Undo/redo core for UI selection, scroll, and filter state.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiStateSnapshot {
    pub selected_loop_id: String,
    pub selected_loop_index: usize,
    pub selected_run_index: usize,
    pub log_scroll: usize,
    pub filter_text: String,
    pub filter_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoRedoConfig {
    pub max_history: usize,
}

impl Default for UndoRedoConfig {
    fn default() -> Self {
        Self { max_history: 128 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoRedoState {
    config: UndoRedoConfig,
    undo_stack: Vec<UiStateSnapshot>,
    redo_stack: Vec<UiStateSnapshot>,
}

impl UndoRedoState {
    #[must_use]
    pub fn new(config: UndoRedoConfig) -> Self {
        let max_history = config.max_history.max(1);
        Self {
            config: UndoRedoConfig { max_history },
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn checkpoint(&mut self, snapshot: UiStateSnapshot) {
        if self
            .undo_stack
            .last()
            .is_some_and(|latest| latest == &snapshot)
        {
            return;
        }
        self.undo_stack.push(snapshot);
        self.redo_stack.clear();
        self.trim_history();
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo_stack.len() > 1
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> Option<UiStateSnapshot> {
        if self.undo_stack.len() <= 1 {
            return None;
        }
        let current = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        self.undo_stack.last().cloned()
    }

    pub fn redo(&mut self) -> Option<UiStateSnapshot> {
        let restored = self.redo_stack.pop()?;
        if self
            .undo_stack
            .last()
            .is_some_and(|latest| latest == &restored)
        {
            return Some(restored);
        }
        self.undo_stack.push(restored.clone());
        self.trim_history();
        Some(restored)
    }

    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    fn trim_history(&mut self) {
        if self.undo_stack.len() <= self.config.max_history {
            return;
        }
        let drop_count = self.undo_stack.len() - self.config.max_history;
        self.undo_stack.drain(0..drop_count);
    }
}

impl Default for UndoRedoState {
    fn default() -> Self {
        Self::new(UndoRedoConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{UiStateSnapshot, UndoRedoConfig, UndoRedoState};

    fn snap(id: &str, idx: usize, run_idx: usize, scroll: usize, filter: &str) -> UiStateSnapshot {
        UiStateSnapshot {
            selected_loop_id: id.to_owned(),
            selected_loop_index: idx,
            selected_run_index: run_idx,
            log_scroll: scroll,
            filter_text: filter.to_owned(),
            filter_state: "all".to_owned(),
        }
    }

    #[test]
    fn undo_restores_prior_selection_scroll_filter_snapshot() {
        let mut history = UndoRedoState::default();
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-2", 2, 3, 25, "error"));

        let restored = match history.undo() {
            Some(snapshot) => snapshot,
            None => panic!("undo snapshot should exist"),
        };
        assert_eq!(restored.selected_loop_id, "loop-1");
        assert_eq!(restored.selected_loop_index, 1);
        assert_eq!(restored.log_scroll, 0);
        assert_eq!(restored.filter_text, "");
    }

    #[test]
    fn redo_restores_snapshot_after_undo() {
        let mut history = UndoRedoState::default();
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-2", 2, 1, 8, "panic"));

        let _ = match history.undo() {
            Some(snapshot) => snapshot,
            None => panic!("undo snapshot should exist"),
        };
        let restored = match history.redo() {
            Some(snapshot) => snapshot,
            None => panic!("redo snapshot should exist"),
        };
        assert_eq!(restored.selected_loop_id, "loop-2");
        assert_eq!(restored.selected_run_index, 1);
        assert_eq!(restored.log_scroll, 8);
        assert_eq!(restored.filter_text, "panic");
    }

    #[test]
    fn checkpoint_deduplicates_identical_state() {
        let mut history = UndoRedoState::default();
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        assert_eq!(history.undo_len(), 1);
        assert!(!history.can_undo());
    }

    #[test]
    fn checkpoint_after_undo_clears_redo_stack() {
        let mut history = UndoRedoState::default();
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-2", 2, 1, 8, "panic"));
        let _ = match history.undo() {
            Some(snapshot) => snapshot,
            None => panic!("undo snapshot should exist"),
        };
        assert_eq!(history.redo_len(), 1);
        history.checkpoint(snap("loop-3", 3, 1, 2, "warn"));
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn max_history_evicts_oldest_snapshots() {
        let mut history = UndoRedoState::new(UndoRedoConfig { max_history: 3 });
        history.checkpoint(snap("loop-1", 1, 0, 0, ""));
        history.checkpoint(snap("loop-2", 2, 0, 0, ""));
        history.checkpoint(snap("loop-3", 3, 0, 0, ""));
        history.checkpoint(snap("loop-4", 4, 0, 0, ""));
        assert_eq!(history.undo_len(), 3);
        let restored = match history.undo() {
            Some(snapshot) => snapshot,
            None => panic!("undo snapshot should exist after history cap"),
        };
        assert_eq!(restored.selected_loop_id, "loop-3");
    }
}

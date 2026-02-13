//! Adaptive footer-hint ranking based on utility and recent command usage.

use std::collections::HashMap;

use crate::keymap::KeyCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub utility: u8,
    pub command: Option<KeyCommand>,
}

impl HintSpec {
    #[must_use]
    pub const fn new(
        key: &'static str,
        label: &'static str,
        utility: u8,
        command: Option<KeyCommand>,
    ) -> Self {
        Self {
            key,
            label,
            utility,
            command,
        }
    }

    #[must_use]
    pub fn render(self) -> String {
        format!("{} {}", self.key, self.label)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AdaptiveHintRanker {
    tick: u64,
    last_used: HashMap<KeyCommand, u64>,
}

impl AdaptiveHintRanker {
    pub fn record(&mut self, command: KeyCommand) {
        self.tick = self.tick.saturating_add(1);
        self.last_used.insert(command, self.tick);
    }

    #[must_use]
    pub fn rank(&self, hints: &[HintSpec], limit: usize) -> Vec<HintSpec> {
        if limit == 0 || hints.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, u32, HintSpec)> = hints
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, hint)| (idx, self.score(hint), hint))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored
            .into_iter()
            .take(limit.min(hints.len()))
            .map(|(_, _, hint)| hint)
            .collect()
    }

    fn score(&self, hint: HintSpec) -> u32 {
        let utility_score = u32::from(hint.utility) * 100;
        let recency_score = hint
            .command
            .map_or(0, |command| self.recency_bonus(command));
        utility_score + recency_score
    }

    fn recency_bonus(&self, command: KeyCommand) -> u32 {
        let Some(last_tick) = self.last_used.get(&command).copied() else {
            return 0;
        };
        let age = self.tick.saturating_sub(last_tick);
        match age {
            0 => 650,
            1..=2 => 380,
            3..=5 => 260,
            6..=10 => 160,
            11..=20 => 90,
            _ => 40,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AdaptiveHintRanker, HintSpec};
    use crate::keymap::KeyCommand;

    #[test]
    fn rank_defaults_to_utility_order_without_history() {
        let ranker = AdaptiveHintRanker::default();
        let hints = [
            HintSpec::new("a", "alpha", 9, Some(KeyCommand::OpenPalette)),
            HintSpec::new("b", "beta", 8, Some(KeyCommand::OpenFilter)),
            HintSpec::new("c", "gamma", 7, Some(KeyCommand::OpenSearch)),
        ];
        let ranked = ranker.rank(&hints, 3);
        assert_eq!(ranked[0].label, "alpha");
        assert_eq!(ranked[1].label, "beta");
        assert_eq!(ranked[2].label, "gamma");
    }

    #[test]
    fn recently_used_command_gets_promoted() {
        let mut ranker = AdaptiveHintRanker::default();
        let hints = [
            HintSpec::new("a", "alpha", 9, Some(KeyCommand::OpenPalette)),
            HintSpec::new("f", "follow", 4, Some(KeyCommand::ToggleFollow)),
            HintSpec::new("b", "beta", 8, Some(KeyCommand::OpenFilter)),
        ];
        ranker.record(KeyCommand::ToggleFollow);
        let ranked = ranker.rank(&hints, 3);
        assert_eq!(ranked[0].label, "follow");
    }

    #[test]
    fn rank_respects_limit() {
        let ranker = AdaptiveHintRanker::default();
        let hints = [
            HintSpec::new("a", "alpha", 9, Some(KeyCommand::OpenPalette)),
            HintSpec::new("b", "beta", 8, Some(KeyCommand::OpenFilter)),
            HintSpec::new("c", "gamma", 7, Some(KeyCommand::OpenSearch)),
        ];
        let ranked = ranker.rank(&hints, 2);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].label, "alpha");
        assert_eq!(ranked[1].label, "beta");
    }
}

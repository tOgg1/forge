//! Per-file agent authorship timeline with intent and revert context.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOutcome {
    Active,
    Reverted,
    PartiallyReverted,
}

impl ChangeOutcome {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ChangeOutcome::Active => "active",
            ChangeOutcome::Reverted => "reverted",
            ChangeOutcome::PartiallyReverted => "partial-revert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameTimelineEntry {
    pub timestamp: String,
    pub file_path: String,
    pub region_start_line: u32,
    pub region_end_line: u32,
    pub agent: String,
    pub task_id: String,
    pub intent: String,
    pub confidence: u8,
    pub commit_id: String,
    pub outcome: ChangeOutcome,
    pub reverted_by: Option<String>,
    pub reverted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContributionSummary {
    pub agent: String,
    pub active_regions: usize,
    pub reverted_regions: usize,
    pub partial_reverted_regions: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileBlameTimeline {
    file_path: String,
    entries: Vec<BlameTimelineEntry>,
}

impl FileBlameTimeline {
    #[must_use]
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    #[must_use]
    pub fn entries(&self) -> &[BlameTimelineEntry] {
        &self.entries
    }

    pub fn record_change(
        &mut self,
        timestamp: &str,
        file_path: &str,
        region_start_line: u32,
        region_end_line: u32,
        agent: &str,
        task_id: &str,
        intent: &str,
        confidence: u8,
        commit_id: &str,
        outcome: ChangeOutcome,
        reverted_by: Option<&str>,
        reverted_at: Option<&str>,
    ) -> Result<(), String> {
        let timestamp = normalize_required(timestamp, "timestamp")?;
        let file_path = normalize_required(file_path, "file_path")?;
        let agent = normalize_required(agent, "agent")?;
        let task_id = normalize_required(task_id, "task_id")?;
        let intent = normalize_required(intent, "intent")?;
        let commit_id = normalize_required(commit_id, "commit_id")?;

        if region_start_line == 0 {
            return Err("region_start_line must be >= 1".to_owned());
        }
        if region_end_line < region_start_line {
            return Err("region_end_line must be >= region_start_line".to_owned());
        }
        if confidence > 100 {
            return Err("confidence must be within 0..=100".to_owned());
        }

        if self.file_path.is_empty() {
            self.file_path = file_path.clone();
        } else if self.file_path != file_path {
            return Err(format!(
                "timeline is scoped to {}, cannot insert {}",
                self.file_path, file_path
            ));
        }

        let reverted_by = reverted_by.map(str::trim).filter(|v| !v.is_empty());
        let reverted_at = reverted_at.map(str::trim).filter(|v| !v.is_empty());
        if outcome == ChangeOutcome::Reverted && (reverted_by.is_none() || reverted_at.is_none()) {
            return Err("reverted entries require reverted_by and reverted_at".to_owned());
        }

        self.entries.push(BlameTimelineEntry {
            timestamp,
            file_path,
            region_start_line,
            region_end_line,
            agent,
            task_id,
            intent,
            confidence,
            commit_id,
            outcome,
            reverted_by: reverted_by.map(str::to_owned),
            reverted_at: reverted_at.map(str::to_owned),
        });
        self.entries.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.region_start_line.cmp(&b.region_start_line))
                .then_with(|| a.region_end_line.cmp(&b.region_end_line))
                .then_with(|| a.agent.cmp(&b.agent))
                .then_with(|| a.commit_id.cmp(&b.commit_id))
        });
        Ok(())
    }

    #[must_use]
    pub fn entries_touching_line(&self, line: u32) -> Vec<&BlameTimelineEntry> {
        if line == 0 {
            return Vec::new();
        }
        self.entries
            .iter()
            .filter(|entry| line >= entry.region_start_line && line <= entry.region_end_line)
            .collect()
    }

    #[must_use]
    pub fn agent_summary(&self) -> Vec<AgentContributionSummary> {
        let mut by_agent: BTreeMap<String, AgentContributionSummary> = BTreeMap::new();
        for entry in &self.entries {
            let summary = by_agent
                .entry(entry.agent.clone())
                .or_insert(AgentContributionSummary {
                    agent: entry.agent.clone(),
                    active_regions: 0,
                    reverted_regions: 0,
                    partial_reverted_regions: 0,
                });
            match entry.outcome {
                ChangeOutcome::Active => summary.active_regions += 1,
                ChangeOutcome::Reverted => summary.reverted_regions += 1,
                ChangeOutcome::PartiallyReverted => summary.partial_reverted_regions += 1,
            }
        }
        by_agent.into_values().collect()
    }

    #[must_use]
    pub fn render_rows(&self, width: usize, max_rows: usize) -> Vec<String> {
        if width == 0 || max_rows == 0 {
            return Vec::new();
        }
        let mut rows = vec![trim_to_width(
            &format!(
                "blame timeline: {} entries file:{}",
                self.entries.len(),
                if self.file_path.is_empty() {
                    "-"
                } else {
                    self.file_path.as_str()
                }
            ),
            width,
        )];
        if rows.len() >= max_rows {
            return rows;
        }

        if self.entries.is_empty() {
            rows.push(trim_to_width("no contributions logged", width));
            rows.truncate(max_rows);
            return rows;
        }

        for entry in &self.entries {
            if rows.len() >= max_rows {
                break;
            }
            let mut row = format!(
                "[{}] L{}-{} {} {} conf:{} task:{} {}",
                shorten_timestamp(&entry.timestamp),
                entry.region_start_line,
                entry.region_end_line,
                entry.agent,
                entry.outcome.label(),
                entry.confidence,
                entry.task_id,
                entry.intent
            );
            if let (Some(reverted_by), Some(reverted_at)) =
                (entry.reverted_by.as_deref(), entry.reverted_at.as_deref())
            {
                row.push_str(&format!(" reverted-by:{}@{}", reverted_by, reverted_at));
            }
            rows.push(trim_to_width(&row, width));
        }
        rows
    }
}

fn normalize_required(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value.to_owned())
    }
}

fn shorten_timestamp(ts: &str) -> String {
    let ts = ts.trim();
    if ts.len() >= 16 {
        ts[0..16].to_owned()
    } else {
        ts.to_owned()
    }
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_owned()
    } else {
        text[0..width].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangeOutcome, FileBlameTimeline};

    #[test]
    fn record_change_rejects_invalid_region_and_confidence() {
        let mut timeline = FileBlameTimeline::default();
        assert!(timeline
            .record_change(
                "2026-02-13T20:00:00Z",
                "crates/forge-tui/src/app.rs",
                0,
                10,
                "agent-a",
                "forge-hj8",
                "added panic guard",
                80,
                "a1b2c3",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_err());
        assert!(timeline
            .record_change(
                "2026-02-13T20:00:00Z",
                "crates/forge-tui/src/app.rs",
                10,
                8,
                "agent-a",
                "forge-hj8",
                "added panic guard",
                80,
                "a1b2c3",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_err());
        assert!(timeline
            .record_change(
                "2026-02-13T20:00:00Z",
                "crates/forge-tui/src/app.rs",
                10,
                12,
                "agent-a",
                "forge-hj8",
                "added panic guard",
                120,
                "a1b2c3",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_err());
    }

    #[test]
    fn record_change_reverted_requires_metadata() {
        let mut timeline = FileBlameTimeline::default();
        assert!(timeline
            .record_change(
                "2026-02-13T20:00:00Z",
                "crates/forge-tui/src/app.rs",
                40,
                48,
                "agent-a",
                "forge-hj8",
                "added temporary workaround",
                42,
                "d4e5f6",
                ChangeOutcome::Reverted,
                None,
                None,
            )
            .is_err());
    }

    #[test]
    fn record_change_sorts_entries_by_timestamp_then_region() {
        let mut timeline = FileBlameTimeline::default();
        assert!(timeline
            .record_change(
                "2026-02-13T20:05:00Z",
                "crates/forge-tui/src/app.rs",
                20,
                28,
                "agent-b",
                "forge-67w",
                "wired hotkey",
                84,
                "bbbb",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_ok());
        assert!(timeline
            .record_change(
                "2026-02-13T20:02:00Z",
                "crates/forge-tui/src/app.rs",
                4,
                12,
                "agent-a",
                "forge-hj8",
                "improved focus traversal",
                92,
                "aaaa",
                ChangeOutcome::PartiallyReverted,
                Some("agent-c"),
                Some("2026-02-13T20:30:00Z"),
            )
            .is_ok());
        assert_eq!(timeline.entries().len(), 2);
        assert_eq!(timeline.entries()[0].task_id, "forge-hj8");
        assert_eq!(timeline.entries()[1].task_id, "forge-67w");
    }

    #[test]
    fn agent_summary_counts_outcome_buckets() {
        let mut timeline = FileBlameTimeline::default();
        assert!(timeline
            .record_change(
                "2026-02-13T20:01:00Z",
                "crates/forge-tui/src/app.rs",
                1,
                4,
                "agent-a",
                "forge-hj8",
                "intent A",
                90,
                "a1",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_ok());
        assert!(timeline
            .record_change(
                "2026-02-13T20:02:00Z",
                "crates/forge-tui/src/app.rs",
                8,
                10,
                "agent-a",
                "forge-hj8",
                "intent B",
                40,
                "a2",
                ChangeOutcome::Reverted,
                Some("agent-z"),
                Some("2026-02-13T20:03:00Z"),
            )
            .is_ok());
        assert!(timeline
            .record_change(
                "2026-02-13T20:05:00Z",
                "crates/forge-tui/src/app.rs",
                12,
                14,
                "agent-b",
                "forge-67w",
                "intent C",
                70,
                "b1",
                ChangeOutcome::PartiallyReverted,
                None,
                None,
            )
            .is_ok());

        let summary = timeline.agent_summary();
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].agent, "agent-a");
        assert_eq!(summary[0].active_regions, 1);
        assert_eq!(summary[0].reverted_regions, 1);
        assert_eq!(summary[1].agent, "agent-b");
        assert_eq!(summary[1].partial_reverted_regions, 1);
    }

    #[test]
    fn render_rows_snapshot_is_deterministic() {
        let mut timeline = FileBlameTimeline::default();
        assert!(timeline
            .record_change(
                "2026-02-13T20:01:00Z",
                "crates/forge-tui/src/app.rs",
                41,
                52,
                "agent-a",
                "forge-hj8",
                "added command-gate rule",
                88,
                "abc123",
                ChangeOutcome::Reverted,
                Some("agent-b"),
                Some("2026-02-13T20:11:00Z"),
            )
            .is_ok());
        assert!(timeline
            .record_change(
                "2026-02-13T20:03:00Z",
                "crates/forge-tui/src/app.rs",
                90,
                99,
                "agent-c",
                "forge-sd4",
                "zoom layer rows",
                79,
                "def456",
                ChangeOutcome::Active,
                None,
                None,
            )
            .is_ok());
        let rows = timeline.render_rows(200, 6);
        assert_eq!(
            rows,
            vec![
                "blame timeline: 2 entries file:crates/forge-tui/src/app.rs".to_owned(),
                "[2026-02-13T20:01] L41-52 agent-a reverted conf:88 task:forge-hj8 added command-gate rule reverted-by:agent-b@2026-02-13T20:11:00Z".to_owned(),
                "[2026-02-13T20:03] L90-99 agent-c active conf:79 task:forge-sd4 zoom layer rows".to_owned(),
            ]
        );
    }
}

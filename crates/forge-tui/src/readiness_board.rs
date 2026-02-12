//! Readiness board model with priority and risk overlays.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessTaskSample {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub project_id: Option<String>,
    pub epic_id: Option<String>,
    pub owner: Option<String>,
    pub updated_at_epoch_s: i64,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessBoardFilter {
    pub project_ids: Vec<String>,
    pub epic_ids: Vec<String>,
    pub include_terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessBoardRow {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub project_id: Option<String>,
    pub epic_id: Option<String>,
    pub owner: Option<String>,
    pub readiness_score: u8,
    pub readiness_label: String,
    pub stale_risk: bool,
    pub ownership_gap: bool,
    pub blocked: bool,
    pub risk_overlays: Vec<String>,
    pub drill_down_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityOverlayCount {
    pub priority: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessBoardSummary {
    pub total_rows: usize,
    pub ready_rows: usize,
    pub stale_risk_rows: usize,
    pub ownership_gap_rows: usize,
    pub blocked_rows: usize,
    pub priority_overlays: Vec<PriorityOverlayCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessBoardView {
    pub rows: Vec<ReadinessBoardRow>,
    pub summary: ReadinessBoardSummary,
}

#[must_use]
pub fn build_readiness_board_view(
    samples: &[ReadinessTaskSample],
    now_epoch_s: i64,
    stale_after_secs: u64,
    filter: &ReadinessBoardFilter,
) -> ReadinessBoardView {
    let now_epoch_s = now_epoch_s.max(0);
    let stale_after_secs = if stale_after_secs == 0 {
        3_600
    } else {
        stale_after_secs
    };
    let project_filter = normalized_set(&filter.project_ids);
    let epic_filter = normalized_set(&filter.epic_ids);

    let mut rows = Vec::new();
    for sample in samples {
        let task_id = normalize_required(&sample.task_id);
        if task_id.is_empty() {
            continue;
        }

        let status = normalize_required(&sample.status);
        if !filter.include_terminal && is_terminal_status(&status) {
            continue;
        }

        let project_id = normalize_optional(sample.project_id.as_deref());
        let epic_id = normalize_optional(sample.epic_id.as_deref());

        if !project_filter.is_empty()
            && !project_id
                .as_deref()
                .is_some_and(|value| project_filter.contains(value))
        {
            continue;
        }
        if !epic_filter.is_empty()
            && !epic_id
                .as_deref()
                .is_some_and(|value| epic_filter.contains(value))
        {
            continue;
        }

        let has_blockers = sample
            .blocked_by
            .iter()
            .filter_map(|entry| {
                let value = normalize_required(entry);
                if value.is_empty() || value == task_id {
                    None
                } else {
                    Some(value)
                }
            })
            .next()
            .is_some();
        let (readiness_score, readiness_label, blocked) = classify_readiness(&status, has_blockers);

        let priority = normalize_priority(&sample.priority);
        let owner = normalize_optional(sample.owner.as_deref());
        let ownership_gap = owner.is_none() && !is_terminal_status(&status);
        let stale_risk = if is_terminal_status(&status) || now_epoch_s <= sample.updated_at_epoch_s
        {
            false
        } else {
            (now_epoch_s - sample.updated_at_epoch_s) as u64 >= stale_after_secs
        };

        let mut risk_overlays = vec![format!("priority:{priority}")];
        if blocked {
            risk_overlays.push("risk:blocked".to_owned());
        }
        if stale_risk {
            risk_overlays.push("risk:stale".to_owned());
        }
        if ownership_gap {
            risk_overlays.push("risk:owner-gap".to_owned());
        }

        let title = normalize_title(&sample.title, &task_id);
        rows.push(ReadinessBoardRow {
            task_id: task_id.clone(),
            title,
            status,
            priority,
            project_id,
            epic_id,
            owner,
            readiness_score,
            readiness_label: readiness_label.to_owned(),
            stale_risk,
            ownership_gap,
            blocked,
            risk_overlays,
            drill_down_link: task_drill_down_link(&task_id),
        });
    }

    rows.sort_by(|a, b| {
        b.stale_risk
            .cmp(&a.stale_risk)
            .then(b.ownership_gap.cmp(&a.ownership_gap))
            .then(a.blocked.cmp(&b.blocked))
            .then(priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
            .then(a.readiness_score.cmp(&b.readiness_score))
            .then(a.task_id.cmp(&b.task_id))
    });

    let mut priority_counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        let count = priority_counts.get(&row.priority).copied().unwrap_or(0);
        priority_counts.insert(row.priority.clone(), count + 1);
    }
    let mut priority_overlays = priority_counts
        .into_iter()
        .map(|(priority, count)| PriorityOverlayCount { priority, count })
        .collect::<Vec<_>>();
    priority_overlays.sort_by(|a, b| {
        priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then(a.priority.cmp(&b.priority))
    });

    let summary = ReadinessBoardSummary {
        total_rows: rows.len(),
        ready_rows: rows
            .iter()
            .filter(|row| row.readiness_label.as_str() == "ready")
            .count(),
        stale_risk_rows: rows.iter().filter(|row| row.stale_risk).count(),
        ownership_gap_rows: rows.iter().filter(|row| row.ownership_gap).count(),
        blocked_rows: rows.iter().filter(|row| row.blocked).count(),
        priority_overlays,
    };

    ReadinessBoardView { rows, summary }
}

fn normalize_title(title: &str, task_id: &str) -> String {
    let normalized = normalize_required(title);
    if normalized.is_empty() {
        format!("Task {task_id}")
    } else {
        normalized
    }
}

fn normalized_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .filter_map(|value| {
            let normalized = normalize_required(value);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let value = value?;
    let normalized = normalize_required(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_priority(priority: &str) -> String {
    let normalized = priority.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return "P3".to_owned();
    }
    normalized
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "P0" => 0,
        "P1" => 1,
        "P2" => 2,
        "P3" => 3,
        _ => 9,
    }
}

fn task_drill_down_link(task_id: &str) -> String {
    format!("sv task show {task_id} --json")
}

fn classify_readiness(status: &str, has_blockers: bool) -> (u8, &'static str, bool) {
    if is_terminal_status(status) {
        return (0, "terminal", false);
    }
    if is_blocked_status(status) || has_blockers {
        return (20, "blocked", true);
    }
    if is_in_progress_status(status) {
        return (60, "active", false);
    }
    if is_ready_status(status) {
        return (90, "ready", false);
    }
    (40, "unknown", false)
}

fn is_ready_status(status: &str) -> bool {
    matches!(status, "open" | "ready" | "queued" | "pending")
}

fn is_blocked_status(status: &str) -> bool {
    matches!(status, "blocked" | "waiting" | "on_hold")
}

fn is_in_progress_status(status: &str) -> bool {
    matches!(status, "in_progress" | "running" | "active" | "started")
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "done" | "closed" | "completed" | "failed" | "canceled" | "cancelled"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_readiness_board_view, ReadinessBoardFilter, ReadinessBoardRow, ReadinessBoardView,
        ReadinessTaskSample,
    };

    fn sample_tasks() -> Vec<ReadinessTaskSample> {
        vec![
            ReadinessTaskSample {
                task_id: "forge-a1".to_owned(),
                title: "Ship search".to_owned(),
                status: "ready".to_owned(),
                priority: "P1".to_owned(),
                project_id: Some("prj-alpha".to_owned()),
                epic_id: Some("epic-nav".to_owned()),
                owner: Some("alice".to_owned()),
                updated_at_epoch_s: 1_000,
                blocked_by: Vec::new(),
            },
            ReadinessTaskSample {
                task_id: "forge-a2".to_owned(),
                title: "Fix pane".to_owned(),
                status: "open".to_owned(),
                priority: "P0".to_owned(),
                project_id: Some("prj-alpha".to_owned()),
                epic_id: Some("epic-nav".to_owned()),
                owner: None,
                updated_at_epoch_s: 100,
                blocked_by: vec!["forge-root".to_owned()],
            },
            ReadinessTaskSample {
                task_id: "forge-a3".to_owned(),
                title: "Audit logs".to_owned(),
                status: "in_progress".to_owned(),
                priority: "P2".to_owned(),
                project_id: Some("prj-beta".to_owned()),
                epic_id: Some("epic-logs".to_owned()),
                owner: Some("bob".to_owned()),
                updated_at_epoch_s: 950,
                blocked_by: Vec::new(),
            },
            ReadinessTaskSample {
                task_id: "forge-a4".to_owned(),
                title: "Done item".to_owned(),
                status: "closed".to_owned(),
                priority: "P3".to_owned(),
                project_id: Some("prj-alpha".to_owned()),
                epic_id: Some("epic-nav".to_owned()),
                owner: None,
                updated_at_epoch_s: 0,
                blocked_by: Vec::new(),
            },
        ]
    }

    fn row_by_id<'a>(view: &'a ReadinessBoardView, task_id: &str) -> &'a ReadinessBoardRow {
        for row in &view.rows {
            if row.task_id == task_id {
                return row;
            }
        }
        panic!("missing row {task_id}");
    }

    #[test]
    fn filters_by_project_and_epic_case_insensitive() {
        let filter = ReadinessBoardFilter {
            project_ids: vec!["PRJ-ALPHA".to_owned()],
            epic_ids: vec!["EPIC-NAV".to_owned()],
            include_terminal: false,
        };
        let view = build_readiness_board_view(&sample_tasks(), 2_000, 300, &filter);
        let ids = view
            .rows
            .iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["forge-a2", "forge-a1"]);
    }

    #[test]
    fn stale_and_owner_gap_risk_overlays_are_derived() {
        let view = build_readiness_board_view(
            &sample_tasks(),
            2_000,
            300,
            &ReadinessBoardFilter::default(),
        );
        let row = row_by_id(&view, "forge-a2");
        assert!(row.stale_risk);
        assert!(row.ownership_gap);
        assert_eq!(
            row.risk_overlays,
            vec![
                "priority:P0".to_owned(),
                "risk:blocked".to_owned(),
                "risk:stale".to_owned(),
                "risk:owner-gap".to_owned()
            ]
        );
    }

    #[test]
    fn blocked_dependencies_reduce_readiness() {
        let view = build_readiness_board_view(
            &sample_tasks(),
            2_000,
            300,
            &ReadinessBoardFilter::default(),
        );
        let row = row_by_id(&view, "forge-a2");
        assert_eq!(row.readiness_label, "blocked");
        assert_eq!(row.readiness_score, 20);
        assert!(row.blocked);
    }

    #[test]
    fn default_stale_window_is_one_hour() {
        let view =
            build_readiness_board_view(&sample_tasks(), 4_900, 0, &ReadinessBoardFilter::default());
        let row = row_by_id(&view, "forge-a1");
        assert!(row.stale_risk);
    }

    #[test]
    fn sort_order_prefers_risk_then_priority_then_readiness() {
        let view = build_readiness_board_view(
            &sample_tasks(),
            2_000,
            300,
            &ReadinessBoardFilter::default(),
        );
        let ids = view
            .rows
            .iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["forge-a2", "forge-a1", "forge-a3"]);
    }

    #[test]
    fn summary_rolls_up_counts_and_priority_overlays() {
        let view = build_readiness_board_view(
            &sample_tasks(),
            2_000,
            300,
            &ReadinessBoardFilter::default(),
        );
        assert_eq!(view.summary.total_rows, 3);
        assert_eq!(view.summary.ready_rows, 1);
        assert_eq!(view.summary.stale_risk_rows, 3);
        assert_eq!(view.summary.ownership_gap_rows, 1);
        assert_eq!(view.summary.blocked_rows, 1);
        assert_eq!(
            view.summary
                .priority_overlays
                .iter()
                .map(|overlay| format!("{}:{}", overlay.priority, overlay.count))
                .collect::<Vec<_>>(),
            vec!["P0:1", "P1:1", "P2:1"]
        );
    }

    #[test]
    fn include_terminal_keeps_closed_rows() {
        let filter = ReadinessBoardFilter {
            include_terminal: true,
            ..ReadinessBoardFilter::default()
        };
        let view = build_readiness_board_view(&sample_tasks(), 2_000, 300, &filter);
        assert!(view.rows.iter().any(|row| row.task_id == "forge-a4"));
    }
}

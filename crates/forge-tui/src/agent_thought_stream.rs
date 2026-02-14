//! Agent thought-stream model and panel rendering helpers.
//!
//! Provides a compact decision-tree view of what an agent considered,
//! rejected, selected, and executed while working a focused loop.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThoughtEventKind {
    ContextLoaded,
    CandidateConsidered,
    CandidateRejected,
    CandidateSelected,
    ToolInvoked,
    ToolCompleted,
    BranchForked,
    BranchPruned,
    Note,
}

impl ThoughtEventKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ContextLoaded => "context-loaded",
            Self::CandidateConsidered => "candidate-considered",
            Self::CandidateRejected => "candidate-rejected",
            Self::CandidateSelected => "candidate-selected",
            Self::ToolInvoked => "tool-invoked",
            Self::ToolCompleted => "tool-completed",
            Self::BranchForked => "branch-forked",
            Self::BranchPruned => "branch-pruned",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThoughtEvent {
    pub seq: u64,
    pub epoch_s: i64,
    pub loop_id: String,
    pub agent_id: String,
    pub branch_id: String,
    pub kind: ThoughtEventKind,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThoughtBranchSummary {
    pub branch_id: String,
    pub considered_count: usize,
    pub rejected_count: usize,
    pub selected_count: usize,
    pub invoked_count: usize,
    pub last_seq: u64,
    pub last_summary: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThoughtStreamReport {
    pub loop_id: String,
    pub agent_id: String,
    pub collapsed: bool,
    pub total_events: usize,
    pub visible_events: Vec<ThoughtEvent>,
    pub branch_summaries: Vec<ThoughtBranchSummary>,
    pub selected_branch_id: Option<String>,
    pub suspected_stuck: bool,
    pub stuck_reason: String,
}

#[must_use]
pub fn build_agent_thought_stream(
    loop_id: &str,
    agent_id: &str,
    events: &[ThoughtEvent],
    collapsed: bool,
    max_visible_events: usize,
) -> ThoughtStreamReport {
    let normalized_loop_id = normalize_id(loop_id);
    let normalized_agent_id = normalize_id(agent_id);

    let mut filtered = events
        .iter()
        .cloned()
        .map(normalize_event)
        .filter(|event| {
            let loop_ok = normalized_loop_id.is_empty() || event.loop_id == normalized_loop_id;
            let agent_ok = normalized_agent_id.is_empty() || event.agent_id == normalized_agent_id;
            loop_ok && agent_ok
        })
        .collect::<Vec<_>>();
    filtered.sort_by(|a, b| a.seq.cmp(&b.seq).then(a.epoch_s.cmp(&b.epoch_s)));

    let selected_branch_id = filtered.iter().rev().find_map(|event| match event.kind {
        ThoughtEventKind::CandidateSelected | ThoughtEventKind::ToolInvoked => {
            Some(event.branch_id.clone())
        }
        _ => None,
    });

    let branch_summaries = summarize_branches(&filtered, selected_branch_id.as_deref());
    let (suspected_stuck, stuck_reason) = detect_stuck_pattern(&filtered);

    let total_events = filtered.len();
    let visible_limit = max_visible_events.max(1);
    let visible_start = total_events.saturating_sub(visible_limit);
    let visible_events = filtered[visible_start..].to_vec();

    let resolved_loop_id = if !normalized_loop_id.is_empty() {
        normalized_loop_id
    } else {
        filtered
            .last()
            .map(|event| event.loop_id.clone())
            .unwrap_or_else(|| "unknown-loop".to_owned())
    };
    let resolved_agent_id = if !normalized_agent_id.is_empty() {
        normalized_agent_id
    } else {
        filtered
            .last()
            .map(|event| event.agent_id.clone())
            .unwrap_or_else(|| "unknown-agent".to_owned())
    };

    ThoughtStreamReport {
        loop_id: resolved_loop_id,
        agent_id: resolved_agent_id,
        collapsed,
        total_events,
        visible_events,
        branch_summaries,
        selected_branch_id,
        suspected_stuck,
        stuck_reason,
    }
}

#[must_use]
pub fn render_agent_thought_stream_panel(
    report: &ThoughtStreamReport,
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut lines = Vec::with_capacity(height.min(32));
    let mode = if report.collapsed {
        "collapsed"
    } else {
        "expanded"
    };
    lines.push(fit_width(
        &format!(
            "Thought stream loop:{} agent:{} mode:{} events:{}",
            report.loop_id, report.agent_id, mode, report.total_events
        ),
        width,
    ));

    let health = if report.suspected_stuck {
        "stuck"
    } else {
        "healthy"
    };
    let selected_branch = report.selected_branch_id.as_deref().unwrap_or("none");
    lines.push(fit_width(
        &format!(
            "status:{} selected-branch:{} reason:{}",
            health, selected_branch, report.stuck_reason
        ),
        width,
    ));

    if report.total_events == 0 {
        lines.push(fit_width(
            "no thought events captured for focused loop",
            width,
        ));
        return lines.into_iter().take(height).collect();
    }

    if report.collapsed {
        if let Some(last) = report.visible_events.last() {
            lines.push(fit_width(
                &format!(
                    "latest #{} {} [{}] {}",
                    last.seq,
                    last.kind.label(),
                    last.branch_id,
                    last.summary
                ),
                width,
            ));
            if !last.detail.is_empty() {
                lines.push(fit_width(&format!("detail: {}", last.detail), width));
            }
        }
        return lines.into_iter().take(height).collect();
    }

    lines.push(fit_width("branches:", width));
    for branch in report
        .branch_summaries
        .iter()
        .take(height.saturating_sub(lines.len() + 1))
    {
        let marker = if branch.active { "*" } else { " " };
        lines.push(fit_width(
            &format!(
                "{} [{}] c:{} r:{} s:{} i:{} last:#{} {}",
                marker,
                branch.branch_id,
                branch.considered_count,
                branch.rejected_count,
                branch.selected_count,
                branch.invoked_count,
                branch.last_seq,
                branch.last_summary
            ),
            width,
        ));
    }

    if lines.len() < height {
        lines.push(fit_width("recent:", width));
    }

    for event in report
        .visible_events
        .iter()
        .rev()
        .take(height.saturating_sub(lines.len()))
    {
        lines.push(fit_width(
            &format!(
                "#{} {} [{}] {}",
                event.seq,
                event.kind.label(),
                event.branch_id,
                event.summary
            ),
            width,
        ));
    }

    lines.into_iter().take(height).collect()
}

fn summarize_branches(
    events: &[ThoughtEvent],
    selected_branch_id: Option<&str>,
) -> Vec<ThoughtBranchSummary> {
    let mut summaries = BTreeMap::<String, ThoughtBranchSummary>::new();

    for event in events {
        let branch_id = if event.branch_id.is_empty() {
            "main".to_owned()
        } else {
            event.branch_id.clone()
        };

        let summary = summaries
            .entry(branch_id.clone())
            .or_insert_with(|| ThoughtBranchSummary {
                branch_id: branch_id.clone(),
                considered_count: 0,
                rejected_count: 0,
                selected_count: 0,
                invoked_count: 0,
                last_seq: event.seq,
                last_summary: event.summary.clone(),
                active: false,
            });

        match event.kind {
            ThoughtEventKind::CandidateConsidered => {
                summary.considered_count = summary.considered_count.saturating_add(1);
            }
            ThoughtEventKind::CandidateRejected => {
                summary.considered_count = summary.considered_count.saturating_add(1);
                summary.rejected_count = summary.rejected_count.saturating_add(1);
            }
            ThoughtEventKind::CandidateSelected => {
                summary.considered_count = summary.considered_count.saturating_add(1);
                summary.selected_count = summary.selected_count.saturating_add(1);
            }
            ThoughtEventKind::ToolInvoked => {
                summary.invoked_count = summary.invoked_count.saturating_add(1);
            }
            _ => {}
        }

        summary.last_seq = event.seq;
        if !event.summary.trim().is_empty() {
            summary.last_summary = event.summary.clone();
        }
    }

    let mut out = summaries.into_values().collect::<Vec<_>>();
    for item in &mut out {
        item.active =
            selected_branch_id.is_some_and(|selected| selected.trim() == item.branch_id.trim());
    }
    out.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.last_seq.cmp(&a.last_seq))
            .then(a.branch_id.cmp(&b.branch_id))
    });
    out
}

fn detect_stuck_pattern(events: &[ThoughtEvent]) -> (bool, String) {
    if events.is_empty() {
        return (false, "no thought events".to_owned());
    }

    let window_start = events.len().saturating_sub(8);
    let window = &events[window_start..];

    let rejected = window
        .iter()
        .filter(|event| event.kind == ThoughtEventKind::CandidateRejected)
        .count();
    let selected = window
        .iter()
        .filter(|event| event.kind == ThoughtEventKind::CandidateSelected)
        .count();
    let invoked = window
        .iter()
        .filter(|event| event.kind == ThoughtEventKind::ToolInvoked)
        .count();

    if rejected >= 3 && selected == 0 && invoked == 0 {
        return (true, "candidate churn without tool execution".to_owned());
    }

    let context_only = window.len() >= 6
        && window.iter().all(|event| {
            matches!(
                event.kind,
                ThoughtEventKind::ContextLoaded
                    | ThoughtEventKind::BranchForked
                    | ThoughtEventKind::BranchPruned
                    | ThoughtEventKind::Note
            )
        });

    if context_only {
        return (
            true,
            "context updates repeating without decision point".to_owned(),
        );
    }

    (false, "decision flow healthy".to_owned())
}

fn normalize_event(mut event: ThoughtEvent) -> ThoughtEvent {
    event.loop_id = normalize_id(&event.loop_id);
    event.agent_id = normalize_id(&event.agent_id);
    event.branch_id = normalize_id(&event.branch_id);
    if event.branch_id.is_empty() {
        event.branch_id = "main".to_owned();
    }
    event.summary = normalize_text(&event.summary);
    if event.summary.is_empty() {
        event.summary = event.kind.label().to_owned();
    }
    event.detail = normalize_text(&event.detail);
    event.epoch_s = event.epoch_s.max(0);
    event
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fit_width(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_owned();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }
    let mut out: String = value.chars().take(width - 3).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        build_agent_thought_stream, render_agent_thought_stream_panel, ThoughtEvent,
        ThoughtEventKind,
    };

    fn event(
        seq: u64,
        loop_id: &str,
        agent_id: &str,
        branch_id: &str,
        kind: ThoughtEventKind,
        summary: &str,
    ) -> ThoughtEvent {
        ThoughtEvent {
            seq,
            epoch_s: 10 + seq as i64,
            loop_id: loop_id.to_owned(),
            agent_id: agent_id.to_owned(),
            branch_id: branch_id.to_owned(),
            kind,
            summary: summary.to_owned(),
            detail: String::new(),
        }
    }

    #[test]
    fn build_filters_and_sorts_events_by_focus() {
        let events = vec![
            event(
                4,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::CandidateRejected,
                "reject tool-b",
            ),
            event(
                2,
                "loop-a",
                "agent-x",
                "b1",
                ThoughtEventKind::CandidateConsidered,
                "consider tool-a",
            ),
            event(
                3,
                "loop-b",
                "agent-x",
                "b1",
                ThoughtEventKind::CandidateSelected,
                "select tool-a",
            ),
            event(
                1,
                "loop-a",
                "agent-y",
                "b1",
                ThoughtEventKind::ContextLoaded,
                "load context",
            ),
            event(
                5,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::ToolInvoked,
                "invoke rg",
            ),
        ];

        let report = build_agent_thought_stream("loop-a", "agent-x", &events, false, 10);
        assert_eq!(report.total_events, 3);
        assert_eq!(report.visible_events[0].seq, 2);
        assert_eq!(report.visible_events[1].seq, 4);
        assert_eq!(report.visible_events[2].seq, 5);
    }

    #[test]
    fn build_marks_selected_branch_and_branch_counts() {
        let events = vec![
            event(
                1,
                "loop-a",
                "agent-x",
                "b1",
                ThoughtEventKind::CandidateConsidered,
                "tool-a",
            ),
            event(
                2,
                "loop-a",
                "agent-x",
                "b1",
                ThoughtEventKind::CandidateRejected,
                "tool-a too broad",
            ),
            event(
                3,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::CandidateSelected,
                "tool-b",
            ),
            event(
                4,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::ToolInvoked,
                "invoke tool-b",
            ),
        ];

        let report = build_agent_thought_stream("loop-a", "agent-x", &events, false, 10);
        assert_eq!(report.selected_branch_id.as_deref(), Some("b2"));
        assert_eq!(report.branch_summaries[0].branch_id, "b2");
        assert!(report.branch_summaries[0].active);
        assert_eq!(report.branch_summaries[0].selected_count, 1);
        assert_eq!(report.branch_summaries[0].invoked_count, 1);
    }

    #[test]
    fn stuck_heuristic_flags_candidate_churn_without_execution() {
        let events = vec![
            event(
                1,
                "loop-a",
                "agent-x",
                "main",
                ThoughtEventKind::CandidateRejected,
                "skip tool-a",
            ),
            event(
                2,
                "loop-a",
                "agent-x",
                "main",
                ThoughtEventKind::CandidateRejected,
                "skip tool-b",
            ),
            event(
                3,
                "loop-a",
                "agent-x",
                "main",
                ThoughtEventKind::CandidateRejected,
                "skip tool-c",
            ),
            event(
                4,
                "loop-a",
                "agent-x",
                "main",
                ThoughtEventKind::Note,
                "still choosing",
            ),
        ];

        let report = build_agent_thought_stream("loop-a", "agent-x", &events, true, 5);
        assert!(report.suspected_stuck);
        assert!(report.stuck_reason.contains("churn"));
    }

    #[test]
    fn render_collapsed_shows_latest_event_and_status() {
        let events = vec![event(
            7,
            "loop-a",
            "agent-x",
            "main",
            ThoughtEventKind::ToolInvoked,
            "run ripgrep",
        )];
        let report = build_agent_thought_stream("loop-a", "agent-x", &events, true, 10);
        let lines = render_agent_thought_stream_panel(&report, 120, 5);

        assert!(lines[0].contains("mode:collapsed"));
        assert!(lines.iter().any(|line| line.contains("latest #7")));
        assert!(lines.iter().any(|line| line.contains("run ripgrep")));
    }

    #[test]
    fn render_expanded_shows_branch_and_recent_sections() {
        let events = vec![
            event(
                1,
                "loop-a",
                "agent-x",
                "b1",
                ThoughtEventKind::CandidateConsidered,
                "tool-a",
            ),
            event(
                2,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::CandidateSelected,
                "tool-b",
            ),
            event(
                3,
                "loop-a",
                "agent-x",
                "b2",
                ThoughtEventKind::ToolInvoked,
                "invoke tool-b",
            ),
        ];
        let report = build_agent_thought_stream("loop-a", "agent-x", &events, false, 10);
        let lines = render_agent_thought_stream_panel(&report, 120, 10);

        assert!(lines.iter().any(|line| line.contains("branches:")));
        assert!(lines.iter().any(|line| line.contains("recent:")));
        assert!(lines.iter().any(|line| line.contains("[b2]")));
        assert!(lines
            .iter()
            .any(|line| line.contains("candidate-selected") || line.contains("tool-invoked")));
    }

    #[test]
    fn render_empty_report_shows_no_events_message() {
        let report = build_agent_thought_stream("loop-a", "agent-x", &[], true, 10);
        let lines = render_agent_thought_stream_panel(&report, 80, 4);
        assert!(lines
            .iter()
            .any(|line| line.contains("no thought events captured")));
    }
}

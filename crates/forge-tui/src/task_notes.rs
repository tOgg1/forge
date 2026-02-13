//! Shared task notes and breadcrumb timeline model for collaboration panes.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreadcrumbKind {
    Note,
    Status,
    Command,
    Handoff,
    Risk,
}

impl BreadcrumbKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            BreadcrumbKind::Note => "note",
            BreadcrumbKind::Status => "status",
            BreadcrumbKind::Command => "command",
            BreadcrumbKind::Handoff => "handoff",
            BreadcrumbKind::Risk => "risk",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNoteEntry {
    pub timestamp: String,
    pub author: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBreadcrumb {
    pub timestamp: String,
    pub author: String,
    pub kind: BreadcrumbKind,
    pub summary: String,
    pub related_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskNotesThread {
    pub task_id: String,
    pub notes: Vec<TaskNoteEntry>,
    pub breadcrumbs: Vec<TaskBreadcrumb>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTimelineRow {
    pub timestamp: String,
    pub author: String,
    pub label: String,
    pub text: String,
    pub related_ref: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskNotesBoard {
    threads: BTreeMap<String, TaskNotesThread>,
}

impl TaskNotesBoard {
    #[must_use]
    pub fn thread(&self, task_id: &str) -> Option<&TaskNotesThread> {
        self.threads.get(task_id.trim())
    }

    pub fn add_note(
        &mut self,
        task_id: &str,
        timestamp: &str,
        author: &str,
        body: &str,
    ) -> Result<(), String> {
        let task_id = normalize_required(task_id, "task_id")?;
        let timestamp = normalize_required(timestamp, "timestamp")?;
        let author = normalize_required(author, "author")?;
        let body = normalize_required(body, "body")?;
        let thread = self
            .threads
            .entry(task_id.clone())
            .or_insert(TaskNotesThread {
                task_id,
                ..TaskNotesThread::default()
            });
        thread.notes.push(TaskNoteEntry {
            timestamp,
            author,
            body,
        });
        thread.notes.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.author.cmp(&b.author))
                .then_with(|| a.body.cmp(&b.body))
        });
        Ok(())
    }

    pub fn add_breadcrumb(
        &mut self,
        task_id: &str,
        timestamp: &str,
        author: &str,
        kind: BreadcrumbKind,
        summary: &str,
        related_ref: Option<&str>,
    ) -> Result<(), String> {
        let task_id = normalize_required(task_id, "task_id")?;
        let timestamp = normalize_required(timestamp, "timestamp")?;
        let author = normalize_required(author, "author")?;
        let summary = normalize_required(summary, "summary")?;
        let related_ref = related_ref.map(str::trim).filter(|value| !value.is_empty());
        let thread = self
            .threads
            .entry(task_id.clone())
            .or_insert(TaskNotesThread {
                task_id,
                ..TaskNotesThread::default()
            });
        thread.breadcrumbs.push(TaskBreadcrumb {
            timestamp,
            author,
            kind,
            summary,
            related_ref: related_ref.map(str::to_owned),
        });
        thread.breadcrumbs.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.author.cmp(&b.author))
                .then_with(|| a.summary.cmp(&b.summary))
        });
        Ok(())
    }

    #[must_use]
    pub fn timeline_rows(&self, task_id: &str) -> Vec<TaskTimelineRow> {
        let Some(thread) = self.thread(task_id) else {
            return Vec::new();
        };
        let mut rows: Vec<TaskTimelineRow> = thread
            .notes
            .iter()
            .map(|note| TaskTimelineRow {
                timestamp: note.timestamp.clone(),
                author: note.author.clone(),
                label: "note".to_owned(),
                text: note.body.clone(),
                related_ref: None,
            })
            .chain(thread.breadcrumbs.iter().map(|breadcrumb| TaskTimelineRow {
                timestamp: breadcrumb.timestamp.clone(),
                author: breadcrumb.author.clone(),
                label: breadcrumb.kind.label().to_owned(),
                text: breadcrumb.summary.clone(),
                related_ref: breadcrumb.related_ref.clone(),
            }))
            .collect();
        rows.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| a.author.cmp(&b.author))
                .then_with(|| a.text.cmp(&b.text))
        });
        rows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorActionKind {
    Pause,
    Resume,
    Restart,
    Kill,
    Approve,
    Reject,
    Triage,
    Custom(String),
}

impl OperatorActionKind {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            OperatorActionKind::Pause => "pause",
            OperatorActionKind::Resume => "resume",
            OperatorActionKind::Restart => "restart",
            OperatorActionKind::Kill => "kill",
            OperatorActionKind::Approve => "approve",
            OperatorActionKind::Reject => "reject",
            OperatorActionKind::Triage => "triage",
            OperatorActionKind::Custom(label) => label.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionScreenState {
    pub view: String,
    pub pane: String,
    pub selection: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionFleetState {
    pub total_loops: usize,
    pub running_loops: usize,
    pub errored_loops: usize,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorDecisionEntry {
    pub timestamp: String,
    pub operator: String,
    pub action: OperatorActionKind,
    pub task_id: Option<String>,
    pub reason: String,
    pub screen: DecisionScreenState,
    pub fleet: DecisionFleetState,
    pub active_alerts: Vec<String>,
    pub since_last_action_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperatorDecisionJournal {
    entries: Vec<OperatorDecisionEntry>,
}

impl OperatorDecisionJournal {
    #[must_use]
    pub fn entries(&self) -> &[OperatorDecisionEntry] {
        &self.entries
    }

    pub fn record_action(
        &mut self,
        timestamp: &str,
        operator: &str,
        action: OperatorActionKind,
        task_id: Option<&str>,
        reason: &str,
        screen_view: &str,
        screen_pane: &str,
        screen_selection: &str,
        fleet: DecisionFleetState,
        active_alerts: &[&str],
        since_last_action_secs: u64,
    ) -> Result<(), String> {
        let timestamp = normalize_required(timestamp, "timestamp")?;
        let operator = normalize_required(operator, "operator")?;
        let reason = normalize_required(reason, "reason")?;
        let screen_view = normalize_required(screen_view, "screen_view")?;
        let screen_pane = normalize_required(screen_pane, "screen_pane")?;
        let screen_selection = normalize_required(screen_selection, "screen_selection")?;
        let task_id = task_id.map(str::trim).filter(|value| !value.is_empty());
        let mut active_alerts = active_alerts
            .iter()
            .map(|alert| alert.trim())
            .filter(|alert| !alert.is_empty())
            .map(str::to_owned)
            .collect::<Vec<String>>();
        active_alerts.sort_unstable();

        self.entries.push(OperatorDecisionEntry {
            timestamp,
            operator,
            action,
            task_id: task_id.map(str::to_owned),
            reason,
            screen: DecisionScreenState {
                view: screen_view,
                pane: screen_pane,
                selection: screen_selection,
            },
            fleet,
            active_alerts,
            since_last_action_secs,
        });
        self.entries.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.operator.cmp(&b.operator))
                .then_with(|| a.action.label().cmp(b.action.label()))
                .then_with(|| a.reason.cmp(&b.reason))
        });
        Ok(())
    }

    #[must_use]
    pub fn export_markdown(&self, max_entries: usize) -> String {
        if self.entries.is_empty() || max_entries == 0 {
            return "# Operator Decision Journal\n\n_no entries_\n".to_owned();
        }

        let start = self.entries.len().saturating_sub(max_entries);
        let mut lines = vec![
            "# Operator Decision Journal".to_owned(),
            String::new(),
            format!("entries: {}", self.entries.len() - start),
            String::new(),
        ];
        for entry in &self.entries[start..] {
            lines.push(format!(
                "## {} · {} · {}",
                entry.timestamp,
                entry.operator,
                entry.action.label()
            ));
            lines.push(format!(
                "- task: {}",
                entry.task_id.as_deref().unwrap_or("-")
            ));
            lines.push(format!("- reason: {}", entry.reason));
            lines.push(format!(
                "- screen: {}/{} ({})",
                entry.screen.view, entry.screen.pane, entry.screen.selection
            ));
            lines.push(format!(
                "- fleet: total={} running={} errored={} queue={}",
                entry.fleet.total_loops,
                entry.fleet.running_loops,
                entry.fleet.errored_loops,
                entry.fleet.queue_depth
            ));
            lines.push(format!("- alerts: {}", render_alerts(&entry.active_alerts)));
            lines.push(format!(
                "- since_last_action_secs: {}",
                entry.since_last_action_secs
            ));
            lines.push(String::new());
        }
        lines.join("\n")
    }
}

#[must_use]
pub fn render_operator_decision_journal_pane(
    entries: &[OperatorDecisionEntry],
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut lines = vec![trim_to_width(
        &format!("decision journal: {} entries", entries.len()),
        width,
    )];
    if height == 1 {
        return lines;
    }
    if entries.is_empty() {
        lines.push(trim_to_width("no operator actions logged", width));
        return lines;
    }
    for entry in entries {
        if lines.len() >= height {
            break;
        }
        let row = format!(
            "[{}] {} {} task:{} alerts:{} +{}s",
            shorten_timestamp(&entry.timestamp),
            entry.operator,
            entry.action.label(),
            entry.task_id.as_deref().unwrap_or("-"),
            render_alerts(&entry.active_alerts),
            entry.since_last_action_secs
        );
        lines.push(trim_to_width(&row, width));
    }
    lines
}

#[must_use]
pub fn render_task_notes_pane(
    task_id: &str,
    rows: &[TaskTimelineRow],
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.push(trim_to_width(
        &format!(
            "task notes: {}  rows:{}",
            if task_id.trim().is_empty() {
                "-"
            } else {
                task_id.trim()
            },
            rows.len()
        ),
        width,
    ));
    if height == 1 {
        return lines;
    }
    if rows.is_empty() {
        lines.push(trim_to_width("no shared notes yet", width));
        return lines;
    }
    for row in rows {
        if lines.len() >= height {
            break;
        }
        let mut line = format!(
            "[{}] {} {}: {}",
            shorten_timestamp(&row.timestamp),
            row.author,
            row.label,
            row.text
        );
        if let Some(ref link) = row.related_ref {
            line.push_str(&format!(" -> {link}"));
        }
        lines.push(trim_to_width(&line, width));
    }
    lines
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

fn normalize_required(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value.to_owned())
    }
}

fn render_alerts(alerts: &[String]) -> String {
    if alerts.is_empty() {
        "none".to_owned()
    } else {
        alerts.join("|")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        render_operator_decision_journal_pane, render_task_notes_pane, BreadcrumbKind,
        DecisionFleetState, OperatorActionKind, OperatorDecisionJournal, TaskNotesBoard,
    };

    #[test]
    fn add_note_requires_non_empty_fields() {
        let mut board = TaskNotesBoard::default();
        assert!(board
            .add_note("", "2026-02-12T11:00:00Z", "agent-a", "hello")
            .is_err());
        assert!(board.add_note("forge-daf", "", "agent-a", "hello").is_err());
        assert!(board
            .add_note("forge-daf", "2026-02-12T11:00:00Z", "", "hello")
            .is_err());
        assert!(board
            .add_note("forge-daf", "2026-02-12T11:00:00Z", "agent-a", "")
            .is_err());
    }

    #[test]
    fn add_breadcrumb_requires_non_empty_fields() {
        let mut board = TaskNotesBoard::default();
        assert!(board
            .add_breadcrumb(
                "forge-daf",
                "2026-02-12T11:00:00Z",
                "agent-a",
                BreadcrumbKind::Status,
                "",
                None
            )
            .is_err());
    }

    #[test]
    fn timeline_rows_merge_and_sort_notes_and_breadcrumbs() {
        let mut board = TaskNotesBoard::default();
        assert!(board
            .add_breadcrumb(
                "forge-daf",
                "2026-02-12T11:02:00Z",
                "agent-b",
                BreadcrumbKind::Status,
                "claimed task",
                None
            )
            .is_ok());
        assert!(board
            .add_note(
                "forge-daf",
                "2026-02-12T11:01:00Z",
                "agent-a",
                "investigating logs"
            )
            .is_ok());
        let rows = board.timeline_rows("forge-daf");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].timestamp, "2026-02-12T11:01:00Z");
        assert_eq!(rows[0].author, "agent-a");
        assert_eq!(rows[0].label, "note");
        assert_eq!(rows[1].label, "status");
    }

    #[test]
    fn breadcrumb_related_ref_is_retained() {
        let mut board = TaskNotesBoard::default();
        assert!(board
            .add_breadcrumb(
                "forge-daf",
                "2026-02-12T11:03:00Z",
                "agent-c",
                BreadcrumbKind::Command,
                "reran workspace tests",
                Some("cargo test --workspace")
            )
            .is_ok());
        let rows = board.timeline_rows("forge-daf");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].related_ref.as_deref(),
            Some("cargo test --workspace")
        );
    }

    #[test]
    fn render_task_notes_pane_header_and_rows() {
        let mut board = TaskNotesBoard::default();
        assert!(board
            .add_note(
                "forge-daf",
                "2026-02-12T11:01:00Z",
                "agent-a",
                "investigating logs"
            )
            .is_ok());
        assert!(board
            .add_breadcrumb(
                "forge-daf",
                "2026-02-12T11:02:00Z",
                "agent-b",
                BreadcrumbKind::Handoff,
                "handoff to next loop",
                Some("thread-123")
            )
            .is_ok());
        let rows = board.timeline_rows("forge-daf");
        let lines = render_task_notes_pane("forge-daf", &rows, 80, 6);
        assert!(lines[0].contains("task notes: forge-daf"));
        assert!(lines.iter().any(|line| line.contains("agent-a note:")));
        assert!(lines.iter().any(|line| line.contains("agent-b handoff:")));
        assert!(lines.iter().any(|line| line.contains("-> thread-123")));
    }

    #[test]
    fn render_empty_rows_shows_hint() {
        let lines = render_task_notes_pane("forge-daf", &[], 40, 3);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("no shared notes yet"));
    }

    #[test]
    fn decision_journal_requires_non_empty_fields() {
        let mut journal = OperatorDecisionJournal::default();
        let fleet = DecisionFleetState {
            total_loops: 10,
            running_loops: 8,
            errored_loops: 2,
            queue_depth: 5,
        };
        assert!(journal
            .record_action(
                "",
                "agent-a",
                OperatorActionKind::Pause,
                None,
                "paused loop",
                "overview",
                "main",
                "loop-7",
                fleet.clone(),
                &[],
                12,
            )
            .is_err());
        assert!(journal
            .record_action(
                "2026-02-13T21:00:00Z",
                "",
                OperatorActionKind::Pause,
                None,
                "paused loop",
                "overview",
                "main",
                "loop-7",
                fleet.clone(),
                &[],
                12,
            )
            .is_err());
        assert!(journal
            .record_action(
                "2026-02-13T21:00:00Z",
                "agent-a",
                OperatorActionKind::Pause,
                None,
                "",
                "overview",
                "main",
                "loop-7",
                fleet.clone(),
                &[],
                12,
            )
            .is_err());
    }

    #[test]
    fn decision_journal_records_sorted_entries_and_context() {
        let mut journal = OperatorDecisionJournal::default();
        assert!(journal
            .record_action(
                "2026-02-13T21:04:00Z",
                "agent-b",
                OperatorActionKind::Restart,
                Some("forge-r8t"),
                "restart after stuck queue",
                "overview",
                "main",
                "loop-4",
                DecisionFleetState {
                    total_loops: 16,
                    running_loops: 12,
                    errored_loops: 4,
                    queue_depth: 38,
                },
                &["queue-high", "error-burst"],
                96,
            )
            .is_ok());
        assert!(journal
            .record_action(
                "2026-02-13T21:01:00Z",
                "agent-a",
                OperatorActionKind::Triage,
                Some("forge-r8t"),
                "triaged failing loop",
                "runs",
                "main",
                "run-884",
                DecisionFleetState {
                    total_loops: 16,
                    running_loops: 13,
                    errored_loops: 3,
                    queue_depth: 32,
                },
                &["error-burst"],
                44,
            )
            .is_ok());

        let entries = journal.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, "2026-02-13T21:01:00Z");
        assert_eq!(entries[0].action, OperatorActionKind::Triage);
        assert_eq!(entries[1].action, OperatorActionKind::Restart);
        assert_eq!(
            entries[1].active_alerts,
            vec!["error-burst".to_owned(), "queue-high".to_owned()]
        );
    }

    #[test]
    fn decision_journal_markdown_export_snapshot() {
        let mut journal = OperatorDecisionJournal::default();
        assert!(journal
            .record_action(
                "2026-02-13T21:07:00Z",
                "agent-a",
                OperatorActionKind::Approve,
                Some("forge-r8t"),
                "approved emergency restart",
                "inbox",
                "detail",
                "thread-42",
                DecisionFleetState {
                    total_loops: 11,
                    running_loops: 9,
                    errored_loops: 2,
                    queue_depth: 14,
                },
                &["pending-approval"],
                18,
            )
            .is_ok());
        let markdown = journal.export_markdown(8);
        let expected = [
            "# Operator Decision Journal",
            "",
            "entries: 1",
            "",
            "## 2026-02-13T21:07:00Z · agent-a · approve",
            "- task: forge-r8t",
            "- reason: approved emergency restart",
            "- screen: inbox/detail (thread-42)",
            "- fleet: total=11 running=9 errored=2 queue=14",
            "- alerts: pending-approval",
            "- since_last_action_secs: 18",
            "",
        ]
        .join("\n");
        assert_eq!(markdown, expected);
    }

    #[test]
    fn decision_journal_pane_rows_show_compact_action_log() {
        let mut journal = OperatorDecisionJournal::default();
        assert!(journal
            .record_action(
                "2026-02-13T21:07:00Z",
                "agent-a",
                OperatorActionKind::Kill,
                Some("forge-r8t"),
                "kill hung loop",
                "overview",
                "main",
                "loop-9",
                DecisionFleetState {
                    total_loops: 11,
                    running_loops: 8,
                    errored_loops: 3,
                    queue_depth: 21,
                },
                &["hung-loop"],
                57,
            )
            .is_ok());
        let rows = render_operator_decision_journal_pane(journal.entries(), 90, 4);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("decision journal: 1 entries"));
        assert!(rows[1].contains("agent-a kill task:forge-r8t"));
        assert!(rows[1].contains("alerts:hung-loop"));
        assert!(rows[1].contains("+57s"));
    }
}

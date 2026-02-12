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

#[cfg(test)]
mod tests {
    use super::{render_task_notes_pane, BreadcrumbKind, TaskNotesBoard};

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
}

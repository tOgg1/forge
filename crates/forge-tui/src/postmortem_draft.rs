//! Postmortem auto-draft builder and export artifact writer.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::incident_replay::{RecordedIncidentEvent, ReplayHotspot, ReplaySeverity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmortemArtifactRef {
    pub label: String,
    pub location: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmortemDraftInput {
    pub incident_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub owner: String,
    pub started_at: String,
    pub resolved_at: String,
    pub summary: String,
    pub impact: String,
    pub timeline_events: Vec<RecordedIncidentEvent>,
    pub hotspots: Vec<ReplayHotspot>,
    pub artifact_refs: Vec<PostmortemArtifactRef>,
    pub follow_up_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmortemDraftPolicy {
    pub max_timeline_rows: usize,
    pub max_hotspots: usize,
    pub max_artifacts: usize,
    pub max_follow_up_actions: usize,
}

impl Default for PostmortemDraftPolicy {
    fn default() -> Self {
        Self {
            max_timeline_rows: 14,
            max_hotspots: 5,
            max_artifacts: 8,
            max_follow_up_actions: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmortemDraft {
    pub markdown: String,
    pub text: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmortemDraftFiles {
    pub markdown_path: PathBuf,
    pub text_path: PathBuf,
    pub metadata_json_path: PathBuf,
}

#[must_use]
pub fn build_postmortem_draft(
    input: &PostmortemDraftInput,
    policy: &PostmortemDraftPolicy,
) -> PostmortemDraft {
    let timeline_rows = build_timeline_rows(input, policy.max_timeline_rows.max(1));
    let hotspot_rows = build_hotspot_rows(input, policy.max_hotspots.max(1));
    let artifact_rows = build_artifact_rows(input, policy.max_artifacts.max(1));
    let follow_up_rows = build_follow_up_rows(input, policy.max_follow_up_actions.max(1));

    let markdown = render_markdown(
        input,
        &timeline_rows,
        &hotspot_rows,
        &artifact_rows,
        &follow_up_rows,
    );
    let text = render_text(
        input,
        &timeline_rows,
        &hotspot_rows,
        &artifact_rows,
        &follow_up_rows,
    );
    let metadata_json = render_metadata_json(
        input,
        &timeline_rows,
        &hotspot_rows,
        &artifact_rows,
        &follow_up_rows,
    );

    PostmortemDraft {
        markdown,
        text,
        metadata_json,
    }
}

pub fn export_postmortem_draft(
    draft: &PostmortemDraft,
    output_dir: &Path,
    basename: &str,
) -> Result<PostmortemDraftFiles, String> {
    fs::create_dir_all(output_dir).map_err(|err| {
        format!(
            "create postmortem export directory {}: {err}",
            output_dir.display()
        )
    })?;
    let markdown_path = output_dir.join(format!("{basename}.md"));
    let text_path = output_dir.join(format!("{basename}.txt"));
    let metadata_json_path = output_dir.join(format!("{basename}.json"));

    fs::write(&markdown_path, &draft.markdown)
        .map_err(|err| format!("write {}: {err}", markdown_path.display()))?;
    fs::write(&text_path, &draft.text)
        .map_err(|err| format!("write {}: {err}", text_path.display()))?;
    fs::write(&metadata_json_path, &draft.metadata_json)
        .map_err(|err| format!("write {}: {err}", metadata_json_path.display()))?;

    Ok(PostmortemDraftFiles {
        markdown_path,
        text_path,
        metadata_json_path,
    })
}

fn build_timeline_rows(input: &PostmortemDraftInput, max_rows: usize) -> Vec<String> {
    let mut events = input.timeline_events.clone();
    events.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    dedupe_events(&mut events);

    let mut rows = events
        .iter()
        .map(|event| {
            format!(
                "- t={} kind={} severity={} source={} summary={}",
                event.timestamp_ms,
                kind_label(event.kind),
                severity_label(event.severity),
                compact_text(&event.source),
                compact_text(&event.summary)
            )
        })
        .collect::<Vec<_>>();

    truncate_with_overflow(&mut rows, max_rows);
    if rows.is_empty() {
        rows.push("- none".to_owned());
    }
    rows
}

fn build_hotspot_rows(input: &PostmortemDraftInput, max_rows: usize) -> Vec<String> {
    let mut hotspots = input.hotspots.clone();
    hotspots.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.error_count.cmp(&a.error_count))
            .then_with(|| a.start_ms.cmp(&b.start_ms))
    });

    let mut rows = hotspots
        .iter()
        .map(|hotspot| {
            format!(
                "- {}..{} score={} events={} errors={}",
                hotspot.start_ms,
                hotspot.end_ms,
                hotspot.score,
                hotspot.event_count,
                hotspot.error_count
            )
        })
        .collect::<Vec<_>>();

    truncate_with_overflow(&mut rows, max_rows);
    if rows.is_empty() {
        rows.push("- none".to_owned());
    }
    rows
}

fn build_artifact_rows(input: &PostmortemDraftInput, max_rows: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for artifact in &input.artifact_refs {
        let label = compact_text(&artifact.label);
        let location = compact_text(&artifact.location);
        let note = compact_text(&artifact.note);
        if label.is_empty() || location.is_empty() {
            continue;
        }
        let signature = format!("{label}|{location}|{note}");
        if !seen.insert(signature) {
            continue;
        }
        let row = if note.is_empty() {
            format!("- {label}: {location}")
        } else {
            format!("- {label}: {location} | {note}")
        };
        rows.push(row);
    }
    truncate_with_overflow(&mut rows, max_rows);
    if rows.is_empty() {
        rows.push("- none".to_owned());
    }
    rows
}

fn build_follow_up_rows(input: &PostmortemDraftInput, max_rows: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for action in &input.follow_up_actions {
        let action = compact_text(action);
        if action.is_empty() || !seen.insert(action.clone()) {
            continue;
        }
        rows.push(format!("- {action}"));
    }
    truncate_with_overflow(&mut rows, max_rows);
    if rows.is_empty() {
        rows.push("- none".to_owned());
    }
    rows
}

fn truncate_with_overflow(rows: &mut Vec<String>, max_rows: usize) {
    let max_rows = max_rows.max(1);
    let total = rows.len();
    rows.truncate(max_rows);
    let overflow = total.saturating_sub(max_rows);
    if overflow > 0 {
        rows.push(format!("- ... +{overflow} more"));
    }
}

fn dedupe_events(events: &mut Vec<RecordedIncidentEvent>) {
    let mut seen = BTreeSet::new();
    events.retain(|event| {
        let key = format!(
            "{}|{}|{}|{}|{}",
            event.timestamp_ms,
            kind_label(event.kind),
            severity_label(event.severity),
            compact_text(&event.source),
            compact_text(&event.summary)
        );
        seen.insert(key)
    });
}

fn render_markdown(
    input: &PostmortemDraftInput,
    timeline_rows: &[String],
    hotspot_rows: &[String],
    artifact_rows: &[String],
    follow_up_rows: &[String],
) -> String {
    let mut lines = vec![
        format!(
            "# Postmortem Draft: {} ({})",
            compact_text(&input.incident_id),
            compact_text(&input.title)
        ),
        String::new(),
        "## Incident Summary".to_owned(),
        format!("- severity: {}", compact_text(&input.severity)),
        format!("- status: {}", compact_text(&input.status)),
        format!("- owner: {}", compact_text(&input.owner)),
        format!("- started_at: {}", compact_text(&input.started_at)),
        format!("- resolved_at: {}", compact_text(&input.resolved_at)),
        format!("- summary: {}", compact_text(&input.summary)),
        format!("- impact: {}", compact_text(&input.impact)),
        String::new(),
        "## Timeline (Auto)".to_owned(),
    ];
    lines.extend_from_slice(timeline_rows);
    lines.push(String::new());
    lines.push("## Hotspots".to_owned());
    lines.extend_from_slice(hotspot_rows);
    lines.push(String::new());
    lines.push("## Key Artifacts".to_owned());
    lines.extend_from_slice(artifact_rows);
    lines.push(String::new());
    lines.push("## Follow-up Actions".to_owned());
    lines.extend_from_slice(follow_up_rows);
    lines.join("\n")
}

fn render_text(
    input: &PostmortemDraftInput,
    timeline_rows: &[String],
    hotspot_rows: &[String],
    artifact_rows: &[String],
    follow_up_rows: &[String],
) -> String {
    let mut lines = vec![
        format!(
            "Postmortem Draft: {} ({})",
            compact_text(&input.incident_id),
            compact_text(&input.title)
        ),
        format!(
            "severity={} status={} owner={}",
            compact_text(&input.severity),
            compact_text(&input.status),
            compact_text(&input.owner),
        ),
        format!(
            "started_at={} resolved_at={}",
            compact_text(&input.started_at),
            compact_text(&input.resolved_at),
        ),
        format!("summary={}", compact_text(&input.summary)),
        format!("impact={}", compact_text(&input.impact)),
        String::new(),
        "Timeline".to_owned(),
    ];
    lines.extend_from_slice(timeline_rows);
    lines.push(String::new());
    lines.push("Hotspots".to_owned());
    lines.extend_from_slice(hotspot_rows);
    lines.push(String::new());
    lines.push("Artifacts".to_owned());
    lines.extend_from_slice(artifact_rows);
    lines.push(String::new());
    lines.push("Follow-up".to_owned());
    lines.extend_from_slice(follow_up_rows);
    lines.join("\n")
}

fn render_metadata_json(
    input: &PostmortemDraftInput,
    timeline_rows: &[String],
    hotspot_rows: &[String],
    artifact_rows: &[String],
    follow_up_rows: &[String],
) -> String {
    let mut root = Map::new();
    root.insert(
        "incident_id".to_owned(),
        Value::from(compact_text(&input.incident_id)),
    );
    root.insert("title".to_owned(), Value::from(compact_text(&input.title)));
    root.insert(
        "severity".to_owned(),
        Value::from(compact_text(&input.severity)),
    );
    root.insert(
        "status".to_owned(),
        Value::from(compact_text(&input.status)),
    );
    root.insert("owner".to_owned(), Value::from(compact_text(&input.owner)));
    root.insert(
        "started_at".to_owned(),
        Value::from(compact_text(&input.started_at)),
    );
    root.insert(
        "resolved_at".to_owned(),
        Value::from(compact_text(&input.resolved_at)),
    );
    root.insert(
        "timeline_rows".to_owned(),
        Value::Array(
            timeline_rows
                .iter()
                .map(|row| Value::from(row.clone()))
                .collect(),
        ),
    );
    root.insert(
        "hotspot_rows".to_owned(),
        Value::Array(
            hotspot_rows
                .iter()
                .map(|row| Value::from(row.clone()))
                .collect(),
        ),
    );
    root.insert(
        "artifact_rows".to_owned(),
        Value::Array(
            artifact_rows
                .iter()
                .map(|row| Value::from(row.clone()))
                .collect(),
        ),
    );
    root.insert(
        "follow_up_rows".to_owned(),
        Value::Array(
            follow_up_rows
                .iter()
                .map(|row| Value::from(row.clone()))
                .collect(),
        ),
    );

    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".to_owned())
}

fn kind_label(kind: crate::incident_replay::ReplayEventKind) -> &'static str {
    match kind {
        crate::incident_replay::ReplayEventKind::Log => "log",
        crate::incident_replay::ReplayEventKind::Task => "task",
        crate::incident_replay::ReplayEventKind::System => "system",
        crate::incident_replay::ReplayEventKind::Alert => "alert",
    }
}

fn severity_label(severity: ReplaySeverity) -> &'static str {
    match severity {
        ReplaySeverity::Info => "info",
        ReplaySeverity::Warn => "warn",
        ReplaySeverity::Error => "error",
        ReplaySeverity::Critical => "critical",
    }
}

fn compact_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::incident_replay::{
        RecordedIncidentEvent, ReplayEventKind, ReplayHotspot, ReplaySeverity,
    };

    use super::{
        build_postmortem_draft, export_postmortem_draft, PostmortemArtifactRef,
        PostmortemDraftInput, PostmortemDraftPolicy,
    };

    fn sample_input() -> PostmortemDraftInput {
        PostmortemDraftInput {
            incident_id: "inc-17".to_owned(),
            title: "queue saturation".to_owned(),
            severity: "SEV1".to_owned(),
            status: "resolved".to_owned(),
            owner: "ops-a".to_owned(),
            started_at: "2026-02-13T10:00:00Z".to_owned(),
            resolved_at: "2026-02-13T10:35:00Z".to_owned(),
            summary: "queue depth spiked and retries failed".to_owned(),
            impact: "dispatch latency > 20m for 41 loops".to_owned(),
            timeline_events: vec![
                RecordedIncidentEvent {
                    event_id: "evt-2".to_owned(),
                    timestamp_ms: 2000,
                    kind: ReplayEventKind::Alert,
                    severity: ReplaySeverity::Error,
                    source: "loop-7".to_owned(),
                    summary: "timeout".to_owned(),
                },
                RecordedIncidentEvent {
                    event_id: "evt-1".to_owned(),
                    timestamp_ms: 1000,
                    kind: ReplayEventKind::System,
                    severity: ReplaySeverity::Warn,
                    source: "scheduler".to_owned(),
                    summary: "queue=50".to_owned(),
                },
                RecordedIncidentEvent {
                    event_id: "evt-3".to_owned(),
                    timestamp_ms: 2000,
                    kind: ReplayEventKind::Alert,
                    severity: ReplaySeverity::Error,
                    source: "loop-7".to_owned(),
                    summary: "timeout".to_owned(),
                },
            ],
            hotspots: vec![
                ReplayHotspot {
                    start_ms: 1000,
                    end_ms: 1400,
                    event_count: 2,
                    error_count: 1,
                    score: 3,
                },
                ReplayHotspot {
                    start_ms: 1500,
                    end_ms: 2200,
                    event_count: 4,
                    error_count: 3,
                    score: 7,
                },
            ],
            artifact_refs: vec![
                PostmortemArtifactRef {
                    label: "view export".to_owned(),
                    location: ".forge-exports/forge-view-logs-1.html".to_owned(),
                    note: "captured during incident".to_owned(),
                },
                PostmortemArtifactRef {
                    label: "view export".to_owned(),
                    location: ".forge-exports/forge-view-logs-1.html".to_owned(),
                    note: "captured during incident".to_owned(),
                },
            ],
            follow_up_actions: vec![
                "add queue cap guardrail".to_owned(),
                "add queue cap guardrail".to_owned(),
                "tune retry timeout".to_owned(),
            ],
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let mut path = env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!(
            "forge-postmortem-{label}-{}-{nanos}",
            process::id()
        ));
        path
    }

    #[test]
    fn draft_contains_sections_and_deduped_rows() {
        let policy = PostmortemDraftPolicy {
            max_timeline_rows: 5,
            max_hotspots: 5,
            max_artifacts: 5,
            max_follow_up_actions: 5,
        };
        let draft = build_postmortem_draft(&sample_input(), &policy);

        assert!(draft.markdown.contains("## Incident Summary"));
        assert!(draft.markdown.contains("## Timeline (Auto)"));
        assert!(draft.markdown.contains("## Hotspots"));
        assert!(draft.markdown.contains("## Key Artifacts"));
        assert!(draft.markdown.contains("## Follow-up Actions"));

        let timeline_matches = draft
            .markdown
            .lines()
            .filter(|line| line.contains("timeout"))
            .count();
        assert_eq!(timeline_matches, 1);

        let artifact_matches = draft
            .markdown
            .lines()
            .filter(|line| line.contains("view export"))
            .count();
        assert_eq!(artifact_matches, 1);

        let action_matches = draft
            .markdown
            .lines()
            .filter(|line| line.contains("add queue cap guardrail"))
            .count();
        assert_eq!(action_matches, 1);

        assert!(draft.metadata_json.contains("\"incident_id\": \"inc-17\""));
        assert!(draft.text.contains("Postmortem Draft: inc-17"));
    }

    #[test]
    fn draft_applies_overflow_caps() {
        let mut input = sample_input();
        input.timeline_events = (0..10)
            .map(|idx| RecordedIncidentEvent {
                event_id: format!("evt-{idx}"),
                timestamp_ms: idx as i64,
                kind: ReplayEventKind::Log,
                severity: ReplaySeverity::Info,
                source: "loop-1".to_owned(),
                summary: format!("line-{idx}"),
            })
            .collect();
        let draft = build_postmortem_draft(
            &input,
            &PostmortemDraftPolicy {
                max_timeline_rows: 3,
                max_hotspots: 1,
                max_artifacts: 1,
                max_follow_up_actions: 1,
            },
        );
        assert!(draft.markdown.contains("- ... +7 more"));
    }

    #[test]
    fn export_writes_markdown_text_and_metadata_json() {
        let draft = build_postmortem_draft(&sample_input(), &PostmortemDraftPolicy::default());
        let dir = temp_dir("files");
        let files = export_postmortem_draft(&draft, &dir, "incident-inc-17").unwrap();
        assert!(files.markdown_path.exists());
        assert!(files.text_path.exists());
        assert!(files.metadata_json_path.exists());

        let markdown = fs::read_to_string(&files.markdown_path).unwrap();
        assert!(markdown.contains("Postmortem Draft"));
        let json = fs::read_to_string(&files.metadata_json_path).unwrap();
        assert!(json.contains("\"timeline_rows\""));

        let _ = fs::remove_file(&files.markdown_path);
        let _ = fs::remove_file(&files.text_path);
        let _ = fs::remove_file(&files.metadata_json_path);
        let _ = fs::remove_dir_all(&dir);
    }
}

//! Daily summary export artifact for operator handoff.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySummaryEntry {
    pub id: String,
    pub title: String,
    pub owner: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentSummaryEntry {
    pub id: String,
    pub severity: String,
    pub status: String,
    pub summary: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DailySummaryInput {
    pub date_utc: String,
    pub completed: Vec<DailySummaryEntry>,
    pub blockers: Vec<DailySummaryEntry>,
    pub incidents: Vec<IncidentSummaryEntry>,
    pub next_actions: Vec<DailySummaryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySummaryPolicy {
    pub max_completed: usize,
    pub max_blockers: usize,
    pub max_incidents: usize,
    pub max_next_actions: usize,
}

impl Default for DailySummaryPolicy {
    fn default() -> Self {
        Self {
            max_completed: 8,
            max_blockers: 8,
            max_incidents: 6,
            max_next_actions: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySummarySection {
    pub title: String,
    pub total_items: usize,
    pub overflow_items: usize,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySummaryArtifact {
    pub headline: String,
    pub sections: Vec<DailySummarySection>,
    pub markdown: String,
    pub text: String,
}

#[must_use]
pub fn build_daily_summary_artifact(
    input: &DailySummaryInput,
    policy: &DailySummaryPolicy,
) -> DailySummaryArtifact {
    let date = normalize_date(&input.date_utc);

    let completed =
        summarize_entry_section("Completed Work", &input.completed, policy.max_completed);
    let blockers = summarize_entry_section("Blockers", &input.blockers, policy.max_blockers);
    let incidents = summarize_incident_section(&input.incidents, policy.max_incidents);
    let next_actions =
        summarize_entry_section("Next Actions", &input.next_actions, policy.max_next_actions);

    let sections = vec![completed, blockers, incidents, next_actions];
    let headline = format!("Forge Daily Summary ({date})");

    let markdown = render_markdown(&headline, &sections);
    let text = render_text(&headline, &sections);

    DailySummaryArtifact {
        headline,
        sections,
        markdown,
        text,
    }
}

fn summarize_entry_section(
    title: &str,
    entries: &[DailySummaryEntry],
    max_items: usize,
) -> DailySummarySection {
    let mut dedup = Vec::new();
    let mut seen = BTreeSet::new();

    for entry in entries {
        let id = normalize_required(&entry.id);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }

        let title_line = normalize_title(&entry.title, &id);
        let owner = normalize_optional(entry.owner.as_deref())
            .map(|owner| format!(" owner={owner}"))
            .unwrap_or_default();
        let detail = normalize_optional(entry.detail.as_deref())
            .map(|detail| format!(" | {detail}"))
            .unwrap_or_default();
        dedup.push(format!("- {id}: {title_line}{owner}{detail}"));
    }

    summarize_lines(title, dedup, max_items)
}

fn summarize_incident_section(
    entries: &[IncidentSummaryEntry],
    max_items: usize,
) -> DailySummarySection {
    let mut dedup = Vec::new();
    let mut seen = BTreeSet::new();

    let mut ordered = entries.to_vec();
    ordered.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(normalize_required(&a.id).cmp(&normalize_required(&b.id)))
    });

    for incident in ordered {
        let id = normalize_required(&incident.id);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }

        let severity = normalize_severity(&incident.severity);
        let status = normalize_required(&incident.status);
        let summary = normalize_title(&incident.summary, &id);
        let owner = normalize_optional(incident.owner.as_deref())
            .map(|owner| format!(" owner={owner}"))
            .unwrap_or_default();
        dedup.push(format!(
            "- {id}: severity={severity} status={status} {summary}{owner}"
        ));
    }

    summarize_lines("Incidents", dedup, max_items)
}

fn summarize_lines(title: &str, mut lines: Vec<String>, max_items: usize) -> DailySummarySection {
    let max_items = max_items.max(1);
    let total_items = lines.len();
    let overflow_items = total_items.saturating_sub(max_items);

    lines.truncate(max_items);
    if lines.is_empty() {
        lines.push("- none".to_owned());
    } else if overflow_items > 0 {
        lines.push(format!("- ... +{overflow_items} more"));
    }

    DailySummarySection {
        title: title.to_owned(),
        total_items,
        overflow_items,
        lines,
    }
}

fn render_markdown(headline: &str, sections: &[DailySummarySection]) -> String {
    let mut out = Vec::new();
    out.push(format!("# {headline}"));
    out.push(String::new());

    for section in sections {
        out.push(format!("## {} ({})", section.title, section.total_items));
        for line in &section.lines {
            out.push(line.clone());
        }
        out.push(String::new());
    }

    out.join("\n").trim_end().to_owned()
}

fn render_text(headline: &str, sections: &[DailySummarySection]) -> String {
    let mut out = Vec::new();
    out.push(headline.to_owned());

    for section in sections {
        out.push(format!("{} [{}]", section.title, section.total_items));
        for line in &section.lines {
            out.push(line.clone());
        }
    }

    out.join("\n")
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

fn normalize_title(title: &str, fallback_id: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        format!("item {fallback_id}")
    } else {
        trimmed.to_owned()
    }
}

fn normalize_severity(severity: &str) -> String {
    let normalized = severity.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        "UNKNOWN".to_owned()
    } else {
        normalized
    }
}

fn severity_rank(severity: &str) -> usize {
    match severity.trim().to_ascii_uppercase().as_str() {
        "SEV0" | "CRITICAL" => 0,
        "SEV1" | "HIGH" => 1,
        "SEV2" | "MEDIUM" => 2,
        "SEV3" | "LOW" => 3,
        _ => 9,
    }
}

fn normalize_date(date_utc: &str) -> String {
    let value = date_utc.trim();
    if value.is_empty() {
        "unknown-date".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{
        build_daily_summary_artifact, DailySummaryEntry, DailySummaryInput, DailySummaryPolicy,
        IncidentSummaryEntry,
    };

    fn sample_input() -> DailySummaryInput {
        DailySummaryInput {
            date_utc: "2026-02-12".to_owned(),
            completed: vec![
                DailySummaryEntry {
                    id: "forge-s1r".to_owned(),
                    title: "bulk planner".to_owned(),
                    owner: Some("agent-a".to_owned()),
                    detail: Some("shipped dry-run and rollback hints".to_owned()),
                },
                DailySummaryEntry {
                    id: "forge-p67".to_owned(),
                    title: "stale detector".to_owned(),
                    owner: Some("agent-a".to_owned()),
                    detail: None,
                },
            ],
            blockers: vec![DailySummaryEntry {
                id: "forge-2er".to_owned(),
                title: "polling gate".to_owned(),
                owner: Some("agent-a".to_owned()),
                detail: Some("waiting for completion goldens".to_owned()),
            }],
            incidents: vec![
                IncidentSummaryEntry {
                    id: "inc-002".to_owned(),
                    severity: "SEV2".to_owned(),
                    status: "open".to_owned(),
                    summary: "queue delay".to_owned(),
                    owner: Some("oncall-b".to_owned()),
                },
                IncidentSummaryEntry {
                    id: "inc-001".to_owned(),
                    severity: "SEV1".to_owned(),
                    status: "mitigating".to_owned(),
                    summary: "scheduler lag".to_owned(),
                    owner: Some("oncall-a".to_owned()),
                },
            ],
            next_actions: vec![DailySummaryEntry {
                id: "forge-8v2".to_owned(),
                title: "bookmark anchors".to_owned(),
                owner: Some("agent-a".to_owned()),
                detail: Some("claim after current close".to_owned()),
            }],
        }
    }

    #[test]
    fn builds_concise_daily_summary_with_required_sections() {
        let artifact =
            build_daily_summary_artifact(&sample_input(), &DailySummaryPolicy::default());

        assert!(artifact.headline.contains("2026-02-12"));
        assert_eq!(artifact.sections.len(), 4);
        assert!(artifact.markdown.contains("## Completed Work (2)"));
        assert!(artifact.markdown.contains("## Blockers (1)"));
        assert!(artifact.markdown.contains("## Incidents (2)"));
        assert!(artifact.markdown.contains("## Next Actions (1)"));
    }

    #[test]
    fn incidents_are_ranked_by_severity() {
        let artifact =
            build_daily_summary_artifact(&sample_input(), &DailySummaryPolicy::default());
        let incidents = artifact
            .sections
            .iter()
            .find(|section| section.title == "Incidents")
            .unwrap_or_else(|| panic!("incidents section should exist"));

        assert!(incidents.lines[0].contains("inc-001"));
        assert!(incidents.lines[1].contains("inc-002"));
    }

    #[test]
    fn duplicate_ids_are_deduped_and_overflow_is_annotated() {
        let mut input = sample_input();
        input.completed.push(input.completed[0].clone());
        input.completed.push(DailySummaryEntry {
            id: "forge-x1".to_owned(),
            title: "x1".to_owned(),
            owner: None,
            detail: None,
        });
        input.completed.push(DailySummaryEntry {
            id: "forge-x2".to_owned(),
            title: "x2".to_owned(),
            owner: None,
            detail: None,
        });

        let artifact = build_daily_summary_artifact(
            &input,
            &DailySummaryPolicy {
                max_completed: 2,
                ..DailySummaryPolicy::default()
            },
        );

        let completed = artifact
            .sections
            .iter()
            .find(|section| section.title == "Completed Work")
            .unwrap_or_else(|| panic!("completed section should exist"));

        assert_eq!(completed.total_items, 4);
        assert_eq!(completed.overflow_items, 2);
        assert!(completed.lines.iter().any(|line| line.contains("+2 more")));
    }

    #[test]
    fn empty_sections_render_none_placeholder() {
        let artifact = build_daily_summary_artifact(
            &DailySummaryInput {
                date_utc: "2026-02-12".to_owned(),
                ..DailySummaryInput::default()
            },
            &DailySummaryPolicy::default(),
        );

        for section in &artifact.sections {
            assert_eq!(section.total_items, 0);
            assert_eq!(section.lines, vec!["- none".to_owned()]);
        }
    }
}

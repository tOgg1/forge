//! Fleet selection engine with expressive filters and action previews.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetLoopRecord {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub profile: String,
    pub pool: String,
    pub state: String,
    pub tags: Vec<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FleetSelectionFilter {
    pub id_prefix: String,
    pub name_contains: String,
    pub repo_contains: String,
    pub profile: String,
    pub pool: String,
    pub states: Vec<String>,
    pub required_tags: Vec<String>,
    pub stale: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetAction {
    Stop,
    Resume,
    Kill,
    Message,
}

impl FleetAction {
    fn command(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Resume => "resume",
            Self::Kill => "kill",
            Self::Message => "msg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetActionPreview {
    pub action: FleetAction,
    pub matched_count: usize,
    pub matched_ids: Vec<String>,
    pub summary: String,
    pub command_preview: String,
}

#[must_use]
pub fn matches_id_prefix(loop_entry: &FleetLoopRecord, id_prefix: &str) -> bool {
    let prefix = normalize(id_prefix);
    if prefix.is_empty() {
        return true;
    }
    normalize(&loop_entry.id).starts_with(&prefix)
}

#[must_use]
pub fn matches_name(loop_entry: &FleetLoopRecord, name_contains: &str) -> bool {
    contains_folded(&loop_entry.name, name_contains)
}

#[must_use]
pub fn matches_repo(loop_entry: &FleetLoopRecord, repo_contains: &str) -> bool {
    contains_folded(&loop_entry.repo, repo_contains)
}

#[must_use]
pub fn matches_profile(loop_entry: &FleetLoopRecord, profile: &str) -> bool {
    equals_folded(&loop_entry.profile, profile)
}

#[must_use]
pub fn matches_pool(loop_entry: &FleetLoopRecord, pool: &str) -> bool {
    equals_folded(&loop_entry.pool, pool)
}

#[must_use]
pub fn matches_state(loop_entry: &FleetLoopRecord, states: &[String]) -> bool {
    if states.is_empty() {
        return true;
    }
    let state = normalize(&loop_entry.state);
    states.iter().any(|candidate| state == normalize(candidate))
}

#[must_use]
pub fn matches_tags(loop_entry: &FleetLoopRecord, required_tags: &[String]) -> bool {
    if required_tags.is_empty() {
        return true;
    }
    let tags: BTreeSet<String> = loop_entry.tags.iter().map(|tag| normalize(tag)).collect();
    required_tags
        .iter()
        .map(|tag| normalize(tag))
        .filter(|tag| !tag.is_empty())
        .all(|tag| tags.contains(&tag))
}

#[must_use]
pub fn matches_stale(loop_entry: &FleetLoopRecord, stale: Option<bool>) -> bool {
    match stale {
        Some(expected) => loop_entry.stale == expected,
        None => true,
    }
}

#[must_use]
pub fn matches_filter(loop_entry: &FleetLoopRecord, filter: &FleetSelectionFilter) -> bool {
    matches_id_prefix(loop_entry, &filter.id_prefix)
        && matches_name(loop_entry, &filter.name_contains)
        && matches_repo(loop_entry, &filter.repo_contains)
        && matches_profile(loop_entry, &filter.profile)
        && matches_pool(loop_entry, &filter.pool)
        && matches_state(loop_entry, &filter.states)
        && matches_tags(loop_entry, &filter.required_tags)
        && matches_stale(loop_entry, filter.stale)
}

#[must_use]
pub fn select_fleet(
    loops: &[FleetLoopRecord],
    filter: &FleetSelectionFilter,
) -> Vec<FleetLoopRecord> {
    loops
        .iter()
        .filter(|loop_entry| matches_filter(loop_entry, filter))
        .cloned()
        .collect()
}

#[must_use]
pub fn preview_fleet_action(
    action: FleetAction,
    selected: &[FleetLoopRecord],
    preview_limit: usize,
) -> FleetActionPreview {
    let mut ids: Vec<String> = selected
        .iter()
        .filter_map(|loop_entry| {
            let id = loop_entry.id.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_owned())
            }
        })
        .collect();
    ids.sort();
    ids.dedup();

    let limit = preview_limit.max(1);
    let matched_ids: Vec<String> = ids.iter().take(limit).cloned().collect();
    let hidden = ids.len().saturating_sub(matched_ids.len());

    let summary = if ids.is_empty() {
        "no loops match current selection".to_owned()
    } else if hidden == 0 {
        format!(
            "{} {} loop(s): {}",
            action.command(),
            ids.len(),
            matched_ids.join(", ")
        )
    } else {
        format!(
            "{} {} loop(s): {} (+{} more)",
            action.command(),
            ids.len(),
            matched_ids.join(", "),
            hidden
        )
    };

    let command_preview = if matched_ids.is_empty() {
        format!("forge {} --loop <id>", action.command())
    } else {
        let args = matched_ids
            .iter()
            .map(|id| format!("--loop {id}"))
            .collect::<Vec<_>>()
            .join(" ");
        if hidden == 0 {
            format!("forge {} {}", action.command(), args)
        } else {
            format!(
                "forge {} {} # +{} more target(s)",
                action.command(),
                args,
                hidden
            )
        }
    };

    FleetActionPreview {
        action,
        matched_count: ids.len(),
        matched_ids,
        summary,
        command_preview,
    }
}

fn contains_folded(value: &str, needle: &str) -> bool {
    let folded = normalize(needle);
    if folded.is_empty() {
        return true;
    }
    normalize(value).contains(&folded)
}

fn equals_folded(value: &str, expected: &str) -> bool {
    let folded_expected = normalize(expected);
    if folded_expected.is_empty() {
        return true;
    }
    normalize(value) == folded_expected
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        matches_filter, preview_fleet_action, select_fleet, FleetAction, FleetLoopRecord,
        FleetSelectionFilter,
    };

    fn sample_loops() -> Vec<FleetLoopRecord> {
        vec![
            FleetLoopRecord {
                id: "loop-aa11".to_owned(),
                name: "frontend-a".to_owned(),
                repo: "/repos/forge".to_owned(),
                profile: "codex3".to_owned(),
                pool: "nightly".to_owned(),
                state: "running".to_owned(),
                tags: vec!["ui".to_owned(), "p0".to_owned()],
                stale: false,
            },
            FleetLoopRecord {
                id: "loop-bb22".to_owned(),
                name: "backend-b".to_owned(),
                repo: "/repos/forge-db".to_owned(),
                profile: "claude".to_owned(),
                pool: "day".to_owned(),
                state: "waiting".to_owned(),
                tags: vec!["infra".to_owned()],
                stale: true,
            },
            FleetLoopRecord {
                id: "loop-cc33".to_owned(),
                name: "frontend-c".to_owned(),
                repo: "/repos/forge".to_owned(),
                profile: "codex3".to_owned(),
                pool: "day".to_owned(),
                state: "running".to_owned(),
                tags: vec!["ui".to_owned(), "qa".to_owned()],
                stale: false,
            },
        ]
    }

    #[test]
    fn selection_primitives_cover_all_filter_dimensions() {
        let loops = sample_loops();
        let filter = FleetSelectionFilter {
            id_prefix: "loop-aa".to_owned(),
            name_contains: "front".to_owned(),
            repo_contains: "forge".to_owned(),
            profile: "codex3".to_owned(),
            pool: "nightly".to_owned(),
            states: vec!["running".to_owned()],
            required_tags: vec!["ui".to_owned(), "p0".to_owned()],
            stale: Some(false),
        };

        let selected = select_fleet(&loops, &filter);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "loop-aa11");
    }

    #[test]
    fn state_tag_and_stale_filters_are_case_insensitive() {
        let loops = sample_loops();
        let filter = FleetSelectionFilter {
            states: vec!["WAITING".to_owned()],
            required_tags: vec!["INFRA".to_owned()],
            stale: Some(true),
            ..FleetSelectionFilter::default()
        };

        let selected = select_fleet(&loops, &filter);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "loop-bb22");
    }

    #[test]
    fn matches_filter_requires_all_requested_tags() {
        let loops = sample_loops();
        let filter = FleetSelectionFilter {
            required_tags: vec!["ui".to_owned(), "missing".to_owned()],
            ..FleetSelectionFilter::default()
        };
        assert!(!matches_filter(&loops[0], &filter));
    }

    #[test]
    fn action_preview_surfaces_deterministic_targets_and_command() {
        let loops = sample_loops();
        let preview = preview_fleet_action(FleetAction::Stop, &loops, 2);

        assert_eq!(preview.matched_count, 3);
        assert_eq!(
            preview.matched_ids,
            vec!["loop-aa11".to_owned(), "loop-bb22".to_owned()]
        );
        assert!(preview.summary.contains("(+1 more)"));
        assert!(preview
            .command_preview
            .contains("forge stop --loop loop-aa11 --loop loop-bb22"));
    }

    #[test]
    fn action_preview_handles_empty_selection() {
        let preview = preview_fleet_action(FleetAction::Message, &[], 3);
        assert_eq!(preview.matched_count, 0);
        assert_eq!(preview.summary, "no loops match current selection");
        assert_eq!(preview.command_preview, "forge msg --loop <id>");
    }
}

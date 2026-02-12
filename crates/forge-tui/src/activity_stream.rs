//! Activity stream model with agent/repo/task filters and jump links.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActivityKind {
    Claim,
    Progress,
    Blocked,
    Closed,
    Comment,
    System,
}

impl ActivityKind {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Progress => "progress",
            Self::Blocked => "blocked",
            Self::Closed => "closed",
            Self::Comment => "comment",
            Self::System => "system",
        }
    }

    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claim" => Some(Self::Claim),
            "progress" => Some(Self::Progress),
            "blocked" => Some(Self::Blocked),
            "closed" => Some(Self::Closed),
            "comment" => Some(Self::Comment),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEvent {
    pub event_id: String,
    pub timestamp_epoch_s: i64,
    pub kind: ActivityKind,
    pub summary: String,
    pub agent_id: Option<String>,
    pub repo: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivityFilter {
    pub agent_ids: Vec<String>,
    pub repos: Vec<String>,
    pub task_ids: Vec<String>,
    pub kinds: Vec<ActivityKind>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityJumpLink {
    pub label: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    pub event_id: String,
    pub timestamp_epoch_s: i64,
    pub kind: ActivityKind,
    pub summary: String,
    pub agent_id: Option<String>,
    pub repo: Option<String>,
    pub task_id: Option<String>,
    pub jump_links: Vec<ActivityJumpLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActivitySnapshot {
    pub rows: Vec<ActivityRow>,
    pub total_events: usize,
    pub matched_events: usize,
    pub dropped_events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityStream {
    max_events: usize,
    events: Vec<ActivityEvent>,
}

impl ActivityStream {
    #[must_use]
    pub fn new(max_events: usize) -> Self {
        Self {
            max_events: max_events.max(1),
            events: Vec::new(),
        }
    }

    pub fn push(&mut self, event: ActivityEvent) -> Result<(), String> {
        let normalized = normalize_event(event)?;
        self.events.push(normalized);
        self.events.sort_by(|a, b| {
            b.timestamp_epoch_s
                .cmp(&a.timestamp_epoch_s)
                .then(a.event_id.cmp(&b.event_id))
        });
        self.events.truncate(self.max_events);
        Ok(())
    }

    #[must_use]
    pub fn events(&self) -> &[ActivityEvent] {
        &self.events
    }

    #[must_use]
    pub fn snapshot(&self, filter: &ActivityFilter, limit: usize) -> ActivitySnapshot {
        build_snapshot(&self.events, filter, limit)
    }

    #[must_use]
    pub fn tail_since(
        &self,
        last_seen_epoch_s: i64,
        filter: &ActivityFilter,
        limit: usize,
    ) -> ActivitySnapshot {
        let filtered = self
            .events
            .iter()
            .filter(|event| event.timestamp_epoch_s > last_seen_epoch_s)
            .cloned()
            .collect::<Vec<_>>();
        build_snapshot(&filtered, filter, limit)
    }
}

fn build_snapshot(
    events: &[ActivityEvent],
    filter: &ActivityFilter,
    limit: usize,
) -> ActivitySnapshot {
    let limit = if limit == 0 { 50 } else { limit };
    let agent_ids = normalize_set(&filter.agent_ids);
    let repos = normalize_set(&filter.repos);
    let task_ids = normalize_set(&filter.task_ids);
    let text = normalize_optional(filter.text.as_deref());
    let kinds = filter.kinds.iter().copied().collect::<BTreeSet<_>>();

    let mut rows = events
        .iter()
        .filter(|event| {
            matches_filter(
                event,
                &agent_ids,
                &repos,
                &task_ids,
                &kinds,
                text.as_deref(),
            )
        })
        .map(build_row)
        .collect::<Vec<_>>();
    let matched_events = rows.len();
    rows.truncate(limit);

    ActivitySnapshot {
        rows,
        total_events: events.len(),
        matched_events,
        dropped_events: matched_events.saturating_sub(limit),
    }
}

fn matches_filter(
    event: &ActivityEvent,
    agent_ids: &BTreeSet<String>,
    repos: &BTreeSet<String>,
    task_ids: &BTreeSet<String>,
    kinds: &BTreeSet<ActivityKind>,
    text: Option<&str>,
) -> bool {
    if !agent_ids.is_empty()
        && !event
            .agent_id
            .as_deref()
            .map(normalize_required)
            .is_some_and(|agent| agent_ids.contains(&agent))
    {
        return false;
    }

    if !repos.is_empty()
        && !event
            .repo
            .as_deref()
            .map(normalize_required)
            .is_some_and(|repo| repos.contains(&repo))
    {
        return false;
    }

    if !task_ids.is_empty()
        && !event
            .task_id
            .as_deref()
            .map(normalize_required)
            .is_some_and(|task| task_ids.contains(&task))
    {
        return false;
    }

    if !kinds.is_empty() && !kinds.contains(&event.kind) {
        return false;
    }

    if let Some(query) = text {
        let haystack = format!(
            "{} {} {} {}",
            event.summary,
            event.agent_id.as_deref().unwrap_or_default(),
            event.repo.as_deref().unwrap_or_default(),
            event.task_id.as_deref().unwrap_or_default(),
        )
        .to_ascii_lowercase();
        if !haystack.contains(query) {
            return false;
        }
    }

    true
}

fn build_row(event: &ActivityEvent) -> ActivityRow {
    ActivityRow {
        event_id: event.event_id.clone(),
        timestamp_epoch_s: event.timestamp_epoch_s,
        kind: event.kind,
        summary: event.summary.clone(),
        agent_id: event.agent_id.clone(),
        repo: event.repo.clone(),
        task_id: event.task_id.clone(),
        jump_links: build_jump_links(event),
    }
}

fn build_jump_links(event: &ActivityEvent) -> Vec<ActivityJumpLink> {
    let task_id = event.task_id.as_deref();
    let repo = event.repo.as_deref();
    let agent_id = event.agent_id.as_deref();

    let mut links = Vec::new();
    let mut seen_targets = BTreeSet::new();

    if let Some(task_id) = task_id {
        push_link(
            &mut links,
            &mut seen_targets,
            "task",
            format!("task:{task_id}"),
        );
        push_link(
            &mut links,
            &mut seen_targets,
            "logs",
            format!("logs:task:{task_id}"),
        );
    }

    match (repo, agent_id) {
        (Some(repo), Some(agent_id)) => push_link(
            &mut links,
            &mut seen_targets,
            "logs",
            format!("logs:repo:{repo}:agent:{agent_id}"),
        ),
        (Some(repo), None) => push_link(
            &mut links,
            &mut seen_targets,
            "logs",
            format!("logs:repo:{repo}"),
        ),
        (None, Some(agent_id)) => push_link(
            &mut links,
            &mut seen_targets,
            "logs",
            format!("logs:agent:{agent_id}"),
        ),
        (None, None) => {}
    }

    links
}

fn push_link(
    links: &mut Vec<ActivityJumpLink>,
    seen_targets: &mut BTreeSet<String>,
    label: &str,
    target: String,
) {
    if seen_targets.insert(target.clone()) {
        links.push(ActivityJumpLink {
            label: label.to_owned(),
            target,
        });
    }
}

fn normalize_event(mut event: ActivityEvent) -> Result<ActivityEvent, String> {
    event.event_id = normalize_required(&event.event_id);
    event.summary = event.summary.trim().to_owned();
    event.agent_id = normalize_optional(event.agent_id.as_deref());
    event.repo = normalize_optional(event.repo.as_deref());
    event.task_id = normalize_optional(event.task_id.as_deref());
    event.timestamp_epoch_s = event.timestamp_epoch_s.max(0);

    if event.event_id.is_empty() {
        return Err("activity event id cannot be empty".to_owned());
    }
    if event.summary.is_empty() {
        return Err(format!(
            "activity event '{}' summary cannot be empty",
            event.event_id
        ));
    }

    Ok(event)
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(normalize_required)
        .filter(|value| !value.is_empty())
}

fn normalize_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(String::as_str)
        .map(normalize_required)
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ActivityEvent, ActivityFilter, ActivityKind, ActivityStream};

    fn sample_event(
        event_id: &str,
        timestamp_epoch_s: i64,
        kind: ActivityKind,
        agent_id: Option<&str>,
        repo: Option<&str>,
        task_id: Option<&str>,
        summary: &str,
    ) -> ActivityEvent {
        ActivityEvent {
            event_id: event_id.to_owned(),
            timestamp_epoch_s,
            kind,
            summary: summary.to_owned(),
            agent_id: agent_id.map(str::to_owned),
            repo: repo.map(str::to_owned),
            task_id: task_id.map(str::to_owned),
        }
    }

    #[test]
    fn push_rejects_empty_id_and_summary() {
        let mut stream = ActivityStream::new(4);
        let missing_id = sample_event("", 10, ActivityKind::Claim, None, None, None, "ok");
        assert!(stream.push(missing_id).is_err());

        let missing_summary =
            sample_event("evt-1", 10, ActivityKind::Claim, None, None, None, "   ");
        assert!(stream.push(missing_summary).is_err());
    }

    #[test]
    fn push_sorts_desc_and_prunes_to_capacity() {
        let mut stream = ActivityStream::new(2);
        let _ = stream.push(sample_event(
            "evt-1",
            10,
            ActivityKind::Claim,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "claimed task",
        ));
        let _ = stream.push(sample_event(
            "evt-2",
            30,
            ActivityKind::Progress,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "progress update",
        ));
        let _ = stream.push(sample_event(
            "evt-3",
            20,
            ActivityKind::Comment,
            Some("agent-b"),
            Some("forge"),
            Some("forge-abc"),
            "commented",
        ));

        let ids = stream
            .events()
            .iter()
            .map(|event| event.event_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["evt-2", "evt-3"]);
    }

    #[test]
    fn snapshot_filters_by_agent_repo_task_kind_and_text() {
        let mut stream = ActivityStream::new(8);
        let _ = stream.push(sample_event(
            "evt-1",
            100,
            ActivityKind::Claim,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "claim activity stream task",
        ));
        let _ = stream.push(sample_event(
            "evt-2",
            101,
            ActivityKind::Progress,
            Some("agent-b"),
            Some("fmail"),
            Some("forge-8v2"),
            "progress bookmarks",
        ));
        let _ = stream.push(sample_event(
            "evt-3",
            102,
            ActivityKind::Blocked,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "blocked on validation",
        ));

        let filter = ActivityFilter {
            agent_ids: vec!["agent-a".to_owned()],
            repos: vec!["forge".to_owned()],
            task_ids: vec!["forge-vz1".to_owned()],
            kinds: vec![ActivityKind::Blocked],
            text: Some("validation".to_owned()),
        };
        let snapshot = stream.snapshot(&filter, 10);
        assert_eq!(snapshot.matched_events, 1);
        assert_eq!(snapshot.rows[0].event_id, "evt-3");
    }

    #[test]
    fn snapshot_rows_include_jump_links_for_task_and_logs() {
        let mut stream = ActivityStream::new(2);
        let _ = stream.push(sample_event(
            "evt-1",
            100,
            ActivityKind::Claim,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "claim",
        ));

        let snapshot = stream.snapshot(&ActivityFilter::default(), 10);
        let links = snapshot.rows[0]
            .jump_links
            .iter()
            .map(|link| format!("{}={}", link.label, link.target))
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            vec![
                "task=task:forge-vz1",
                "logs=logs:task:forge-vz1",
                "logs=logs:repo:forge:agent:agent-a",
            ]
        );
    }

    #[test]
    fn tail_since_returns_only_newer_events() {
        let mut stream = ActivityStream::new(4);
        let _ = stream.push(sample_event(
            "evt-1",
            50,
            ActivityKind::Claim,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "claim",
        ));
        let _ = stream.push(sample_event(
            "evt-2",
            60,
            ActivityKind::Progress,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "progress",
        ));
        let _ = stream.push(sample_event(
            "evt-3",
            70,
            ActivityKind::Closed,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "closed",
        ));

        let snapshot = stream.tail_since(60, &ActivityFilter::default(), 10);
        let ids = snapshot
            .rows
            .iter()
            .map(|row| row.event_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["evt-3"]);
    }

    #[test]
    fn snapshot_order_is_deterministic_for_same_timestamp() {
        let mut stream = ActivityStream::new(4);
        let _ = stream.push(sample_event(
            "evt-b",
            200,
            ActivityKind::Comment,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "second id",
        ));
        let _ = stream.push(sample_event(
            "evt-a",
            200,
            ActivityKind::Comment,
            Some("agent-a"),
            Some("forge"),
            Some("forge-vz1"),
            "first id",
        ));

        let snapshot = stream.snapshot(&ActivityFilter::default(), 10);
        let ids = snapshot
            .rows
            .iter()
            .map(|row| row.event_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["evt-a", "evt-b"]);
    }
}

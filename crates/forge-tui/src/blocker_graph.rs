//! Blocker dependency graph and bottleneck extraction for task analytics views.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDependencySample {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEdge {
    pub blocker_task_id: String,
    pub blocked_task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionableTaskLink {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub drill_down_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockerGraphNode {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub direct_blocked_count: usize,
    pub transitive_blocked_count: usize,
    pub impact_score: usize,
    pub actionable: bool,
    pub drill_down_link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BottleneckView {
    pub task_id: String,
    pub impact_score: usize,
    pub direct_blocked_count: usize,
    pub transitive_blocked_count: usize,
    pub actionable_tasks: Vec<ActionableTaskLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockerGraphView {
    pub nodes: Vec<BlockerGraphNode>,
    pub edges: Vec<DependencyEdge>,
    pub bottlenecks: Vec<BottleneckView>,
}

#[must_use]
pub fn build_blocker_graph_view(
    samples: &[TaskDependencySample],
    bottleneck_limit: usize,
) -> BlockerGraphView {
    let mut tasks_by_id = build_tasks_by_id(samples);
    let edges = build_edges(&tasks_by_id);

    for (blocker, blocked) in &edges {
        tasks_by_id
            .entry(blocker.clone())
            .or_insert_with(|| InternalTask::placeholder(blocker));
        tasks_by_id
            .entry(blocked.clone())
            .or_insert_with(|| InternalTask::placeholder(blocked));
    }

    for (blocker, blocked) in &edges {
        if let Some(task) = tasks_by_id.get_mut(blocker) {
            task.blocks.insert(blocked.clone());
        }
        if let Some(task) = tasks_by_id.get_mut(blocked) {
            task.blocked_by.insert(blocker.clone());
        }
    }

    let outgoing = build_adjacency_map(&edges, true);
    let incoming = build_adjacency_map(&edges, false);

    let mut transitive_by_id: BTreeMap<String, usize> = BTreeMap::new();
    let mut actionable_by_id: BTreeMap<String, bool> = BTreeMap::new();
    for task_id in tasks_by_id.keys() {
        transitive_by_id.insert(task_id.clone(), reachable_nodes(task_id, &outgoing).len());
        actionable_by_id.insert(
            task_id.clone(),
            is_task_actionable(task_id, &tasks_by_id, &incoming),
        );
    }

    let mut nodes = Vec::new();
    for (task_id, task) in &tasks_by_id {
        let direct_blocked_count = outgoing.get(task_id).map_or(0, BTreeSet::len);
        let transitive_blocked_count = transitive_by_id.get(task_id).copied().unwrap_or(0);
        let impact_score = direct_blocked_count + transitive_blocked_count;
        nodes.push(BlockerGraphNode {
            task_id: task_id.clone(),
            title: task.title.clone(),
            status: task.status.clone(),
            blocked_by: task.blocked_by.iter().cloned().collect(),
            blocks: task.blocks.iter().cloned().collect(),
            direct_blocked_count,
            transitive_blocked_count,
            impact_score,
            actionable: actionable_by_id.get(task_id).copied().unwrap_or(false),
            drill_down_link: task_drill_down_link(task_id),
        });
    }
    nodes.sort_by(|a, b| {
        b.impact_score
            .cmp(&a.impact_score)
            .then(a.task_id.cmp(&b.task_id))
    });

    let edge_rows = edges
        .iter()
        .map(|(blocker, blocked)| DependencyEdge {
            blocker_task_id: blocker.clone(),
            blocked_task_id: blocked.clone(),
        })
        .collect::<Vec<_>>();

    let node_by_id = nodes
        .iter()
        .map(|node| (node.task_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();

    let bottlenecks = nodes
        .iter()
        .filter(|node| node.impact_score > 0)
        .take(bottleneck_limit)
        .map(|node| BottleneckView {
            task_id: node.task_id.clone(),
            impact_score: node.impact_score,
            direct_blocked_count: node.direct_blocked_count,
            transitive_blocked_count: node.transitive_blocked_count,
            actionable_tasks: actionable_task_links_for_bottleneck(
                &node.task_id,
                &actionable_by_id,
                &incoming,
                &outgoing,
                &node_by_id,
            ),
        })
        .collect();

    BlockerGraphView {
        nodes,
        edges: edge_rows,
        bottlenecks,
    }
}

#[must_use]
pub fn render_dependency_rows(view: &BlockerGraphView) -> Vec<String> {
    if view.edges.is_empty() {
        return vec!["(no dependencies)".to_owned()];
    }
    view.edges
        .iter()
        .map(|edge| format!("{} -> {}", edge.blocker_task_id, edge.blocked_task_id))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalTask {
    title: String,
    status: String,
    blocked_by: BTreeSet<String>,
    blocks: BTreeSet<String>,
}

impl InternalTask {
    fn placeholder(task_id: &str) -> Self {
        Self {
            title: format!("Task {}", task_id),
            status: "unknown".to_owned(),
            blocked_by: BTreeSet::new(),
            blocks: BTreeSet::new(),
        }
    }
}

fn build_tasks_by_id(samples: &[TaskDependencySample]) -> BTreeMap<String, InternalTask> {
    let mut tasks_by_id: BTreeMap<String, InternalTask> = BTreeMap::new();
    for sample in samples {
        let task_id = normalize_id(&sample.task_id);
        if task_id.is_empty() {
            continue;
        }
        let title = normalize_title(&sample.title, &task_id);
        let status = normalize_status(&sample.status);
        let entry = tasks_by_id
            .entry(task_id.clone())
            .or_insert_with(|| InternalTask {
                title: title.clone(),
                status: status.clone(),
                blocked_by: BTreeSet::new(),
                blocks: BTreeSet::new(),
            });

        if entry.title.starts_with("Task ") && !title.starts_with("Task ") {
            entry.title = title;
        }
        if entry.status == "unknown" && status != "unknown" {
            entry.status = status;
        }

        for blocker in &sample.blocked_by {
            let normalized = normalize_id(blocker);
            if normalized.is_empty() || normalized == task_id {
                continue;
            }
            entry.blocked_by.insert(normalized);
        }
        for blocked in &sample.blocks {
            let normalized = normalize_id(blocked);
            if normalized.is_empty() || normalized == task_id {
                continue;
            }
            entry.blocks.insert(normalized);
        }
    }
    tasks_by_id
}

fn build_edges(tasks_by_id: &BTreeMap<String, InternalTask>) -> BTreeSet<(String, String)> {
    let mut edges = BTreeSet::new();
    for (task_id, task) in tasks_by_id {
        for blocker in &task.blocked_by {
            edges.insert((blocker.clone(), task_id.clone()));
        }
        for blocked in &task.blocks {
            edges.insert((task_id.clone(), blocked.clone()));
        }
    }
    edges
}

fn build_adjacency_map(
    edges: &BTreeSet<(String, String)>,
    outgoing: bool,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (blocker, blocked) in edges {
        if outgoing {
            map.entry(blocker.clone())
                .or_default()
                .insert(blocked.clone());
        } else {
            map.entry(blocked.clone())
                .or_default()
                .insert(blocker.clone());
        }
    }
    map
}

fn reachable_nodes(
    start: &str,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    if let Some(neighbors) = adjacency.get(start) {
        for neighbor in neighbors {
            queue.push_back(neighbor.clone());
        }
    }
    while let Some(next) = queue.pop_front() {
        if next == start || !visited.insert(next.clone()) {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&next) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }
    visited
}

fn is_task_actionable(
    task_id: &str,
    tasks_by_id: &BTreeMap<String, InternalTask>,
    incoming: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let Some(task) = tasks_by_id.get(task_id) else {
        return false;
    };
    if task.status == "unknown" || is_terminal_status(&task.status) {
        return false;
    }
    match incoming.get(task_id) {
        None => true,
        Some(blockers) => blockers.iter().all(|blocker_id| {
            tasks_by_id
                .get(blocker_id)
                .is_some_and(|blocker| is_terminal_status(&blocker.status))
        }),
    }
}

fn actionable_task_links_for_bottleneck(
    task_id: &str,
    actionable_by_id: &BTreeMap<String, bool>,
    incoming: &BTreeMap<String, BTreeSet<String>>,
    outgoing: &BTreeMap<String, BTreeSet<String>>,
    node_by_id: &BTreeMap<&str, &BlockerGraphNode>,
) -> Vec<ActionableTaskLink> {
    let mut candidates = BTreeSet::new();

    if actionable_by_id.get(task_id).copied().unwrap_or(false) {
        candidates.insert(task_id.to_owned());
    } else {
        for upstream in reachable_nodes(task_id, incoming) {
            if actionable_by_id.get(&upstream).copied().unwrap_or(false) {
                candidates.insert(upstream);
            }
        }
        if candidates.is_empty() {
            for downstream in reachable_nodes(task_id, outgoing) {
                if actionable_by_id.get(&downstream).copied().unwrap_or(false) {
                    candidates.insert(downstream);
                }
            }
        }
    }

    candidates
        .into_iter()
        .take(6)
        .filter_map(|candidate_id| {
            let node = node_by_id.get(candidate_id.as_str())?;
            Some(ActionableTaskLink {
                task_id: node.task_id.clone(),
                title: node.title.clone(),
                status: node.status.clone(),
                drill_down_link: node.drill_down_link.clone(),
            })
        })
        .collect()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_owned()
}

fn normalize_title(value: &str, task_id: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        format!("Task {}", task_id)
    } else {
        trimmed.to_owned()
    }
}

fn normalize_status(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn task_drill_down_link(task_id: &str) -> String {
    format!("sv task show {} --json", task_id)
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "done" | "closed" | "complete" | "completed" | "success" | "resolved" | "canceled"
    )
}

#[cfg(test)]
mod tests {
    use super::{build_blocker_graph_view, render_dependency_rows, TaskDependencySample};

    #[test]
    fn ranks_bottlenecks_by_transitive_impact() {
        let samples = vec![
            TaskDependencySample {
                task_id: "task-a".to_owned(),
                title: "A".to_owned(),
                status: "in_progress".to_owned(),
                blocked_by: vec![],
                blocks: vec!["task-b".to_owned(), "task-c".to_owned()],
            },
            TaskDependencySample {
                task_id: "task-b".to_owned(),
                title: "B".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-a".to_owned()],
                blocks: vec!["task-d".to_owned()],
            },
            TaskDependencySample {
                task_id: "task-c".to_owned(),
                title: "C".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-a".to_owned()],
                blocks: vec![],
            },
            TaskDependencySample {
                task_id: "task-d".to_owned(),
                title: "D".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-b".to_owned()],
                blocks: vec![],
            },
        ];

        let view = build_blocker_graph_view(&samples, 2);
        assert_eq!(view.bottlenecks.len(), 2);
        assert_eq!(view.bottlenecks[0].task_id, "task-a");
        assert_eq!(view.bottlenecks[0].direct_blocked_count, 2);
        assert_eq!(view.bottlenecks[0].transitive_blocked_count, 3);
        assert_eq!(view.bottlenecks[0].impact_score, 5);
        assert_eq!(view.bottlenecks[0].actionable_tasks.len(), 1);
        assert_eq!(view.bottlenecks[0].actionable_tasks[0].task_id, "task-a");
        assert_eq!(
            view.bottlenecks[0].actionable_tasks[0].drill_down_link,
            "sv task show task-a --json"
        );
    }

    #[test]
    fn uses_upstream_actionable_tasks_for_blocked_bottleneck() {
        let samples = vec![
            TaskDependencySample {
                task_id: "task-root".to_owned(),
                title: "Root".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec![],
                blocks: vec!["task-a".to_owned()],
            },
            TaskDependencySample {
                task_id: "task-a".to_owned(),
                title: "A".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-root".to_owned()],
                blocks: vec!["task-b".to_owned(), "task-c".to_owned()],
            },
            TaskDependencySample {
                task_id: "task-b".to_owned(),
                title: "B".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-a".to_owned()],
                blocks: vec![],
            },
            TaskDependencySample {
                task_id: "task-c".to_owned(),
                title: "C".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-a".to_owned()],
                blocks: vec![],
            },
        ];

        let view = build_blocker_graph_view(&samples, 1);
        assert_eq!(view.bottlenecks[0].task_id, "task-a");
        let actionable_ids = view.bottlenecks[0]
            .actionable_tasks
            .iter()
            .map(|task| task.task_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actionable_ids, vec!["task-root"]);
    }

    #[test]
    fn handles_dependency_cycles_without_infinite_walks() {
        let samples = vec![
            TaskDependencySample {
                task_id: "task-a".to_owned(),
                title: "A".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-b".to_owned()],
                blocks: vec!["task-b".to_owned()],
            },
            TaskDependencySample {
                task_id: "task-b".to_owned(),
                title: "B".to_owned(),
                status: "open".to_owned(),
                blocked_by: vec!["task-a".to_owned()],
                blocks: vec![],
            },
        ];

        let view = build_blocker_graph_view(&samples, 2);
        assert_eq!(view.nodes.len(), 2);
        assert_eq!(view.bottlenecks.len(), 2);
        assert!(view
            .nodes
            .iter()
            .all(|node| node.transitive_blocked_count <= 1));
    }

    #[test]
    fn render_rows_are_stable() {
        let samples = vec![TaskDependencySample {
            task_id: "task-a".to_owned(),
            title: "A".to_owned(),
            status: "open".to_owned(),
            blocked_by: vec![],
            blocks: vec!["task-b".to_owned()],
        }];
        let view = build_blocker_graph_view(&samples, 1);
        assert_eq!(render_dependency_rows(&view), vec!["task-a -> task-b"]);
    }
}

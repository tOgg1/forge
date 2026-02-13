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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencySample {
    pub loop_id: String,
    pub state: String,
    pub depends_on: Vec<String>,
    pub subtree_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencyNode {
    pub loop_id: String,
    pub state: String,
    pub depth: usize,
    pub upstream: Vec<String>,
    pub downstream: Vec<String>,
    pub critical_path_index: Option<usize>,
    pub collapsed_member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencyEdge {
    pub from_loop_id: String,
    pub to_loop_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedLoopSubtree {
    pub subtree_key: String,
    pub representative_loop_id: String,
    pub hidden_loop_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailurePropagationPath {
    pub failing_loop_id: String,
    pub impacted_loop_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopDependencyGraphView {
    pub nodes: Vec<LoopDependencyNode>,
    pub edges: Vec<LoopDependencyEdge>,
    pub critical_path: Vec<String>,
    pub propagation_paths: Vec<FailurePropagationPath>,
    pub collapsed_subtrees: Vec<CollapsedLoopSubtree>,
}

#[must_use]
pub fn build_loop_dependency_graph_view(
    samples: &[LoopDependencySample],
    focus_failing_loop: Option<&str>,
    collapsed_subtree_keys: &BTreeSet<String>,
) -> LoopDependencyGraphView {
    let mut all_ids = BTreeSet::new();
    let mut states_by_id: BTreeMap<String, String> = BTreeMap::new();
    let mut subtree_by_id: BTreeMap<String, String> = BTreeMap::new();
    let mut incoming_raw: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut outgoing_raw: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for sample in samples {
        let loop_id = normalize_id(&sample.loop_id);
        if loop_id.is_empty() {
            continue;
        }
        all_ids.insert(loop_id.clone());
        states_by_id.insert(loop_id.clone(), normalize_status(&sample.state));

        if let Some(key) = sample.subtree_key.as_ref() {
            let normalized_key = key.trim();
            if !normalized_key.is_empty() {
                subtree_by_id.insert(loop_id.clone(), normalized_key.to_owned());
            }
        }

        for upstream in &sample.depends_on {
            let upstream_id = normalize_id(upstream);
            if upstream_id.is_empty() || upstream_id == loop_id {
                continue;
            }
            all_ids.insert(upstream_id.clone());
            incoming_raw
                .entry(loop_id.clone())
                .or_default()
                .insert(upstream_id.clone());
            outgoing_raw
                .entry(upstream_id)
                .or_default()
                .insert(loop_id.clone());
        }
    }

    for loop_id in &all_ids {
        states_by_id
            .entry(loop_id.clone())
            .or_insert_with(|| "unknown".to_owned());
    }

    let (collapse_map, mut collapsed_subtrees) =
        build_loop_collapse_map(&subtree_by_id, collapsed_subtree_keys);
    collapsed_subtrees.sort_by(|a, b| a.subtree_key.cmp(&b.subtree_key));

    let mut collapsed_members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for loop_id in &all_ids {
        let representative = collapse_loop_id(loop_id, &collapse_map);
        collapsed_members
            .entry(representative)
            .or_default()
            .insert(loop_id.clone());
    }

    let mut edge_set: BTreeSet<(String, String)> = BTreeSet::new();
    for (upstream_id, downstream_ids) in &outgoing_raw {
        for downstream_id in downstream_ids {
            let from = collapse_loop_id(upstream_id, &collapse_map);
            let to = collapse_loop_id(downstream_id, &collapse_map);
            if from.is_empty() || to.is_empty() || from == to {
                continue;
            }
            edge_set.insert((from, to));
        }
    }

    let mut visible_ids = BTreeSet::new();
    for loop_id in &all_ids {
        visible_ids.insert(collapse_loop_id(loop_id, &collapse_map));
    }
    for (from, to) in &edge_set {
        visible_ids.insert(from.clone());
        visible_ids.insert(to.clone());
    }

    let outgoing = build_loop_adjacency_map(&edge_set, true);
    let incoming = build_loop_adjacency_map(&edge_set, false);
    let depths = compute_loop_depths(&visible_ids, &incoming, &outgoing);
    let states_by_visible = aggregate_collapsed_states(&collapsed_members, &states_by_id);

    let mut critical_focus = focus_failing_loop
        .map(|loop_id| collapse_loop_id(&normalize_id(loop_id), &collapse_map))
        .filter(|loop_id| visible_ids.contains(loop_id));
    if critical_focus.is_none() {
        critical_focus = default_failing_loop_focus(&visible_ids, &states_by_visible, &depths);
    }
    let critical_path = critical_focus.as_deref().map_or_else(Vec::new, |loop_id| {
        longest_upstream_path(loop_id, &incoming)
    });

    let critical_index = critical_path
        .iter()
        .enumerate()
        .map(|(idx, loop_id)| (loop_id.clone(), idx))
        .collect::<BTreeMap<_, _>>();

    let mut nodes = Vec::new();
    for loop_id in &visible_ids {
        let upstream = incoming
            .get(loop_id)
            .map_or_else(Vec::new, |neighbors| neighbors.iter().cloned().collect());
        let downstream = outgoing
            .get(loop_id)
            .map_or_else(Vec::new, |neighbors| neighbors.iter().cloned().collect());
        let collapsed_member_count = collapsed_members
            .get(loop_id)
            .map_or(0usize, |members| members.len().saturating_sub(1));
        nodes.push(LoopDependencyNode {
            loop_id: loop_id.clone(),
            state: states_by_visible
                .get(loop_id)
                .cloned()
                .unwrap_or_else(|| "unknown".to_owned()),
            depth: depths.get(loop_id).copied().unwrap_or(0),
            upstream,
            downstream,
            critical_path_index: critical_index.get(loop_id).copied(),
            collapsed_member_count,
        });
    }
    nodes.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.loop_id.cmp(&b.loop_id)));

    let mut edges = edge_set
        .into_iter()
        .map(|(from, to)| LoopDependencyEdge {
            from_loop_id: from,
            to_loop_id: to,
        })
        .collect::<Vec<_>>();
    edges.sort_by(|a, b| {
        a.from_loop_id
            .cmp(&b.from_loop_id)
            .then(a.to_loop_id.cmp(&b.to_loop_id))
    });

    let mut propagation_paths = Vec::new();
    for loop_id in &visible_ids {
        let is_failure = states_by_visible
            .get(loop_id)
            .is_some_and(|state| is_failure_status(state));
        if !is_failure {
            continue;
        }
        let impacted = reachable_nodes(loop_id, &outgoing)
            .into_iter()
            .collect::<Vec<_>>();
        propagation_paths.push(FailurePropagationPath {
            failing_loop_id: loop_id.clone(),
            impacted_loop_ids: impacted,
        });
    }
    propagation_paths.sort_by(|a, b| a.failing_loop_id.cmp(&b.failing_loop_id));

    LoopDependencyGraphView {
        nodes,
        edges,
        critical_path,
        propagation_paths,
        collapsed_subtrees,
    }
}

#[must_use]
pub fn render_loop_dependency_rows(view: &LoopDependencyGraphView, max_rows: usize) -> Vec<String> {
    if max_rows == 0 {
        return Vec::new();
    }
    let mut rows = Vec::new();
    if !view.critical_path.is_empty() {
        rows.push(format!("critical: {}", view.critical_path.join(" -> ")));
    }
    for summary in &view.collapsed_subtrees {
        rows.push(format!(
            "collapsed[{key}] rep={rep} hidden={hidden}",
            key = summary.subtree_key,
            rep = summary.representative_loop_id,
            hidden = summary.hidden_loop_count
        ));
    }
    for path in &view.propagation_paths {
        if path.impacted_loop_ids.is_empty() {
            rows.push(format!("propagate {} -> (none)", path.failing_loop_id));
        } else {
            rows.push(format!(
                "propagate {} -> {}",
                path.failing_loop_id,
                path.impacted_loop_ids.join(", ")
            ));
        }
    }
    for edge in &view.edges {
        rows.push(format!("{} -> {}", edge.from_loop_id, edge.to_loop_id));
    }
    if rows.is_empty() {
        rows.push("(no loop dependencies)".to_owned());
    }
    rows.truncate(max_rows);
    rows
}

fn collapse_loop_id(loop_id: &str, collapse_map: &BTreeMap<String, String>) -> String {
    collapse_map
        .get(loop_id)
        .cloned()
        .unwrap_or_else(|| loop_id.to_owned())
}

fn build_loop_collapse_map(
    subtree_by_id: &BTreeMap<String, String>,
    collapsed_subtree_keys: &BTreeSet<String>,
) -> (BTreeMap<String, String>, Vec<CollapsedLoopSubtree>) {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (loop_id, key) in subtree_by_id {
        grouped
            .entry(key.clone())
            .or_default()
            .push(loop_id.clone());
    }

    let mut collapse_map = BTreeMap::new();
    let mut summaries = Vec::new();
    for key in collapsed_subtree_keys {
        let Some(mut members) = grouped.get(key).cloned() else {
            continue;
        };
        members.sort();
        let Some(representative) = members.first().cloned() else {
            continue;
        };
        let hidden_loop_count = members.len().saturating_sub(1);
        if hidden_loop_count == 0 {
            continue;
        }
        for loop_id in members.iter().skip(1) {
            collapse_map.insert(loop_id.clone(), representative.clone());
        }
        summaries.push(CollapsedLoopSubtree {
            subtree_key: key.clone(),
            representative_loop_id: representative,
            hidden_loop_count,
        });
    }
    (collapse_map, summaries)
}

fn build_loop_adjacency_map(
    edges: &BTreeSet<(String, String)>,
    outgoing: bool,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (from, to) in edges {
        if outgoing {
            map.entry(from.clone()).or_default().insert(to.clone());
        } else {
            map.entry(to.clone()).or_default().insert(from.clone());
        }
    }
    map
}

fn compute_loop_depths(
    loop_ids: &BTreeSet<String>,
    incoming: &BTreeMap<String, BTreeSet<String>>,
    outgoing: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, usize> {
    let mut depths = BTreeMap::new();
    let mut queue = VecDeque::new();

    let mut roots = loop_ids
        .iter()
        .filter(|loop_id| incoming.get(*loop_id).is_none_or(BTreeSet::is_empty))
        .cloned()
        .collect::<Vec<_>>();
    if roots.is_empty() {
        roots = loop_ids.iter().cloned().collect();
    }

    roots.sort();
    for root in roots {
        if depths.insert(root.clone(), 0).is_none() {
            queue.push_back(root);
        }
    }

    while let Some(current) = queue.pop_front() {
        let current_depth = depths.get(&current).copied().unwrap_or(0usize);
        if let Some(neighbors) = outgoing.get(&current) {
            for neighbor in neighbors {
                let next_depth = current_depth.saturating_add(1);
                let should_update = depths
                    .get(neighbor)
                    .is_none_or(|existing| next_depth < *existing);
                if should_update {
                    depths.insert(neighbor.clone(), next_depth);
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    for loop_id in loop_ids {
        depths.entry(loop_id.clone()).or_insert(0);
    }
    depths
}

fn aggregate_collapsed_states(
    collapsed_members: &BTreeMap<String, BTreeSet<String>>,
    states_by_id: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut states = BTreeMap::new();
    for (visible_id, members) in collapsed_members {
        let mut best: Option<(usize, String)> = None;
        for member_id in members {
            let state = states_by_id
                .get(member_id)
                .cloned()
                .unwrap_or_else(|| "unknown".to_owned());
            let rank = state_severity_rank(&state);
            if best.as_ref().is_none_or(|(best_rank, best_state)| {
                rank > *best_rank || (rank == *best_rank && state < *best_state)
            }) {
                best = Some((rank, state));
            }
        }
        states.insert(
            visible_id.clone(),
            best.map_or_else(|| "unknown".to_owned(), |(_, state)| state),
        );
    }
    states
}

fn default_failing_loop_focus(
    loop_ids: &BTreeSet<String>,
    states_by_visible: &BTreeMap<String, String>,
    depths: &BTreeMap<String, usize>,
) -> Option<String> {
    let mut failures = loop_ids
        .iter()
        .filter(|loop_id| {
            states_by_visible
                .get(*loop_id)
                .is_some_and(|state| is_failure_status(state))
        })
        .cloned()
        .collect::<Vec<_>>();
    failures.sort_by(|a, b| {
        depths
            .get(b)
            .copied()
            .unwrap_or(0)
            .cmp(&depths.get(a).copied().unwrap_or(0))
            .then(a.cmp(b))
    });
    failures.into_iter().next()
}

fn longest_upstream_path(
    loop_id: &str,
    incoming: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    fn dfs(
        node: &str,
        incoming: &BTreeMap<String, BTreeSet<String>>,
        stack: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, Vec<String>>,
    ) -> Vec<String> {
        if let Some(cached) = memo.get(node) {
            return cached.clone();
        }
        if !stack.insert(node.to_owned()) {
            return vec![node.to_owned()];
        }

        let mut best_path = vec![node.to_owned()];
        if let Some(parents) = incoming.get(node) {
            for parent in parents {
                let mut candidate = dfs(parent, incoming, stack, memo);
                candidate.push(node.to_owned());
                if candidate.len() > best_path.len()
                    || (candidate.len() == best_path.len() && candidate < best_path)
                {
                    best_path = candidate;
                }
            }
        }
        stack.remove(node);
        memo.insert(node.to_owned(), best_path.clone());
        best_path
    }

    let mut stack = BTreeSet::new();
    let mut memo = BTreeMap::new();
    dfs(loop_id, incoming, &mut stack, &mut memo)
}

fn state_severity_rank(state: &str) -> usize {
    if is_failure_status(state) {
        return 5;
    }
    if is_warning_status(state) {
        return 4;
    }
    match state {
        "running" => 3,
        "waiting" | "sleeping" => 2,
        "stopped" | "paused" => 1,
        _ => 0,
    }
}

fn is_failure_status(state: &str) -> bool {
    matches!(
        state,
        "error" | "failed" | "panic" | "fatal" | "crashed" | "degraded-error"
    )
}

fn is_warning_status(state: &str) -> bool {
    matches!(
        state,
        "warn" | "warning" | "degraded" | "retrying" | "throttled"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_blocker_graph_view, build_loop_dependency_graph_view, render_dependency_rows,
        render_loop_dependency_rows, LoopDependencySample, TaskDependencySample,
    };
    use std::collections::BTreeSet;

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

    #[test]
    fn loop_dependency_graph_reports_critical_path_for_failing_loop() {
        let samples = vec![
            LoopDependencySample {
                loop_id: "loop-ingest".to_owned(),
                state: "running".to_owned(),
                depends_on: Vec::new(),
                subtree_key: Some("cluster-a".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-transform".to_owned(),
                state: "warning".to_owned(),
                depends_on: vec!["loop-ingest".to_owned()],
                subtree_key: Some("cluster-a".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-serve".to_owned(),
                state: "error".to_owned(),
                depends_on: vec!["loop-transform".to_owned()],
                subtree_key: Some("cluster-a".to_owned()),
            },
        ];
        let view = build_loop_dependency_graph_view(&samples, Some("loop-serve"), &BTreeSet::new());
        assert_eq!(
            view.critical_path,
            vec![
                "loop-ingest".to_owned(),
                "loop-transform".to_owned(),
                "loop-serve".to_owned()
            ]
        );
        assert_eq!(view.propagation_paths.len(), 1);
        assert_eq!(view.propagation_paths[0].failing_loop_id, "loop-serve");
    }

    #[test]
    fn loop_dependency_graph_tracks_failure_propagation_downstream() {
        let samples = vec![
            LoopDependencySample {
                loop_id: "loop-upstream".to_owned(),
                state: "failed".to_owned(),
                depends_on: Vec::new(),
                subtree_key: None,
            },
            LoopDependencySample {
                loop_id: "loop-mid".to_owned(),
                state: "running".to_owned(),
                depends_on: vec!["loop-upstream".to_owned()],
                subtree_key: None,
            },
            LoopDependencySample {
                loop_id: "loop-downstream".to_owned(),
                state: "running".to_owned(),
                depends_on: vec!["loop-mid".to_owned()],
                subtree_key: None,
            },
        ];
        let view = build_loop_dependency_graph_view(&samples, None, &BTreeSet::new());
        assert_eq!(view.propagation_paths.len(), 1);
        assert_eq!(
            view.propagation_paths[0].impacted_loop_ids,
            vec!["loop-downstream".to_owned(), "loop-mid".to_owned()]
        );
    }

    #[test]
    fn loop_dependency_graph_collapses_subtrees_with_summary() {
        let samples = vec![
            LoopDependencySample {
                loop_id: "loop-a1".to_owned(),
                state: "running".to_owned(),
                depends_on: Vec::new(),
                subtree_key: Some("pool-a".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-a2".to_owned(),
                state: "error".to_owned(),
                depends_on: vec!["loop-a1".to_owned()],
                subtree_key: Some("pool-a".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-b1".to_owned(),
                state: "running".to_owned(),
                depends_on: vec!["loop-a2".to_owned()],
                subtree_key: Some("pool-b".to_owned()),
            },
        ];
        let mut collapsed = BTreeSet::new();
        collapsed.insert("pool-a".to_owned());
        let view = build_loop_dependency_graph_view(&samples, None, &collapsed);
        assert_eq!(view.collapsed_subtrees.len(), 1);
        assert_eq!(view.collapsed_subtrees[0].subtree_key, "pool-a");
        assert_eq!(view.collapsed_subtrees[0].hidden_loop_count, 1);
        assert!(view
            .nodes
            .iter()
            .any(|node| node.loop_id == "loop-a1" && node.collapsed_member_count == 1));
        assert!(view.nodes.iter().all(|node| node.loop_id != "loop-a2"));
    }

    #[test]
    fn render_loop_dependency_rows_snapshot() {
        let samples = vec![
            LoopDependencySample {
                loop_id: "loop-source".to_owned(),
                state: "running".to_owned(),
                depends_on: Vec::new(),
                subtree_key: Some("core".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-core".to_owned(),
                state: "error".to_owned(),
                depends_on: vec!["loop-source".to_owned()],
                subtree_key: Some("core".to_owned()),
            },
            LoopDependencySample {
                loop_id: "loop-api".to_owned(),
                state: "running".to_owned(),
                depends_on: vec!["loop-core".to_owned()],
                subtree_key: Some("edge".to_owned()),
            },
        ];
        let view = build_loop_dependency_graph_view(&samples, Some("loop-core"), &BTreeSet::new());
        let rows = render_loop_dependency_rows(&view, 8);
        assert_eq!(
            rows,
            vec![
                "critical: loop-source -> loop-core".to_owned(),
                "propagate loop-core -> loop-api".to_owned(),
                "loop-core -> loop-api".to_owned(),
                "loop-source -> loop-core".to_owned(),
            ]
        );
    }
}

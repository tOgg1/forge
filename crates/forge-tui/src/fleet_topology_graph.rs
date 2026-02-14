//! Fleet topology graph model for loop relationship exploration.

use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetLoopTopologyInput {
    pub loop_id: String,
    pub label: String,
    pub state: String,
    pub queue_depth: usize,
    pub last_error: String,
    pub repo_path: String,
    pub owned_crates: Vec<String>,
    pub touched_files: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetFmailEdgeInput {
    pub from_loop_id: String,
    pub to_loop_id: String,
    pub message_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopologyEdgeKind {
    SharedFiles,
    Fmail,
    Dependency,
    CrateOwnership,
}

impl TopologyEdgeKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SharedFiles => "shared-files",
            Self::Fmail => "fmail",
            Self::Dependency => "dependency",
            Self::CrateOwnership => "crate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyNodeHealth {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

impl TopologyNodeHealth {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "ok",
            Self::Warning => "warn",
            Self::Critical => "crit",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTopologyNode {
    pub loop_id: String,
    pub label: String,
    pub health: TopologyNodeHealth,
    pub cluster_key: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTopologyEdge {
    pub from_loop_id: String,
    pub to_loop_id: String,
    pub kind: TopologyEdgeKind,
    pub intensity: u16,
    pub interaction_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTopologyCluster {
    pub key: String,
    pub label: String,
    pub loop_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FleetTopologyGraph {
    pub nodes: Vec<FleetTopologyNode>,
    pub edges: Vec<FleetTopologyEdge>,
    pub clusters: Vec<FleetTopologyCluster>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTopologyConfig {
    pub shared_file_weight: u16,
    pub fmail_weight: u16,
    pub dependency_weight: u16,
    pub crate_weight: u16,
    pub queue_pressure_threshold: usize,
    pub max_intensity: u16,
}

impl Default for FleetTopologyConfig {
    fn default() -> Self {
        Self {
            shared_file_weight: 10,
            fmail_weight: 4,
            dependency_weight: 12,
            crate_weight: 8,
            queue_pressure_threshold: 12,
            max_intensity: 99,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyEdgeFilter {
    pub enabled: BTreeSet<TopologyEdgeKind>,
}

impl Default for TopologyEdgeFilter {
    fn default() -> Self {
        let mut enabled = BTreeSet::new();
        enabled.insert(TopologyEdgeKind::SharedFiles);
        enabled.insert(TopologyEdgeKind::Fmail);
        enabled.insert(TopologyEdgeKind::Dependency);
        enabled.insert(TopologyEdgeKind::CrateOwnership);
        Self { enabled }
    }
}

impl TopologyEdgeFilter {
    #[must_use]
    pub fn allows(&self, kind: TopologyEdgeKind) -> bool {
        self.enabled.contains(&kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyFocusNeighbor {
    pub neighbor_loop_id: String,
    pub kind: TopologyEdgeKind,
    pub intensity: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyFocusView {
    pub focus_loop_id: String,
    pub total_intensity: u32,
    pub neighbors: Vec<TopologyFocusNeighbor>,
}

#[must_use]
pub fn build_fleet_topology_graph(
    loops: &[FleetLoopTopologyInput],
    fmail_edges: &[FleetFmailEdgeInput],
    config: &FleetTopologyConfig,
) -> FleetTopologyGraph {
    let normalized_loops = normalize_loop_inputs(loops);
    if normalized_loops.is_empty() {
        return FleetTopologyGraph::default();
    }
    let known_ids = normalized_loops
        .iter()
        .map(|entry| entry.loop_id.clone())
        .collect::<BTreeSet<String>>();

    let clusters = build_clusters(&normalized_loops);
    let cluster_lookup = clusters
        .iter()
        .flat_map(|cluster| {
            cluster
                .loop_ids
                .iter()
                .map(|loop_id| (loop_id.clone(), cluster.key.clone()))
        })
        .collect::<HashMap<_, _>>();

    let grid_width = ((normalized_loops.len() as f64).sqrt().ceil() as i32).max(1);
    let mut nodes = Vec::with_capacity(normalized_loops.len());
    for (index, loop_input) in normalized_loops.iter().enumerate() {
        let index = index as i32;
        let x = (index % grid_width) * 8 + 2;
        let y = (index / grid_width) * 4 + 1;
        nodes.push(FleetTopologyNode {
            loop_id: loop_input.loop_id.clone(),
            label: normalize_label(&loop_input.label, &loop_input.loop_id),
            health: classify_health(loop_input, config),
            cluster_key: cluster_lookup
                .get(&loop_input.loop_id)
                .cloned()
                .unwrap_or_else(|| "cluster:unassigned".to_owned()),
            x,
            y,
        });
    }

    let mut edges = Vec::new();
    edges.extend(build_shared_file_edges(&normalized_loops, config));
    edges.extend(build_fmail_edges(fmail_edges, &known_ids, config));
    edges.extend(build_dependency_edges(
        &normalized_loops,
        &known_ids,
        config,
    ));
    edges.extend(build_crate_ownership_edges(&clusters, config));

    edges.sort_by(|a, b| {
        b.intensity
            .cmp(&a.intensity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.from_loop_id.cmp(&b.from_loop_id))
            .then_with(|| a.to_loop_id.cmp(&b.to_loop_id))
    });

    FleetTopologyGraph {
        nodes,
        edges,
        clusters,
    }
}

#[must_use]
pub fn filter_fleet_topology_edges(
    graph: &FleetTopologyGraph,
    filter: &TopologyEdgeFilter,
) -> Vec<FleetTopologyEdge> {
    graph
        .edges
        .iter()
        .filter(|edge| filter.allows(edge.kind))
        .cloned()
        .collect()
}

#[must_use]
pub fn focus_loop_topology(
    graph: &FleetTopologyGraph,
    loop_id: &str,
    filter: &TopologyEdgeFilter,
) -> Option<TopologyFocusView> {
    let focus_id = normalize(loop_id);
    if focus_id.is_empty() {
        return None;
    }
    if graph.nodes.iter().all(|node| node.loop_id != focus_id) {
        return None;
    }

    let mut neighbors = Vec::new();
    let mut total_intensity = 0u32;
    for edge in graph.edges.iter().filter(|edge| filter.allows(edge.kind)) {
        let neighbor = if edge.from_loop_id == focus_id {
            Some(edge.to_loop_id.clone())
        } else if edge.to_loop_id == focus_id {
            Some(edge.from_loop_id.clone())
        } else {
            None
        };
        if let Some(neighbor_loop_id) = neighbor {
            neighbors.push(TopologyFocusNeighbor {
                neighbor_loop_id,
                kind: edge.kind,
                intensity: edge.intensity,
            });
            total_intensity = total_intensity.saturating_add(edge.intensity as u32);
        }
    }
    neighbors.sort_by(|a, b| {
        b.intensity
            .cmp(&a.intensity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.neighbor_loop_id.cmp(&b.neighbor_loop_id))
    });

    Some(TopologyFocusView {
        focus_loop_id: focus_id,
        total_intensity,
        neighbors,
    })
}

pub fn drag_topology_node(
    graph: &mut FleetTopologyGraph,
    loop_id: &str,
    x: i32,
    y: i32,
    max_x: i32,
    max_y: i32,
) -> bool {
    let id = normalize(loop_id);
    if id.is_empty() {
        return false;
    }
    let Some(node) = graph.nodes.iter_mut().find(|entry| entry.loop_id == id) else {
        return false;
    };

    let clamped_max_x = max_x.max(0);
    let clamped_max_y = max_y.max(0);
    node.x = x.clamp(0, clamped_max_x);
    node.y = y.clamp(0, clamped_max_y);
    true
}

#[must_use]
pub fn render_fleet_topology_lines(
    graph: &FleetTopologyGraph,
    filter: &TopologyEdgeFilter,
    focused_loop_id: Option<&str>,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }

    let filtered_edges = filter_fleet_topology_edges(graph, filter);
    let mut lines = vec![trim_line(
        &format!(
            "topology nodes={} edges={} filters={}",
            graph.nodes.len(),
            filtered_edges.len(),
            format_filter_labels(filter)
        ),
        width,
    )];
    if lines.len() >= max_rows {
        return lines;
    }

    if let Some(loop_id) = focused_loop_id {
        if let Some(focus) = focus_loop_topology(graph, loop_id, filter) {
            lines.push(trim_line(
                &format!(
                    "focus {} links={} intensity={}",
                    focus.focus_loop_id,
                    focus.neighbors.len(),
                    focus.total_intensity
                ),
                width,
            ));
        }
    }
    if lines.len() >= max_rows {
        return lines;
    }

    for cluster in graph.clusters.iter().take(3) {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim_line(
            &format!(
                "cluster {} size={} loops={}",
                cluster.label,
                cluster.loop_ids.len(),
                cluster.loop_ids.join(",")
            ),
            width,
        ));
    }
    if lines.len() >= max_rows {
        return lines;
    }

    for edge in filtered_edges.iter().take(8) {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim_line(
            &format!(
                "edge {}<->{} {} {} ({})",
                edge.from_loop_id,
                edge.to_loop_id,
                edge.kind.label(),
                "=".repeat(edge_thickness(edge.intensity)),
                edge.intensity
            ),
            width,
        ));
    }
    if lines.len() >= max_rows {
        return lines;
    }

    for node in &graph.nodes {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim_line(
            &format!(
                "node {} [{}] cluster={} pos=({}, {})",
                node.loop_id,
                node.health.label(),
                node.cluster_key,
                node.x,
                node.y
            ),
            width,
        ));
    }

    lines
}

fn normalize_loop_inputs(loops: &[FleetLoopTopologyInput]) -> Vec<FleetLoopTopologyInput> {
    let mut normalized = Vec::new();
    for loop_input in loops {
        let loop_id = normalize(&loop_input.loop_id);
        if loop_id.is_empty() {
            continue;
        }
        normalized.push(FleetLoopTopologyInput {
            loop_id,
            label: loop_input.label.trim().to_owned(),
            state: normalize(&loop_input.state),
            queue_depth: loop_input.queue_depth,
            last_error: loop_input.last_error.trim().to_owned(),
            repo_path: loop_input.repo_path.trim().to_owned(),
            owned_crates: normalized_vec_tokens(&loop_input.owned_crates),
            touched_files: normalized_vec_tokens(&loop_input.touched_files),
            depends_on: normalized_vec_tokens(&loop_input.depends_on),
        });
    }
    normalized.sort_by(|a, b| a.loop_id.cmp(&b.loop_id));
    normalized.dedup_by(|a, b| a.loop_id == b.loop_id);
    normalized
}

fn build_clusters(loops: &[FleetLoopTopologyInput]) -> Vec<FleetTopologyCluster> {
    let mut by_cluster: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for loop_input in loops {
        let owned = if loop_input.owned_crates.is_empty() {
            inferred_crates_from_repo_path(&loop_input.repo_path)
        } else {
            loop_input.owned_crates.clone()
        };
        if owned.is_empty() {
            by_cluster
                .entry("cluster:unassigned".to_owned())
                .or_default()
                .insert(loop_input.loop_id.clone());
        } else {
            for crate_name in owned {
                by_cluster
                    .entry(format!("crate:{crate_name}"))
                    .or_default()
                    .insert(loop_input.loop_id.clone());
            }
        }
    }

    let mut clusters = by_cluster
        .into_iter()
        .map(|(key, loop_ids)| {
            let label = key
                .strip_prefix("crate:")
                .unwrap_or("unassigned")
                .to_owned();
            FleetTopologyCluster {
                key,
                label,
                loop_ids: loop_ids.into_iter().collect(),
            }
        })
        .collect::<Vec<_>>();
    clusters.sort_by(|a, b| {
        b.loop_ids
            .len()
            .cmp(&a.loop_ids.len())
            .then_with(|| a.key.cmp(&b.key))
    });
    clusters
}

fn build_shared_file_edges(
    loops: &[FleetLoopTopologyInput],
    config: &FleetTopologyConfig,
) -> Vec<FleetTopologyEdge> {
    let mut file_to_loops: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for loop_input in loops {
        for file in &loop_input.touched_files {
            if file.is_empty() {
                continue;
            }
            file_to_loops
                .entry(file.clone())
                .or_default()
                .insert(loop_input.loop_id.clone());
        }
    }

    let mut pair_count: BTreeMap<(String, String), u32> = BTreeMap::new();
    for loop_ids in file_to_loops.values() {
        let loop_ids = loop_ids.iter().cloned().collect::<Vec<_>>();
        for i in 0..loop_ids.len() {
            for j in i + 1..loop_ids.len() {
                let key = canonical_pair(&loop_ids[i], &loop_ids[j]);
                *pair_count.entry(key).or_insert(0) += 1;
            }
        }
    }

    pair_count
        .into_iter()
        .map(|((from, to), count)| FleetTopologyEdge {
            from_loop_id: from,
            to_loop_id: to,
            kind: TopologyEdgeKind::SharedFiles,
            intensity: saturating_intensity(count, config.shared_file_weight, config.max_intensity),
            interaction_count: count,
        })
        .collect()
}

fn build_fmail_edges(
    fmail_edges: &[FleetFmailEdgeInput],
    known_ids: &BTreeSet<String>,
    config: &FleetTopologyConfig,
) -> Vec<FleetTopologyEdge> {
    let mut pair_count: BTreeMap<(String, String), u32> = BTreeMap::new();
    for edge in fmail_edges {
        let from = normalize(&edge.from_loop_id);
        let to = normalize(&edge.to_loop_id);
        if from.is_empty() || to.is_empty() || from == to {
            continue;
        }
        if !known_ids.contains(&from) || !known_ids.contains(&to) {
            continue;
        }
        let key = canonical_pair(&from, &to);
        *pair_count.entry(key).or_insert(0) += edge.message_count.max(1);
    }

    pair_count
        .into_iter()
        .map(|((from, to), count)| FleetTopologyEdge {
            from_loop_id: from,
            to_loop_id: to,
            kind: TopologyEdgeKind::Fmail,
            intensity: saturating_intensity(count, config.fmail_weight, config.max_intensity),
            interaction_count: count,
        })
        .collect()
}

fn build_dependency_edges(
    loops: &[FleetLoopTopologyInput],
    known_ids: &BTreeSet<String>,
    config: &FleetTopologyConfig,
) -> Vec<FleetTopologyEdge> {
    let mut pair_count: BTreeMap<(String, String), u32> = BTreeMap::new();
    for loop_input in loops {
        for dependency in &loop_input.depends_on {
            if !known_ids.contains(dependency) || dependency == &loop_input.loop_id {
                continue;
            }
            let key = canonical_pair(&loop_input.loop_id, dependency);
            *pair_count.entry(key).or_insert(0) += 1;
        }
    }

    pair_count
        .into_iter()
        .map(|((from, to), count)| FleetTopologyEdge {
            from_loop_id: from,
            to_loop_id: to,
            kind: TopologyEdgeKind::Dependency,
            intensity: saturating_intensity(count, config.dependency_weight, config.max_intensity),
            interaction_count: count,
        })
        .collect()
}

fn build_crate_ownership_edges(
    clusters: &[FleetTopologyCluster],
    config: &FleetTopologyConfig,
) -> Vec<FleetTopologyEdge> {
    let mut pair_count: BTreeMap<(String, String), u32> = BTreeMap::new();
    for cluster in clusters {
        if !cluster.key.starts_with("crate:") {
            continue;
        }
        for i in 0..cluster.loop_ids.len() {
            for j in i + 1..cluster.loop_ids.len() {
                let key = canonical_pair(&cluster.loop_ids[i], &cluster.loop_ids[j]);
                *pair_count.entry(key).or_insert(0) += 1;
            }
        }
    }

    pair_count
        .into_iter()
        .map(|((from, to), count)| FleetTopologyEdge {
            from_loop_id: from,
            to_loop_id: to,
            kind: TopologyEdgeKind::CrateOwnership,
            intensity: saturating_intensity(count, config.crate_weight, config.max_intensity),
            interaction_count: count,
        })
        .collect()
}

fn canonical_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}

fn classify_health(
    loop_input: &FleetLoopTopologyInput,
    config: &FleetTopologyConfig,
) -> TopologyNodeHealth {
    let state = normalize(&loop_input.state);
    if !loop_input.last_error.trim().is_empty()
        || state.contains("error")
        || state.contains("failed")
        || state.contains("panic")
    {
        return TopologyNodeHealth::Critical;
    }
    if loop_input.queue_depth >= config.queue_pressure_threshold
        || matches!(state.as_str(), "waiting" | "sleeping" | "stopped")
    {
        return TopologyNodeHealth::Warning;
    }
    if matches!(
        state.as_str(),
        "running" | "ready" | "idle" | "healthy" | "ok"
    ) {
        return TopologyNodeHealth::Healthy;
    }
    TopologyNodeHealth::Unknown
}

fn saturating_intensity(count: u32, weight: u16, max: u16) -> u16 {
    let raw = count.saturating_mul(weight as u32);
    raw.min(max as u32) as u16
}

fn normalize_label(label: &str, fallback_loop_id: &str) -> String {
    let normalized = label.trim();
    if normalized.is_empty() {
        fallback_loop_id.to_owned()
    } else {
        normalized.to_owned()
    }
}

fn inferred_crates_from_repo_path(path: &str) -> Vec<String> {
    let mut inferred = Vec::new();
    let parts = path
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for pair in parts.windows(2) {
        if pair[0] == "crates" {
            inferred.push(normalize(pair[1]));
        }
    }
    inferred
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect()
}

fn normalized_vec_tokens(values: &[String]) -> Vec<String> {
    let mut out = values
        .iter()
        .map(|value| normalize(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn format_filter_labels(filter: &TopologyEdgeFilter) -> String {
    if filter.enabled.is_empty() {
        return "none".to_owned();
    }
    filter
        .enabled
        .iter()
        .map(|kind| kind.label())
        .collect::<Vec<_>>()
        .join(",")
}

fn edge_thickness(intensity: u16) -> usize {
    match intensity {
        0..=9 => 1,
        10..=24 => 2,
        25..=49 => 3,
        _ => 4,
    }
}

fn trim_line(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        line.to_owned()
    } else {
        line.chars().take(width).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_fleet_topology_graph, drag_topology_node, filter_fleet_topology_edges,
        focus_loop_topology, render_fleet_topology_lines, FleetFmailEdgeInput,
        FleetLoopTopologyInput, FleetTopologyConfig, TopologyEdgeFilter, TopologyEdgeKind,
        TopologyNodeHealth,
    };

    fn loop_input(
        loop_id: &str,
        state: &str,
        queue_depth: usize,
        repo_path: &str,
    ) -> FleetLoopTopologyInput {
        FleetLoopTopologyInput {
            loop_id: loop_id.to_owned(),
            label: loop_id.to_owned(),
            state: state.to_owned(),
            queue_depth,
            last_error: String::new(),
            repo_path: repo_path.to_owned(),
            owned_crates: Vec::new(),
            touched_files: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn builds_edges_across_shared_files_fmail_dependency_and_crate_clusters() {
        let mut loop_a = loop_input("loop-a", "running", 2, "/repo/crates/core/src");
        loop_a.touched_files = vec!["src/shared.rs".to_owned(), "src/a.rs".to_owned()];
        loop_a.depends_on = vec!["loop-c".to_owned()];
        let mut loop_b = loop_input("loop-b", "running", 1, "/repo/crates/core/tests");
        loop_b.touched_files = vec!["src/shared.rs".to_owned(), "src/b.rs".to_owned()];
        let mut loop_c = loop_input("loop-c", "waiting", 5, "/repo/crates/ui/src");
        loop_c.touched_files = vec!["src/ui.rs".to_owned()];

        let fmail_edges = vec![
            FleetFmailEdgeInput {
                from_loop_id: "loop-a".to_owned(),
                to_loop_id: "loop-b".to_owned(),
                message_count: 3,
            },
            FleetFmailEdgeInput {
                from_loop_id: "loop-b".to_owned(),
                to_loop_id: "loop-a".to_owned(),
                message_count: 1,
            },
            FleetFmailEdgeInput {
                from_loop_id: "loop-c".to_owned(),
                to_loop_id: "loop-a".to_owned(),
                message_count: 2,
            },
        ];

        let graph = build_fleet_topology_graph(
            &[loop_a, loop_b, loop_c],
            &fmail_edges,
            &FleetTopologyConfig::default(),
        );

        assert_eq!(graph.nodes.len(), 3);
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == TopologyEdgeKind::SharedFiles));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == TopologyEdgeKind::Fmail));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == TopologyEdgeKind::Dependency));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == TopologyEdgeKind::CrateOwnership));
        assert!(graph
            .clusters
            .iter()
            .any(|cluster| cluster.key == "crate:core" && cluster.loop_ids.len() == 2));
    }

    #[test]
    fn classifies_health_with_state_queue_and_error_signals() {
        let healthy = loop_input("loop-healthy", "running", 2, "/repo/crates/a/src");
        let warning = loop_input("loop-warning", "waiting", 20, "/repo/crates/b/src");
        let mut critical = loop_input("loop-critical", "running", 1, "/repo/crates/c/src");
        critical.last_error = "panic: failed".to_owned();

        let graph = build_fleet_topology_graph(
            &[healthy, warning, critical],
            &[],
            &FleetTopologyConfig::default(),
        );

        let health_by_loop = graph
            .nodes
            .iter()
            .map(|node| (node.loop_id.clone(), node.health))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            health_by_loop.get("loop-healthy"),
            Some(&TopologyNodeHealth::Healthy)
        );
        assert_eq!(
            health_by_loop.get("loop-warning"),
            Some(&TopologyNodeHealth::Warning)
        );
        assert_eq!(
            health_by_loop.get("loop-critical"),
            Some(&TopologyNodeHealth::Critical)
        );
    }

    #[test]
    fn edge_filter_returns_only_enabled_kinds() {
        let mut loop_a = loop_input("loop-a", "running", 0, "/repo/crates/core/src");
        loop_a.touched_files = vec!["src/shared.rs".to_owned()];
        let mut loop_b = loop_input("loop-b", "running", 0, "/repo/crates/core/src");
        loop_b.touched_files = vec!["src/shared.rs".to_owned()];
        let graph = build_fleet_topology_graph(
            &[loop_a, loop_b],
            &[FleetFmailEdgeInput {
                from_loop_id: "loop-a".to_owned(),
                to_loop_id: "loop-b".to_owned(),
                message_count: 2,
            }],
            &FleetTopologyConfig::default(),
        );

        let mut filter = TopologyEdgeFilter::default();
        filter.enabled.clear();
        filter.enabled.insert(TopologyEdgeKind::Fmail);
        let filtered = filter_fleet_topology_edges(&graph, &filter);
        assert!(!filtered.is_empty());
        assert!(filtered
            .iter()
            .all(|edge| edge.kind == TopologyEdgeKind::Fmail));
    }

    #[test]
    fn focus_view_sorts_neighbors_by_intensity_desc() {
        let graph = build_fleet_topology_graph(
            &[
                loop_input("loop-a", "running", 0, "/repo/crates/a/src"),
                loop_input("loop-b", "running", 0, "/repo/crates/a/src"),
                loop_input("loop-c", "running", 0, "/repo/crates/c/src"),
            ],
            &[
                FleetFmailEdgeInput {
                    from_loop_id: "loop-a".to_owned(),
                    to_loop_id: "loop-b".to_owned(),
                    message_count: 9,
                },
                FleetFmailEdgeInput {
                    from_loop_id: "loop-a".to_owned(),
                    to_loop_id: "loop-c".to_owned(),
                    message_count: 1,
                },
            ],
            &FleetTopologyConfig::default(),
        );
        let focus = match focus_loop_topology(&graph, "loop-a", &TopologyEdgeFilter::default()) {
            Some(focus) => focus,
            None => panic!("focus should resolve for loop-a"),
        };
        assert_eq!(focus.focus_loop_id, "loop-a");
        assert!(focus.neighbors.len() >= 2);
        assert!(focus.neighbors[0].intensity >= focus.neighbors[1].intensity);
    }

    #[test]
    fn drag_clamps_node_position_to_bounds() {
        let mut graph = build_fleet_topology_graph(
            &[loop_input("loop-a", "running", 0, "/repo/crates/a/src")],
            &[],
            &FleetTopologyConfig::default(),
        );
        assert!(drag_topology_node(&mut graph, "loop-a", 150, -4, 60, 30));
        assert_eq!(graph.nodes[0].x, 60);
        assert_eq!(graph.nodes[0].y, 0);
        assert!(!drag_topology_node(&mut graph, "missing", 1, 1, 20, 20));
    }

    #[test]
    fn render_lines_include_focus_clusters_and_edges() {
        let mut loop_a = loop_input("loop-a", "running", 0, "/repo/crates/core/src");
        loop_a.touched_files = vec!["src/shared.rs".to_owned()];
        let mut loop_b = loop_input("loop-b", "running", 0, "/repo/crates/core/src");
        loop_b.touched_files = vec!["src/shared.rs".to_owned()];
        let graph = build_fleet_topology_graph(
            &[loop_a, loop_b],
            &[FleetFmailEdgeInput {
                from_loop_id: "loop-a".to_owned(),
                to_loop_id: "loop-b".to_owned(),
                message_count: 2,
            }],
            &FleetTopologyConfig::default(),
        );
        let lines = render_fleet_topology_lines(
            &graph,
            &TopologyEdgeFilter::default(),
            Some("loop-a"),
            120,
            12,
        );
        assert!(lines.iter().any(|line| line.contains("topology nodes=2")));
        assert!(lines.iter().any(|line| line.contains("focus loop-a")));
        assert!(lines
            .iter()
            .any(|line| line.contains("cluster core size=2")));
        assert!(lines
            .iter()
            .any(|line| line.contains("edge loop-a<->loop-b")));
        assert!(lines.iter().any(|line| line.contains("node loop-a")));
    }
}

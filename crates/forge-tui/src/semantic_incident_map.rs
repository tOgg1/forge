//! Semantic incident map overlay linking loops, runs, inbox threads, and failures.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncidentNodeKind {
    Loop,
    Run,
    InboxThread,
    FailureSignature,
}

impl IncidentNodeKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            IncidentNodeKind::Loop => "loop",
            IncidentNodeKind::Run => "run",
            IncidentNodeKind::InboxThread => "inbox",
            IncidentNodeKind::FailureSignature => "failure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentNode {
    pub id: String,
    pub kind: IncidentNodeKind,
    pub label: String,
    pub severity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncidentEdgeKind {
    Triggered,
    Mentions,
    FailedWith,
    CorrelatedLoop,
}

impl IncidentEdgeKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            IncidentEdgeKind::Triggered => "triggered",
            IncidentEdgeKind::Mentions => "mentions",
            IncidentEdgeKind::FailedWith => "failed-with",
            IncidentEdgeKind::CorrelatedLoop => "correlated-loop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentEdge {
    pub from: String,
    pub to: String,
    pub kind: IncidentEdgeKind,
    pub weight: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentSample {
    pub loop_id: String,
    pub run_id: String,
    pub inbox_thread_id: String,
    pub failure_signature: String,
    pub severity: u8,
    pub mention_count: u8,
    pub correlated_loops: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticIncidentMap {
    pub nodes: Vec<IncidentNode>,
    pub edges: Vec<IncidentEdge>,
}

#[must_use]
pub fn build_semantic_incident_map(samples: &[IncidentSample]) -> SemanticIncidentMap {
    let mut nodes: BTreeMap<String, IncidentNode> = BTreeMap::new();
    let mut edge_set: BTreeSet<(String, String, IncidentEdgeKind, u8)> = BTreeSet::new();

    for sample in samples {
        let loop_key = format!("loop:{}", sample.loop_id.trim());
        let run_key = format!("run:{}", sample.run_id.trim());
        let inbox_key = format!("inbox:{}", sample.inbox_thread_id.trim());
        let failure_key = format!("failure:{}", sample.failure_signature.trim());

        if let Some(node) = make_node(
            &loop_key,
            IncidentNodeKind::Loop,
            sample.loop_id.trim(),
            sample.severity,
        ) {
            insert_or_upgrade_node(&mut nodes, node);
        }
        if let Some(node) = make_node(
            &run_key,
            IncidentNodeKind::Run,
            sample.run_id.trim(),
            sample.severity,
        ) {
            insert_or_upgrade_node(&mut nodes, node);
        }
        if let Some(node) = make_node(
            &inbox_key,
            IncidentNodeKind::InboxThread,
            sample.inbox_thread_id.trim(),
            sample.severity,
        ) {
            insert_or_upgrade_node(&mut nodes, node);
        }
        if let Some(node) = make_node(
            &failure_key,
            IncidentNodeKind::FailureSignature,
            sample.failure_signature.trim(),
            sample.severity,
        ) {
            insert_or_upgrade_node(&mut nodes, node);
        }

        insert_edge(
            &mut edge_set,
            &loop_key,
            &run_key,
            IncidentEdgeKind::Triggered,
            sample.severity.max(1),
        );
        insert_edge(
            &mut edge_set,
            &inbox_key,
            &run_key,
            IncidentEdgeKind::Mentions,
            sample.mention_count.max(1),
        );
        insert_edge(
            &mut edge_set,
            &run_key,
            &failure_key,
            IncidentEdgeKind::FailedWith,
            sample.severity.max(1),
        );

        for correlated_loop in &sample.correlated_loops {
            let correlated = correlated_loop.trim();
            if correlated.is_empty() {
                continue;
            }
            let correlated_key = format!("loop:{correlated}");
            if let Some(node) = make_node(
                &correlated_key,
                IncidentNodeKind::Loop,
                correlated,
                sample.severity,
            ) {
                insert_or_upgrade_node(&mut nodes, node);
            }
            insert_edge(
                &mut edge_set,
                &failure_key,
                &correlated_key,
                IncidentEdgeKind::CorrelatedLoop,
                sample.severity.max(1),
            );
        }
    }

    let mut node_values = nodes.into_values().collect::<Vec<IncidentNode>>();
    node_values.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut edge_values = edge_set
        .into_iter()
        .map(|(from, to, kind, weight)| IncidentEdge {
            from,
            to,
            kind,
            weight,
        })
        .collect::<Vec<IncidentEdge>>();
    edge_values.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| b.weight.cmp(&a.weight))
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });

    SemanticIncidentMap {
        nodes: node_values,
        edges: edge_values,
    }
}

#[must_use]
pub fn render_incident_map_rows(
    map: &SemanticIncidentMap,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut rows = vec![trim_to_width(
        &format!(
            "incident-map nodes:{} edges:{}",
            map.nodes.len(),
            map.edges.len()
        ),
        width,
    )];
    if rows.len() >= max_rows {
        return rows;
    }
    if map.nodes.is_empty() {
        rows.push(trim_to_width("no incident graph data", width));
        return rows;
    }
    for node in &map.nodes {
        if rows.len() >= max_rows {
            break;
        }
        let row = format!(
            "node {}:{} sev:{} id:{}",
            node.kind.label(),
            node.label,
            node.severity,
            node.id
        );
        rows.push(trim_to_width(&row, width));
    }
    rows
}

fn make_node(id: &str, kind: IncidentNodeKind, label: &str, severity: u8) -> Option<IncidentNode> {
    let id = id.trim();
    let label = label.trim();
    if id.is_empty() || label.is_empty() {
        return None;
    }
    Some(IncidentNode {
        id: id.to_owned(),
        kind,
        label: label.to_owned(),
        severity,
    })
}

fn insert_or_upgrade_node(nodes: &mut BTreeMap<String, IncidentNode>, candidate: IncidentNode) {
    match nodes.get_mut(&candidate.id) {
        Some(existing) => {
            if candidate.severity > existing.severity {
                existing.severity = candidate.severity;
            }
        }
        None => {
            nodes.insert(candidate.id.clone(), candidate);
        }
    }
}

fn insert_edge(
    edges: &mut BTreeSet<(String, String, IncidentEdgeKind, u8)>,
    from: &str,
    to: &str,
    kind: IncidentEdgeKind,
    weight: u8,
) {
    if from.trim().is_empty() || to.trim().is_empty() {
        return;
    }
    edges.insert((
        from.trim().to_owned(),
        to.trim().to_owned(),
        kind,
        weight.max(1),
    ));
}

fn trim_to_width(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_owned()
    } else {
        text[0..width].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_semantic_incident_map, render_incident_map_rows, IncidentSample};

    fn sample_incidents() -> Vec<IncidentSample> {
        vec![
            IncidentSample {
                loop_id: "loop-a".to_owned(),
                run_id: "run-100".to_owned(),
                inbox_thread_id: "thread-7".to_owned(),
                failure_signature: "timeout db".to_owned(),
                severity: 90,
                mention_count: 3,
                correlated_loops: vec!["loop-b".to_owned()],
            },
            IncidentSample {
                loop_id: "loop-b".to_owned(),
                run_id: "run-101".to_owned(),
                inbox_thread_id: "thread-8".to_owned(),
                failure_signature: "timeout db".to_owned(),
                severity: 70,
                mention_count: 2,
                correlated_loops: vec!["loop-a".to_owned(), " ".to_owned()],
            },
        ]
    }

    #[test]
    fn build_map_contains_expected_entity_kinds() {
        let map = build_semantic_incident_map(&sample_incidents());
        assert!(map.nodes.iter().any(|node| node.id == "loop:loop-a"));
        assert!(map.nodes.iter().any(|node| node.id == "run:run-100"));
        assert!(map.nodes.iter().any(|node| node.id == "inbox:thread-7"));
        assert!(map.nodes.iter().any(|node| node.id == "failure:timeout db"));
    }

    #[test]
    fn build_map_deduplicates_shared_failure_nodes() {
        let map = build_semantic_incident_map(&sample_incidents());
        let shared_failure_count = map
            .nodes
            .iter()
            .filter(|node| node.id == "failure:timeout db")
            .count();
        assert_eq!(shared_failure_count, 1);
    }

    #[test]
    fn build_map_contains_correlation_edges() {
        let map = build_semantic_incident_map(&sample_incidents());
        assert!(map.edges.iter().any(|edge| {
            edge.from == "failure:timeout db" && edge.to == "loop:loop-b" && edge.weight == 90
        }));
    }

    #[test]
    fn incident_rows_snapshot_is_deterministic() {
        let map = build_semantic_incident_map(&sample_incidents());
        let rows = render_incident_map_rows(&map, 200, 6);
        assert_eq!(
            rows,
            vec![
                "incident-map nodes:7 edges:8".to_owned(),
                "node loop:loop-a sev:90 id:loop:loop-a".to_owned(),
                "node loop:loop-b sev:90 id:loop:loop-b".to_owned(),
                "node run:run-100 sev:90 id:run:run-100".to_owned(),
                "node inbox:thread-7 sev:90 id:inbox:thread-7".to_owned(),
                "node failure:timeout db sev:90 id:failure:timeout db".to_owned(),
            ]
        );
    }
}

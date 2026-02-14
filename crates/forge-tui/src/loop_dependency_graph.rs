//! Loop dependency graph model + text rendering helpers.

use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencyInput {
    pub loop_id: String,
    pub state: String,
    pub queue_depth: usize,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDependencyNode {
    pub loop_id: String,
    pub state: String,
    pub queue_depth: usize,
    pub incoming: usize,
    pub outgoing: usize,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopDependencyGraph {
    pub nodes: Vec<LoopDependencyNode>,
    pub edges: Vec<(String, String)>,
    pub cycle_nodes: Vec<String>,
    pub longest_chain: Vec<String>,
}

#[must_use]
pub fn build_loop_dependency_graph(inputs: &[LoopDependencyInput]) -> LoopDependencyGraph {
    let known_ids: HashSet<String> = inputs.iter().map(|entry| entry.loop_id.clone()).collect();

    let mut edges = Vec::new();
    let mut incoming: HashMap<String, usize> = HashMap::new();
    let mut outgoing: HashMap<String, usize> = HashMap::new();
    let mut blocked_by: HashMap<String, Vec<String>> = HashMap::new();

    for entry in inputs {
        incoming.entry(entry.loop_id.clone()).or_insert(0);
        outgoing.entry(entry.loop_id.clone()).or_insert(0);
    }

    for entry in inputs {
        for dep in &entry.depends_on {
            if !known_ids.contains(dep) {
                continue;
            }
            edges.push((dep.clone(), entry.loop_id.clone()));
            *incoming.entry(entry.loop_id.clone()).or_insert(0) += 1;
            *outgoing.entry(dep.clone()).or_insert(0) += 1;
            blocked_by
                .entry(entry.loop_id.clone())
                .or_default()
                .push(dep.clone());
        }
    }

    let cycle_nodes = detect_cycle_nodes(&known_ids, &edges);
    let longest_chain = compute_longest_chain(&known_ids, &edges);

    let mut nodes = Vec::with_capacity(inputs.len());
    for entry in inputs {
        let mut blockers = blocked_by.remove(&entry.loop_id).unwrap_or_default();
        blockers.sort();
        blockers.dedup();
        nodes.push(LoopDependencyNode {
            loop_id: entry.loop_id.clone(),
            state: normalize_state(&entry.state),
            queue_depth: entry.queue_depth,
            incoming: incoming.get(&entry.loop_id).copied().unwrap_or(0),
            outgoing: outgoing.get(&entry.loop_id).copied().unwrap_or(0),
            blocked_by: blockers,
        });
    }
    nodes.sort_by(|a, b| a.loop_id.cmp(&b.loop_id));

    LoopDependencyGraph {
        nodes,
        edges,
        cycle_nodes,
        longest_chain,
    }
}

#[must_use]
pub fn render_loop_dependency_lines(
    graph: &LoopDependencyGraph,
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if max_rows == 0 || width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.push(trim_line(
        &format!(
            "dependency-graph nodes={} edges={} cycles={} chain={}",
            graph.nodes.len(),
            graph.edges.len(),
            if graph.cycle_nodes.is_empty() {
                "none"
            } else {
                "yes"
            },
            if graph.longest_chain.is_empty() {
                "-".to_owned()
            } else {
                graph.longest_chain.join(" -> ")
            }
        ),
        width,
    ));
    if lines.len() >= max_rows {
        return lines;
    }

    for node in &graph.nodes {
        if lines.len() >= max_rows {
            break;
        }
        let blockers = if node.blocked_by.is_empty() {
            "none".to_owned()
        } else {
            node.blocked_by.join(",")
        };
        lines.push(trim_line(
            &format!(
                "{} state={} q={} in={} out={} blocked_by={}",
                node.loop_id, node.state, node.queue_depth, node.incoming, node.outgoing, blockers
            ),
            width,
        ));
    }

    if !graph.cycle_nodes.is_empty() && lines.len() < max_rows {
        lines.push(trim_line(
            &format!("cycle nodes: {}", graph.cycle_nodes.join(",")),
            width,
        ));
    }

    lines
}

fn normalize_state(state: &str) -> String {
    let trimmed = state.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed
    }
}

fn detect_cycle_nodes(known_ids: &HashSet<String>, edges: &[(String, String)]) -> Vec<String> {
    let mut in_degree: HashMap<String, usize> =
        known_ids.iter().map(|id| (id.clone(), 0)).collect();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges {
        outgoing.entry(from.clone()).or_default().push(to.clone());
        *in_degree.entry(to.clone()).or_insert(0) += 1;
    }

    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    queue.sort();

    let mut processed = 0usize;
    while let Some(id) = queue.pop() {
        processed += 1;
        if let Some(children) = outgoing.get(&id) {
            for child in children {
                if let Some(value) = in_degree.get_mut(child) {
                    *value = value.saturating_sub(1);
                    if *value == 0 {
                        queue.push(child.clone());
                    }
                }
            }
        }
    }

    if processed == known_ids.len() {
        return Vec::new();
    }

    let mut cycle: Vec<String> = in_degree
        .into_iter()
        .filter_map(|(id, degree)| if degree > 0 { Some(id) } else { None })
        .collect();
    cycle.sort();
    cycle
}

fn compute_longest_chain(known_ids: &HashSet<String>, edges: &[(String, String)]) -> Vec<String> {
    let mut predecessors: HashMap<String, Vec<String>> = known_ids
        .iter()
        .map(|id| (id.clone(), Vec::new()))
        .collect();
    let mut successors: HashMap<String, Vec<String>> = known_ids
        .iter()
        .map(|id| (id.clone(), Vec::new()))
        .collect();

    for (from, to) in edges {
        predecessors
            .entry(to.clone())
            .or_default()
            .push(from.clone());
        successors.entry(from.clone()).or_default().push(to.clone());
    }

    // If graph has cycles, return empty chain.
    if !detect_cycle_nodes(known_ids, edges).is_empty() {
        return Vec::new();
    }

    let mut ordered: BTreeSet<String> = BTreeSet::new();
    ordered.extend(known_ids.iter().cloned());

    let mut best_len: HashMap<String, usize> = HashMap::new();
    let mut best_prev: HashMap<String, String> = HashMap::new();
    for id in ordered {
        let prevs = predecessors.get(&id).cloned().unwrap_or_default();
        let mut best = 1usize;
        let mut prev_choice: Option<String> = None;
        for prev in prevs {
            let candidate = best_len.get(&prev).copied().unwrap_or(1) + 1;
            if candidate > best {
                best = candidate;
                prev_choice = Some(prev);
            }
        }
        best_len.insert(id.clone(), best);
        if let Some(prev) = prev_choice {
            best_prev.insert(id, prev);
        }
    }

    let Some((mut end, _)) = best_len
        .iter()
        .max_by(|(left_id, left_len), (right_id, right_len)| {
            left_len.cmp(right_len).then_with(|| right_id.cmp(left_id))
        })
        .map(|(id, len)| (id.clone(), *len))
    else {
        return Vec::new();
    };

    let mut chain = vec![end.clone()];
    while let Some(prev) = best_prev.get(&end).cloned() {
        chain.push(prev.clone());
        end = prev;
    }
    chain.reverse();

    if chain.len() <= 1
        && successors
            .get(&chain[0])
            .is_some_and(|children| children.is_empty())
    {
        return Vec::new();
    }
    chain
}

fn trim_line(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if line.chars().count() <= width {
        return line.to_owned();
    }
    line.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{build_loop_dependency_graph, render_loop_dependency_lines, LoopDependencyInput};

    #[test]
    fn builds_edges_and_blocker_counts_for_known_dependencies() {
        let graph = build_loop_dependency_graph(&[
            LoopDependencyInput {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                queue_depth: 1,
                depends_on: Vec::new(),
            },
            LoopDependencyInput {
                loop_id: "loop-b".to_owned(),
                state: "waiting".to_owned(),
                queue_depth: 2,
                depends_on: vec!["loop-a".to_owned(), "missing".to_owned()],
            },
        ]);

        assert_eq!(
            graph.edges,
            vec![("loop-a".to_owned(), "loop-b".to_owned())]
        );
        let loop_b = match graph.nodes.iter().find(|node| node.loop_id == "loop-b") {
            Some(node) => node,
            None => panic!("loop-b should exist"),
        };
        assert_eq!(loop_b.incoming, 1);
        assert_eq!(loop_b.blocked_by, vec!["loop-a".to_owned()]);
    }

    #[test]
    fn detects_cycle_nodes() {
        let graph = build_loop_dependency_graph(&[
            LoopDependencyInput {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec!["loop-b".to_owned()],
            },
            LoopDependencyInput {
                loop_id: "loop-b".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec!["loop-a".to_owned()],
            },
        ]);

        assert_eq!(
            graph.cycle_nodes,
            vec!["loop-a".to_owned(), "loop-b".to_owned()]
        );
        assert!(graph.longest_chain.is_empty());
    }

    #[test]
    fn longest_chain_prefers_deeper_dependency_path() {
        let graph = build_loop_dependency_graph(&[
            LoopDependencyInput {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec![],
            },
            LoopDependencyInput {
                loop_id: "loop-b".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec!["loop-a".to_owned()],
            },
            LoopDependencyInput {
                loop_id: "loop-c".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec!["loop-b".to_owned()],
            },
        ]);

        assert_eq!(
            graph.longest_chain,
            vec![
                "loop-a".to_owned(),
                "loop-b".to_owned(),
                "loop-c".to_owned()
            ]
        );
    }

    #[test]
    fn render_lines_include_summary_and_nodes() {
        let graph = build_loop_dependency_graph(&[
            LoopDependencyInput {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                queue_depth: 0,
                depends_on: vec![],
            },
            LoopDependencyInput {
                loop_id: "loop-b".to_owned(),
                state: "waiting".to_owned(),
                queue_depth: 3,
                depends_on: vec!["loop-a".to_owned()],
            },
        ]);
        let lines = render_loop_dependency_lines(&graph, 120, 8);
        assert!(lines[0].contains("dependency-graph"));
        assert!(lines.iter().any(|line| line.contains("loop-a")));
        assert!(lines.iter().any(|line| line.contains("loop-b")));
    }
}

//! Agent performance profiler core with flame-style aggregation.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilerSpan {
    pub agent_id: String,
    pub stack: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlameNode {
    pub label: String,
    pub total_ms: u64,
    pub children: Vec<FlameNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTimeSummary {
    pub agent_id: String,
    pub total_ms: u64,
    pub top_frame: String,
}

#[must_use]
pub fn build_flame_tree(spans: &[ProfilerSpan]) -> FlameNode {
    let mut root = FlameNode {
        label: "all-agents".to_owned(),
        total_ms: 0,
        children: Vec::new(),
    };
    for span in spans {
        if span.duration_ms == 0 {
            continue;
        }
        root.total_ms = root.total_ms.saturating_add(span.duration_ms);
        let mut path = Vec::with_capacity(span.stack.len() + 1);
        path.push(span.agent_id.as_str());
        for frame in &span.stack {
            path.push(frame.as_str());
        }
        insert_path(&mut root, &path, span.duration_ms);
    }
    sort_flame_nodes(&mut root);
    root
}

#[must_use]
pub fn summarize_agent_time(spans: &[ProfilerSpan]) -> Vec<AgentTimeSummary> {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut top_frames: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();

    for span in spans {
        if span.duration_ms == 0 || span.agent_id.trim().is_empty() {
            continue;
        }
        *totals.entry(span.agent_id.clone()).or_insert(0) += span.duration_ms;
        let frame = span
            .stack
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned());
        *top_frames
            .entry(span.agent_id.clone())
            .or_default()
            .entry(frame)
            .or_insert(0) += span.duration_ms;
    }

    let mut summaries = Vec::with_capacity(totals.len());
    for (agent_id, total_ms) in totals {
        let top_frame = top_frames
            .remove(&agent_id)
            .and_then(|entries| {
                entries
                    .into_iter()
                    .max_by(|(left_name, left_ms), (right_name, right_ms)| {
                        left_ms
                            .cmp(right_ms)
                            .then_with(|| right_name.cmp(left_name))
                    })
                    .map(|(name, _)| name)
            })
            .unwrap_or_else(|| "unknown".to_owned());
        summaries.push(AgentTimeSummary {
            agent_id,
            total_ms,
            top_frame,
        });
    }
    summaries.sort_by(|a, b| {
        b.total_ms
            .cmp(&a.total_ms)
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });
    summaries
}

#[must_use]
pub fn render_flame_lines(root: &FlameNode, width: usize, max_rows: usize) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }

    let mut lines = vec![trim_line(
        &format!(
            "flame total={}ms nodes={}",
            root.total_ms,
            count_nodes(root).saturating_sub(1)
        ),
        width,
    )];
    render_flame_node(root, root.total_ms.max(1), 0, width, max_rows, &mut lines);
    lines.truncate(max_rows);
    lines
}

fn insert_path(node: &mut FlameNode, path: &[&str], duration_ms: u64) {
    if path.is_empty() {
        return;
    }
    let label = path[0];
    let child_index = node
        .children
        .iter()
        .position(|child| child.label == label)
        .unwrap_or_else(|| {
            node.children.push(FlameNode {
                label: label.to_owned(),
                total_ms: 0,
                children: Vec::new(),
            });
            node.children.len() - 1
        });
    let child = &mut node.children[child_index];
    child.total_ms = child.total_ms.saturating_add(duration_ms);
    insert_path(child, &path[1..], duration_ms);
}

fn sort_flame_nodes(node: &mut FlameNode) {
    node.children.sort_by(|a, b| {
        b.total_ms
            .cmp(&a.total_ms)
            .then_with(|| a.label.cmp(&b.label))
    });
    for child in &mut node.children {
        sort_flame_nodes(child);
    }
}

fn render_flame_node(
    node: &FlameNode,
    root_total: u64,
    depth: usize,
    width: usize,
    max_rows: usize,
    lines: &mut Vec<String>,
) {
    for child in &node.children {
        if lines.len() >= max_rows {
            return;
        }
        let percent = ((child.total_ms as f64 / root_total as f64) * 100.0).round() as u64;
        let bar_len = ((child.total_ms as f64 / root_total as f64) * (width as f64 * 0.35))
            .round()
            .clamp(1.0, width as f64) as usize;
        let indent = "  ".repeat(depth);
        let line = format!(
            "{}{} {}ms ({}%) {}",
            indent,
            child.label,
            child.total_ms,
            percent,
            "█".repeat(bar_len)
        );
        lines.push(trim_line(&line, width));
        render_flame_node(child, root_total, depth + 1, width, max_rows, lines);
    }
}

fn count_nodes(node: &FlameNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

fn trim_line(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        return line.to_owned();
    }
    line.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{build_flame_tree, render_flame_lines, summarize_agent_time, ProfilerSpan};

    #[test]
    fn flame_tree_aggregates_duplicate_paths() {
        let spans = vec![
            ProfilerSpan {
                agent_id: "agent-a".to_owned(),
                stack: vec!["tool".to_owned(), "grep".to_owned()],
                duration_ms: 120,
            },
            ProfilerSpan {
                agent_id: "agent-a".to_owned(),
                stack: vec!["tool".to_owned(), "grep".to_owned()],
                duration_ms: 80,
            },
        ];
        let tree = build_flame_tree(&spans);
        assert_eq!(tree.total_ms, 200);
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].label, "agent-a");
        assert_eq!(tree.children[0].total_ms, 200);
    }

    #[test]
    fn summarize_agent_time_sorts_by_total_desc() {
        let spans = vec![
            ProfilerSpan {
                agent_id: "agent-b".to_owned(),
                stack: vec!["tool".to_owned()],
                duration_ms: 100,
            },
            ProfilerSpan {
                agent_id: "agent-a".to_owned(),
                stack: vec!["thinking".to_owned()],
                duration_ms: 250,
            },
        ];
        let summaries = summarize_agent_time(&spans);
        assert_eq!(summaries[0].agent_id, "agent-a");
        assert_eq!(summaries[0].total_ms, 250);
        assert_eq!(summaries[1].agent_id, "agent-b");
    }

    #[test]
    fn render_flame_lines_includes_header_and_bars() {
        let spans = vec![
            ProfilerSpan {
                agent_id: "agent-a".to_owned(),
                stack: vec!["tool".to_owned(), "rg".to_owned()],
                duration_ms: 180,
            },
            ProfilerSpan {
                agent_id: "agent-a".to_owned(),
                stack: vec!["thinking".to_owned()],
                duration_ms: 60,
            },
        ];
        let tree = build_flame_tree(&spans);
        let lines = render_flame_lines(&tree, 120, 12);
        assert!(lines[0].contains("flame total=240ms"));
        assert!(lines.iter().any(|line| line.contains("agent-a")));
        assert!(lines.iter().any(|line| line.contains("█")));
    }
}

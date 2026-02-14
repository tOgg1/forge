//! Multi-node compare split model for side-by-side state/log inspection.

#[derive(Debug, Clone, PartialEq)]
pub struct NodeCompareSample {
    pub node_id: String,
    pub label: String,
    pub status: String,
    pub queue_depth: i64,
    pub error_count: i64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub last_log_line: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiNodeCompareConfig {
    pub baseline_node_id: Option<String>,
    pub limit: usize,
    pub status_mismatch_penalty: i32,
    pub queue_weight: i32,
    pub error_weight: i32,
    pub cpu_weight: i32,
    pub memory_weight: i32,
    pub log_mismatch_bonus: i32,
}

impl Default for MultiNodeCompareConfig {
    fn default() -> Self {
        Self {
            baseline_node_id: None,
            limit: 8,
            status_mismatch_penalty: 40,
            queue_weight: 3,
            error_weight: 5,
            cpu_weight: 2,
            memory_weight: 1,
            log_mismatch_bonus: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeCompareDelta {
    pub node_id: String,
    pub label: String,
    pub status: String,
    pub status_match: bool,
    pub queue_depth_delta: i64,
    pub error_count_delta: i64,
    pub cpu_delta: f64,
    pub memory_delta: f64,
    pub divergence_score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiNodeCompareReport {
    pub baseline_node_id: String,
    pub baseline_label: String,
    pub baseline_status: String,
    pub nodes: Vec<NodeCompareDelta>,
    pub excluded_invalid: usize,
    pub baseline_missing: bool,
}

#[must_use]
pub fn build_multi_node_compare_report(
    samples: &[NodeCompareSample],
    config: &MultiNodeCompareConfig,
) -> MultiNodeCompareReport {
    let mut normalized = Vec::new();
    let mut excluded_invalid = 0usize;
    for sample in samples {
        let node_id = normalize(&sample.node_id);
        if node_id.is_empty() {
            excluded_invalid += 1;
            continue;
        }
        normalized.push(NodeCompareSample {
            node_id: node_id.clone(),
            label: normalize_label(&sample.label, &node_id),
            status: normalize(&sample.status),
            queue_depth: sample.queue_depth.max(0),
            error_count: sample.error_count.max(0),
            cpu_percent: sample.cpu_percent.max(0.0),
            memory_mb: sample.memory_mb.max(0.0),
            last_log_line: sample.last_log_line.trim().to_owned(),
        });
    }

    if normalized.is_empty() {
        return MultiNodeCompareReport {
            baseline_node_id: String::new(),
            baseline_label: String::new(),
            baseline_status: String::new(),
            nodes: Vec::new(),
            excluded_invalid,
            baseline_missing: true,
        };
    }

    let baseline_request = config
        .baseline_node_id
        .as_deref()
        .map(normalize)
        .unwrap_or_default();
    let baseline_index = if baseline_request.is_empty() {
        0
    } else {
        normalized
            .iter()
            .position(|sample| sample.node_id == baseline_request)
            .unwrap_or(0)
    };
    let baseline_missing = !baseline_request.is_empty()
        && normalized
            .iter()
            .all(|sample| sample.node_id != baseline_request);
    let baseline = normalized[baseline_index].clone();

    let mut nodes = Vec::new();
    for sample in &normalized {
        if sample.node_id == baseline.node_id {
            continue;
        }

        let status_match = sample.status == baseline.status;
        let queue_depth_delta = sample.queue_depth - baseline.queue_depth;
        let error_count_delta = sample.error_count - baseline.error_count;
        let cpu_delta = sample.cpu_percent - baseline.cpu_percent;
        let memory_delta = sample.memory_mb - baseline.memory_mb;
        let mut divergence_score = 0;
        let mut reasons = Vec::new();

        if !status_match {
            divergence_score += config.status_mismatch_penalty;
            reasons.push(format!(
                "status mismatch {} vs {} (+{})",
                sample.status, baseline.status, config.status_mismatch_penalty
            ));
        }

        let queue_score = i64::abs(queue_depth_delta) as i32 * config.queue_weight.max(0);
        if queue_score > 0 {
            divergence_score += queue_score;
            reasons.push(format!(
                "queue delta {queue_depth_delta:+} (+{queue_score})"
            ));
        }
        let error_score = i64::abs(error_count_delta) as i32 * config.error_weight.max(0);
        if error_score > 0 {
            divergence_score += error_score;
            reasons.push(format!(
                "error delta {error_count_delta:+} (+{error_score})"
            ));
        }
        let cpu_score = cpu_delta.abs().round() as i32 * config.cpu_weight.max(0);
        if cpu_score > 0 {
            divergence_score += cpu_score;
            reasons.push(format!("cpu delta {cpu_delta:+.1}% (+{cpu_score})"));
        }
        let memory_score =
            ((memory_delta.abs() / 128.0).round() as i32).max(0) * config.memory_weight.max(0);
        if memory_score > 0 {
            divergence_score += memory_score;
            reasons.push(format!(
                "memory delta {memory_delta:+.0}MB (+{memory_score})"
            ));
        }
        if !sample.last_log_line.is_empty()
            && !baseline.last_log_line.is_empty()
            && normalize(&sample.last_log_line) != normalize(&baseline.last_log_line)
        {
            divergence_score += config.log_mismatch_bonus.max(0);
            reasons.push(format!(
                "last-log divergence (+{})",
                config.log_mismatch_bonus
            ));
        }

        nodes.push(NodeCompareDelta {
            node_id: sample.node_id.clone(),
            label: sample.label.clone(),
            status: sample.status.clone(),
            status_match,
            queue_depth_delta,
            error_count_delta,
            cpu_delta,
            memory_delta,
            divergence_score,
            reasons,
        });
    }

    nodes.sort_by(|a, b| {
        b.divergence_score
            .cmp(&a.divergence_score)
            .then(b.error_count_delta.abs().cmp(&a.error_count_delta.abs()))
            .then(b.queue_depth_delta.abs().cmp(&a.queue_depth_delta.abs()))
            .then(a.node_id.cmp(&b.node_id))
    });
    nodes.truncate(config.limit.max(1));

    MultiNodeCompareReport {
        baseline_node_id: baseline.node_id,
        baseline_label: baseline.label,
        baseline_status: baseline.status,
        nodes,
        excluded_invalid,
        baseline_missing,
    }
}

#[must_use]
pub fn render_multi_node_compare_split(
    report: &MultiNodeCompareReport,
    width: usize,
    height: usize,
) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut lines = vec![
        fit_width("MULTI-NODE COMPARE", width),
        fit_width(
            &format!(
                "baseline:{} status:{} peers:{}",
                report.baseline_label,
                report.baseline_status,
                report.nodes.len()
            ),
            width,
        ),
    ];

    if report.baseline_missing {
        lines.push(fit_width(
            "note: requested baseline missing; using first node",
            width,
        ));
    }
    if report.nodes.is_empty() {
        lines.push(fit_width("no peer nodes to compare", width));
        lines.truncate(height);
        return lines;
    }

    for delta in &report.nodes {
        if lines.len() >= height {
            break;
        }
        lines.push(fit_width(
            &format!(
                "{} st:{} q:{:+} e:{:+} cpu:{:+.1}% mem:{:+.0} score:{}",
                delta.label,
                delta.status,
                delta.queue_depth_delta,
                delta.error_count_delta,
                delta.cpu_delta,
                delta.memory_delta,
                delta.divergence_score
            ),
            width,
        ));
    }

    lines.truncate(height);
    lines
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_label(label: &str, node_id: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        node_id.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn fit_width(value: &str, width: usize) -> String {
    if value.len() <= width {
        value.to_owned()
    } else {
        value.chars().take(width).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_multi_node_compare_report, render_multi_node_compare_split, MultiNodeCompareConfig,
        NodeCompareSample,
    };

    fn sample(
        node: &str,
        status: &str,
        queue: i64,
        errors: i64,
        cpu: f64,
        memory: f64,
        log: &str,
    ) -> NodeCompareSample {
        NodeCompareSample {
            node_id: node.to_owned(),
            label: node.to_owned(),
            status: status.to_owned(),
            queue_depth: queue,
            error_count: errors,
            cpu_percent: cpu,
            memory_mb: memory,
            last_log_line: log.to_owned(),
        }
    }

    #[test]
    fn falls_back_to_first_node_when_requested_baseline_missing() {
        let config = MultiNodeCompareConfig {
            baseline_node_id: Some("missing".to_owned()),
            ..MultiNodeCompareConfig::default()
        };
        let report = build_multi_node_compare_report(
            &[
                sample("node-a", "running", 1, 0, 40.0, 1024.0, "ok"),
                sample("node-b", "running", 2, 0, 41.0, 1024.0, "ok"),
            ],
            &config,
        );
        assert_eq!(report.baseline_node_id, "node-a");
        assert!(report.baseline_missing);
    }

    #[test]
    fn computes_divergence_and_sorts_desc() {
        let config = MultiNodeCompareConfig {
            baseline_node_id: Some("node-a".to_owned()),
            ..MultiNodeCompareConfig::default()
        };
        let report = build_multi_node_compare_report(
            &[
                sample("node-a", "running", 2, 1, 40.0, 1024.0, "ok"),
                sample("node-b", "running", 12, 5, 70.0, 2048.0, "warn"),
                sample("node-c", "running", 4, 1, 42.0, 1080.0, "ok"),
            ],
            &config,
        );
        assert_eq!(report.nodes.len(), 2);
        assert!(report.nodes[0].divergence_score >= report.nodes[1].divergence_score);
        assert_eq!(report.nodes[0].node_id, "node-b");
    }

    #[test]
    fn status_mismatch_penalty_applies() {
        let config = MultiNodeCompareConfig {
            baseline_node_id: Some("node-a".to_owned()),
            status_mismatch_penalty: 55,
            ..MultiNodeCompareConfig::default()
        };
        let report = build_multi_node_compare_report(
            &[
                sample("node-a", "running", 1, 0, 40.0, 1000.0, "ok"),
                sample("node-b", "error", 1, 0, 40.0, 1000.0, "ok"),
            ],
            &config,
        );
        assert_eq!(report.nodes.len(), 1);
        assert!(report.nodes[0].divergence_score >= 55);
        assert!(report.nodes[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("status mismatch")));
    }

    #[test]
    fn render_outputs_baseline_and_rows() {
        let report = build_multi_node_compare_report(
            &[
                sample("node-a", "running", 1, 0, 40.0, 1000.0, "ok"),
                sample("node-b", "running", 4, 2, 52.0, 1200.0, "warn"),
            ],
            &MultiNodeCompareConfig {
                baseline_node_id: Some("node-a".to_owned()),
                ..MultiNodeCompareConfig::default()
            },
        );
        let lines = render_multi_node_compare_split(&report, 120, 8);
        assert!(lines[0].contains("MULTI-NODE COMPARE"));
        assert!(lines[1].contains("baseline:node-a"));
        assert!(lines.iter().any(|line| line.contains("node-b")));
    }
}

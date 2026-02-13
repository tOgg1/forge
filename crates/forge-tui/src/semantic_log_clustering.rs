//! Semantic log clustering for deduplicating repeated error signatures.

use std::collections::{BTreeSet, HashMap};

use crate::lane_model::{classify_line, LogLane};

/// One concrete line instance belonging to an error cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorInstance {
    pub loop_id: String,
    pub line_index: usize,
    pub line: String,
}

/// Aggregated semantic error cluster across one or more loops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticErrorCluster {
    pub signature: String,
    pub representative: String,
    pub occurrences: usize,
    pub loop_count: usize,
    pub loops: Vec<String>,
    pub first_seen_index: usize,
    pub instances: Vec<ErrorInstance>,
}

#[derive(Debug, Default)]
struct ClusterBuilder {
    signature: String,
    representative: String,
    occurrences: usize,
    loops: BTreeSet<String>,
    first_seen_index: usize,
    instances: Vec<ErrorInstance>,
}

/// Cluster semantically similar error lines across loop log streams.
#[must_use]
pub fn cluster_semantic_errors_by_loop(
    logs_by_loop: &[(String, Vec<String>)],
) -> Vec<SemanticErrorCluster> {
    let mut clusters: HashMap<String, ClusterBuilder> = HashMap::new();

    for (loop_id, lines) in logs_by_loop {
        for (line_index, line) in lines.iter().enumerate() {
            let Some(signature) = canonical_error_signature(line) else {
                continue;
            };

            let clean_line = strip_anomaly_prefix(line).trim().to_owned();
            let entry = clusters
                .entry(signature.clone())
                .or_insert_with(|| ClusterBuilder {
                    signature,
                    representative: clean_line.clone(),
                    occurrences: 0,
                    loops: BTreeSet::new(),
                    first_seen_index: line_index,
                    instances: Vec::new(),
                });

            entry.occurrences = entry.occurrences.saturating_add(1);
            entry.loops.insert(loop_id.clone());
            if line_index < entry.first_seen_index {
                entry.first_seen_index = line_index;
            }
            if entry.instances.len() < 64 {
                entry.instances.push(ErrorInstance {
                    loop_id: loop_id.clone(),
                    line_index,
                    line: clean_line,
                });
            }
        }
    }

    let mut out: Vec<SemanticErrorCluster> = clusters
        .into_values()
        .map(|builder| SemanticErrorCluster {
            signature: builder.signature,
            representative: builder.representative,
            occurrences: builder.occurrences,
            loop_count: builder.loops.len(),
            loops: builder.loops.into_iter().collect(),
            first_seen_index: builder.first_seen_index,
            instances: builder.instances,
        })
        .collect();

    out.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| b.loop_count.cmp(&a.loop_count))
            .then_with(|| a.first_seen_index.cmp(&b.first_seen_index))
            .then_with(|| a.representative.cmp(&b.representative))
    });
    out
}

/// Render compact summary: `clusters:N top:<msg> xOCC/L`.
#[must_use]
pub fn compact_cluster_summary(clusters: &[SemanticErrorCluster], max_repr_chars: usize) -> String {
    if clusters.is_empty() {
        return "clusters:none".to_owned();
    }
    let top = &clusters[0];
    let representative = trim_chars(&top.representative, max_repr_chars.max(12));
    format!(
        "clusters:{} top:{} x{}/{}l",
        clusters.len(),
        representative,
        top.occurrences,
        top.loop_count
    )
}

fn canonical_error_signature(line: &str) -> Option<String> {
    let clean = strip_anomaly_prefix(line).trim();
    if clean.is_empty() {
        return None;
    }

    let lower = clean.to_ascii_lowercase();
    if classify_line(clean) != LogLane::Stderr && !looks_error_like(&lower) {
        return None;
    }

    let signature = lower
        .split_whitespace()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join(" ");
    let signature = signature.trim();
    if signature.is_empty() {
        None
    } else {
        Some(signature.to_owned())
    }
}

fn strip_anomaly_prefix(line: &str) -> &str {
    if !line.starts_with("! [ANOM:") {
        return line;
    }
    let Some(end) = line.find("] ") else {
        return line;
    };
    &line[end + 2..]
}

fn normalize_token(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    if looks_like_generated_id(token) {
        return "<id>".to_owned();
    }

    let mut normalized = String::with_capacity(token.len());
    let mut in_digits = false;
    for ch in token.chars() {
        if ch.is_ascii_digit() {
            if !in_digits {
                normalized.push('#');
                in_digits = true;
            }
        } else {
            in_digits = false;
            normalized.push(ch);
        }
    }
    normalized
}

fn looks_like_generated_id(token: &str) -> bool {
    let clean = token.trim_matches(|c: char| matches!(c, ',' | ';' | ':' | ')' | '(' | '[' | ']'));
    if clean.len() < 7 {
        return false;
    }

    let hexish = clean
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() || ch == '-' || ch == '_');
    let has_digit = clean.chars().any(|ch| ch.is_ascii_digit());
    let has_alpha = clean.chars().any(|ch| ch.is_ascii_alphabetic());
    hexish && has_digit && has_alpha
}

fn looks_error_like(lower: &str) -> bool {
    lower.contains("error")
        || lower.contains("failed")
        || lower.contains("panic")
        || lower.contains("fatal")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("exception")
        || lower.contains("refused")
        || lower.contains("denied")
        || lower.contains("exit code")
        || lower.contains("exit status")
}

fn trim_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    if max_chars <= 1 {
        return value.chars().take(max_chars).collect();
    }
    let mut out: String = value.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{cluster_semantic_errors_by_loop, compact_cluster_summary};

    #[test]
    fn clusters_similar_error_signatures_across_loops() {
        let logs = vec![
            (
                "loop-a".to_owned(),
                vec![
                    "Error: request timed out after 30s".to_owned(),
                    "Error: request timed out after 31s".to_owned(),
                ],
            ),
            (
                "loop-b".to_owned(),
                vec!["error: request timed out after 5s".to_owned()],
            ),
            (
                "loop-c".to_owned(),
                vec!["panic: invariant failed in worker".to_owned()],
            ),
        ];

        let clusters = cluster_semantic_errors_by_loop(&logs);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].occurrences, 3);
        assert_eq!(clusters[0].loop_count, 2);
        assert!(clusters[0].representative.contains("timed out"));
    }

    #[test]
    fn ignores_non_error_lines() {
        let logs = vec![(
            "loop-a".to_owned(),
            vec![
                "info: started".to_owned(),
                "tool: Bash(command=\"ls\")".to_owned(),
                "stdout: all good".to_owned(),
            ],
        )];
        let clusters = cluster_semantic_errors_by_loop(&logs);
        assert!(clusters.is_empty());
    }

    #[test]
    fn strips_anomaly_prefix_before_grouping() {
        let logs = vec![(
            "loop-a".to_owned(),
            vec![
                "! [ANOM:TIMEOUT] Error: connection refused".to_owned(),
                "Error: connection refused".to_owned(),
            ],
        )];
        let clusters = cluster_semantic_errors_by_loop(&logs);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].occurrences, 2);
    }

    #[test]
    fn compact_summary_includes_counts() {
        let logs = vec![(
            "loop-a".to_owned(),
            vec!["Error: request timed out after 30s".to_owned()],
        )];
        let clusters = cluster_semantic_errors_by_loop(&logs);
        let summary = compact_cluster_summary(&clusters, 24);
        assert!(summary.contains("clusters:1"));
        assert!(summary.contains("x1/1l"));
    }
}

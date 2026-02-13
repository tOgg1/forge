//! Smart error correlation engine for cross-loop failure clustering.

use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorSignal {
    pub loop_id: String,
    pub loop_name: String,
    pub message: String,
    pub stack_signature: Option<String>,
    pub observed_at_epoch_s: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrelationConfig {
    pub temporal_window_s: i64,
    pub similarity_threshold_pct: u8,
}

impl Default for CorrelationConfig {
    fn default() -> Self {
        Self {
            temporal_window_s: 90,
            similarity_threshold_pct: 62,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorCluster {
    pub cluster_id: String,
    pub canonical_signature: String,
    pub representative_message: String,
    pub loop_ids: Vec<String>,
    pub loop_count: usize,
    pub event_count: usize,
    pub first_seen_epoch_s: i64,
    pub last_seen_epoch_s: i64,
    pub confidence_pct: u8,
    pub sample_messages: Vec<String>,
}

#[derive(Debug, Clone)]
struct NormalizedSignal {
    loop_id: String,
    message: String,
    normalized_message: String,
    signature: String,
    has_stack_signature: bool,
    observed_at_epoch_s: i64,
}

#[derive(Debug, Clone)]
struct WorkingCluster {
    canonical_signature: String,
    first_seen_epoch_s: i64,
    last_seen_epoch_s: i64,
    loop_ids: BTreeSet<String>,
    event_count: usize,
    representative_message: String,
    sample_messages: Vec<String>,
    normalized_samples: Vec<String>,
    stack_signatures: BTreeSet<String>,
    similarity_sum: u64,
    similarity_count: u64,
}

impl WorkingCluster {
    fn new(signal: NormalizedSignal) -> Self {
        let mut loop_ids = BTreeSet::new();
        loop_ids.insert(signal.loop_id.clone());

        let mut stack_signatures = BTreeSet::new();
        if signal.has_stack_signature {
            stack_signatures.insert(signal.signature.clone());
        }

        Self {
            canonical_signature: signal.signature,
            first_seen_epoch_s: signal.observed_at_epoch_s,
            last_seen_epoch_s: signal.observed_at_epoch_s,
            loop_ids,
            event_count: 1,
            representative_message: signal.message.clone(),
            sample_messages: vec![signal.message],
            normalized_samples: vec![signal.normalized_message],
            stack_signatures,
            similarity_sum: 0,
            similarity_count: 0,
        }
    }

    fn add(&mut self, signal: NormalizedSignal, score: u8) {
        self.first_seen_epoch_s = self.first_seen_epoch_s.min(signal.observed_at_epoch_s);
        self.last_seen_epoch_s = self.last_seen_epoch_s.max(signal.observed_at_epoch_s);
        self.loop_ids.insert(signal.loop_id);
        self.event_count = self.event_count.saturating_add(1);
        if signal.has_stack_signature {
            self.stack_signatures.insert(signal.signature);
        }
        if self.sample_messages.len() < 3
            && !self
                .sample_messages
                .iter()
                .any(|msg| msg == &signal.message)
        {
            self.normalized_samples.push(signal.normalized_message);
            self.sample_messages.push(signal.message);
        }
        self.similarity_sum = self.similarity_sum.saturating_add(u64::from(score));
        self.similarity_count = self.similarity_count.saturating_add(1);
    }

    fn score(&self, signal: &NormalizedSignal) -> u8 {
        if signal.has_stack_signature && self.stack_signatures.contains(&signal.signature) {
            return 100;
        }
        let signature_score = similarity_pct(&signal.signature, &self.canonical_signature);
        let message_score = self
            .normalized_samples
            .iter()
            .map(|sample| similarity_pct(&signal.normalized_message, sample))
            .max()
            .unwrap_or(0);
        signature_score.max(message_score)
    }

    fn finish(self) -> ErrorCluster {
        let mean_similarity = if self.similarity_count == 0 {
            55
        } else {
            self.similarity_sum / self.similarity_count
        };
        let breadth_boost = ((self.loop_ids.len().saturating_sub(1) as u64) * 8).min(24);
        let confidence = if self.event_count <= 1 {
            45
        } else {
            (mean_similarity.saturating_mul(3) / 4 + breadth_boost).clamp(35, 100) as u8
        };

        let loop_count = self.loop_ids.len();
        ErrorCluster {
            cluster_id: String::new(),
            canonical_signature: self.canonical_signature,
            representative_message: self.representative_message,
            loop_ids: self.loop_ids.into_iter().collect(),
            loop_count,
            event_count: self.event_count,
            first_seen_epoch_s: self.first_seen_epoch_s,
            last_seen_epoch_s: self.last_seen_epoch_s,
            confidence_pct: confidence,
            sample_messages: self.sample_messages,
        }
    }
}

#[must_use]
pub fn correlate_errors(signals: &[ErrorSignal], config: CorrelationConfig) -> Vec<ErrorCluster> {
    let temporal_window_s = config.temporal_window_s.max(1);
    let threshold = config.similarity_threshold_pct.clamp(1, 100);

    let mut normalized = signals
        .iter()
        .filter_map(normalize_signal)
        .collect::<Vec<_>>();
    normalized.sort_by(|a, b| a.observed_at_epoch_s.cmp(&b.observed_at_epoch_s));

    let mut clusters: Vec<WorkingCluster> = Vec::new();
    for signal in normalized {
        let mut best: Option<(usize, u8)> = None;
        for (index, cluster) in clusters.iter().enumerate() {
            let distance = (signal.observed_at_epoch_s - cluster.last_seen_epoch_s).abs();
            if distance > temporal_window_s {
                continue;
            }
            let score = cluster.score(&signal);
            if score < threshold {
                continue;
            }
            if let Some((_, best_score)) = best {
                if score > best_score {
                    best = Some((index, score));
                }
            } else {
                best = Some((index, score));
            }
        }

        if let Some((index, score)) = best {
            if let Some(cluster) = clusters.get_mut(index) {
                cluster.add(signal, score);
            }
        } else {
            clusters.push(WorkingCluster::new(signal));
        }
    }

    let mut out = clusters
        .into_iter()
        .map(WorkingCluster::finish)
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.loop_count
            .cmp(&a.loop_count)
            .then(b.event_count.cmp(&a.event_count))
            .then(b.last_seen_epoch_s.cmp(&a.last_seen_epoch_s))
            .then(a.canonical_signature.cmp(&b.canonical_signature))
    });
    for (index, cluster) in out.iter_mut().enumerate() {
        cluster.cluster_id = format!("corr-{:03}", index + 1);
    }
    out
}

fn normalize_signal(signal: &ErrorSignal) -> Option<NormalizedSignal> {
    let loop_id = signal.loop_id.trim().to_owned();
    if loop_id.is_empty() {
        return None;
    }
    let message = normalize_ws(signal.message.trim());
    if message.is_empty() {
        return None;
    }
    let normalized_message = normalize_for_similarity(&message);
    let stack_signature = signal
        .stack_signature
        .as_deref()
        .map(normalize_for_similarity)
        .unwrap_or_default();
    let has_stack_signature = !stack_signature.is_empty();
    let signature = if has_stack_signature {
        stack_signature
    } else {
        signature_from_message(&normalized_message)
    };
    if signature.is_empty() {
        return None;
    }

    Some(NormalizedSignal {
        loop_id,
        message,
        normalized_message,
        signature,
        has_stack_signature,
        observed_at_epoch_s: signal.observed_at_epoch_s,
    })
}

fn normalize_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_for_similarity(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = false;
    for ch in input.chars() {
        let lowered = ch.to_ascii_lowercase();
        let normalized = if lowered.is_ascii_alphanumeric() {
            if lowered.is_ascii_digit() {
                '#'
            } else {
                lowered
            }
        } else {
            ' '
        };
        if normalized == ' ' {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(normalized);
            last_was_space = false;
        }
    }
    out.trim().to_owned()
}

fn signature_from_message(normalized_message: &str) -> String {
    let tokens = normalized_message
        .split_whitespace()
        .filter(|token| !is_noise_word(token))
        .take(8)
        .collect::<Vec<_>>();
    tokens.join(" ")
}

fn is_noise_word(token: &str) -> bool {
    matches!(
        token,
        "error"
            | "err"
            | "failed"
            | "failure"
            | "panic"
            | "at"
            | "line"
            | "column"
            | "loop"
            | "run"
    )
}

fn similarity_pct(left: &str, right: &str) -> u8 {
    if left.is_empty() || right.is_empty() {
        return 0;
    }
    if left == right {
        return 100;
    }

    let left_tokens = left
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let right_tokens = right
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let union = left_tokens.union(&right_tokens).count();
    let intersection = left_tokens.intersection(&right_tokens).count();
    let jaccard = if union == 0 {
        0
    } else {
        ((intersection * 100) / union) as u8
    };

    let prefix = common_prefix_pct(left, right);
    jaccard.max(prefix)
}

fn common_prefix_pct(left: &str, right: &str) -> u8 {
    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    let max_len = left_chars.len().max(right_chars.len());
    if max_len == 0 {
        return 0;
    }
    let mut matched = 0usize;
    for (a, b) in left_chars.iter().zip(right_chars.iter()) {
        if a == b {
            matched += 1;
        } else {
            break;
        }
    }
    ((matched * 100) / max_len) as u8
}

#[cfg(test)]
mod tests {
    use super::{correlate_errors, CorrelationConfig, ErrorSignal};

    #[test]
    fn stack_signature_clusters_six_loops_in_one_group() {
        let mut signals = Vec::new();
        for index in 0..6 {
            signals.push(ErrorSignal {
                loop_id: format!("loop-{index}"),
                loop_name: format!("Worker {index}"),
                message: format!("error: panic in worker {index}"),
                stack_signature: Some("forge::runner::execute -> forge::pool::dispatch".to_owned()),
                observed_at_epoch_s: 1_700_000_000 + index as i64,
            });
        }

        let clusters = correlate_errors(
            &signals,
            CorrelationConfig {
                temporal_window_s: 30,
                similarity_threshold_pct: 60,
            },
        );

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].loop_count, 6);
        assert_eq!(clusters[0].event_count, 6);
        assert!(clusters[0].confidence_pct >= 80);
    }

    #[test]
    fn temporal_window_split_prevents_false_merge() {
        let signals = vec![
            ErrorSignal {
                loop_id: "loop-a".to_owned(),
                loop_name: "A".to_owned(),
                message: "error: timeout waiting for daemon".to_owned(),
                stack_signature: Some("wait_for_daemon".to_owned()),
                observed_at_epoch_s: 100,
            },
            ErrorSignal {
                loop_id: "loop-b".to_owned(),
                loop_name: "B".to_owned(),
                message: "error: timeout waiting for daemon".to_owned(),
                stack_signature: Some("wait_for_daemon".to_owned()),
                observed_at_epoch_s: 500,
            },
        ];

        let clusters = correlate_errors(
            &signals,
            CorrelationConfig {
                temporal_window_s: 60,
                similarity_threshold_pct: 60,
            },
        );
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn message_similarity_clusters_without_stack_signature() {
        let signals = vec![
            ErrorSignal {
                loop_id: "loop-1".to_owned(),
                loop_name: "one".to_owned(),
                message: "error: timeout waiting for daemon pid 1234".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1_000,
            },
            ErrorSignal {
                loop_id: "loop-2".to_owned(),
                loop_name: "two".to_owned(),
                message: "ERROR timeout waiting for daemon pid 9876".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1_015,
            },
            ErrorSignal {
                loop_id: "loop-3".to_owned(),
                loop_name: "three".to_owned(),
                message: "network unreachable while pulling repo".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1_020,
            },
        ];

        let clusters = correlate_errors(
            &signals,
            CorrelationConfig {
                temporal_window_s: 90,
                similarity_threshold_pct: 60,
            },
        );

        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].loop_count, 2);
        assert!(clusters[0].canonical_signature.contains("timeout"));
    }

    #[test]
    fn ignores_empty_loop_id_or_message() {
        let signals = vec![
            ErrorSignal {
                loop_id: " ".to_owned(),
                loop_name: "bad".to_owned(),
                message: "error: boom".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1,
            },
            ErrorSignal {
                loop_id: "loop-1".to_owned(),
                loop_name: "ok".to_owned(),
                message: " ".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1,
            },
            ErrorSignal {
                loop_id: "loop-2".to_owned(),
                loop_name: "ok".to_owned(),
                message: "error: panic in pool dispatch".to_owned(),
                stack_signature: None,
                observed_at_epoch_s: 1,
            },
        ];

        let clusters = correlate_errors(&signals, CorrelationConfig::default());
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].loop_ids, vec!["loop-2"]);
    }
}

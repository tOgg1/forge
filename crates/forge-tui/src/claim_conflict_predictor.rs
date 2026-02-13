//! Predict likely claim collisions before explicit conflict resolution.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimSignalSample {
    pub task_id: String,
    pub claimed_by: String,
    pub claimed_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimConflictPredictorConfig {
    pub lookback_window_s: i64,
    pub fresh_window_s: i64,
    pub min_score: i32,
    pub limit: usize,
}

impl Default for ClaimConflictPredictorConfig {
    fn default() -> Self {
        Self {
            lookback_window_s: 3 * 60 * 60,
            fresh_window_s: 15 * 60,
            min_score: 45,
            limit: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimConflictRisk {
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimConflictPrediction {
    pub task_id: String,
    pub latest_owner: String,
    pub previous_owner: Option<String>,
    pub score: i32,
    pub risk: ClaimConflictRisk,
    pub reasons: Vec<String>,
}

#[must_use]
pub fn predict_claim_conflicts(
    samples: &[ClaimSignalSample],
    now_epoch_s: i64,
    config: &ClaimConflictPredictorConfig,
) -> Vec<ClaimConflictPrediction> {
    let mut by_task: HashMap<String, Vec<&ClaimSignalSample>> = HashMap::new();
    for sample in samples {
        let task_id = normalize(&sample.task_id);
        let claimed_by = normalize(&sample.claimed_by);
        if task_id.is_empty() || claimed_by.is_empty() {
            continue;
        }
        if sample.claimed_at_epoch_s > now_epoch_s {
            continue;
        }
        if now_epoch_s - sample.claimed_at_epoch_s > config.lookback_window_s {
            continue;
        }
        by_task.entry(task_id).or_default().push(sample);
    }

    let mut predictions = Vec::new();
    for (task_id, mut task_events) in by_task {
        if task_events.len() < 2 {
            continue;
        }
        task_events.sort_by(|a, b| b.claimed_at_epoch_s.cmp(&a.claimed_at_epoch_s));

        let latest = task_events[0];
        let previous = task_events
            .iter()
            .skip(1)
            .find(|event| normalize(&event.claimed_by) != normalize(&latest.claimed_by));

        let Some(previous) = previous else {
            continue;
        };

        let unique_agents = task_events
            .iter()
            .map(|event| normalize(&event.claimed_by))
            .collect::<HashSet<String>>()
            .len();
        let ownership_switches = ownership_switches(&task_events);
        let recent_conflict = latest.claimed_at_epoch_s - previous.claimed_at_epoch_s;
        let is_fresh = now_epoch_s - latest.claimed_at_epoch_s <= config.fresh_window_s;

        let mut score = 0;
        let mut reasons = Vec::new();

        if unique_agents >= 2 {
            score += 28;
            reasons.push(format!("multi-owner:{} (+28)", unique_agents));
        }
        if unique_agents >= 3 {
            score += 14;
            reasons.push("owner-churn-3plus (+14)".to_owned());
        }
        if ownership_switches >= 2 {
            score += 22;
            reasons.push(format!("ownership-switches:{} (+22)", ownership_switches));
        } else if ownership_switches == 1 {
            score += 12;
            reasons.push("single-switch (+12)".to_owned());
        }
        if task_events.len() >= 4 {
            score += 10;
            reasons.push(format!("claim-volume:{} (+10)", task_events.len()));
        }
        if is_fresh {
            score += 16;
            reasons.push("fresh-claim (+16)".to_owned());
        }
        if recent_conflict <= config.fresh_window_s {
            score += 18;
            reasons.push(format!("rapid-reclaim:{}s (+18)", recent_conflict.max(0)));
        }

        if score < config.min_score {
            continue;
        }

        predictions.push(ClaimConflictPrediction {
            task_id,
            latest_owner: normalize(&latest.claimed_by),
            previous_owner: Some(normalize(&previous.claimed_by)),
            score,
            risk: classify_risk(score),
            reasons,
        });
    }

    predictions.sort_by(|a, b| b.score.cmp(&a.score).then(a.task_id.cmp(&b.task_id)));
    predictions.truncate(config.limit.max(1));
    predictions
}

fn ownership_switches(events_desc: &[&ClaimSignalSample]) -> usize {
    if events_desc.is_empty() {
        return 0;
    }
    let mut switches = 0usize;
    let mut prev_owner = normalize(&events_desc[0].claimed_by);
    for event in events_desc.iter().skip(1) {
        let owner = normalize(&event.claimed_by);
        if owner != prev_owner {
            switches += 1;
            prev_owner = owner;
        }
    }
    switches
}

fn classify_risk(score: i32) -> ClaimConflictRisk {
    if score >= 80 {
        ClaimConflictRisk::Critical
    } else if score >= 60 {
        ClaimConflictRisk::High
    } else {
        ClaimConflictRisk::Medium
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        predict_claim_conflicts, ClaimConflictPredictorConfig, ClaimConflictRisk, ClaimSignalSample,
    };

    fn sample(task: &str, owner: &str, ts: i64) -> ClaimSignalSample {
        ClaimSignalSample {
            task_id: task.to_owned(),
            claimed_by: owner.to_owned(),
            claimed_at_epoch_s: ts,
        }
    }

    #[test]
    fn predicts_conflict_on_rapid_owner_flips() {
        let now = 2_000_000;
        let samples = vec![
            sample("forge-a", "agent-1", now - 10),
            sample("forge-a", "agent-2", now - 50),
            sample("forge-a", "agent-1", now - 90),
            sample("forge-a", "agent-2", now - 130),
        ];
        let predictions =
            predict_claim_conflicts(&samples, now, &ClaimConflictPredictorConfig::default());
        assert_eq!(predictions.len(), 1);
        assert_eq!(predictions[0].task_id, "forge-a");
        assert!(matches!(
            predictions[0].risk,
            ClaimConflictRisk::High | ClaimConflictRisk::Critical
        ));
    }

    #[test]
    fn ignores_single_owner_claim_stream() {
        let now = 2_000_000;
        let samples = vec![
            sample("forge-a", "agent-1", now - 10),
            sample("forge-a", "agent-1", now - 50),
            sample("forge-a", "agent-1", now - 90),
        ];
        let predictions =
            predict_claim_conflicts(&samples, now, &ClaimConflictPredictorConfig::default());
        assert!(predictions.is_empty());
    }

    #[test]
    fn drops_stale_events_outside_lookback_window() {
        let now = 2_000_000;
        let samples = vec![
            sample("forge-a", "agent-1", now - 20_000),
            sample("forge-a", "agent-2", now - 18_000),
        ];
        let predictions =
            predict_claim_conflicts(&samples, now, &ClaimConflictPredictorConfig::default());
        assert!(predictions.is_empty());
    }

    #[test]
    fn sorts_predictions_by_score_desc() {
        let now = 2_000_000;
        let samples = vec![
            sample("forge-a", "agent-1", now - 10),
            sample("forge-a", "agent-2", now - 20),
            sample("forge-a", "agent-1", now - 30),
            sample("forge-b", "agent-1", now - 10),
            sample("forge-b", "agent-2", now - 580),
        ];
        let predictions =
            predict_claim_conflicts(&samples, now, &ClaimConflictPredictorConfig::default());
        assert_eq!(predictions.len(), 2);
        assert!(predictions[0].score >= predictions[1].score);
        assert_eq!(predictions[0].task_id, "forge-a");
    }
}

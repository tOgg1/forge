//! Concurrency governor and starvation prevention for swarm orchestration.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolUsageSample {
    pub pool: String,
    pub profile: String,
    pub active_loops: usize,
    pub queued_tasks: usize,
    pub profile_capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernorPolicy {
    pub starvation_queue_threshold: usize,
    pub min_reserved_active_per_pool: usize,
}

impl Default for GovernorPolicy {
    fn default() -> Self {
        Self {
            starvation_queue_threshold: 3,
            min_reserved_active_per_pool: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrottleRecommendation {
    pub pool: String,
    pub profile: String,
    pub reduce_active_by: usize,
    pub new_soft_limit: usize,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GovernorReport {
    pub starvation_detected: bool,
    pub starved_pools: Vec<String>,
    pub recommendations: Vec<ThrottleRecommendation>,
    pub notes: Vec<String>,
}

#[must_use]
pub fn evaluate_concurrency_governor(
    samples: &[PoolUsageSample],
    policy: &GovernorPolicy,
) -> GovernorReport {
    let mut starved_pool_set = BTreeSet::new();
    let mut notes = Vec::new();
    let mut recommendations = Vec::new();
    let mut profile_active: BTreeMap<&str, usize> = BTreeMap::new();
    let mut profile_capacity: BTreeMap<&str, usize> = BTreeMap::new();

    for sample in samples {
        let profile = sample.profile.trim();
        if profile.is_empty() {
            continue;
        }
        let active = profile_active.get(profile).copied().unwrap_or(0);
        profile_active.insert(profile, active.saturating_add(sample.active_loops));
        let capacity = profile_capacity.get(profile).copied().unwrap_or(0);
        profile_capacity.insert(profile, capacity.saturating_add(sample.profile_capacity));
        if sample.active_loops > sample.profile_capacity {
            notes.push(format!(
                "pool {} exceeds profile {} capacity: {} > {}",
                sample.pool, sample.profile, sample.active_loops, sample.profile_capacity
            ));
        }
    }

    for sample in samples {
        if sample.queued_tasks < policy.starvation_queue_threshold {
            continue;
        }
        if sample.active_loops < sample.profile_capacity {
            continue;
        }
        starved_pool_set.insert(sample.pool.clone());

        let donor = samples
            .iter()
            .filter(|candidate| {
                candidate.profile == sample.profile
                    && candidate.pool != sample.pool
                    && candidate.queued_tasks == 0
                    && candidate.active_loops > policy.min_reserved_active_per_pool
            })
            .max_by(|a, b| {
                a.active_loops
                    .cmp(&b.active_loops)
                    .then_with(|| b.pool.cmp(&a.pool))
            });

        if let Some(donor) = donor {
            let new_soft_limit = donor.active_loops.saturating_sub(1);
            recommendations.push(ThrottleRecommendation {
                pool: donor.pool.clone(),
                profile: donor.profile.clone(),
                reduce_active_by: 1,
                new_soft_limit,
                reason: format!(
                    "free slot for starved pool {} (queue={})",
                    sample.pool, sample.queued_tasks
                ),
            });
        } else {
            notes.push(format!(
                "no safe donor pool for {} on profile {}",
                sample.pool, sample.profile
            ));
            let profile = sample.profile.as_str();
            let active = profile_active.get(profile).copied().unwrap_or(0);
            let capacity = profile_capacity.get(profile).copied().unwrap_or(0);
            if active >= capacity && capacity > 0 {
                notes.push(format!(
                    "profile {} exhausted: active {} / capacity {}",
                    sample.profile, active, capacity
                ));
            }
        }
    }

    recommendations.sort_by(|a, b| a.pool.cmp(&b.pool).then(a.profile.cmp(&b.profile)));
    recommendations.dedup_by(|a, b| a.pool == b.pool && a.profile == b.profile);

    GovernorReport {
        starvation_detected: !starved_pool_set.is_empty(),
        starved_pools: starved_pool_set.into_iter().collect(),
        recommendations,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_concurrency_governor, GovernorPolicy, PoolUsageSample};

    #[test]
    fn starvation_generates_throttle_for_idle_queue_peer() {
        let samples = vec![
            PoolUsageSample {
                pool: "pool-a".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 8,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-b".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 2,
                queued_tasks: 0,
                profile_capacity: 3,
            },
        ];
        let report = evaluate_concurrency_governor(&samples, &GovernorPolicy::default());
        assert!(report.starvation_detected);
        assert_eq!(report.starved_pools, vec!["pool-a"]);
        assert_eq!(report.recommendations.len(), 1);
        assert_eq!(report.recommendations[0].pool, "pool-b");
        assert_eq!(report.recommendations[0].new_soft_limit, 1);
    }

    #[test]
    fn donor_is_not_throttled_below_reserved_floor() {
        let samples = vec![
            PoolUsageSample {
                pool: "pool-a".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 8,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-b".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 1,
                queued_tasks: 0,
                profile_capacity: 3,
            },
        ];
        let report = evaluate_concurrency_governor(&samples, &GovernorPolicy::default());
        assert!(report.starvation_detected);
        assert!(report.recommendations.is_empty());
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("no safe donor pool")));
    }

    #[test]
    fn low_queue_pressure_does_not_mark_starvation() {
        let samples = vec![
            PoolUsageSample {
                pool: "pool-a".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 2,
                queued_tasks: 1,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-b".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 2,
                queued_tasks: 0,
                profile_capacity: 3,
            },
        ];
        let report = evaluate_concurrency_governor(&samples, &GovernorPolicy::default());
        assert!(!report.starvation_detected);
        assert!(report.recommendations.is_empty());
    }

    #[test]
    fn notes_profile_exhaustion_when_starvation_has_no_donor() {
        let samples = vec![
            PoolUsageSample {
                pool: "pool-a".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 9,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-b".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 2,
                queued_tasks: 4,
                profile_capacity: 2,
            },
        ];
        let report = evaluate_concurrency_governor(&samples, &GovernorPolicy::default());
        assert!(report.starvation_detected);
        assert!(report.recommendations.is_empty());
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("profile codex3 exhausted")));
    }

    #[test]
    fn recommendations_are_deduplicated_by_pool_and_profile() {
        let samples = vec![
            PoolUsageSample {
                pool: "pool-a".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 8,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-c".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 7,
                profile_capacity: 3,
            },
            PoolUsageSample {
                pool: "pool-b".to_owned(),
                profile: "codex3".to_owned(),
                active_loops: 3,
                queued_tasks: 0,
                profile_capacity: 3,
            },
        ];
        let report = evaluate_concurrency_governor(&samples, &GovernorPolicy::default());
        assert_eq!(report.recommendations.len(), 1);
        assert_eq!(report.recommendations[0].pool, "pool-b");
    }
}

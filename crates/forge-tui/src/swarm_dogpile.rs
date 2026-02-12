//! Dogpile detection and redistribution planning for swarm orchestration.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskClaimSample {
    pub task_id: String,
    pub loop_id: String,
    pub agent: String,
    pub claimed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopLoadSample {
    pub loop_id: String,
    pub agent: String,
    pub active_tasks: usize,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DogpileAlert {
    pub task_id: String,
    pub keeper_loop: String,
    pub keeper_agent: String,
    pub claimant_loops: Vec<String>,
    pub claimant_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedistributionAction {
    pub task_id: String,
    pub from_loop: String,
    pub to_loop: String,
    pub reason: String,
    pub command_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DogpileReport {
    pub alerts: Vec<DogpileAlert>,
    pub actions: Vec<RedistributionAction>,
}

#[must_use]
pub fn detect_dogpile_report(
    claims: &[TaskClaimSample],
    loop_loads: &[LoopLoadSample],
    min_duplicate_claims: usize,
) -> DogpileReport {
    if min_duplicate_claims < 2 {
        return DogpileReport::default();
    }

    let mut claims_by_task: BTreeMap<String, Vec<&TaskClaimSample>> = BTreeMap::new();
    for claim in claims {
        if claim.task_id.trim().is_empty() {
            continue;
        }
        claims_by_task
            .entry(claim.task_id.clone())
            .or_default()
            .push(claim);
    }

    let mut alerts = Vec::new();
    let mut actions = Vec::new();

    for (task_id, mut task_claims) in claims_by_task {
        task_claims.sort_by(|a, b| {
            a.claimed_at
                .cmp(&b.claimed_at)
                .then(a.loop_id.cmp(&b.loop_id))
                .then(a.agent.cmp(&b.agent))
        });

        let mut distinct_claimant_keys = BTreeSet::new();
        for claim in &task_claims {
            distinct_claimant_keys.insert(format!(
                "{}:{}",
                claim.loop_id.trim().to_ascii_lowercase(),
                claim.agent.trim().to_ascii_lowercase()
            ));
        }
        if distinct_claimant_keys.len() < min_duplicate_claims {
            continue;
        }

        let Some(keeper) = task_claims.first() else {
            continue;
        };

        let mut claimant_loops: Vec<String> = task_claims
            .iter()
            .map(|claim| normalize_non_empty(&claim.loop_id, "unknown-loop"))
            .collect();
        claimant_loops.sort();
        claimant_loops.dedup();

        let mut claimant_agents: Vec<String> = task_claims
            .iter()
            .map(|claim| normalize_non_empty(&claim.agent, "unknown-agent"))
            .collect();
        claimant_agents.sort();
        claimant_agents.dedup();

        alerts.push(DogpileAlert {
            task_id: task_id.clone(),
            keeper_loop: normalize_non_empty(&keeper.loop_id, "unknown-loop"),
            keeper_agent: normalize_non_empty(&keeper.agent, "unknown-agent"),
            claimant_loops: claimant_loops.clone(),
            claimant_agents,
        });

        for duplicate in task_claims.iter().skip(1) {
            let from_loop = normalize_non_empty(&duplicate.loop_id, "unknown-loop");
            let from_agent = normalize_non_empty(&duplicate.agent, "unknown-agent");
            let to_loop = select_redistribution_target(loop_loads, &claimant_loops)
                .unwrap_or_else(|| from_loop.clone());
            let reason = format!(
                "task {} has {} concurrent claimants",
                task_id,
                distinct_claimant_keys.len()
            );
            let command_hint = if to_loop == from_loop {
                format!(
                    "fmail send task \"release claim: {} by {}; picking next ready\"",
                    task_id, from_agent
                )
            } else {
                format!(
                    "fmail send task \"handoff {}: {} -> {}\"",
                    task_id, from_loop, to_loop
                )
            };
            actions.push(RedistributionAction {
                task_id: task_id.clone(),
                from_loop,
                to_loop,
                reason,
                command_hint,
            });
        }
    }

    actions.sort_by(|a, b| {
        a.task_id
            .cmp(&b.task_id)
            .then(a.from_loop.cmp(&b.from_loop))
            .then(a.to_loop.cmp(&b.to_loop))
    });

    DogpileReport { alerts, actions }
}

fn select_redistribution_target(
    loop_loads: &[LoopLoadSample],
    claimant_loops: &[String],
) -> Option<String> {
    let claimant_set: BTreeSet<&str> = claimant_loops.iter().map(String::as_str).collect();
    loop_loads
        .iter()
        .filter(|sample| {
            !sample.loop_id.trim().is_empty()
                && !claimant_set.contains(sample.loop_id.trim())
                && sample.active_tasks == 0
        })
        .min_by(|a, b| {
            a.queue_depth
                .cmp(&b.queue_depth)
                .then(a.active_tasks.cmp(&b.active_tasks))
                .then(a.loop_id.cmp(&b.loop_id))
        })
        .map(|sample| sample.loop_id.clone())
}

fn normalize_non_empty(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_dogpile_report, LoopLoadSample, TaskClaimSample};

    #[test]
    fn detects_dogpile_when_task_has_multiple_claimants() {
        let claims = vec![
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-a".to_owned(),
                agent: "agent-a".to_owned(),
                claimed_at: "2026-02-12T08:00:00Z".to_owned(),
            },
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-b".to_owned(),
                agent: "agent-b".to_owned(),
                claimed_at: "2026-02-12T08:01:00Z".to_owned(),
            },
        ];
        let report = detect_dogpile_report(&claims, &[], 2);
        assert_eq!(report.alerts.len(), 1);
        assert_eq!(report.alerts[0].task_id, "forge-abc");
        assert_eq!(report.alerts[0].keeper_loop, "loop-a");
    }

    #[test]
    fn chooses_idle_non_claimant_loop_for_redistribution() {
        let claims = vec![
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-a".to_owned(),
                agent: "agent-a".to_owned(),
                claimed_at: "2026-02-12T08:00:00Z".to_owned(),
            },
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-b".to_owned(),
                agent: "agent-b".to_owned(),
                claimed_at: "2026-02-12T08:01:00Z".to_owned(),
            },
        ];
        let loads = vec![
            LoopLoadSample {
                loop_id: "loop-c".to_owned(),
                agent: "agent-c".to_owned(),
                active_tasks: 0,
                queue_depth: 0,
            },
            LoopLoadSample {
                loop_id: "loop-d".to_owned(),
                agent: "agent-d".to_owned(),
                active_tasks: 0,
                queue_depth: 3,
            },
        ];
        let report = detect_dogpile_report(&claims, &loads, 2);
        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.actions[0].from_loop, "loop-b");
        assert_eq!(report.actions[0].to_loop, "loop-c");
    }

    #[test]
    fn falls_back_to_release_hint_when_no_target_loop_available() {
        let claims = vec![
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-a".to_owned(),
                agent: "agent-a".to_owned(),
                claimed_at: "2026-02-12T08:00:00Z".to_owned(),
            },
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-b".to_owned(),
                agent: "agent-b".to_owned(),
                claimed_at: "2026-02-12T08:01:00Z".to_owned(),
            },
        ];
        let loads = vec![LoopLoadSample {
            loop_id: "loop-a".to_owned(),
            agent: "agent-a".to_owned(),
            active_tasks: 1,
            queue_depth: 2,
        }];
        let report = detect_dogpile_report(&claims, &loads, 2);
        assert_eq!(report.actions.len(), 1);
        assert_eq!(report.actions[0].to_loop, "loop-b");
        assert!(report.actions[0].command_hint.contains("release claim"));
    }

    #[test]
    fn ignores_tasks_below_duplicate_threshold() {
        let claims = vec![TaskClaimSample {
            task_id: "forge-abc".to_owned(),
            loop_id: "loop-a".to_owned(),
            agent: "agent-a".to_owned(),
            claimed_at: "2026-02-12T08:00:00Z".to_owned(),
        }];
        let report = detect_dogpile_report(&claims, &[], 2);
        assert!(report.alerts.is_empty());
        assert!(report.actions.is_empty());
    }

    #[test]
    fn min_duplicate_claims_below_two_disables_detection() {
        let claims = vec![
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-a".to_owned(),
                agent: "agent-a".to_owned(),
                claimed_at: "2026-02-12T08:00:00Z".to_owned(),
            },
            TaskClaimSample {
                task_id: "forge-abc".to_owned(),
                loop_id: "loop-b".to_owned(),
                agent: "agent-b".to_owned(),
                claimed_at: "2026-02-12T08:01:00Z".to_owned(),
            },
        ];
        let report = detect_dogpile_report(&claims, &[], 1);
        assert!(report.alerts.is_empty());
    }
}

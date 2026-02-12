//! Stale in-progress detector with takeover and recovery suggestions.
//!
//! Models stale task/loop detection with explicit false-positive mitigation
//! controls so TUI can surface safer recommendations.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleTaskSample {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub owner: Option<String>,
    pub updated_at_epoch_s: i64,
    pub stale_observation_count: usize,
    pub last_activity_epoch_s: Option<i64>,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleLoopSample {
    pub loop_id: String,
    pub state: String,
    pub owner: Option<String>,
    pub updated_at_epoch_s: i64,
    pub stale_observation_count: usize,
    pub last_activity_epoch_s: Option<i64>,
    pub queue_depth: usize,
    pub active_tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleDetectionPolicy {
    pub task_stale_after_secs: u64,
    pub loop_stale_after_secs: u64,
    pub required_observations: usize,
    pub recent_activity_grace_secs: u64,
    pub min_loop_queue_depth: usize,
}

impl Default for StaleDetectionPolicy {
    fn default() -> Self {
        Self {
            task_stale_after_secs: 2_700,
            loop_stale_after_secs: 1_800,
            required_observations: 2,
            recent_activity_grace_secs: 300,
            min_loop_queue_depth: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleEntityKind {
    Task,
    Loop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleSeverity {
    Watch,
    Takeover,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleSuggestion {
    pub headline: String,
    pub command_hint: String,
    pub safeguards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleAlert {
    pub kind: StaleEntityKind,
    pub id: String,
    pub owner: Option<String>,
    pub stale_for_secs: u64,
    pub severity: StaleSeverity,
    pub reasons: Vec<String>,
    pub suggestion: StaleSuggestion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressedStaleCandidate {
    pub kind: StaleEntityKind,
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StaleTakeoverReport {
    pub alerts: Vec<StaleAlert>,
    pub suppressed: Vec<SuppressedStaleCandidate>,
}

#[must_use]
pub fn build_stale_takeover_report(
    tasks: &[StaleTaskSample],
    loops: &[StaleLoopSample],
    now_epoch_s: i64,
    policy: &StaleDetectionPolicy,
) -> StaleTakeoverReport {
    let now_epoch_s = now_epoch_s.max(0);
    let mut alerts = Vec::new();
    let mut suppressed = Vec::new();

    for task in tasks {
        evaluate_task(task, now_epoch_s, policy, &mut alerts, &mut suppressed);
    }
    for loop_entry in loops {
        evaluate_loop(
            loop_entry,
            now_epoch_s,
            policy,
            &mut alerts,
            &mut suppressed,
        );
    }

    alerts.sort_by(|a, b| {
        b.stale_for_secs
            .cmp(&a.stale_for_secs)
            .then(a.id.cmp(&b.id))
    });
    suppressed.sort_by(|a, b| a.id.cmp(&b.id).then(a.reason.cmp(&b.reason)));

    StaleTakeoverReport { alerts, suppressed }
}

fn evaluate_task(
    task: &StaleTaskSample,
    now_epoch_s: i64,
    policy: &StaleDetectionPolicy,
    alerts: &mut Vec<StaleAlert>,
    suppressed: &mut Vec<SuppressedStaleCandidate>,
) {
    let task_id = normalize_required(&task.task_id);
    if task_id.is_empty() {
        return;
    }

    let status = normalize_required(&task.status);
    if !is_active_task_status(&status) {
        push_suppressed(
            suppressed,
            StaleEntityKind::Task,
            task_id,
            format!("status '{}' is not takeover-eligible", task.status.trim()),
        );
        return;
    }

    let stale_for_secs = age_seconds(now_epoch_s, task.updated_at_epoch_s);
    if stale_for_secs < policy.task_stale_after_secs {
        push_suppressed(
            suppressed,
            StaleEntityKind::Task,
            task_id,
            format!(
                "stale window not reached ({}s < {}s)",
                stale_for_secs, policy.task_stale_after_secs
            ),
        );
        return;
    }

    if task.stale_observation_count < policy.required_observations {
        push_suppressed(
            suppressed,
            StaleEntityKind::Task,
            task_id,
            format!(
                "false-positive mitigation: observations {} < required {}",
                task.stale_observation_count, policy.required_observations
            ),
        );
        return;
    }

    if let Some(last_activity_epoch_s) = task.last_activity_epoch_s {
        let idle_for_secs = age_seconds(now_epoch_s, last_activity_epoch_s);
        if idle_for_secs < policy.recent_activity_grace_secs {
            push_suppressed(
                suppressed,
                StaleEntityKind::Task,
                task_id,
                format!(
                    "false-positive mitigation: recent activity {}s ago (<{}s grace)",
                    idle_for_secs, policy.recent_activity_grace_secs
                ),
            );
            return;
        }
    }

    let owner = normalize_optional(task.owner.as_deref());
    let blocked = task
        .blocked_by
        .iter()
        .filter_map(|dependency| {
            let value = normalize_required(dependency);
            if value.is_empty() || value == task_id {
                None
            } else {
                Some(value)
            }
        })
        .next()
        .is_some();

    let mut reasons = vec![
        format!("status={} stale_for={}s", status, stale_for_secs),
        format!(
            "observations={} (required {})",
            task.stale_observation_count, policy.required_observations
        ),
    ];
    if blocked {
        reasons.push("task has blockers; takeover may be ineffective".to_owned());
    }

    let (severity, suggestion) = if blocked {
        (
            StaleSeverity::Watch,
            StaleSuggestion {
                headline: "Blocked stale task: clear blockers first".to_owned(),
                command_hint: format!("sv task show {task_id} --json"),
                safeguards: vec![
                    "confirm blocker owner is active before takeover".to_owned(),
                    "request unblock plan in task topic".to_owned(),
                ],
            },
        )
    } else {
        (
            StaleSeverity::Takeover,
            StaleSuggestion {
                headline: "Stale in-progress task: prepare takeover claim".to_owned(),
                command_hint: format!(
                    "fmail send task \"takeover claim: {} by <agent>\" && sv task start {}",
                    task_id, task_id
                ),
                safeguards: vec![
                    "confirm owner idle window exceeds policy".to_owned(),
                    "post takeover claim before running commands".to_owned(),
                    "include rationale + rollback handoff in task thread".to_owned(),
                ],
            },
        )
    };

    alerts.push(StaleAlert {
        kind: StaleEntityKind::Task,
        id: task_id,
        owner,
        stale_for_secs,
        severity,
        reasons,
        suggestion,
    });
}

fn evaluate_loop(
    loop_entry: &StaleLoopSample,
    now_epoch_s: i64,
    policy: &StaleDetectionPolicy,
    alerts: &mut Vec<StaleAlert>,
    suppressed: &mut Vec<SuppressedStaleCandidate>,
) {
    let loop_id = normalize_required(&loop_entry.loop_id);
    if loop_id.is_empty() {
        return;
    }

    let state = normalize_required(&loop_entry.state);
    if !is_active_loop_state(&state) {
        push_suppressed(
            suppressed,
            StaleEntityKind::Loop,
            loop_id,
            format!("state '{}' is not active", loop_entry.state.trim()),
        );
        return;
    }

    let stale_for_secs = age_seconds(now_epoch_s, loop_entry.updated_at_epoch_s);
    if stale_for_secs < policy.loop_stale_after_secs {
        push_suppressed(
            suppressed,
            StaleEntityKind::Loop,
            loop_id,
            format!(
                "stale window not reached ({}s < {}s)",
                stale_for_secs, policy.loop_stale_after_secs
            ),
        );
        return;
    }

    if loop_entry.stale_observation_count < policy.required_observations {
        push_suppressed(
            suppressed,
            StaleEntityKind::Loop,
            loop_id,
            format!(
                "false-positive mitigation: observations {} < required {}",
                loop_entry.stale_observation_count, policy.required_observations
            ),
        );
        return;
    }

    if loop_entry.active_tasks == 0 && loop_entry.queue_depth < policy.min_loop_queue_depth {
        push_suppressed(
            suppressed,
            StaleEntityKind::Loop,
            loop_id,
            format!(
                "false-positive mitigation: queue depth {} < minimum {}",
                loop_entry.queue_depth, policy.min_loop_queue_depth
            ),
        );
        return;
    }

    if let Some(last_activity_epoch_s) = loop_entry.last_activity_epoch_s {
        let idle_for_secs = age_seconds(now_epoch_s, last_activity_epoch_s);
        if idle_for_secs < policy.recent_activity_grace_secs {
            push_suppressed(
                suppressed,
                StaleEntityKind::Loop,
                loop_id,
                format!(
                    "false-positive mitigation: recent activity {}s ago (<{}s grace)",
                    idle_for_secs, policy.recent_activity_grace_secs
                ),
            );
            return;
        }
    }

    alerts.push(StaleAlert {
        kind: StaleEntityKind::Loop,
        id: loop_id,
        owner: normalize_optional(loop_entry.owner.as_deref()),
        stale_for_secs,
        severity: StaleSeverity::Watch,
        reasons: vec![
            format!("state={} stale_for={}s", state, stale_for_secs),
            format!(
                "queue_depth={} active_tasks={}",
                loop_entry.queue_depth, loop_entry.active_tasks
            ),
        ],
        suggestion: StaleSuggestion {
            headline: "Stale active loop: run recovery workflow".to_owned(),
            command_hint: format!(
                "forge ps --loop {} && forge msg --loop {} --template stop-and-refocus",
                normalize_required(&loop_entry.loop_id),
                normalize_required(&loop_entry.loop_id)
            ),
            safeguards: vec![
                "check latest logs before intervention".to_owned(),
                "prefer guided msg/refocus before hard stop".to_owned(),
            ],
        },
    });
}

fn push_suppressed(
    suppressed: &mut Vec<SuppressedStaleCandidate>,
    kind: StaleEntityKind,
    id: String,
    reason: String,
) {
    suppressed.push(SuppressedStaleCandidate { kind, id, reason });
}

fn is_active_task_status(status: &str) -> bool {
    matches!(status, "in_progress" | "running" | "active" | "started")
}

fn is_active_loop_state(state: &str) -> bool {
    matches!(state, "running" | "waiting" | "sleeping")
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let value = value?;
    let normalized = normalize_required(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn age_seconds(now_epoch_s: i64, then_epoch_s: i64) -> u64 {
    if now_epoch_s <= then_epoch_s {
        0
    } else {
        (now_epoch_s - then_epoch_s) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_stale_takeover_report, StaleDetectionPolicy, StaleEntityKind, StaleLoopSample,
        StaleSeverity, StaleTaskSample,
    };

    fn sample_policy() -> StaleDetectionPolicy {
        StaleDetectionPolicy {
            task_stale_after_secs: 1_800,
            loop_stale_after_secs: 1_200,
            required_observations: 2,
            recent_activity_grace_secs: 300,
            min_loop_queue_depth: 1,
        }
    }

    #[test]
    fn stale_in_progress_task_gets_takeover_suggestion() {
        let report = build_stale_takeover_report(
            &[StaleTaskSample {
                task_id: "forge-a1".to_owned(),
                title: "task".to_owned(),
                status: "in_progress".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 3,
                last_activity_epoch_s: Some(900),
                blocked_by: Vec::new(),
            }],
            &[],
            4_000,
            &sample_policy(),
        );

        assert_eq!(report.alerts.len(), 1);
        assert_eq!(report.alerts[0].kind, StaleEntityKind::Task);
        assert_eq!(report.alerts[0].severity, StaleSeverity::Takeover);
        assert!(report.alerts[0]
            .suggestion
            .command_hint
            .contains("takeover claim"));
    }

    #[test]
    fn blocked_task_is_watch_only_not_takeover() {
        let report = build_stale_takeover_report(
            &[StaleTaskSample {
                task_id: "forge-a1".to_owned(),
                title: "task".to_owned(),
                status: "in_progress".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 3,
                last_activity_epoch_s: Some(900),
                blocked_by: vec!["forge-dep".to_owned()],
            }],
            &[],
            4_000,
            &sample_policy(),
        );

        assert_eq!(report.alerts.len(), 1);
        assert_eq!(report.alerts[0].severity, StaleSeverity::Watch);
        assert!(report.alerts[0]
            .suggestion
            .headline
            .contains("clear blockers"));
    }

    #[test]
    fn recent_activity_suppresses_task_false_positive() {
        let report = build_stale_takeover_report(
            &[StaleTaskSample {
                task_id: "forge-a1".to_owned(),
                title: "task".to_owned(),
                status: "in_progress".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 3,
                last_activity_epoch_s: Some(3_900),
                blocked_by: Vec::new(),
            }],
            &[],
            4_000,
            &sample_policy(),
        );

        assert!(report.alerts.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        assert!(report.suppressed[0].reason.contains("recent activity"));
    }

    #[test]
    fn observation_threshold_suppresses_false_positive() {
        let report = build_stale_takeover_report(
            &[StaleTaskSample {
                task_id: "forge-a1".to_owned(),
                title: "task".to_owned(),
                status: "in_progress".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 1,
                last_activity_epoch_s: Some(900),
                blocked_by: Vec::new(),
            }],
            &[],
            4_000,
            &sample_policy(),
        );

        assert!(report.alerts.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        assert!(report.suppressed[0]
            .reason
            .contains("observations 1 < required 2"));
    }

    #[test]
    fn stale_loop_with_backlog_gets_recovery_suggestion() {
        let report = build_stale_takeover_report(
            &[],
            &[StaleLoopSample {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 3,
                last_activity_epoch_s: Some(100),
                queue_depth: 3,
                active_tasks: 1,
            }],
            4_000,
            &sample_policy(),
        );

        assert_eq!(report.alerts.len(), 1);
        assert_eq!(report.alerts[0].kind, StaleEntityKind::Loop);
        assert!(report.alerts[0]
            .suggestion
            .command_hint
            .contains("stop-and-refocus"));
    }

    #[test]
    fn low_queue_idle_loop_is_suppressed_by_mitigation_control() {
        let report = build_stale_takeover_report(
            &[],
            &[StaleLoopSample {
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
                owner: Some("agent-a".to_owned()),
                updated_at_epoch_s: 1_000,
                stale_observation_count: 3,
                last_activity_epoch_s: Some(100),
                queue_depth: 0,
                active_tasks: 0,
            }],
            4_000,
            &sample_policy(),
        );

        assert!(report.alerts.is_empty());
        assert_eq!(report.suppressed.len(), 1);
        assert!(report.suppressed[0]
            .reason
            .contains("queue depth 0 < minimum 1"));
    }
}

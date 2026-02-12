//! Unified fact model for runs, tasks, queues, and agents.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSourceRecord {
    pub run_id: String,
    pub loop_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSourceRecord {
    pub task_id: String,
    pub loop_id: String,
    pub status: String,
    pub assignee_agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSourceRecord {
    pub loop_id: String,
    pub pending: usize,
    pub in_progress: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSourceRecord {
    pub agent_id: String,
    pub loop_id: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceRepositories {
    pub runs: Vec<RunSourceRecord>,
    pub tasks: Vec<TaskSourceRecord>,
    pub queues: Vec<QueueSourceRecord>,
    pub agents: Vec<AgentSourceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFact {
    pub run_id: String,
    pub loop_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFact {
    pub task_id: String,
    pub loop_id: String,
    pub status: String,
    pub assignee_agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueFact {
    pub loop_id: String,
    pub pending: usize,
    pub in_progress: usize,
    pub derived_pending: usize,
    pub derived_in_progress: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFact {
    pub agent_id: String,
    pub loop_id: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FactTotals {
    pub runs: usize,
    pub tasks: usize,
    pub pending_tasks: usize,
    pub in_progress_tasks: usize,
    pub active_agents: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnifiedFactModel {
    pub runs: Vec<RunFact>,
    pub tasks: Vec<TaskFact>,
    pub queues: Vec<QueueFact>,
    pub agents: Vec<AgentFact>,
    pub totals: FactTotals,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsistencyIssueKind {
    DuplicateRunId,
    DuplicateTaskId,
    DuplicateAgentId,
    QueueCountMismatch,
    OrphanRunLoop,
    OrphanAgentLoop,
    MissingTaskAssignee,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyIssue {
    pub kind: ConsistencyIssueKind,
    pub key: String,
    pub detail: String,
}

#[must_use]
pub fn build_unified_fact_model(source: &SourceRepositories) -> UnifiedFactModel {
    let mut runs: Vec<RunFact> = source
        .runs
        .iter()
        .filter(|row| !row.run_id.trim().is_empty() && !row.loop_id.trim().is_empty())
        .map(|row| RunFact {
            run_id: row.run_id.trim().to_owned(),
            loop_id: row.loop_id.trim().to_owned(),
            status: normalize_status(&row.status),
        })
        .collect();
    runs.sort_by(|a, b| a.loop_id.cmp(&b.loop_id).then(a.run_id.cmp(&b.run_id)));

    let mut tasks: Vec<TaskFact> = source
        .tasks
        .iter()
        .filter(|row| !row.task_id.trim().is_empty() && !row.loop_id.trim().is_empty())
        .map(|row| TaskFact {
            task_id: row.task_id.trim().to_owned(),
            loop_id: row.loop_id.trim().to_owned(),
            status: normalize_status(&row.status),
            assignee_agent_id: normalize_optional(row.assignee_agent_id.as_deref()),
        })
        .collect();
    tasks.sort_by(|a, b| a.loop_id.cmp(&b.loop_id).then(a.task_id.cmp(&b.task_id)));

    let mut agents: Vec<AgentFact> = source
        .agents
        .iter()
        .filter(|row| !row.agent_id.trim().is_empty() && !row.loop_id.trim().is_empty())
        .map(|row| AgentFact {
            agent_id: row.agent_id.trim().to_owned(),
            loop_id: row.loop_id.trim().to_owned(),
            state: normalize_status(&row.state),
        })
        .collect();
    agents.sort_by(|a, b| a.loop_id.cmp(&b.loop_id).then(a.agent_id.cmp(&b.agent_id)));

    let mut derived_pending_by_loop: BTreeMap<String, usize> = BTreeMap::new();
    let mut derived_in_progress_by_loop: BTreeMap<String, usize> = BTreeMap::new();
    for task in &tasks {
        if is_pending_status(&task.status) {
            let count = derived_pending_by_loop
                .get(&task.loop_id)
                .copied()
                .unwrap_or(0);
            derived_pending_by_loop.insert(task.loop_id.clone(), count + 1);
        }
        if is_in_progress_status(&task.status) {
            let count = derived_in_progress_by_loop
                .get(&task.loop_id)
                .copied()
                .unwrap_or(0);
            derived_in_progress_by_loop.insert(task.loop_id.clone(), count + 1);
        }
    }

    let mut queue_loop_ids: BTreeSet<String> = BTreeSet::new();
    let mut queues: Vec<QueueFact> = source
        .queues
        .iter()
        .filter(|row| !row.loop_id.trim().is_empty())
        .map(|row| {
            let loop_id = row.loop_id.trim().to_owned();
            queue_loop_ids.insert(loop_id.clone());
            QueueFact {
                loop_id: loop_id.clone(),
                pending: row.pending,
                in_progress: row.in_progress,
                derived_pending: derived_pending_by_loop.get(&loop_id).copied().unwrap_or(0),
                derived_in_progress: derived_in_progress_by_loop
                    .get(&loop_id)
                    .copied()
                    .unwrap_or(0),
            }
        })
        .collect();

    for loop_id in derived_pending_by_loop.keys() {
        if queue_loop_ids.contains(loop_id) {
            continue;
        }
        queues.push(QueueFact {
            loop_id: loop_id.clone(),
            pending: 0,
            in_progress: 0,
            derived_pending: derived_pending_by_loop.get(loop_id).copied().unwrap_or(0),
            derived_in_progress: derived_in_progress_by_loop
                .get(loop_id)
                .copied()
                .unwrap_or(0),
        });
    }

    queues.sort_by(|a, b| a.loop_id.cmp(&b.loop_id));

    let totals = FactTotals {
        runs: runs.len(),
        tasks: tasks.len(),
        pending_tasks: tasks
            .iter()
            .filter(|task| is_pending_status(&task.status))
            .count(),
        in_progress_tasks: tasks
            .iter()
            .filter(|task| is_in_progress_status(&task.status))
            .count(),
        active_agents: agents
            .iter()
            .filter(|agent| is_active_agent_state(&agent.state))
            .count(),
    };

    UnifiedFactModel {
        runs,
        tasks,
        queues,
        agents,
        totals,
    }
}

#[must_use]
pub fn consistency_checks_against_sources(
    source: &SourceRepositories,
    model: &UnifiedFactModel,
) -> Vec<ConsistencyIssue> {
    let mut issues = Vec::new();

    push_duplicate_issues(
        &source
            .runs
            .iter()
            .map(|row| row.run_id.as_str())
            .collect::<Vec<_>>(),
        ConsistencyIssueKind::DuplicateRunId,
        &mut issues,
    );
    push_duplicate_issues(
        &source
            .tasks
            .iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>(),
        ConsistencyIssueKind::DuplicateTaskId,
        &mut issues,
    );
    push_duplicate_issues(
        &source
            .agents
            .iter()
            .map(|row| row.agent_id.as_str())
            .collect::<Vec<_>>(),
        ConsistencyIssueKind::DuplicateAgentId,
        &mut issues,
    );

    let loop_ids_from_queues: BTreeSet<&str> = model
        .queues
        .iter()
        .map(|queue| queue.loop_id.as_str())
        .collect();
    let loop_ids_from_tasks: BTreeSet<&str> = model
        .tasks
        .iter()
        .map(|task| task.loop_id.as_str())
        .collect();

    for run in &model.runs {
        if !loop_ids_from_queues.contains(run.loop_id.as_str())
            && !loop_ids_from_tasks.contains(run.loop_id.as_str())
        {
            issues.push(ConsistencyIssue {
                kind: ConsistencyIssueKind::OrphanRunLoop,
                key: run.run_id.clone(),
                detail: format!("run {} points to unknown loop {}", run.run_id, run.loop_id),
            });
        }
    }

    let known_agent_ids: BTreeSet<&str> = model
        .agents
        .iter()
        .map(|agent| agent.agent_id.as_str())
        .collect();
    for task in &model.tasks {
        if let Some(agent_id) = task.assignee_agent_id.as_deref() {
            if known_agent_ids.contains(agent_id) {
                continue;
            }
            issues.push(ConsistencyIssue {
                kind: ConsistencyIssueKind::MissingTaskAssignee,
                key: task.task_id.clone(),
                detail: format!("task {} assignee {} not found", task.task_id, agent_id),
            });
        }
    }

    let known_loop_ids: BTreeSet<&str> = model
        .queues
        .iter()
        .map(|queue| queue.loop_id.as_str())
        .collect();
    for agent in &model.agents {
        if !known_loop_ids.contains(agent.loop_id.as_str()) {
            issues.push(ConsistencyIssue {
                kind: ConsistencyIssueKind::OrphanAgentLoop,
                key: agent.agent_id.clone(),
                detail: format!(
                    "agent {} points to unknown loop {}",
                    agent.agent_id, agent.loop_id
                ),
            });
        }
    }

    for queue in &model.queues {
        if queue.pending != queue.derived_pending || queue.in_progress != queue.derived_in_progress
        {
            issues.push(ConsistencyIssue {
                kind: ConsistencyIssueKind::QueueCountMismatch,
                key: queue.loop_id.clone(),
                detail: format!(
                    "loop {} queue mismatch pending {}!=derived {} in_progress {}!=derived {}",
                    queue.loop_id,
                    queue.pending,
                    queue.derived_pending,
                    queue.in_progress,
                    queue.derived_in_progress
                ),
            });
        }
    }

    issues.sort_by(|a, b| a.key.cmp(&b.key).then(a.detail.cmp(&b.detail)));
    issues
}

fn push_duplicate_issues(
    ids: &[&str],
    kind: ConsistencyIssueKind,
    issues: &mut Vec<ConsistencyIssue>,
) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !seen.insert(trimmed.to_owned()) {
            duplicates.insert(trimmed.to_owned());
        }
    }
    for duplicate in duplicates {
        issues.push(ConsistencyIssue {
            kind: kind.clone(),
            key: duplicate.clone(),
            detail: format!("duplicate id detected: {duplicate}"),
        });
    }
}

fn normalize_status(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn is_pending_status(status: &str) -> bool {
    matches!(status, "queued" | "pending" | "ready")
}

fn is_in_progress_status(status: &str) -> bool {
    matches!(status, "running" | "in_progress" | "working")
}

fn is_active_agent_state(state: &str) -> bool {
    matches!(state, "running" | "working" | "idle" | "awaiting_approval")
}

#[cfg(test)]
mod tests {
    use super::{
        build_unified_fact_model, consistency_checks_against_sources, AgentSourceRecord,
        QueueSourceRecord, RunSourceRecord, SourceRepositories, TaskSourceRecord,
    };

    fn aligned_sources() -> SourceRepositories {
        SourceRepositories {
            runs: vec![RunSourceRecord {
                run_id: "run-1".to_owned(),
                loop_id: "loop-a".to_owned(),
                status: "success".to_owned(),
            }],
            tasks: vec![
                TaskSourceRecord {
                    task_id: "task-1".to_owned(),
                    loop_id: "loop-a".to_owned(),
                    status: "queued".to_owned(),
                    assignee_agent_id: Some("agent-a".to_owned()),
                },
                TaskSourceRecord {
                    task_id: "task-2".to_owned(),
                    loop_id: "loop-a".to_owned(),
                    status: "running".to_owned(),
                    assignee_agent_id: Some("agent-a".to_owned()),
                },
            ],
            queues: vec![QueueSourceRecord {
                loop_id: "loop-a".to_owned(),
                pending: 1,
                in_progress: 1,
            }],
            agents: vec![AgentSourceRecord {
                agent_id: "agent-a".to_owned(),
                loop_id: "loop-a".to_owned(),
                state: "running".to_owned(),
            }],
        }
    }

    #[test]
    fn derives_totals_and_normalizes_statuses() {
        let model = build_unified_fact_model(&aligned_sources());
        assert_eq!(model.totals.runs, 1);
        assert_eq!(model.totals.tasks, 2);
        assert_eq!(model.totals.pending_tasks, 1);
        assert_eq!(model.totals.in_progress_tasks, 1);
        assert_eq!(model.totals.active_agents, 1);
        assert_eq!(model.queues[0].derived_pending, 1);
        assert_eq!(model.queues[0].derived_in_progress, 1);
    }

    #[test]
    fn consistency_checks_pass_on_aligned_sources() {
        let sources = aligned_sources();
        let model = build_unified_fact_model(&sources);
        let issues = consistency_checks_against_sources(&sources, &model);
        assert!(issues.is_empty());
    }

    #[test]
    fn detects_queue_mismatch_orphans_and_missing_assignee() {
        let sources = SourceRepositories {
            runs: vec![
                RunSourceRecord {
                    run_id: "run-1".to_owned(),
                    loop_id: "loop-orphan".to_owned(),
                    status: "success".to_owned(),
                },
                RunSourceRecord {
                    run_id: "run-1".to_owned(),
                    loop_id: "loop-orphan".to_owned(),
                    status: "success".to_owned(),
                },
            ],
            tasks: vec![TaskSourceRecord {
                task_id: "task-1".to_owned(),
                loop_id: "loop-a".to_owned(),
                status: "queued".to_owned(),
                assignee_agent_id: Some("agent-missing".to_owned()),
            }],
            queues: vec![QueueSourceRecord {
                loop_id: "loop-a".to_owned(),
                pending: 0,
                in_progress: 0,
            }],
            agents: vec![AgentSourceRecord {
                agent_id: "agent-a".to_owned(),
                loop_id: "loop-orphan-agent".to_owned(),
                state: "idle".to_owned(),
            }],
        };
        let model = build_unified_fact_model(&sources);
        let issues = consistency_checks_against_sources(&sources, &model);
        assert!(issues
            .iter()
            .any(|issue| issue.detail.contains("duplicate id")));
        assert!(issues
            .iter()
            .any(|issue| issue.detail.contains("queue mismatch")));
        assert!(issues
            .iter()
            .any(|issue| issue.detail.contains("assignee agent-missing")));
        assert!(issues
            .iter()
            .any(|issue| issue.detail.contains("unknown loop loop-orphan")));
        assert!(issues
            .iter()
            .any(|issue| issue.detail.contains("unknown loop loop-orphan-agent")));
    }
}

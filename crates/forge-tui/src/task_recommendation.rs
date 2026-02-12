//! Next-best-task recommendation model for operator workflows.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecommendationSample {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub owner: Option<String>,
    pub project_id: Option<String>,
    pub epic_id: Option<String>,
    pub blocked_by: Vec<String>,
    pub updated_at_epoch_s: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecommendationContext {
    pub operator_id: Option<String>,
    pub project_focus: Option<String>,
    pub epic_focus: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecommendationBreakdown {
    pub priority_score: i32,
    pub readiness_score: i32,
    pub dependency_score: i32,
    pub ownership_score: i32,
    pub context_score: i32,
    pub total_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecommendation {
    pub task_id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub owner: Option<String>,
    pub blocked: bool,
    pub breakdown: RecommendationBreakdown,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecommendationReport {
    pub recommendations: Vec<TaskRecommendation>,
    pub excluded_terminal: usize,
    pub excluded_invalid: usize,
}

#[must_use]
pub fn recommend_next_best_tasks(
    samples: &[TaskRecommendationSample],
    context: &RecommendationContext,
) -> RecommendationReport {
    let limit = if context.limit == 0 { 5 } else { context.limit };
    let operator_id = normalize_optional(context.operator_id.as_deref());
    let project_focus = normalize_optional(context.project_focus.as_deref());
    let epic_focus = normalize_optional(context.epic_focus.as_deref());

    let mut excluded_terminal = 0usize;
    let mut excluded_invalid = 0usize;
    let mut recommendations = Vec::new();

    for sample in samples {
        let task_id = normalize_required(&sample.task_id);
        if task_id.is_empty() {
            excluded_invalid += 1;
            continue;
        }
        let status = normalize_required(&sample.status);
        if is_terminal_status(&status) {
            excluded_terminal += 1;
            continue;
        }
        let title = normalize_title(&sample.title, &task_id);
        let priority = normalize_priority(&sample.priority);
        let owner = normalize_optional(sample.owner.as_deref());
        let blocked = sample
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

        let mut reasons = Vec::new();

        let priority_score = score_priority(&priority);
        reasons.push(format!("priority:{priority} ({priority_score:+})"));

        let readiness_score = score_readiness(&status, blocked);
        reasons.push(format!("readiness:{status} ({readiness_score:+})"));

        let dependency_score = if blocked { -30 } else { 20 };
        reasons.push(if blocked {
            "dependency:blocked (-30)".to_owned()
        } else {
            "dependency:clear (+20)".to_owned()
        });

        let ownership_score = score_ownership(owner.as_deref(), operator_id.as_deref());
        let ownership_label = if owner.is_none() {
            "unowned"
        } else if owner == operator_id {
            "owned-by-operator"
        } else {
            "owned-by-other"
        };
        reasons.push(format!("ownership:{ownership_label} ({ownership_score:+})"));

        let context_score = score_context(
            normalize_optional(sample.project_id.as_deref()).as_deref(),
            normalize_optional(sample.epic_id.as_deref()).as_deref(),
            project_focus.as_deref(),
            epic_focus.as_deref(),
        );
        if context_score != 0 {
            reasons.push(format!("context-focus ({context_score:+})"));
        }

        let breakdown = RecommendationBreakdown {
            priority_score,
            readiness_score,
            dependency_score,
            ownership_score,
            context_score,
            total_score: priority_score
                + readiness_score
                + dependency_score
                + ownership_score
                + context_score,
        };

        recommendations.push(TaskRecommendation {
            task_id,
            title,
            priority,
            status,
            owner,
            blocked,
            breakdown,
            reasons,
        });
    }

    recommendations.sort_by(|a, b| {
        b.breakdown
            .total_score
            .cmp(&a.breakdown.total_score)
            .then(priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
            .then(a.blocked.cmp(&b.blocked))
            .then(a.task_id.cmp(&b.task_id))
    });
    recommendations.truncate(limit);

    RecommendationReport {
        recommendations,
        excluded_terminal,
        excluded_invalid,
    }
}

fn score_priority(priority: &str) -> i32 {
    match priority {
        "P0" => 60,
        "P1" => 45,
        "P2" => 30,
        "P3" => 15,
        "P4" => 10,
        _ => 5,
    }
}

fn score_readiness(status: &str, blocked: bool) -> i32 {
    if blocked {
        return -20;
    }
    match status {
        "ready" => 40,
        "open" => 30,
        "in_progress" => 10,
        _ => 0,
    }
}

fn score_ownership(owner: Option<&str>, operator_id: Option<&str>) -> i32 {
    match (owner, operator_id) {
        (None, _) => 15,
        (Some(owner), Some(operator)) if owner == operator => 12,
        (Some(_), _) => -18,
    }
}

fn score_context(
    project_id: Option<&str>,
    epic_id: Option<&str>,
    project_focus: Option<&str>,
    epic_focus: Option<&str>,
) -> i32 {
    let mut score = 0;
    if let (Some(task_project), Some(focus_project)) = (project_id, project_focus) {
        if task_project == focus_project {
            score += 10;
        }
    }
    if let (Some(task_epic), Some(focus_epic)) = (epic_id, epic_focus) {
        if task_epic == focus_epic {
            score += 10;
        }
    }
    score
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "done" | "closed" | "failed" | "canceled")
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_priority(priority: &str) -> String {
    let normalized = priority.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        "P3".to_owned()
    } else {
        normalized
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
}

fn normalize_title(title: &str, task_id: &str) -> String {
    let normalized = title.trim();
    if normalized.is_empty() {
        format!("Task {}", task_id.to_ascii_uppercase())
    } else {
        normalized.to_owned()
    }
}

fn priority_rank(priority: &str) -> usize {
    match priority {
        "P0" => 0,
        "P1" => 1,
        "P2" => 2,
        "P3" => 3,
        "P4" => 4,
        _ => 5,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{recommend_next_best_tasks, RecommendationContext, TaskRecommendationSample};

    fn sample_tasks() -> Vec<TaskRecommendationSample> {
        vec![
            TaskRecommendationSample {
                task_id: "forge-a1".to_owned(),
                title: "high priority ready".to_owned(),
                status: "ready".to_owned(),
                priority: "P0".to_owned(),
                owner: None,
                project_id: Some("prj-x".to_owned()),
                epic_id: Some("epic-a".to_owned()),
                blocked_by: vec![],
                updated_at_epoch_s: 0,
            },
            TaskRecommendationSample {
                task_id: "forge-a2".to_owned(),
                title: "blocked task".to_owned(),
                status: "ready".to_owned(),
                priority: "P0".to_owned(),
                owner: None,
                project_id: Some("prj-x".to_owned()),
                epic_id: Some("epic-a".to_owned()),
                blocked_by: vec!["forge-a9".to_owned()],
                updated_at_epoch_s: 0,
            },
            TaskRecommendationSample {
                task_id: "forge-a3".to_owned(),
                title: "owned by me".to_owned(),
                status: "open".to_owned(),
                priority: "P1".to_owned(),
                owner: Some("alice".to_owned()),
                project_id: Some("prj-y".to_owned()),
                epic_id: Some("epic-b".to_owned()),
                blocked_by: vec![],
                updated_at_epoch_s: 0,
            },
            TaskRecommendationSample {
                task_id: "forge-a4".to_owned(),
                title: "closed task".to_owned(),
                status: "closed".to_owned(),
                priority: "P0".to_owned(),
                owner: None,
                project_id: Some("prj-x".to_owned()),
                epic_id: Some("epic-a".to_owned()),
                blocked_by: vec![],
                updated_at_epoch_s: 0,
            },
        ]
    }

    #[test]
    fn recommendation_prefers_priority_readiness_dependency_and_ownership() {
        let report = recommend_next_best_tasks(
            &sample_tasks(),
            &RecommendationContext {
                operator_id: Some("alice".to_owned()),
                ..RecommendationContext::default()
            },
        );
        assert_eq!(report.recommendations[0].task_id, "forge-a1");
        assert_eq!(report.recommendations[1].task_id, "forge-a3");
        assert!(report
            .recommendations
            .iter()
            .any(|entry| entry.task_id == "forge-a2" && entry.blocked));
    }

    #[test]
    fn blocked_dependencies_get_penalized() {
        let report = recommend_next_best_tasks(&sample_tasks(), &RecommendationContext::default());
        let blocked = report
            .recommendations
            .iter()
            .find(|entry| entry.task_id == "forge-a2")
            .unwrap_or_else(|| panic!("missing blocked recommendation"));
        let unblocked = report
            .recommendations
            .iter()
            .find(|entry| entry.task_id == "forge-a1")
            .unwrap_or_else(|| panic!("missing unblocked recommendation"));
        assert!(blocked.breakdown.total_score < unblocked.breakdown.total_score);
    }

    #[test]
    fn context_focus_boosts_matching_project_and_epic() {
        let report = recommend_next_best_tasks(
            &sample_tasks(),
            &RecommendationContext {
                project_focus: Some("prj-y".to_owned()),
                epic_focus: Some("epic-b".to_owned()),
                operator_id: Some("alice".to_owned()),
                limit: 3,
            },
        );
        assert_eq!(report.recommendations[0].task_id, "forge-a3");
        assert!(report.recommendations[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("context-focus")));
    }

    #[test]
    fn terminal_tasks_are_excluded_from_candidates() {
        let report = recommend_next_best_tasks(&sample_tasks(), &RecommendationContext::default());
        assert_eq!(report.excluded_terminal, 1);
        assert!(!report
            .recommendations
            .iter()
            .any(|entry| entry.task_id == "forge-a4"));
    }

    #[test]
    fn limit_defaults_to_five_and_is_respected() {
        let mut samples = sample_tasks();
        for index in 0..8 {
            samples.push(TaskRecommendationSample {
                task_id: format!("forge-extra-{index}"),
                title: format!("extra task {index}"),
                status: "open".to_owned(),
                priority: "P3".to_owned(),
                owner: None,
                project_id: None,
                epic_id: None,
                blocked_by: vec![],
                updated_at_epoch_s: 0,
            });
        }
        let default_limited =
            recommend_next_best_tasks(&samples, &RecommendationContext::default());
        assert_eq!(default_limited.recommendations.len(), 5);

        let explicitly_limited = recommend_next_best_tasks(
            &samples,
            &RecommendationContext {
                limit: 2,
                ..RecommendationContext::default()
            },
        );
        assert_eq!(explicitly_limited.recommendations.len(), 2);
    }
}

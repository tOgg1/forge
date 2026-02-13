//! Triage score queue for ranking next operator actions.
//!
//! Scores combine urgency, risk, and staleness with small workflow heuristics
//! (blocked penalty, owner bias, incident-state boost).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageQueueSample {
    pub item_id: String,
    pub title: String,
    pub status: String,
    pub owner: Option<String>,
    pub urgency: u8,
    pub risk: u8,
    pub staleness_minutes: u32,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageQueueWeights {
    pub urgency_weight: i32,
    pub risk_weight: i32,
    pub staleness_weight: i32,
    pub blocked_penalty: i32,
    pub incident_boost: i32,
    pub unowned_boost: i32,
    pub foreign_owner_penalty: i32,
}

impl Default for TriageQueueWeights {
    fn default() -> Self {
        Self {
            urgency_weight: 5,
            risk_weight: 4,
            staleness_weight: 3,
            blocked_penalty: 240,
            incident_boost: 20,
            unowned_boost: 10,
            foreign_owner_penalty: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TriageQueueContext {
    pub operator_id: Option<String>,
    pub limit: usize,
    pub weights: TriageQueueWeights,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TriageScoreBreakdown {
    pub urgency_score: i32,
    pub risk_score: i32,
    pub staleness_score: i32,
    pub blocked_score: i32,
    pub ownership_score: i32,
    pub status_score: i32,
    pub total_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageQueueItem {
    pub item_id: String,
    pub title: String,
    pub status: String,
    pub owner: Option<String>,
    pub blocked: bool,
    pub breakdown: TriageScoreBreakdown,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TriageQueueReport {
    pub items: Vec<TriageQueueItem>,
    pub excluded_terminal: usize,
    pub excluded_invalid: usize,
}

#[must_use]
pub fn rank_triage_queue(
    samples: &[TriageQueueSample],
    context: &TriageQueueContext,
) -> TriageQueueReport {
    let operator_id = normalize_optional(context.operator_id.as_deref());
    let limit = if context.limit == 0 {
        10
    } else {
        context.limit
    };
    let weights = &context.weights;

    let mut excluded_terminal = 0usize;
    let mut excluded_invalid = 0usize;
    let mut items = Vec::new();

    for sample in samples {
        let item_id = normalize_required(&sample.item_id);
        if item_id.is_empty() {
            excluded_invalid += 1;
            continue;
        }
        let status = normalize_required(&sample.status);
        if is_terminal_status(&status) {
            excluded_terminal += 1;
            continue;
        }

        let title = normalize_title(&sample.title, &item_id);
        let owner = normalize_optional(sample.owner.as_deref());
        let urgency = i32::from(sample.urgency.min(100));
        let risk = i32::from(sample.risk.min(100));
        let staleness_band = staleness_band(sample.staleness_minutes);

        let urgency_score = urgency * weights.urgency_weight;
        let risk_score = risk * weights.risk_weight;
        let staleness_score = staleness_band * weights.staleness_weight;
        let blocked_score = if sample.blocked {
            -weights.blocked_penalty
        } else {
            0
        };
        let ownership_score = ownership_score(
            owner.as_deref(),
            operator_id.as_deref(),
            weights.unowned_boost,
            weights.foreign_owner_penalty,
        );
        let status_score = if is_incident_status(&status) {
            weights.incident_boost
        } else {
            0
        };
        let total_score = urgency_score
            + risk_score
            + staleness_score
            + blocked_score
            + ownership_score
            + status_score;

        let mut reasons = Vec::new();
        reasons.push(format!("urgency:{} ({:+})", urgency, urgency_score));
        reasons.push(format!("risk:{} ({:+})", risk, risk_score));
        reasons.push(format!(
            "staleness:{}m band={} ({:+})",
            sample.staleness_minutes, staleness_band, staleness_score
        ));
        if sample.blocked {
            reasons.push(format!("blocked (-{})", weights.blocked_penalty));
        }
        if status_score != 0 {
            reasons.push(format!("incident-status:{} ({:+})", status, status_score));
        }
        if ownership_score != 0 {
            reasons.push(format!("ownership ({:+})", ownership_score));
        }

        items.push(TriageQueueItem {
            item_id,
            title,
            status,
            owner,
            blocked: sample.blocked,
            breakdown: TriageScoreBreakdown {
                urgency_score,
                risk_score,
                staleness_score,
                blocked_score,
                ownership_score,
                status_score,
                total_score,
            },
            reasons,
        });
    }

    items.sort_by(|a, b| {
        b.breakdown
            .total_score
            .cmp(&a.breakdown.total_score)
            .then(b.breakdown.urgency_score.cmp(&a.breakdown.urgency_score))
            .then(b.breakdown.risk_score.cmp(&a.breakdown.risk_score))
            .then(a.blocked.cmp(&b.blocked))
            .then(a.item_id.cmp(&b.item_id))
    });
    items.truncate(limit);

    TriageQueueReport {
        items,
        excluded_terminal,
        excluded_invalid,
    }
}

fn staleness_band(staleness_minutes: u32) -> i32 {
    match staleness_minutes {
        0..=4 => 0,
        5..=14 => 8,
        15..=29 => 20,
        30..=59 => 35,
        60..=179 => 55,
        180..=719 => 75,
        720..=1439 => 90,
        _ => 100,
    }
}

fn ownership_score(
    owner: Option<&str>,
    operator_id: Option<&str>,
    unowned_boost: i32,
    foreign_owner_penalty: i32,
) -> i32 {
    match (owner, operator_id) {
        (None, _) => unowned_boost,
        (Some(owner), Some(operator)) if owner == operator => 6,
        (Some(_), _) => -foreign_owner_penalty,
    }
}

fn is_incident_status(status: &str) -> bool {
    matches!(
        status,
        "error" | "failed" | "degraded" | "blocked" | "stuck" | "flaky"
    )
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "done" | "closed" | "resolved" | "canceled")
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
}

fn normalize_title(title: &str, item_id: &str) -> String {
    let normalized = title.trim();
    if normalized.is_empty() {
        format!("Item {}", item_id.to_ascii_uppercase())
    } else {
        normalized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{rank_triage_queue, TriageQueueContext, TriageQueueSample, TriageQueueWeights};

    fn sample(
        id: &str,
        status: &str,
        urgency: u8,
        risk: u8,
        staleness_minutes: u32,
        blocked: bool,
    ) -> TriageQueueSample {
        TriageQueueSample {
            item_id: id.to_owned(),
            title: format!("task-{id}"),
            status: status.to_owned(),
            owner: None,
            urgency,
            risk,
            staleness_minutes,
            blocked,
        }
    }

    #[test]
    fn ranks_by_urgency_risk_and_staleness() {
        let samples = vec![
            sample("a", "open", 30, 30, 2, false),
            sample("b", "open", 90, 70, 10, false),
            sample("c", "open", 60, 80, 240, false),
        ];
        let report = rank_triage_queue(&samples, &TriageQueueContext::default());
        let ids = report
            .items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<String>>();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    #[test]
    fn blocked_penalty_pushes_item_down() {
        let samples = vec![
            sample("a", "degraded", 80, 80, 100, true),
            sample("b", "open", 60, 60, 100, false),
        ];
        let report = rank_triage_queue(&samples, &TriageQueueContext::default());
        assert_eq!(report.items[0].item_id, "b");
        assert_eq!(report.items[1].item_id, "a");
    }

    #[test]
    fn excludes_terminal_and_invalid_rows() {
        let mut invalid = sample("", "open", 60, 60, 10, false);
        invalid.title = "missing-id".to_owned();
        let samples = vec![
            invalid,
            sample("a", "resolved", 90, 90, 10, false),
            sample("b", "open", 20, 20, 10, false),
        ];
        let report = rank_triage_queue(&samples, &TriageQueueContext::default());
        assert_eq!(report.excluded_invalid, 1);
        assert_eq!(report.excluded_terminal, 1);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].item_id, "b");
    }

    #[test]
    fn respects_limit_and_owner_penalty() {
        let samples = vec![
            sample("a", "open", 40, 40, 120, false),
            sample("b", "open", 45, 45, 120, false),
            sample("c", "open", 50, 50, 120, false),
        ];
        let context = TriageQueueContext {
            operator_id: Some("agent-x".to_owned()),
            limit: 2,
            weights: TriageQueueWeights::default(),
        };
        let mut owned = samples.clone();
        owned[1].owner = Some("agent-z".to_owned());
        let report = rank_triage_queue(&owned, &context);
        assert_eq!(report.items.len(), 2);
        assert!(report.items.iter().any(|item| item.item_id == "c"));
        assert!(report.items.iter().all(|item| item.item_id != "b"));
    }
}

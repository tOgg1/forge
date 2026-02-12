//! Communication quality checks for unanswered asks, stale threads, and closure hygiene.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationThreadSample {
    pub thread_id: String,
    pub task_id: Option<String>,
    pub repo: Option<String>,
    pub owner: Option<String>,
    pub task_status: Option<String>,
    pub last_message_at_epoch_s: i64,
    pub open_asks: usize,
    pub oldest_open_ask_age_secs: Option<u64>,
    pub has_closure_note: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationQualityPolicy {
    pub unanswered_ask_after_secs: u64,
    pub stale_thread_after_secs: u64,
}

impl Default for CommunicationQualityPolicy {
    fn default() -> Self {
        Self {
            unanswered_ask_after_secs: 600,
            stale_thread_after_secs: 1_800,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommunicationAlertKind {
    UnansweredAsk,
    StaleThread,
    MissingClosureNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommunicationSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationSuggestion {
    pub headline: String,
    pub command_hint: String,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationAlert {
    pub kind: CommunicationAlertKind,
    pub severity: CommunicationSeverity,
    pub thread_id: String,
    pub task_id: Option<String>,
    pub repo: Option<String>,
    pub owner: Option<String>,
    pub idle_for_secs: u64,
    pub reasons: Vec<String>,
    pub suggestion: CommunicationSuggestion,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationQualitySummary {
    pub total_threads: usize,
    pub unanswered_ask_alerts: usize,
    pub stale_thread_alerts: usize,
    pub missing_closure_alerts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationQualityReport {
    pub alerts: Vec<CommunicationAlert>,
    pub summary: CommunicationQualitySummary,
}

#[must_use]
pub fn build_communication_quality_report(
    samples: &[CommunicationThreadSample],
    now_epoch_s: i64,
    policy: &CommunicationQualityPolicy,
) -> CommunicationQualityReport {
    let now_epoch_s = now_epoch_s.max(0);
    let mut alerts = Vec::new();
    let mut summary = CommunicationQualitySummary {
        total_threads: samples.len(),
        ..CommunicationQualitySummary::default()
    };

    for sample in samples {
        evaluate_thread(sample, now_epoch_s, policy, &mut alerts, &mut summary);
    }

    alerts.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(b.idle_for_secs.cmp(&a.idle_for_secs))
            .then(a.thread_id.cmp(&b.thread_id))
            .then(a.kind.cmp(&b.kind))
    });

    CommunicationQualityReport { alerts, summary }
}

fn evaluate_thread(
    sample: &CommunicationThreadSample,
    now_epoch_s: i64,
    policy: &CommunicationQualityPolicy,
    alerts: &mut Vec<CommunicationAlert>,
    summary: &mut CommunicationQualitySummary,
) {
    let thread_id = normalize_required(&sample.thread_id);
    if thread_id.is_empty() {
        return;
    }

    let task_id = normalize_optional(sample.task_id.as_deref());
    let repo = normalize_optional(sample.repo.as_deref());
    let owner = normalize_optional(sample.owner.as_deref());
    let status =
        normalize_optional(sample.task_status.as_deref()).unwrap_or_else(|| "open".to_owned());
    let idle_for_secs = age_seconds(now_epoch_s, sample.last_message_at_epoch_s);

    if sample.open_asks > 0 {
        let ask_age_secs = sample.oldest_open_ask_age_secs.unwrap_or(idle_for_secs);
        if ask_age_secs >= policy.unanswered_ask_after_secs {
            summary.unanswered_ask_alerts += 1;
            let severity = if ask_age_secs >= policy.unanswered_ask_after_secs.saturating_mul(2) {
                CommunicationSeverity::Critical
            } else {
                CommunicationSeverity::Warning
            };
            let task_ref = task_id.clone().unwrap_or_else(|| thread_id.clone());
            alerts.push(CommunicationAlert {
                kind: CommunicationAlertKind::UnansweredAsk,
                severity,
                thread_id: thread_id.clone(),
                task_id: task_id.clone(),
                repo: repo.clone(),
                owner: owner.clone(),
                idle_for_secs: ask_age_secs,
                reasons: vec![
                    format!("open asks={}", sample.open_asks),
                    format!(
                        "oldest ask age={}s (threshold {}s)",
                        ask_age_secs, policy.unanswered_ask_after_secs
                    ),
                ],
                suggestion: CommunicationSuggestion {
                    headline: "Unanswered ask: send explicit reply or escalation".to_owned(),
                    command_hint: format!(
                        "fmail send task \"reply needed: {task_ref} thread={thread_id}\""
                    ),
                    checks: vec![
                        "answer each open ask directly".to_owned(),
                        "if blocked, post owner + unblock request".to_owned(),
                        "confirm requester acknowledged response".to_owned(),
                    ],
                },
            });
        }
    }

    if idle_for_secs >= policy.stale_thread_after_secs && is_active_status(&status) {
        summary.stale_thread_alerts += 1;
        let task_ref = task_id.clone().unwrap_or_else(|| thread_id.clone());
        alerts.push(CommunicationAlert {
            kind: CommunicationAlertKind::StaleThread,
            severity: CommunicationSeverity::Warning,
            thread_id: thread_id.clone(),
            task_id: task_id.clone(),
            repo: repo.clone(),
            owner: owner.clone(),
            idle_for_secs,
            reasons: vec![
                format!(
                    "idle={}s (threshold {}s)",
                    idle_for_secs, policy.stale_thread_after_secs
                ),
                format!("task status={status}"),
            ],
            suggestion: CommunicationSuggestion {
                headline: "Stale coordination thread: post a status heartbeat".to_owned(),
                command_hint: format!("fmail send task \"status heartbeat: {task_ref}\""),
                checks: vec![
                    "summarize progress/blocker in one line".to_owned(),
                    "include next action + owner".to_owned(),
                ],
            },
        });
    }

    if is_terminal_status(&status) && !sample.has_closure_note {
        summary.missing_closure_alerts += 1;
        let task_ref = task_id.clone().unwrap_or_else(|| thread_id.clone());
        alerts.push(CommunicationAlert {
            kind: CommunicationAlertKind::MissingClosureNote,
            severity: CommunicationSeverity::Info,
            thread_id,
            task_id,
            repo,
            owner,
            idle_for_secs,
            reasons: vec![
                format!("task status={status}"),
                "closure note missing".to_owned(),
            ],
            suggestion: CommunicationSuggestion {
                headline: "Missing closure note: publish concise completion summary".to_owned(),
                command_hint: format!(
                    "fmail send task \"{task_ref} closed: summary + validation status\""
                ),
                checks: vec![
                    "state what shipped".to_owned(),
                    "state validation result".to_owned(),
                    "include follow-ups if any".to_owned(),
                ],
            },
        });
    }
}

fn is_active_status(status: &str) -> bool {
    matches!(status, "open" | "ready" | "in_progress" | "blocked")
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "closed" | "done" | "failed" | "canceled")
}

fn age_seconds(now_epoch_s: i64, then_epoch_s: i64) -> u64 {
    if now_epoch_s <= then_epoch_s {
        0
    } else {
        (now_epoch_s - then_epoch_s) as u64
    }
}

fn normalize_required(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(normalize_required)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        build_communication_quality_report, CommunicationAlertKind, CommunicationQualityPolicy,
        CommunicationSeverity, CommunicationThreadSample,
    };

    fn sample_thread(
        thread_id: &str,
        task_id: Option<&str>,
        task_status: Option<&str>,
    ) -> CommunicationThreadSample {
        CommunicationThreadSample {
            thread_id: thread_id.to_owned(),
            task_id: task_id.map(str::to_owned),
            repo: Some("forge".to_owned()),
            owner: Some("rewrite-tui-codex-2".to_owned()),
            task_status: task_status.map(str::to_owned),
            last_message_at_epoch_s: 1_000,
            open_asks: 0,
            oldest_open_ask_age_secs: None,
            has_closure_note: false,
        }
    }

    #[test]
    fn unanswered_ask_generates_alert_with_escalation_hint() {
        let mut thread = sample_thread("thread-a", Some("forge-z33"), Some("in_progress"));
        thread.open_asks = 2;
        thread.oldest_open_ask_age_secs = Some(2_000);

        let report = build_communication_quality_report(
            &[thread],
            2_500,
            &CommunicationQualityPolicy::default(),
        );
        assert_eq!(report.summary.unanswered_ask_alerts, 1);
        let alert = match report
            .alerts
            .iter()
            .find(|alert| alert.kind == CommunicationAlertKind::UnansweredAsk)
        {
            Some(alert) => alert,
            None => panic!("missing unanswered ask alert"),
        };
        assert_eq!(alert.severity, CommunicationSeverity::Critical);
        assert!(alert.suggestion.command_hint.contains("reply needed"));
    }

    #[test]
    fn stale_thread_alert_created_for_active_status() {
        let thread = sample_thread("thread-b", Some("forge-z33"), Some("in_progress"));
        let report = build_communication_quality_report(
            &[thread],
            4_000,
            &CommunicationQualityPolicy::default(),
        );
        assert_eq!(report.summary.stale_thread_alerts, 1);
        assert!(report
            .alerts
            .iter()
            .any(|alert| alert.kind == CommunicationAlertKind::StaleThread));
    }

    #[test]
    fn missing_closure_note_alert_for_terminal_status() {
        let thread = sample_thread("thread-c", Some("forge-z33"), Some("closed"));
        let report = build_communication_quality_report(
            &[thread],
            1_200,
            &CommunicationQualityPolicy::default(),
        );
        assert_eq!(report.summary.missing_closure_alerts, 1);
        let alert = match report
            .alerts
            .iter()
            .find(|alert| alert.kind == CommunicationAlertKind::MissingClosureNote)
        {
            Some(alert) => alert,
            None => panic!("missing closure alert"),
        };
        assert_eq!(alert.severity, CommunicationSeverity::Info);
        assert!(alert.suggestion.command_hint.contains("closed: summary"));
    }

    #[test]
    fn closure_alert_not_emitted_when_note_exists() {
        let mut thread = sample_thread("thread-d", Some("forge-z33"), Some("closed"));
        thread.has_closure_note = true;

        let report = build_communication_quality_report(
            &[thread],
            1_200,
            &CommunicationQualityPolicy::default(),
        );
        assert_eq!(report.summary.missing_closure_alerts, 0);
    }

    #[test]
    fn alerts_are_sorted_by_severity_then_idle() {
        let mut unanswered = sample_thread("thread-a", Some("forge-z33"), Some("in_progress"));
        unanswered.open_asks = 1;
        unanswered.oldest_open_ask_age_secs = Some(1_400);
        unanswered.last_message_at_epoch_s = 3_500;

        let stale = sample_thread("thread-b", Some("forge-vz1"), Some("in_progress"));
        let closed = sample_thread("thread-c", Some("forge-vz1"), Some("closed"));

        let report = build_communication_quality_report(
            &[closed, stale, unanswered],
            4_000,
            &CommunicationQualityPolicy::default(),
        );
        assert_eq!(report.alerts[0].kind, CommunicationAlertKind::UnansweredAsk);
        assert_eq!(report.alerts[1].kind, CommunicationAlertKind::StaleThread);
        assert_eq!(
            report.alerts[2].kind,
            CommunicationAlertKind::MissingClosureNote
        );
    }
}

//! Emergency safe-stop-all workflow model with scope preview and integrity checks.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopRuntimeState {
    Running,
    Sleeping,
    Waiting,
    Stopped,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyLoopSample {
    pub loop_id: String,
    pub project: String,
    pub pool: String,
    pub tags: Vec<String>,
    pub runtime_state: LoopRuntimeState,
    pub queue_depth: usize,
    pub ledger_synced: bool,
    pub runner_healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmergencyScopeFilter {
    pub projects: Vec<String>,
    pub pools: Vec<String>,
    pub tags_any: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyScopePreview {
    pub selected_loop_ids: Vec<String>,
    pub selected_total: usize,
    pub selected_running: usize,
    pub selected_risky: usize,
    pub excluded_total: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeStopStage {
    ScopePreview,
    HotkeyConfirm,
    StopRequests,
    AwaitStopped,
    IntegrityChecks,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeStopStageStatus {
    Pending,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeStopStageResult {
    pub stage: SafeStopStage,
    pub status: SafeStopStageStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityCheckResult {
    pub name: String,
    pub status: SafeStopStageStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencySafeStopReport {
    pub preview: EmergencyScopePreview,
    pub stages: Vec<SafeStopStageResult>,
    pub integrity_checks: Vec<IntegrityCheckResult>,
    pub can_execute: bool,
    pub completed: bool,
    pub escalation_hint: Option<String>,
}

#[must_use]
pub fn evaluate_emergency_safe_stop(
    loops: &[EmergencyLoopSample],
    scope: &EmergencyScopeFilter,
    hotkey_confirmed: bool,
    stop_requested_loop_ids: &[String],
) -> EmergencySafeStopReport {
    let preview = build_scope_preview(loops, scope);
    let selected_loops = selected_loops(loops, &preview.selected_loop_ids);
    let requested_count =
        count_requested_targets(&preview.selected_loop_ids, stop_requested_loop_ids);
    let stopped_count = selected_loops
        .iter()
        .filter(|sample| sample.runtime_state == LoopRuntimeState::Stopped)
        .count();

    let mut stages = Vec::new();
    let mut escalation_hint = None;

    if preview.selected_total == 0 {
        stages.push(SafeStopStageResult {
            stage: SafeStopStage::ScopePreview,
            status: SafeStopStageStatus::Blocked,
            detail: "scope preview resolved no loops".to_owned(),
        });
        escalation_hint = Some("broaden scope filters before triggering emergency stop".to_owned());
        return EmergencySafeStopReport {
            preview,
            stages,
            integrity_checks: Vec::new(),
            can_execute: false,
            completed: false,
            escalation_hint,
        };
    }

    stages.push(SafeStopStageResult {
        stage: SafeStopStage::ScopePreview,
        status: SafeStopStageStatus::Completed,
        detail: format!(
            "{} loop(s) selected for emergency stop",
            preview.selected_total
        ),
    });

    if hotkey_confirmed {
        stages.push(SafeStopStageResult {
            stage: SafeStopStage::HotkeyConfirm,
            status: SafeStopStageStatus::Completed,
            detail: "emergency hotkey confirmed".to_owned(),
        });
    } else {
        stages.push(SafeStopStageResult {
            stage: SafeStopStage::HotkeyConfirm,
            status: SafeStopStageStatus::Blocked,
            detail: "one-key stop not confirmed".to_owned(),
        });
        escalation_hint = Some("press Shift+X to confirm safe-stop-all".to_owned());
    }

    let stop_request_status = if requested_count == preview.selected_total {
        SafeStopStageStatus::Completed
    } else {
        SafeStopStageStatus::Pending
    };
    stages.push(SafeStopStageResult {
        stage: SafeStopStage::StopRequests,
        status: stop_request_status,
        detail: format!(
            "stop requests staged for {}/{} targets",
            requested_count, preview.selected_total
        ),
    });

    let await_status = if stopped_count == preview.selected_total {
        SafeStopStageStatus::Completed
    } else {
        SafeStopStageStatus::Pending
    };
    stages.push(SafeStopStageResult {
        stage: SafeStopStage::AwaitStopped,
        status: await_status,
        detail: format!(
            "stopped targets: {}/{}",
            stopped_count, preview.selected_total
        ),
    });

    let integrity_checks = build_integrity_checks(&selected_loops);
    let integrity_ok = integrity_checks
        .iter()
        .all(|check| check.status == SafeStopStageStatus::Completed);
    stages.push(SafeStopStageResult {
        stage: SafeStopStage::IntegrityChecks,
        status: if integrity_ok {
            SafeStopStageStatus::Completed
        } else {
            SafeStopStageStatus::Blocked
        },
        detail: if integrity_ok {
            "post-stop integrity checks passed".to_owned()
        } else {
            "integrity checks failed; stop reconciliation required".to_owned()
        },
    });

    let can_execute = hotkey_confirmed;
    let completed = can_execute
        && stop_request_status == SafeStopStageStatus::Completed
        && await_status == SafeStopStageStatus::Completed
        && integrity_ok;
    stages.push(SafeStopStageResult {
        stage: SafeStopStage::Complete,
        status: if completed {
            SafeStopStageStatus::Completed
        } else if can_execute {
            SafeStopStageStatus::Pending
        } else {
            SafeStopStageStatus::Blocked
        },
        detail: if completed {
            "safe-stop-all completed".to_owned()
        } else {
            "safe-stop-all not yet complete".to_owned()
        },
    });

    if escalation_hint.is_none() && !integrity_ok {
        escalation_hint =
            Some("run queue/ledger repair before declaring emergency stop complete".to_owned());
    }

    EmergencySafeStopReport {
        preview,
        stages,
        integrity_checks,
        can_execute,
        completed,
        escalation_hint,
    }
}

#[must_use]
pub fn build_scope_preview(
    loops: &[EmergencyLoopSample],
    scope: &EmergencyScopeFilter,
) -> EmergencyScopePreview {
    let selected: Vec<&EmergencyLoopSample> = loops
        .iter()
        .filter(|sample| matches_scope(sample, scope))
        .collect();
    let selected_loop_ids: Vec<String> = selected
        .iter()
        .map(|sample| sample.loop_id.clone())
        .collect();
    let selected_total = selected.len();
    let selected_running = selected
        .iter()
        .filter(|sample| {
            matches!(
                sample.runtime_state,
                LoopRuntimeState::Running | LoopRuntimeState::Sleeping | LoopRuntimeState::Waiting
            )
        })
        .count();
    let selected_risky = selected
        .iter()
        .filter(|sample| !sample.runner_healthy || sample.runtime_state == LoopRuntimeState::Error)
        .count();
    let excluded_total = loops.len().saturating_sub(selected_total);

    EmergencyScopePreview {
        selected_loop_ids,
        selected_total,
        selected_running,
        selected_risky,
        excluded_total,
        summary: format!(
            "scope={} selected={} running={} risky={} excluded={}",
            scope_label(scope),
            selected_total,
            selected_running,
            selected_risky,
            excluded_total
        ),
    }
}

fn build_integrity_checks(selected_loops: &[EmergencyLoopSample]) -> Vec<IntegrityCheckResult> {
    let queue_ok = selected_loops.iter().all(|sample| sample.queue_depth == 0);
    let ledger_ok = selected_loops.iter().all(|sample| sample.ledger_synced);
    let runner_ok = selected_loops.iter().all(|sample| sample.runner_healthy);

    vec![
        IntegrityCheckResult {
            name: "queue-drained".to_owned(),
            status: if queue_ok {
                SafeStopStageStatus::Completed
            } else {
                SafeStopStageStatus::Blocked
            },
            detail: if queue_ok {
                "all selected loop queues drained".to_owned()
            } else {
                "pending queue items remain after stop".to_owned()
            },
        },
        IntegrityCheckResult {
            name: "ledger-synced".to_owned(),
            status: if ledger_ok {
                SafeStopStageStatus::Completed
            } else {
                SafeStopStageStatus::Blocked
            },
            detail: if ledger_ok {
                "ledger sync confirmed for all selected loops".to_owned()
            } else {
                "one or more selected loops have unsynced ledger state".to_owned()
            },
        },
        IntegrityCheckResult {
            name: "runner-health".to_owned(),
            status: if runner_ok {
                SafeStopStageStatus::Completed
            } else {
                SafeStopStageStatus::Blocked
            },
            detail: if runner_ok {
                "runner health probes are clean".to_owned()
            } else {
                "runner health degraded or unknown post-stop".to_owned()
            },
        },
    ]
}

fn selected_loops(
    loops: &[EmergencyLoopSample],
    selected_loop_ids: &[String],
) -> Vec<EmergencyLoopSample> {
    loops
        .iter()
        .filter(|sample| {
            selected_loop_ids
                .iter()
                .any(|loop_id| loop_id == &sample.loop_id)
        })
        .cloned()
        .collect()
}

fn count_requested_targets(
    selected_loop_ids: &[String],
    stop_requested_loop_ids: &[String],
) -> usize {
    selected_loop_ids
        .iter()
        .filter(|loop_id| {
            stop_requested_loop_ids
                .iter()
                .any(|requested| requested == *loop_id)
        })
        .count()
}

fn matches_scope(sample: &EmergencyLoopSample, scope: &EmergencyScopeFilter) -> bool {
    if !scope.projects.is_empty() {
        let project = sample.project.trim().to_ascii_lowercase();
        let projects: Vec<String> = scope
            .projects
            .iter()
            .map(|candidate| candidate.trim().to_ascii_lowercase())
            .filter(|candidate| !candidate.is_empty())
            .collect();
        if !projects.iter().any(|candidate| candidate == &project) {
            return false;
        }
    }

    if !scope.pools.is_empty() {
        let pool = sample.pool.trim().to_ascii_lowercase();
        let pools: Vec<String> = scope
            .pools
            .iter()
            .map(|candidate| candidate.trim().to_ascii_lowercase())
            .filter(|candidate| !candidate.is_empty())
            .collect();
        if !pools.iter().any(|candidate| candidate == &pool) {
            return false;
        }
    }

    if !scope.tags_any.is_empty() {
        let tags: Vec<String> = sample
            .tags
            .iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        let scope_tags: Vec<String> = scope
            .tags_any
            .iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        if !scope_tags
            .iter()
            .any(|scope_tag| tags.iter().any(|tag| tag == scope_tag))
        {
            return false;
        }
    }

    true
}

fn scope_label(scope: &EmergencyScopeFilter) -> String {
    let mut parts = Vec::new();
    if !scope.projects.is_empty() {
        parts.push(format!("projects={}", scope.projects.join(",")));
    }
    if !scope.pools.is_empty() {
        parts.push(format!("pools={}", scope.pools.join(",")));
    }
    if !scope.tags_any.is_empty() {
        parts.push(format!("tags={}", scope.tags_any.join(",")));
    }
    if parts.is_empty() {
        "all".to_owned()
    } else {
        parts.join(";")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_scope_preview, evaluate_emergency_safe_stop, EmergencyLoopSample,
        EmergencyScopeFilter, LoopRuntimeState, SafeStopStage, SafeStopStageStatus,
    };

    fn sample_loops() -> Vec<EmergencyLoopSample> {
        vec![
            EmergencyLoopSample {
                loop_id: "loop-a".to_owned(),
                project: "prj-a".to_owned(),
                pool: "core".to_owned(),
                tags: vec!["prod".to_owned(), "api".to_owned()],
                runtime_state: LoopRuntimeState::Running,
                queue_depth: 2,
                ledger_synced: false,
                runner_healthy: true,
            },
            EmergencyLoopSample {
                loop_id: "loop-b".to_owned(),
                project: "prj-a".to_owned(),
                pool: "core".to_owned(),
                tags: vec!["canary".to_owned()],
                runtime_state: LoopRuntimeState::Stopped,
                queue_depth: 0,
                ledger_synced: true,
                runner_healthy: true,
            },
            EmergencyLoopSample {
                loop_id: "loop-c".to_owned(),
                project: "prj-b".to_owned(),
                pool: "edge".to_owned(),
                tags: vec!["prod".to_owned()],
                runtime_state: LoopRuntimeState::Error,
                queue_depth: 0,
                ledger_synced: true,
                runner_healthy: false,
            },
        ]
    }

    #[test]
    fn scope_preview_filters_and_counts_are_stable() {
        let preview = build_scope_preview(
            &sample_loops(),
            &EmergencyScopeFilter {
                projects: vec!["prj-a".to_owned()],
                pools: vec!["core".to_owned()],
                tags_any: vec!["prod".to_owned()],
            },
        );
        assert_eq!(preview.selected_loop_ids, vec!["loop-a"]);
        assert_eq!(preview.selected_total, 1);
        assert_eq!(preview.selected_running, 1);
        assert_eq!(preview.selected_risky, 0);
        assert_eq!(preview.excluded_total, 2);
        assert!(preview.summary.contains("projects=prj-a"));
    }

    #[test]
    fn hotkey_is_required_for_one_key_safe_stop() {
        let report = evaluate_emergency_safe_stop(
            &sample_loops(),
            &EmergencyScopeFilter::default(),
            false,
            &[],
        );
        let confirm_stage = match report
            .stages
            .iter()
            .find(|stage| stage.stage == SafeStopStage::HotkeyConfirm)
        {
            Some(stage) => stage,
            None => panic!("missing hotkey confirm stage"),
        };
        assert_eq!(confirm_stage.status, SafeStopStageStatus::Blocked);
        assert_eq!(
            report.escalation_hint.as_deref(),
            Some("press Shift+X to confirm safe-stop-all")
        );
        assert!(!report.can_execute);
    }

    #[test]
    fn staged_execution_completes_when_all_targets_stop_and_integrity_passes() {
        let loops = vec![
            EmergencyLoopSample {
                loop_id: "loop-a".to_owned(),
                project: "prj-a".to_owned(),
                pool: "core".to_owned(),
                tags: vec!["prod".to_owned()],
                runtime_state: LoopRuntimeState::Stopped,
                queue_depth: 0,
                ledger_synced: true,
                runner_healthy: true,
            },
            EmergencyLoopSample {
                loop_id: "loop-b".to_owned(),
                project: "prj-a".to_owned(),
                pool: "core".to_owned(),
                tags: vec!["prod".to_owned()],
                runtime_state: LoopRuntimeState::Stopped,
                queue_depth: 0,
                ledger_synced: true,
                runner_healthy: true,
            },
        ];
        let requested = vec!["loop-a".to_owned(), "loop-b".to_owned()];
        let report = evaluate_emergency_safe_stop(
            &loops,
            &EmergencyScopeFilter::default(),
            true,
            &requested,
        );
        assert!(report.can_execute);
        assert!(report.completed);
        assert!(report
            .stages
            .iter()
            .any(|stage| stage.stage == SafeStopStage::Complete
                && stage.status == SafeStopStageStatus::Completed));
    }

    #[test]
    fn integrity_checks_block_completion_when_queue_or_ledger_is_dirty() {
        let requested = vec!["loop-a".to_owned(), "loop-c".to_owned()];
        let report = evaluate_emergency_safe_stop(
            &sample_loops(),
            &EmergencyScopeFilter {
                tags_any: vec!["prod".to_owned()],
                ..EmergencyScopeFilter::default()
            },
            true,
            &requested,
        );
        assert!(!report.completed);
        assert!(report
            .integrity_checks
            .iter()
            .any(|check| check.name == "queue-drained"
                && check.status == SafeStopStageStatus::Blocked));
        assert!(report
            .integrity_checks
            .iter()
            .any(|check| check.name == "ledger-synced"
                && check.status == SafeStopStageStatus::Blocked));
        assert!(report.escalation_hint.is_some());
    }

    #[test]
    fn empty_scope_is_blocked_with_scope_escalation_hint() {
        let report = evaluate_emergency_safe_stop(
            &sample_loops(),
            &EmergencyScopeFilter {
                projects: vec!["missing".to_owned()],
                ..EmergencyScopeFilter::default()
            },
            true,
            &[],
        );
        assert!(!report.can_execute);
        assert!(!report.completed);
        assert!(report
            .stages
            .iter()
            .any(|stage| stage.stage == SafeStopStage::ScopePreview
                && stage.status == SafeStopStageStatus::Blocked));
        assert_eq!(
            report.escalation_hint.as_deref(),
            Some("broaden scope filters before triggering emergency stop")
        );
    }
}

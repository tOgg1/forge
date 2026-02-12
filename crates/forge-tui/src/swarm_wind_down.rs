//! Wind-down workflow and final state reconciliation for swarm orchestration.

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
pub struct WindDownLoopSample {
    pub loop_id: String,
    pub swarm_id: String,
    pub runtime_state: LoopRuntimeState,
    pub stale_minutes: u32,
    pub stale_threshold_minutes: u32,
    pub ledger_synced: bool,
    pub outstanding_tasks: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindDownStepStatus {
    Pending,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindDownStep {
    pub loop_id: String,
    pub stage: String,
    pub status: WindDownStepStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindDownLoopSummary {
    pub loop_id: String,
    pub swarm_id: String,
    pub can_close: bool,
    pub blockers: Vec<String>,
    pub closure_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindDownReport {
    pub loops: Vec<WindDownLoopSummary>,
    pub steps: Vec<WindDownStep>,
    pub closable_loops: usize,
    pub blocked_loops: usize,
}

#[must_use]
pub fn evaluate_wind_down_report(samples: &[WindDownLoopSample]) -> WindDownReport {
    let mut loops = Vec::new();
    let mut steps = Vec::new();
    let mut closable_loops = 0usize;
    let mut blocked_loops = 0usize;

    for sample in samples {
        let loop_id = normalize_or_fallback(&sample.loop_id, "unknown-loop");
        let swarm_id = normalize_or_fallback(&sample.swarm_id, "unknown-swarm");
        let mut blockers = Vec::new();

        let (stop_status, stop_detail) = graceful_stop_stage(sample);
        if stop_status != WindDownStepStatus::Completed {
            blockers.push(stop_detail.clone());
        }
        steps.push(WindDownStep {
            loop_id: loop_id.clone(),
            stage: "graceful-stop".to_owned(),
            status: stop_status,
            detail: stop_detail,
        });

        let (stale_status, stale_detail) = stale_check_stage(sample);
        if stale_status != WindDownStepStatus::Completed {
            blockers.push(stale_detail.clone());
        }
        steps.push(WindDownStep {
            loop_id: loop_id.clone(),
            stage: "stale-check".to_owned(),
            status: stale_status,
            detail: stale_detail,
        });

        let (ledger_status, ledger_detail) = ledger_sync_stage(sample);
        if ledger_status != WindDownStepStatus::Completed {
            blockers.push(ledger_detail.clone());
        }
        steps.push(WindDownStep {
            loop_id: loop_id.clone(),
            stage: "ledger-sync".to_owned(),
            status: ledger_status,
            detail: ledger_detail,
        });

        if sample.outstanding_tasks > 0 {
            blockers.push(format!(
                "outstanding tasks remain: {}",
                sample.outstanding_tasks
            ));
        }

        let can_close = blockers.is_empty();
        if can_close {
            closable_loops += 1;
        } else {
            blocked_loops += 1;
        }

        loops.push(WindDownLoopSummary {
            loop_id: loop_id.clone(),
            swarm_id: swarm_id.clone(),
            can_close,
            blockers,
            closure_summary: build_closure_summary(sample, &loop_id, &swarm_id),
        });
    }

    loops.sort_by(|a, b| a.swarm_id.cmp(&b.swarm_id).then(a.loop_id.cmp(&b.loop_id)));
    steps.sort_by(|a, b| a.loop_id.cmp(&b.loop_id).then(a.stage.cmp(&b.stage)));

    WindDownReport {
        loops,
        steps,
        closable_loops,
        blocked_loops,
    }
}

fn graceful_stop_stage(sample: &WindDownLoopSample) -> (WindDownStepStatus, String) {
    match sample.runtime_state {
        LoopRuntimeState::Stopped => (
            WindDownStepStatus::Completed,
            "loop reached stopped state".to_owned(),
        ),
        LoopRuntimeState::Error => (
            WindDownStepStatus::Blocked,
            format!(
                "loop in error state{}",
                sample
                    .last_error
                    .as_ref()
                    .map(|err| format!(": {}", err.trim()))
                    .unwrap_or_default()
            ),
        ),
        LoopRuntimeState::Running | LoopRuntimeState::Sleeping | LoopRuntimeState::Waiting => (
            WindDownStepStatus::Pending,
            "loop still active; graceful stop pending".to_owned(),
        ),
        LoopRuntimeState::Unknown => (
            WindDownStepStatus::Blocked,
            "loop state unknown; reconcile runtime before close".to_owned(),
        ),
    }
}

fn stale_check_stage(sample: &WindDownLoopSample) -> (WindDownStepStatus, String) {
    if sample.stale_minutes > sample.stale_threshold_minutes {
        return (
            WindDownStepStatus::Blocked,
            format!(
                "stale threshold exceeded: {}m > {}m",
                sample.stale_minutes, sample.stale_threshold_minutes
            ),
        );
    }

    (
        WindDownStepStatus::Completed,
        format!(
            "stale check passed: {}m <= {}m",
            sample.stale_minutes, sample.stale_threshold_minutes
        ),
    )
}

fn ledger_sync_stage(sample: &WindDownLoopSample) -> (WindDownStepStatus, String) {
    if sample.ledger_synced {
        (
            WindDownStepStatus::Completed,
            "ledger sync confirmed".to_owned(),
        )
    } else {
        (
            WindDownStepStatus::Blocked,
            "ledger not synced; run final ledger sync before close".to_owned(),
        )
    }
}

fn build_closure_summary(sample: &WindDownLoopSample, loop_id: &str, swarm_id: &str) -> String {
    format!(
        "loop={} swarm={} state={} stale={}m/{}m ledger_synced={} outstanding_tasks={}",
        loop_id,
        swarm_id,
        runtime_state_label(sample.runtime_state),
        sample.stale_minutes,
        sample.stale_threshold_minutes,
        sample.ledger_synced,
        sample.outstanding_tasks
    )
}

fn runtime_state_label(state: LoopRuntimeState) -> &'static str {
    match state {
        LoopRuntimeState::Running => "running",
        LoopRuntimeState::Sleeping => "sleeping",
        LoopRuntimeState::Waiting => "waiting",
        LoopRuntimeState::Stopped => "stopped",
        LoopRuntimeState::Error => "error",
        LoopRuntimeState::Unknown => "unknown",
    }
}

fn normalize_or_fallback(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_wind_down_report, LoopRuntimeState, WindDownLoopSample, WindDownStepStatus,
    };

    #[test]
    fn stopped_fresh_synced_loop_is_closable() {
        let report = evaluate_wind_down_report(&[WindDownLoopSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            runtime_state: LoopRuntimeState::Stopped,
            stale_minutes: 2,
            stale_threshold_minutes: 45,
            ledger_synced: true,
            outstanding_tasks: 0,
            last_error: None,
        }]);

        assert_eq!(report.closable_loops, 1);
        assert_eq!(report.blocked_loops, 0);
        assert!(report.loops[0].can_close);
        assert!(report.loops[0].closure_summary.contains("state=stopped"));
    }

    #[test]
    fn stale_unsynced_or_pending_work_blocks_close() {
        let report = evaluate_wind_down_report(&[WindDownLoopSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            runtime_state: LoopRuntimeState::Stopped,
            stale_minutes: 90,
            stale_threshold_minutes: 45,
            ledger_synced: false,
            outstanding_tasks: 2,
            last_error: None,
        }]);

        assert_eq!(report.closable_loops, 0);
        assert_eq!(report.blocked_loops, 1);
        assert!(!report.loops[0].can_close);
        assert!(report.loops[0]
            .blockers
            .iter()
            .any(|reason| reason.contains("stale threshold exceeded")));
        assert!(report.loops[0]
            .blockers
            .iter()
            .any(|reason| reason.contains("ledger not synced")));
        assert!(report.loops[0]
            .blockers
            .iter()
            .any(|reason| reason.contains("outstanding tasks remain")));
    }

    #[test]
    fn running_loop_marks_graceful_stop_pending() {
        let report = evaluate_wind_down_report(&[WindDownLoopSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            runtime_state: LoopRuntimeState::Running,
            stale_minutes: 5,
            stale_threshold_minutes: 45,
            ledger_synced: true,
            outstanding_tasks: 0,
            last_error: None,
        }]);

        assert_eq!(report.blocked_loops, 1);
        let step = match report
            .steps
            .iter()
            .find(|step| step.stage == "graceful-stop")
        {
            Some(step) => step,
            None => panic!("missing graceful-stop step"),
        };
        assert_eq!(step.status, WindDownStepStatus::Pending);
    }

    #[test]
    fn sorts_output_by_swarm_then_loop() {
        let report = evaluate_wind_down_report(&[
            WindDownLoopSample {
                loop_id: "loop-z".to_owned(),
                swarm_id: "swarm-b".to_owned(),
                runtime_state: LoopRuntimeState::Stopped,
                stale_minutes: 1,
                stale_threshold_minutes: 45,
                ledger_synced: true,
                outstanding_tasks: 0,
                last_error: None,
            },
            WindDownLoopSample {
                loop_id: "loop-a".to_owned(),
                swarm_id: "swarm-a".to_owned(),
                runtime_state: LoopRuntimeState::Stopped,
                stale_minutes: 1,
                stale_threshold_minutes: 45,
                ledger_synced: true,
                outstanding_tasks: 0,
                last_error: None,
            },
        ]);

        assert_eq!(report.loops[0].swarm_id, "swarm-a");
        assert_eq!(report.loops[0].loop_id, "loop-a");
        assert_eq!(report.loops[1].swarm_id, "swarm-b");
        assert_eq!(report.loops[1].loop_id, "loop-z");
    }
}

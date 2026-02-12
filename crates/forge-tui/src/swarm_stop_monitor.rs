//! Quant/qual stop-condition monitor for swarm orchestration.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdDirection {
    AtMost,
    AtLeast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantThresholdSample {
    pub label: String,
    pub direction: ThresholdDirection,
    pub current: i64,
    pub threshold: i64,
    pub seconds_to_trigger: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualSignalSample {
    pub label: String,
    pub expected: String,
    pub observed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopStopSignalSample {
    pub loop_id: String,
    pub swarm_id: String,
    pub quant_thresholds: Vec<QuantThresholdSample>,
    pub qual_signals: Vec<QualSignalSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopSignalState {
    Healthy,
    Warning,
    Mismatch,
    Triggered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThresholdStatusView {
    pub label: String,
    pub direction: ThresholdDirection,
    pub current: i64,
    pub threshold: i64,
    pub breached: bool,
    pub seconds_to_trigger: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopStopSignalStatus {
    pub loop_id: String,
    pub swarm_id: String,
    pub state: StopSignalState,
    pub thresholds: Vec<ThresholdStatusView>,
    pub time_to_trigger_seconds: Option<u64>,
    pub mismatch_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StopSignalReport {
    pub rows: Vec<LoopStopSignalStatus>,
    pub healthy: usize,
    pub warning: usize,
    pub mismatch: usize,
    pub triggered: usize,
}

#[must_use]
pub fn evaluate_stop_signal_report(samples: &[LoopStopSignalSample]) -> StopSignalReport {
    let mut rows = Vec::new();
    let mut healthy = 0usize;
    let mut warning = 0usize;
    let mut mismatch = 0usize;
    let mut triggered = 0usize;

    for sample in samples {
        let mut threshold_views = Vec::new();
        let mut mismatch_reasons = Vec::new();
        let mut breached_any = false;
        let mut warning_any = false;
        let mut time_to_trigger: Option<u64> = None;

        for threshold in &sample.quant_thresholds {
            let breached = is_breached(threshold.direction, threshold.current, threshold.threshold);
            if breached {
                breached_any = true;
                time_to_trigger = Some(0);
            } else {
                if is_near_threshold(threshold.direction, threshold.current, threshold.threshold) {
                    warning_any = true;
                }
                if let Some(seconds) = threshold.seconds_to_trigger {
                    if seconds <= 300 {
                        warning_any = true;
                    }
                    time_to_trigger = Some(min_time_to_trigger(time_to_trigger, seconds));
                }
            }

            threshold_views.push(ThresholdStatusView {
                label: normalize_or_fallback(&threshold.label, "quant-threshold"),
                direction: threshold.direction,
                current: threshold.current,
                threshold: threshold.threshold,
                breached,
                seconds_to_trigger: threshold.seconds_to_trigger,
            });
        }

        for qual in &sample.qual_signals {
            let label = normalize_or_fallback(&qual.label, "qual-signal");
            let expected = qual.expected.trim();
            let observed = qual.observed.trim();
            if expected.is_empty() && observed.is_empty() {
                mismatch_reasons.push(format!("{label}: expected and observed signals missing"));
                continue;
            }
            if expected.is_empty() {
                mismatch_reasons.push(format!(
                    "{label}: expected signal missing (observed {observed})"
                ));
                continue;
            }
            if observed.is_empty() {
                mismatch_reasons.push(format!(
                    "{label}: observed signal missing (expected {expected})"
                ));
                continue;
            }
            if !expected.eq_ignore_ascii_case(observed) {
                mismatch_reasons.push(format!(
                    "{label}: expected {expected} but observed {observed}"
                ));
            }
        }

        let state = if breached_any {
            StopSignalState::Triggered
        } else if !mismatch_reasons.is_empty() {
            StopSignalState::Mismatch
        } else if warning_any {
            StopSignalState::Warning
        } else {
            StopSignalState::Healthy
        };

        match state {
            StopSignalState::Healthy => healthy += 1,
            StopSignalState::Warning => warning += 1,
            StopSignalState::Mismatch => mismatch += 1,
            StopSignalState::Triggered => triggered += 1,
        }

        rows.push(LoopStopSignalStatus {
            loop_id: normalize_or_fallback(&sample.loop_id, "unknown-loop"),
            swarm_id: normalize_or_fallback(&sample.swarm_id, "unknown-swarm"),
            state,
            thresholds: threshold_views,
            time_to_trigger_seconds: time_to_trigger,
            mismatch_reasons,
        });
    }

    rows.sort_by(|a, b| a.swarm_id.cmp(&b.swarm_id).then(a.loop_id.cmp(&b.loop_id)));

    StopSignalReport {
        rows,
        healthy,
        warning,
        mismatch,
        triggered,
    }
}

fn is_breached(direction: ThresholdDirection, current: i64, threshold: i64) -> bool {
    match direction {
        ThresholdDirection::AtMost => current > threshold,
        ThresholdDirection::AtLeast => current < threshold,
    }
}

fn is_near_threshold(direction: ThresholdDirection, current: i64, threshold: i64) -> bool {
    let margin = (threshold.abs() / 10).max(1);
    match direction {
        ThresholdDirection::AtMost => threshold.saturating_sub(current) <= margin,
        ThresholdDirection::AtLeast => current.saturating_sub(threshold) <= margin,
    }
}

fn min_time_to_trigger(current: Option<u64>, candidate: u64) -> u64 {
    match current {
        Some(existing) => existing.min(candidate),
        None => candidate,
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
        evaluate_stop_signal_report, LoopStopSignalSample, QualSignalSample, QuantThresholdSample,
        StopSignalState, ThresholdDirection,
    };

    #[test]
    fn threshold_breach_marks_loop_triggered() {
        let report = evaluate_stop_signal_report(&[LoopStopSignalSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            quant_thresholds: vec![QuantThresholdSample {
                label: "error-count".to_owned(),
                direction: ThresholdDirection::AtMost,
                current: 6,
                threshold: 5,
                seconds_to_trigger: Some(30),
            }],
            qual_signals: Vec::new(),
        }]);

        assert_eq!(report.triggered, 1);
        assert_eq!(report.rows[0].state, StopSignalState::Triggered);
        assert_eq!(report.rows[0].time_to_trigger_seconds, Some(0));
        assert!(report.rows[0].thresholds[0].breached);
    }

    #[test]
    fn near_threshold_and_short_timer_marks_warning() {
        let report = evaluate_stop_signal_report(&[LoopStopSignalSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            quant_thresholds: vec![QuantThresholdSample {
                label: "queue-growth".to_owned(),
                direction: ThresholdDirection::AtMost,
                current: 9,
                threshold: 10,
                seconds_to_trigger: Some(240),
            }],
            qual_signals: Vec::new(),
        }]);

        assert_eq!(report.warning, 1);
        assert_eq!(report.rows[0].state, StopSignalState::Warning);
        assert_eq!(report.rows[0].time_to_trigger_seconds, Some(240));
    }

    #[test]
    fn qual_mismatch_surfaces_reason() {
        let report = evaluate_stop_signal_report(&[LoopStopSignalSample {
            loop_id: "loop-a".to_owned(),
            swarm_id: "swarm-1".to_owned(),
            quant_thresholds: vec![QuantThresholdSample {
                label: "success-rate".to_owned(),
                direction: ThresholdDirection::AtLeast,
                current: 95,
                threshold: 90,
                seconds_to_trigger: None,
            }],
            qual_signals: vec![QualSignalSample {
                label: "approval-state".to_owned(),
                expected: "stable".to_owned(),
                observed: "degraded".to_owned(),
            }],
        }]);

        assert_eq!(report.mismatch, 1);
        assert_eq!(report.rows[0].state, StopSignalState::Mismatch);
        assert!(report.rows[0].mismatch_reasons[0].contains("expected stable"));
    }

    #[test]
    fn report_rows_are_sorted_by_swarm_then_loop() {
        let report = evaluate_stop_signal_report(&[
            LoopStopSignalSample {
                loop_id: "loop-z".to_owned(),
                swarm_id: "swarm-b".to_owned(),
                quant_thresholds: Vec::new(),
                qual_signals: Vec::new(),
            },
            LoopStopSignalSample {
                loop_id: "loop-a".to_owned(),
                swarm_id: "swarm-a".to_owned(),
                quant_thresholds: Vec::new(),
                qual_signals: Vec::new(),
            },
        ]);

        assert_eq!(report.rows.len(), 2);
        assert_eq!(report.rows[0].swarm_id, "swarm-a");
        assert_eq!(report.rows[0].loop_id, "loop-a");
        assert_eq!(report.rows[1].swarm_id, "swarm-b");
        assert_eq!(report.rows[1].loop_id, "loop-z");
    }
}

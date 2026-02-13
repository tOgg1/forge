//! Predictive queue ETA estimator for queued work completion.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueEtaState {
    Empty,
    Healthy,
    Risky,
    Stalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueDepthSnapshot {
    pub pending_items: u64,
    pub active_workers: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueRateSample {
    pub interval_seconds: u64,
    pub completed_runs: u64,
    pub failed_runs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunDurationSample {
    pub duration_ms: u64,
    pub successful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueEtaPolicy {
    pub min_runs_per_second_for_clear: f64,
    pub high_error_rate_threshold: f64,
    pub low_confidence_threshold: f64,
    pub min_duration_samples: usize,
    pub min_rate_samples: usize,
}

impl Default for QueueEtaPolicy {
    fn default() -> Self {
        Self {
            min_runs_per_second_for_clear: 0.02,
            high_error_rate_threshold: 0.35,
            low_confidence_threshold: 0.55,
            min_duration_samples: 3,
            min_rate_samples: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueEtaEstimate {
    pub state: QueueEtaState,
    pub will_clear: bool,
    pub eta_seconds: Option<u64>,
    pub eta_display: String,
    pub throughput_runs_per_min: f64,
    pub throughput_sources: usize,
    pub error_rate: f64,
    pub confidence: f64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtaAccuracySample {
    pub predicted_seconds: u64,
    pub actual_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EtaAccuracyReport {
    pub total_samples: usize,
    pub within_20pct_samples: usize,
    pub within_20pct_ratio: f64,
}

#[must_use]
pub fn estimate_queue_eta(
    queue: QueueDepthSnapshot,
    rate_samples: &[QueueRateSample],
    duration_samples: &[RunDurationSample],
    policy: &QueueEtaPolicy,
) -> QueueEtaEstimate {
    if queue.pending_items == 0 {
        return QueueEtaEstimate {
            state: QueueEtaState::Empty,
            will_clear: true,
            eta_seconds: Some(0),
            eta_display: "now".to_owned(),
            throughput_runs_per_min: 0.0,
            throughput_sources: 0,
            error_rate: 0.0,
            confidence: 1.0,
            warnings: Vec::new(),
        };
    }

    let (rate_rps, error_rate, rate_volume) = aggregate_rate(rate_samples);
    let (duration_rps, duration_volume, duration_cv) =
        throughput_from_durations(duration_samples, queue.active_workers);
    let blended_rps = blend_throughput(rate_rps, duration_rps, rate_volume, duration_volume);
    let throughput_runs_per_min = blended_rps * 60.0;
    let eta_seconds = if blended_rps > 0.0 {
        Some(((queue.pending_items as f64) / blended_rps).ceil() as u64)
    } else {
        None
    };

    let mut warnings = Vec::new();
    if error_rate >= policy.high_error_rate_threshold {
        warnings.push(format!("high failure rate {:.0}%", error_rate * 100.0));
    }
    if queue.active_workers == 0 {
        warnings.push("no active workers".to_owned());
    }
    if blended_rps <= policy.min_runs_per_second_for_clear {
        warnings.push("throughput below clear threshold".to_owned());
    }

    let confidence = confidence_score(
        rate_samples.len(),
        duration_samples.len(),
        error_rate,
        duration_cv,
        policy,
    );
    if confidence < policy.low_confidence_threshold {
        warnings.push("low-confidence estimate".to_owned());
    }

    let will_clear = blended_rps > policy.min_runs_per_second_for_clear
        && queue.active_workers > 0
        && eta_seconds.is_some();
    let state = if eta_seconds.is_none() || queue.active_workers == 0 || blended_rps <= 0.0 {
        QueueEtaState::Stalled
    } else if !will_clear || error_rate >= policy.high_error_rate_threshold {
        QueueEtaState::Risky
    } else {
        QueueEtaState::Healthy
    };

    QueueEtaEstimate {
        state,
        will_clear,
        eta_seconds,
        eta_display: format_eta(eta_seconds),
        throughput_runs_per_min,
        throughput_sources: usize::from(rate_rps > 0.0) + usize::from(duration_rps > 0.0),
        error_rate,
        confidence,
        warnings,
    }
}

#[must_use]
pub fn evaluate_eta_accuracy(samples: &[EtaAccuracySample]) -> EtaAccuracyReport {
    if samples.is_empty() {
        return EtaAccuracyReport {
            total_samples: 0,
            within_20pct_samples: 0,
            within_20pct_ratio: 0.0,
        };
    }

    let mut within = 0usize;
    for sample in samples {
        if sample.actual_seconds == 0 {
            if sample.predicted_seconds == 0 {
                within = within.saturating_add(1);
            }
            continue;
        }
        let error = sample.predicted_seconds.abs_diff(sample.actual_seconds) as f64;
        let ratio = error / sample.actual_seconds as f64;
        if ratio <= 0.20 {
            within = within.saturating_add(1);
        }
    }

    EtaAccuracyReport {
        total_samples: samples.len(),
        within_20pct_samples: within,
        within_20pct_ratio: within as f64 / samples.len() as f64,
    }
}

fn aggregate_rate(samples: &[QueueRateSample]) -> (f64, f64, u64) {
    let mut total_seconds = 0u64;
    let mut total_completed = 0u64;
    let mut total_failed = 0u64;
    for sample in samples {
        total_seconds = total_seconds.saturating_add(sample.interval_seconds);
        total_completed = total_completed.saturating_add(sample.completed_runs);
        total_failed = total_failed.saturating_add(sample.failed_runs);
    }
    if total_seconds == 0 {
        return (0.0, 0.0, 0);
    }
    let rps = total_completed as f64 / total_seconds as f64;
    let attempts = total_completed.saturating_add(total_failed);
    let error_rate = if attempts == 0 {
        0.0
    } else {
        total_failed as f64 / attempts as f64
    };
    (rps, error_rate, total_completed)
}

fn throughput_from_durations(
    samples: &[RunDurationSample],
    active_workers: u64,
) -> (f64, usize, f64) {
    if samples.is_empty() || active_workers == 0 {
        return (0.0, 0, 0.0);
    }
    let durations: Vec<f64> = samples
        .iter()
        .filter(|sample| sample.successful && sample.duration_ms > 0)
        .map(|sample| sample.duration_ms as f64)
        .collect();
    if durations.is_empty() {
        return (0.0, 0, 0.0);
    }

    let mean = durations.iter().sum::<f64>() / durations.len() as f64;
    if mean <= 0.0 {
        return (0.0, durations.len(), 0.0);
    }
    let variance = durations
        .iter()
        .map(|duration| {
            let delta = *duration - mean;
            delta * delta
        })
        .sum::<f64>()
        / durations.len() as f64;
    let std_dev = variance.sqrt();
    let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };
    let worker_seconds_per_run = mean / 1000.0;
    let rps = (active_workers as f64) / worker_seconds_per_run;
    (rps, durations.len(), cv)
}

fn blend_throughput(
    rate_rps: f64,
    duration_rps: f64,
    rate_volume: u64,
    duration_volume: usize,
) -> f64 {
    match (rate_rps > 0.0, duration_rps > 0.0) {
        (true, true) => {
            let rate_weight = 1.0 + (rate_volume.min(200) as f64 / 200.0);
            let duration_weight = 0.8 + (duration_volume.min(60) as f64 / 60.0);
            ((rate_rps * rate_weight) + (duration_rps * duration_weight))
                / (rate_weight + duration_weight)
        }
        (true, false) => rate_rps,
        (false, true) => duration_rps,
        (false, false) => 0.0,
    }
}

fn confidence_score(
    rate_count: usize,
    duration_count: usize,
    error_rate: f64,
    duration_cv: f64,
    policy: &QueueEtaPolicy,
) -> f64 {
    let rate_component =
        (rate_count as f64 / policy.min_rate_samples.max(1) as f64).min(1.0) * 0.35;
    let duration_component =
        (duration_count as f64 / policy.min_duration_samples.max(1) as f64).min(1.0) * 0.30;
    let quality_component = 0.25;
    let error_penalty = (error_rate * 0.30).min(0.20);
    let variance_penalty = (duration_cv * 0.25).min(0.20);
    (0.10 + rate_component + duration_component + quality_component
        - error_penalty
        - variance_penalty)
        .clamp(0.0, 1.0)
}

fn format_eta(eta_seconds: Option<u64>) -> String {
    let Some(seconds) = eta_seconds else {
        return "unknown".to_owned();
    };
    if seconds == 0 {
        return "now".to_owned();
    }
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        return format!("{}m {}s", seconds / 60, seconds % 60);
    }
    let hours = seconds / 3600;
    let rem = seconds % 3600;
    format!("{hours}h {}m", rem / 60)
}

#[cfg(test)]
mod tests {
    use super::{
        estimate_queue_eta, evaluate_eta_accuracy, EtaAccuracySample, QueueDepthSnapshot,
        QueueEtaPolicy, QueueEtaState, QueueRateSample, RunDurationSample,
    };

    #[test]
    fn empty_queue_returns_now() {
        let estimate = estimate_queue_eta(
            QueueDepthSnapshot {
                pending_items: 0,
                active_workers: 2,
            },
            &[],
            &[],
            &QueueEtaPolicy::default(),
        );
        assert_eq!(estimate.state, QueueEtaState::Empty);
        assert_eq!(estimate.eta_seconds, Some(0));
        assert_eq!(estimate.eta_display, "now");
        assert!(estimate.will_clear);
    }

    #[test]
    fn blended_rate_produces_eta_and_healthy_state() {
        let estimate = estimate_queue_eta(
            QueueDepthSnapshot {
                pending_items: 120,
                active_workers: 6,
            },
            &[
                QueueRateSample {
                    interval_seconds: 300,
                    completed_runs: 150,
                    failed_runs: 12,
                },
                QueueRateSample {
                    interval_seconds: 300,
                    completed_runs: 162,
                    failed_runs: 9,
                },
            ],
            &[
                RunDurationSample {
                    duration_ms: 9000,
                    successful: true,
                },
                RunDurationSample {
                    duration_ms: 10000,
                    successful: true,
                },
                RunDurationSample {
                    duration_ms: 11000,
                    successful: true,
                },
            ],
            &QueueEtaPolicy::default(),
        );
        assert_eq!(estimate.state, QueueEtaState::Healthy);
        assert!(estimate.will_clear);
        assert!(estimate.eta_seconds.is_some());
        assert!(estimate.throughput_runs_per_min > 20.0);
        assert!(estimate.confidence > 0.6);
    }

    #[test]
    fn no_workers_marks_stalled() {
        let estimate = estimate_queue_eta(
            QueueDepthSnapshot {
                pending_items: 50,
                active_workers: 0,
            },
            &[QueueRateSample {
                interval_seconds: 120,
                completed_runs: 0,
                failed_runs: 12,
            }],
            &[],
            &QueueEtaPolicy::default(),
        );
        assert_eq!(estimate.state, QueueEtaState::Stalled);
        assert!(!estimate.will_clear);
        assert!(estimate
            .warnings
            .iter()
            .any(|w| w.contains("no active workers")));
    }

    #[test]
    fn high_failure_rate_marks_risky() {
        let estimate = estimate_queue_eta(
            QueueDepthSnapshot {
                pending_items: 80,
                active_workers: 3,
            },
            &[QueueRateSample {
                interval_seconds: 300,
                completed_runs: 30,
                failed_runs: 35,
            }],
            &[RunDurationSample {
                duration_ms: 20000,
                successful: true,
            }],
            &QueueEtaPolicy::default(),
        );
        assert_eq!(estimate.state, QueueEtaState::Risky);
        assert!(estimate
            .warnings
            .iter()
            .any(|w| w.contains("high failure rate")));
    }

    #[test]
    fn accuracy_report_counts_within_twenty_percent() {
        let report = evaluate_eta_accuracy(&[
            EtaAccuracySample {
                predicted_seconds: 100,
                actual_seconds: 90,
            },
            EtaAccuracySample {
                predicted_seconds: 60,
                actual_seconds: 100,
            },
            EtaAccuracySample {
                predicted_seconds: 200,
                actual_seconds: 220,
            },
            EtaAccuracySample {
                predicted_seconds: 0,
                actual_seconds: 0,
            },
        ]);
        assert_eq!(report.total_samples, 4);
        assert_eq!(report.within_20pct_samples, 3);
        assert!(report.within_20pct_ratio > 0.70);
    }
}

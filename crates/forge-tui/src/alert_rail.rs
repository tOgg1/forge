//! Sticky alert rail planning for failures, stuck loops, and queue growth.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopAlertSample {
    pub loop_id: String,
    pub loop_label: String,
    pub recent_failures: u32,
    pub last_progress_epoch_s: Option<i64>,
    pub queue_depth: usize,
    pub previous_queue_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Warning,
    Critical,
}

impl AlertSeverity {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    #[must_use]
    fn rank(self) -> u8 {
        match self {
            Self::Critical => 2,
            Self::Warning => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertKind {
    FailureSpike,
    StuckLoop,
    QueueGrowth,
}

impl AlertKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::FailureSpike => "failure spike",
            Self::StuckLoop => "stuck loop",
            Self::QueueGrowth => "queue growth",
        }
    }

    #[must_use]
    fn slug(self) -> &'static str {
        match self {
            Self::FailureSpike => "failure",
            Self::StuckLoop => "stuck",
            Self::QueueGrowth => "queue-growth",
        }
    }

    #[must_use]
    fn sort_rank(self) -> u8 {
        match self {
            Self::FailureSpike => 0,
            Self::StuckLoop => 1,
            Self::QueueGrowth => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertRailPolicy {
    pub failure_threshold: u32,
    pub stuck_after_secs: u64,
    pub queue_growth_min_delta: i64,
    pub queue_growth_min_percent: u8,
    pub queue_growth_min_depth: usize,
    pub sticky_recovery_ticks: usize,
    pub max_alerts: usize,
}

impl Default for AlertRailPolicy {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            stuck_after_secs: 900,
            queue_growth_min_delta: 5,
            queue_growth_min_percent: 50,
            queue_growth_min_depth: 8,
            sticky_recovery_ticks: 2,
            max_alerts: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertRailAlert {
    pub id: String,
    pub loop_id: String,
    pub loop_label: String,
    pub kind: AlertKind,
    pub severity: AlertSeverity,
    pub summary: String,
    pub detail: String,
    pub sticky_recovery_ticks: usize,
    pub quick_jump_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AlertRailState {
    pub alerts: Vec<AlertRailAlert>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertStripPlan {
    pub sticky: bool,
    pub headline: String,
    pub alert_count: usize,
    pub quick_jump_targets: Vec<String>,
}

#[must_use]
pub fn build_alert_rail_state(
    samples: &[LoopAlertSample],
    now_epoch_s: i64,
    previous: &AlertRailState,
    policy: &AlertRailPolicy,
) -> AlertRailState {
    let mut next_by_id: BTreeMap<String, AlertRailAlert> = BTreeMap::new();
    for sample in samples {
        for alert in detect_alerts(sample, now_epoch_s.max(0), policy) {
            next_by_id.insert(alert.id.clone(), alert);
        }
    }

    let previous_by_id = previous
        .alerts
        .iter()
        .cloned()
        .map(|alert| (alert.id.clone(), alert))
        .collect::<BTreeMap<_, _>>();

    let mut alerts = next_by_id.into_values().collect::<Vec<_>>();
    for previous_alert in previous_by_id.values() {
        if alerts.iter().any(|alert| alert.id == previous_alert.id) {
            continue;
        }

        let recovery_tick = previous_alert.sticky_recovery_ticks.saturating_add(1);
        if recovery_tick > policy.sticky_recovery_ticks {
            continue;
        }

        let mut carried = previous_alert.clone();
        carried.sticky_recovery_ticks = recovery_tick;
        carried.severity = AlertSeverity::Warning;
        carried.summary = format!("Recovered: {}", previous_alert.summary);
        carried.detail = format!(
            "condition cleared; sticky hold tick {}/{}",
            recovery_tick, policy.sticky_recovery_ticks
        );
        alerts.push(carried);
    }

    alerts.sort_by(|a, b| {
        a.sticky_recovery_ticks
            .cmp(&b.sticky_recovery_ticks)
            .then(b.severity.rank().cmp(&a.severity.rank()))
            .then(a.kind.sort_rank().cmp(&b.kind.sort_rank()))
            .then(a.loop_id.cmp(&b.loop_id))
    });
    alerts.truncate(policy.max_alerts.max(1));

    assign_quick_jump_hints(&mut alerts);

    AlertRailState { alerts }
}

#[must_use]
pub fn plan_alert_strip(state: &AlertRailState) -> AlertStripPlan {
    if state.alerts.is_empty() {
        return AlertStripPlan {
            sticky: false,
            headline: "alerts: none".to_owned(),
            alert_count: 0,
            quick_jump_targets: Vec::new(),
        };
    }

    let quick_jump_targets = collect_quick_jump_targets(&state.alerts);
    let mut headline = format!("alerts:{} {}", state.alerts.len(), state.alerts[0].summary);
    if state.alerts.len() > 1 {
        headline.push_str(&format!(" (+{} more)", state.alerts.len() - 1));
    }

    AlertStripPlan {
        sticky: true,
        headline,
        alert_count: state.alerts.len(),
        quick_jump_targets,
    }
}

#[must_use]
pub fn quick_jump_loop_id(state: &AlertRailState, slot: usize) -> Option<&str> {
    let mut unique_slots = Vec::new();
    for alert in &state.alerts {
        if unique_slots.contains(&alert.loop_id.as_str()) {
            continue;
        }
        unique_slots.push(alert.loop_id.as_str());
        if unique_slots.len() > 9 {
            break;
        }
    }
    unique_slots.get(slot).copied()
}

fn detect_alerts(
    sample: &LoopAlertSample,
    now_epoch_s: i64,
    policy: &AlertRailPolicy,
) -> Vec<AlertRailAlert> {
    let loop_id = normalize_required(&sample.loop_id);
    if loop_id.is_empty() {
        return Vec::new();
    }
    let loop_label = normalize_label(&sample.loop_label, &loop_id);
    let mut alerts = Vec::new();

    let failure_threshold = policy.failure_threshold.max(1);
    if sample.recent_failures >= failure_threshold {
        let severity = if sample.recent_failures >= failure_threshold.saturating_mul(2) {
            AlertSeverity::Critical
        } else {
            AlertSeverity::Warning
        };
        alerts.push(AlertRailAlert {
            id: alert_id(&loop_id, AlertKind::FailureSpike),
            loop_id: loop_id.clone(),
            loop_label: loop_label.clone(),
            kind: AlertKind::FailureSpike,
            severity,
            summary: format!(
                "{}: {} ({} failures)",
                loop_label,
                AlertKind::FailureSpike.label(),
                sample.recent_failures
            ),
            detail: format!(
                "recent_failures={} threshold={}",
                sample.recent_failures, failure_threshold
            ),
            sticky_recovery_ticks: 0,
            quick_jump_hint: String::new(),
        });
    }

    let stuck_after_secs = policy.stuck_after_secs.max(60);
    if let Some(last_progress_epoch_s) = sample.last_progress_epoch_s {
        let idle_for_secs = age_seconds(now_epoch_s, last_progress_epoch_s);
        if idle_for_secs >= stuck_after_secs {
            let severity = if idle_for_secs >= stuck_after_secs.saturating_mul(2) {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(AlertRailAlert {
                id: alert_id(&loop_id, AlertKind::StuckLoop),
                loop_id: loop_id.clone(),
                loop_label: loop_label.clone(),
                kind: AlertKind::StuckLoop,
                severity,
                summary: format!(
                    "{}: {} (idle {}s)",
                    loop_label,
                    AlertKind::StuckLoop.label(),
                    idle_for_secs
                ),
                detail: format!(
                    "idle_for={}s threshold={}s last_progress_epoch_s={}",
                    idle_for_secs, stuck_after_secs, last_progress_epoch_s
                ),
                sticky_recovery_ticks: 0,
                quick_jump_hint: String::new(),
            });
        }
    }

    let delta = sample.queue_depth as i64 - sample.previous_queue_depth as i64;
    let queue_growth_min_delta = policy.queue_growth_min_delta.max(1);
    if delta >= queue_growth_min_delta && sample.queue_depth >= policy.queue_growth_min_depth {
        let baseline = sample.previous_queue_depth.max(1) as i64;
        let growth_pct = ((delta * 100) / baseline).max(0) as u8;
        if growth_pct >= policy.queue_growth_min_percent {
            let severity = if delta >= queue_growth_min_delta.saturating_mul(2)
                || growth_pct >= policy.queue_growth_min_percent.saturating_mul(2)
            {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };
            alerts.push(AlertRailAlert {
                id: alert_id(&loop_id, AlertKind::QueueGrowth),
                loop_id,
                loop_label: loop_label.clone(),
                kind: AlertKind::QueueGrowth,
                severity,
                summary: format!(
                    "{}: {} (+{}; {}->{})",
                    loop_label,
                    AlertKind::QueueGrowth.label(),
                    delta,
                    sample.previous_queue_depth,
                    sample.queue_depth
                ),
                detail: format!(
                    "queue_depth={} previous={} delta={} growth={}%",
                    sample.queue_depth, sample.previous_queue_depth, delta, growth_pct
                ),
                sticky_recovery_ticks: 0,
                quick_jump_hint: String::new(),
            });
        }
    }

    alerts
}

fn assign_quick_jump_hints(alerts: &mut [AlertRailAlert]) {
    let targets = collect_quick_jump_targets(alerts);
    let target_slots = targets
        .iter()
        .enumerate()
        .map(|(index, loop_id)| (loop_id.clone(), index + 1))
        .collect::<BTreeMap<_, _>>();

    for alert in alerts {
        alert.quick_jump_hint = target_slots
            .get(&alert.loop_id)
            .map_or_else(|| "-".to_owned(), |slot| slot.to_string());
    }
}

fn collect_quick_jump_targets(alerts: &[AlertRailAlert]) -> Vec<String> {
    let mut targets = Vec::new();
    for alert in alerts {
        if targets.iter().any(|loop_id| loop_id == &alert.loop_id) {
            continue;
        }
        targets.push(alert.loop_id.clone());
        if targets.len() == 9 {
            break;
        }
    }
    targets
}

fn normalize_required(value: &str) -> String {
    value.trim().to_owned()
}

fn normalize_label(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn alert_id(loop_id: &str, kind: AlertKind) -> String {
    format!("{loop_id}:{}", kind.slug())
}

fn age_seconds(now_epoch_s: i64, reference_epoch_s: i64) -> u64 {
    now_epoch_s.saturating_sub(reference_epoch_s).max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::{
        build_alert_rail_state, plan_alert_strip, quick_jump_loop_id, AlertKind, AlertRailPolicy,
        AlertRailState, AlertSeverity, LoopAlertSample,
    };

    #[test]
    fn detects_failure_stuck_and_queue_growth_alerts() {
        let policy = AlertRailPolicy {
            failure_threshold: 3,
            stuck_after_secs: 600,
            queue_growth_min_delta: 4,
            queue_growth_min_percent: 40,
            queue_growth_min_depth: 8,
            sticky_recovery_ticks: 2,
            max_alerts: 8,
        };
        let state = build_alert_rail_state(
            &[
                LoopAlertSample {
                    loop_id: "loop-f".to_owned(),
                    loop_label: "Loop F".to_owned(),
                    recent_failures: 7,
                    last_progress_epoch_s: Some(980),
                    queue_depth: 3,
                    previous_queue_depth: 3,
                },
                LoopAlertSample {
                    loop_id: "loop-s".to_owned(),
                    loop_label: "Loop S".to_owned(),
                    recent_failures: 0,
                    last_progress_epoch_s: Some(100),
                    queue_depth: 2,
                    previous_queue_depth: 2,
                },
                LoopAlertSample {
                    loop_id: "loop-q".to_owned(),
                    loop_label: "Loop Q".to_owned(),
                    recent_failures: 0,
                    last_progress_epoch_s: Some(990),
                    queue_depth: 20,
                    previous_queue_depth: 10,
                },
            ],
            1_000,
            &AlertRailState::default(),
            &policy,
        );

        assert_eq!(state.alerts.len(), 3);
        assert_eq!(state.alerts[0].kind, AlertKind::FailureSpike);
        assert_eq!(state.alerts[0].severity, AlertSeverity::Critical);
        // QueueGrowth is Critical (delta 10 >= 2*threshold), sorts before Warning StuckLoop
        assert_eq!(state.alerts[1].kind, AlertKind::QueueGrowth);
        assert_eq!(state.alerts[1].severity, AlertSeverity::Critical);
        assert_eq!(state.alerts[2].kind, AlertKind::StuckLoop);
        assert_eq!(state.alerts[2].severity, AlertSeverity::Warning);
    }

    #[test]
    fn sticky_alerts_hold_for_recovery_window_then_drop() {
        let policy = AlertRailPolicy {
            sticky_recovery_ticks: 2,
            ..AlertRailPolicy::default()
        };
        let initial = build_alert_rail_state(
            &[LoopAlertSample {
                loop_id: "loop-a".to_owned(),
                loop_label: "Loop A".to_owned(),
                recent_failures: 4,
                last_progress_epoch_s: Some(100),
                queue_depth: 1,
                previous_queue_depth: 1,
            }],
            200,
            &AlertRailState::default(),
            &policy,
        );
        assert_eq!(initial.alerts.len(), 1);

        let recovered_tick_1 = build_alert_rail_state(
            &[LoopAlertSample {
                loop_id: "loop-a".to_owned(),
                loop_label: "Loop A".to_owned(),
                recent_failures: 0,
                last_progress_epoch_s: Some(199),
                queue_depth: 1,
                previous_queue_depth: 1,
            }],
            200,
            &initial,
            &policy,
        );
        assert_eq!(recovered_tick_1.alerts.len(), 1);
        assert_eq!(recovered_tick_1.alerts[0].sticky_recovery_ticks, 1);
        assert!(recovered_tick_1.alerts[0].summary.starts_with("Recovered:"));

        let recovered_tick_2 = build_alert_rail_state(&[], 200, &recovered_tick_1, &policy);
        assert_eq!(recovered_tick_2.alerts.len(), 1);
        assert_eq!(recovered_tick_2.alerts[0].sticky_recovery_ticks, 2);

        let recovered_tick_3 = build_alert_rail_state(&[], 200, &recovered_tick_2, &policy);
        assert!(recovered_tick_3.alerts.is_empty());
    }

    #[test]
    fn quick_jump_targets_are_unique_and_stable() {
        let policy = AlertRailPolicy {
            max_alerts: 8,
            ..AlertRailPolicy::default()
        };
        let state = build_alert_rail_state(
            &[
                LoopAlertSample {
                    loop_id: "loop-a".to_owned(),
                    loop_label: "Loop A".to_owned(),
                    recent_failures: 6,
                    last_progress_epoch_s: Some(0),
                    queue_depth: 20,
                    previous_queue_depth: 5,
                },
                LoopAlertSample {
                    loop_id: "loop-b".to_owned(),
                    loop_label: "Loop B".to_owned(),
                    recent_failures: 4,
                    last_progress_epoch_s: Some(0),
                    queue_depth: 1,
                    previous_queue_depth: 1,
                },
            ],
            2_000,
            &AlertRailState::default(),
            &policy,
        );
        let plan = plan_alert_strip(&state);
        assert_eq!(plan.quick_jump_targets, vec!["loop-a", "loop-b"]);
        assert_eq!(quick_jump_loop_id(&state, 0), Some("loop-a"));
        assert_eq!(quick_jump_loop_id(&state, 1), Some("loop-b"));
        assert_eq!(quick_jump_loop_id(&state, 2), None);
    }

    #[test]
    fn queue_growth_requires_delta_percent_and_depth() {
        let policy = AlertRailPolicy {
            queue_growth_min_delta: 5,
            queue_growth_min_percent: 50,
            queue_growth_min_depth: 8,
            ..AlertRailPolicy::default()
        };
        let state = build_alert_rail_state(
            &[LoopAlertSample {
                loop_id: "loop-q".to_owned(),
                loop_label: "Loop Q".to_owned(),
                recent_failures: 0,
                last_progress_epoch_s: Some(990),
                queue_depth: 12,
                previous_queue_depth: 9,
            }],
            1_000,
            &AlertRailState::default(),
            &policy,
        );
        assert!(state.alerts.is_empty());
    }

    #[test]
    fn strip_plan_uses_none_headline_when_empty() {
        let plan = plan_alert_strip(&AlertRailState::default());
        assert!(!plan.sticky);
        assert_eq!(plan.headline, "alerts: none");
        assert_eq!(plan.alert_count, 0);
        assert!(plan.quick_jump_targets.is_empty());
    }
}

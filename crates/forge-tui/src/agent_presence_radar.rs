//! Agent presence radar with active/idle/stuck/offline indicators.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPresenceSample {
    pub agent_id: String,
    pub last_heartbeat_epoch_s: Option<i64>,
    pub last_progress_epoch_s: Option<i64>,
    pub in_progress_tasks: usize,
    pub pending_inbox_acks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPresencePolicy {
    pub idle_after_secs: u64,
    pub stuck_after_secs: u64,
    pub offline_after_secs: u64,
}

impl Default for AgentPresencePolicy {
    fn default() -> Self {
        Self {
            idle_after_secs: 300,
            stuck_after_secs: 1_200,
            offline_after_secs: 1_800,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentPresenceState {
    Active,
    Idle,
    Stuck,
    Offline,
    Unknown,
}

impl AgentPresenceState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Stuck => "stuck",
            Self::Offline => "offline",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub fn indicator(self) -> char {
        match self {
            Self::Active => '●',
            Self::Idle => '◐',
            Self::Stuck => '▲',
            Self::Offline => '○',
            Self::Unknown => '?',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PresenceSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPresenceRow {
    pub agent_id: String,
    pub state: AgentPresenceState,
    pub severity: PresenceSeverity,
    pub idle_for_secs: u64,
    pub heartbeat_age_secs: Option<u64>,
    pub progress_age_secs: Option<u64>,
    pub in_progress_tasks: usize,
    pub pending_inbox_acks: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentPresenceSummary {
    pub total_agents: usize,
    pub active: usize,
    pub idle: usize,
    pub stuck: usize,
    pub offline: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentPresenceRadar {
    pub rows: Vec<AgentPresenceRow>,
    pub summary: AgentPresenceSummary,
}

#[must_use]
pub fn build_agent_presence_radar(
    samples: &[AgentPresenceSample],
    now_epoch_s: i64,
    policy: &AgentPresencePolicy,
) -> AgentPresenceRadar {
    let now_epoch_s = now_epoch_s.max(0);
    let merged = merge_samples(samples);

    let mut rows = merged
        .into_iter()
        .map(|sample| evaluate_agent(sample, now_epoch_s, policy))
        .collect::<Vec<_>>();

    rows.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| b.idle_for_secs.cmp(&a.idle_for_secs))
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });

    let mut summary = AgentPresenceSummary {
        total_agents: rows.len(),
        ..AgentPresenceSummary::default()
    };
    for row in &rows {
        match row.state {
            AgentPresenceState::Active => summary.active += 1,
            AgentPresenceState::Idle => summary.idle += 1,
            AgentPresenceState::Stuck => summary.stuck += 1,
            AgentPresenceState::Offline => summary.offline += 1,
            AgentPresenceState::Unknown => summary.unknown += 1,
        }
    }

    AgentPresenceRadar { rows, summary }
}

fn merge_samples(samples: &[AgentPresenceSample]) -> Vec<AgentPresenceSample> {
    let mut merged: HashMap<String, AgentPresenceSample> = HashMap::new();
    for sample in samples {
        let agent_id = normalize_required(&sample.agent_id);
        if agent_id.is_empty() {
            continue;
        }
        let entry = merged
            .entry(agent_id.clone())
            .or_insert_with(|| AgentPresenceSample {
                agent_id: agent_id.clone(),
                last_heartbeat_epoch_s: None,
                last_progress_epoch_s: None,
                in_progress_tasks: 0,
                pending_inbox_acks: 0,
            });

        entry.last_heartbeat_epoch_s =
            latest_timestamp(entry.last_heartbeat_epoch_s, sample.last_heartbeat_epoch_s);
        entry.last_progress_epoch_s =
            latest_timestamp(entry.last_progress_epoch_s, sample.last_progress_epoch_s);
        entry.in_progress_tasks += sample.in_progress_tasks;
        entry.pending_inbox_acks += sample.pending_inbox_acks;
    }
    merged.into_values().collect()
}

fn evaluate_agent(
    sample: AgentPresenceSample,
    now_epoch_s: i64,
    policy: &AgentPresencePolicy,
) -> AgentPresenceRow {
    let heartbeat_age_secs = sample
        .last_heartbeat_epoch_s
        .map(|epoch| age_seconds(now_epoch_s, epoch));
    let progress_age_secs = sample
        .last_progress_epoch_s
        .map(|epoch| age_seconds(now_epoch_s, epoch));
    let idle_for_secs = heartbeat_age_secs
        .into_iter()
        .chain(progress_age_secs)
        .max()
        .unwrap_or(0);

    let (state, severity, reasons) = classify_presence_state(
        heartbeat_age_secs,
        progress_age_secs,
        sample.in_progress_tasks,
        sample.pending_inbox_acks,
        policy,
    );

    AgentPresenceRow {
        agent_id: sample.agent_id,
        state,
        severity,
        idle_for_secs,
        heartbeat_age_secs,
        progress_age_secs,
        in_progress_tasks: sample.in_progress_tasks,
        pending_inbox_acks: sample.pending_inbox_acks,
        reasons,
    }
}

fn classify_presence_state(
    heartbeat_age_secs: Option<u64>,
    progress_age_secs: Option<u64>,
    in_progress_tasks: usize,
    pending_inbox_acks: usize,
    policy: &AgentPresencePolicy,
) -> (AgentPresenceState, PresenceSeverity, Vec<String>) {
    let mut reasons = Vec::new();
    if let Some(age) = heartbeat_age_secs {
        reasons.push(format!("heartbeat={}s", age));
    } else {
        reasons.push("heartbeat=missing".to_owned());
    }
    if let Some(age) = progress_age_secs {
        reasons.push(format!("progress={}s", age));
    } else {
        reasons.push("progress=missing".to_owned());
    }
    reasons.push(format!("tasks=in_progress:{in_progress_tasks}"));
    if pending_inbox_acks > 0 {
        reasons.push(format!("acks=pending:{pending_inbox_acks}"));
    }

    let idle_age = heartbeat_age_secs
        .into_iter()
        .chain(progress_age_secs)
        .max()
        .unwrap_or(0);

    let offline = heartbeat_age_secs.is_some_and(|age| age >= policy.offline_after_secs)
        || (heartbeat_age_secs.is_none()
            && progress_age_secs.is_some_and(|age| age >= policy.offline_after_secs));
    if offline {
        return (
            AgentPresenceState::Offline,
            PresenceSeverity::Critical,
            reasons,
        );
    }

    let stuck = in_progress_tasks > 0
        && progress_age_secs.is_some_and(|age| age >= policy.stuck_after_secs)
        && heartbeat_age_secs.is_some_and(|age| age >= policy.idle_after_secs / 2);
    if stuck {
        return (
            AgentPresenceState::Stuck,
            PresenceSeverity::Critical,
            reasons,
        );
    }

    if heartbeat_age_secs.is_none() && progress_age_secs.is_none() {
        return (
            AgentPresenceState::Unknown,
            PresenceSeverity::Warning,
            reasons,
        );
    }

    if idle_age >= policy.idle_after_secs {
        return (AgentPresenceState::Idle, PresenceSeverity::Warning, reasons);
    }

    (AgentPresenceState::Active, PresenceSeverity::Info, reasons)
}

fn latest_timestamp(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
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

#[cfg(test)]
mod tests {
    use super::{
        build_agent_presence_radar, AgentPresencePolicy, AgentPresenceSample, AgentPresenceState,
        PresenceSeverity,
    };

    #[test]
    fn classifies_active_idle_stuck_offline_and_unknown() {
        let samples = vec![
            AgentPresenceSample {
                agent_id: "agent-active".to_owned(),
                last_heartbeat_epoch_s: Some(2_995),
                last_progress_epoch_s: Some(2_994),
                in_progress_tasks: 2,
                pending_inbox_acks: 0,
            },
            AgentPresenceSample {
                agent_id: "agent-idle".to_owned(),
                last_heartbeat_epoch_s: Some(2_600),
                last_progress_epoch_s: Some(2_590),
                in_progress_tasks: 1,
                pending_inbox_acks: 1,
            },
            AgentPresenceSample {
                agent_id: "agent-stuck".to_owned(),
                last_heartbeat_epoch_s: Some(2_800),
                last_progress_epoch_s: Some(1_000),
                in_progress_tasks: 3,
                pending_inbox_acks: 2,
            },
            AgentPresenceSample {
                agent_id: "agent-offline".to_owned(),
                last_heartbeat_epoch_s: Some(500),
                last_progress_epoch_s: Some(600),
                in_progress_tasks: 1,
                pending_inbox_acks: 0,
            },
            AgentPresenceSample {
                agent_id: "agent-unknown".to_owned(),
                last_heartbeat_epoch_s: None,
                last_progress_epoch_s: None,
                in_progress_tasks: 0,
                pending_inbox_acks: 0,
            },
        ];

        let policy = AgentPresencePolicy::default();
        let radar = build_agent_presence_radar(&samples, 3_000, &policy);
        assert_eq!(radar.summary.total_agents, 5);
        assert_eq!(radar.summary.active, 1);
        assert_eq!(radar.summary.idle, 1);
        assert_eq!(radar.summary.stuck, 1);
        assert_eq!(radar.summary.offline, 1);
        assert_eq!(radar.summary.unknown, 1);

        let state_for = |agent_id: &str| {
            radar
                .rows
                .iter()
                .find(|row| row.agent_id == agent_id)
                .map(|row| (row.state, row.severity))
        };
        assert_eq!(
            state_for("agent-active"),
            Some((AgentPresenceState::Active, PresenceSeverity::Info))
        );
        assert_eq!(
            state_for("agent-idle"),
            Some((AgentPresenceState::Idle, PresenceSeverity::Warning))
        );
        assert_eq!(
            state_for("agent-stuck"),
            Some((AgentPresenceState::Stuck, PresenceSeverity::Critical))
        );
        assert_eq!(
            state_for("agent-offline"),
            Some((AgentPresenceState::Offline, PresenceSeverity::Critical))
        );
        assert_eq!(
            state_for("agent-unknown"),
            Some((AgentPresenceState::Unknown, PresenceSeverity::Warning))
        );
    }

    #[test]
    fn merge_samples_keeps_latest_timestamps_and_aggregates_counts() {
        let samples = vec![
            AgentPresenceSample {
                agent_id: "agent-a".to_owned(),
                last_heartbeat_epoch_s: Some(100),
                last_progress_epoch_s: Some(90),
                in_progress_tasks: 1,
                pending_inbox_acks: 2,
            },
            AgentPresenceSample {
                agent_id: "agent-a".to_owned(),
                last_heartbeat_epoch_s: Some(150),
                last_progress_epoch_s: Some(120),
                in_progress_tasks: 3,
                pending_inbox_acks: 0,
            },
        ];

        let radar = build_agent_presence_radar(&samples, 200, &AgentPresencePolicy::default());
        assert_eq!(radar.summary.total_agents, 1);
        let row = &radar.rows[0];
        assert_eq!(row.heartbeat_age_secs, Some(50));
        assert_eq!(row.progress_age_secs, Some(80));
        assert_eq!(row.in_progress_tasks, 4);
        assert_eq!(row.pending_inbox_acks, 2);
    }

    #[test]
    fn future_timestamps_clamp_age_to_zero() {
        let samples = vec![AgentPresenceSample {
            agent_id: "agent-a".to_owned(),
            last_heartbeat_epoch_s: Some(1_200),
            last_progress_epoch_s: Some(1_300),
            in_progress_tasks: 1,
            pending_inbox_acks: 0,
        }];

        let radar = build_agent_presence_radar(&samples, 1_000, &AgentPresencePolicy::default());
        let row = &radar.rows[0];
        assert_eq!(row.heartbeat_age_secs, Some(0));
        assert_eq!(row.progress_age_secs, Some(0));
        assert_eq!(row.state, AgentPresenceState::Active);
    }

    #[test]
    fn stale_progress_without_tasks_is_idle_not_stuck() {
        let samples = vec![AgentPresenceSample {
            agent_id: "agent-a".to_owned(),
            last_heartbeat_epoch_s: Some(700),
            last_progress_epoch_s: Some(0),
            in_progress_tasks: 0,
            pending_inbox_acks: 0,
        }];

        let radar = build_agent_presence_radar(&samples, 1_000, &AgentPresencePolicy::default());
        let row = &radar.rows[0];
        assert_eq!(row.state, AgentPresenceState::Idle);
        assert_eq!(row.severity, PresenceSeverity::Warning);
    }
}

//! War-room synchronized view state and reconciliation helpers.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarRoomParticipant {
    pub participant_id: String,
    pub display_name: String,
    pub tab_id: String,
    pub loop_id: Option<String>,
    pub run_id: Option<String>,
    pub log_scroll: usize,
    pub last_seen_epoch_s: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarRoomSyncMode {
    Off,
    FollowLeader,
    Consensus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedViewState {
    pub tab_id: String,
    pub loop_id: Option<String>,
    pub run_id: Option<String>,
    pub log_scroll: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarRoomState {
    pub enabled: bool,
    pub sync_mode: WarRoomSyncMode,
    pub leader_id: Option<String>,
    pub shared: SharedViewState,
    pub participants: Vec<WarRoomParticipant>,
}

impl Default for WarRoomState {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_mode: WarRoomSyncMode::Off,
            leader_id: None,
            shared: SharedViewState {
                tab_id: "overview".to_owned(),
                loop_id: None,
                run_id: None,
                log_scroll: 0,
            },
            participants: Vec::new(),
        }
    }
}

#[must_use]
pub fn upsert_participant(
    mut state: WarRoomState,
    participant: WarRoomParticipant,
) -> WarRoomState {
    if let Some(existing) = state
        .participants
        .iter_mut()
        .find(|entry| entry.participant_id == participant.participant_id)
    {
        *existing = participant;
    } else {
        state.participants.push(participant);
    }
    state
}

#[must_use]
pub fn set_war_room_leader(mut state: WarRoomState, leader_id: Option<&str>) -> WarRoomState {
    state.leader_id = leader_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    state
}

#[must_use]
pub fn reconcile_shared_view(mut state: WarRoomState) -> WarRoomState {
    if !state.enabled || state.sync_mode == WarRoomSyncMode::Off || state.participants.is_empty() {
        return state;
    }

    match state.sync_mode {
        WarRoomSyncMode::FollowLeader => {
            let leader = state
                .leader_id
                .as_ref()
                .and_then(|leader_id| {
                    state
                        .participants
                        .iter()
                        .find(|entry| &entry.participant_id == leader_id)
                })
                .or_else(|| state.participants.first());
            if let Some(leader) = leader {
                state.shared = SharedViewState {
                    tab_id: leader.tab_id.clone(),
                    loop_id: leader.loop_id.clone(),
                    run_id: leader.run_id.clone(),
                    log_scroll: leader.log_scroll,
                };
            }
        }
        WarRoomSyncMode::Consensus => {
            let mut tab_counts: HashMap<String, usize> = HashMap::new();
            let mut tab_scroll_sum: HashMap<String, usize> = HashMap::new();
            for participant in &state.participants {
                *tab_counts.entry(participant.tab_id.clone()).or_insert(0) += 1;
                *tab_scroll_sum
                    .entry(participant.tab_id.clone())
                    .or_insert(0) += participant.log_scroll;
            }
            if let Some((tab_id, _)) =
                tab_counts
                    .into_iter()
                    .max_by(|(left_tab, left_count), (right_tab, right_count)| {
                        left_count
                            .cmp(right_count)
                            .then_with(|| right_tab.cmp(left_tab))
                    })
            {
                let matching = state
                    .participants
                    .iter()
                    .filter(|participant| participant.tab_id == tab_id)
                    .collect::<Vec<_>>();
                let loop_id = matching
                    .iter()
                    .find_map(|participant| participant.loop_id.clone());
                let run_id = matching
                    .iter()
                    .find_map(|participant| participant.run_id.clone());
                let avg_scroll = if matching.is_empty() {
                    0
                } else {
                    tab_scroll_sum.get(&tab_id).copied().unwrap_or(0) / matching.len()
                };
                state.shared = SharedViewState {
                    tab_id,
                    loop_id,
                    run_id,
                    log_scroll: avg_scroll,
                };
            }
        }
        WarRoomSyncMode::Off => {}
    }
    state
}

#[must_use]
pub fn stale_participants(
    state: &WarRoomState,
    now_epoch_s: i64,
    stale_after_s: i64,
) -> Vec<String> {
    state
        .participants
        .iter()
        .filter(|participant| {
            now_epoch_s.saturating_sub(participant.last_seen_epoch_s) > stale_after_s.max(1)
        })
        .map(|participant| participant.participant_id.clone())
        .collect()
}

#[must_use]
pub fn render_war_room_lines(state: &WarRoomState, width: usize, max_rows: usize) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.push(trim(
        &format!(
            "war-room enabled={} mode={:?} leader={} participants={} shared_tab={} shared_loop={} shared_run={} scroll={}",
            state.enabled,
            state.sync_mode,
            state.leader_id.as_deref().unwrap_or("-"),
            state.participants.len(),
            state.shared.tab_id,
            state.shared.loop_id.as_deref().unwrap_or("-"),
            state.shared.run_id.as_deref().unwrap_or("-"),
            state.shared.log_scroll
        ),
        width,
    ));
    for participant in &state.participants {
        if lines.len() >= max_rows {
            break;
        }
        lines.push(trim(
            &format!(
                "{} tab={} loop={} run={} scroll={}",
                participant.display_name,
                participant.tab_id,
                participant.loop_id.as_deref().unwrap_or("-"),
                participant.run_id.as_deref().unwrap_or("-"),
                participant.log_scroll
            ),
            width,
        ));
    }
    lines
}

fn trim(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile_shared_view, render_war_room_lines, set_war_room_leader, stale_participants,
        upsert_participant, WarRoomParticipant, WarRoomState, WarRoomSyncMode,
    };

    fn participant(
        id: &str,
        tab: &str,
        loop_id: Option<&str>,
        run_id: Option<&str>,
        scroll: usize,
        seen: i64,
    ) -> WarRoomParticipant {
        WarRoomParticipant {
            participant_id: id.to_owned(),
            display_name: id.to_owned(),
            tab_id: tab.to_owned(),
            loop_id: loop_id.map(|value| value.to_owned()),
            run_id: run_id.map(|value| value.to_owned()),
            log_scroll: scroll,
            last_seen_epoch_s: seen,
        }
    }

    #[test]
    fn follow_leader_sync_uses_leader_view() {
        let mut state = WarRoomState {
            enabled: true,
            sync_mode: WarRoomSyncMode::FollowLeader,
            ..WarRoomState::default()
        };
        state = upsert_participant(
            state,
            participant("alpha", "logs", Some("loop-a"), Some("run-1"), 22, 100),
        );
        state = upsert_participant(
            state,
            participant("beta", "runs", Some("loop-b"), Some("run-9"), 4, 100),
        );
        state = set_war_room_leader(state, Some("beta"));

        let state = reconcile_shared_view(state);
        assert_eq!(state.shared.tab_id, "runs");
        assert_eq!(state.shared.loop_id.as_deref(), Some("loop-b"));
        assert_eq!(state.shared.run_id.as_deref(), Some("run-9"));
        assert_eq!(state.shared.log_scroll, 4);
    }

    #[test]
    fn consensus_sync_uses_majority_tab_and_average_scroll() {
        let mut state = WarRoomState {
            enabled: true,
            sync_mode: WarRoomSyncMode::Consensus,
            ..WarRoomState::default()
        };
        state = upsert_participant(
            state,
            participant("a", "logs", Some("loop-a"), None, 10, 100),
        );
        state = upsert_participant(
            state,
            participant("b", "logs", Some("loop-a"), None, 20, 100),
        );
        state = upsert_participant(
            state,
            participant("c", "runs", Some("loop-c"), Some("run-2"), 2, 100),
        );

        let state = reconcile_shared_view(state);
        assert_eq!(state.shared.tab_id, "logs");
        assert_eq!(state.shared.log_scroll, 15);
    }

    #[test]
    fn stale_participant_detection_filters_by_last_seen() {
        let mut state = WarRoomState::default();
        state = upsert_participant(state, participant("a", "logs", None, None, 0, 100));
        state = upsert_participant(state, participant("b", "logs", None, None, 0, 180));

        let stale = stale_participants(&state, 200, 30);
        assert_eq!(stale, vec!["a".to_owned()]);
    }

    #[test]
    fn render_lines_include_header_and_participants() {
        let mut state = WarRoomState {
            enabled: true,
            sync_mode: WarRoomSyncMode::FollowLeader,
            ..WarRoomState::default()
        };
        state = upsert_participant(
            state,
            participant("alpha", "logs", Some("loop-a"), Some("run-1"), 9, 100),
        );
        let lines = render_war_room_lines(&state, 120, 6);
        assert!(lines[0].contains("war-room enabled=true"));
        assert!(lines.iter().any(|line| line.contains("alpha tab=logs")));
    }
}

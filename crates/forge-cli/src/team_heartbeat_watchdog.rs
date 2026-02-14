use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamHeartbeatConfig {
    pub team_id: String,
    pub interval_seconds: u64,
    pub stale_after_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TeamHeartbeatEntry {
    pub last_heartbeat_epoch_s: Option<i64>,
    pub active: bool,
    pub restart_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TeamHeartbeatState {
    pub entries: BTreeMap<String, TeamHeartbeatEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamHeartbeatSnapshot {
    pub team_id: String,
    pub last_heartbeat_epoch_s: Option<i64>,
    pub age_seconds: Option<u64>,
    pub stale: bool,
    pub degraded: bool,
    pub restart_count: u64,
    pub status: String,
    pub next_due_epoch_s: Option<i64>,
}

pub fn load_team_heartbeat_configs_from_db(
    db: &forge_db::Db,
) -> Result<Vec<TeamHeartbeatConfig>, String> {
    let service = forge_db::team_repository::TeamService::new(db);
    let mut teams = service
        .list_teams()
        .map_err(|err| format!("list teams: {err}"))?;
    teams.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(teams
        .into_iter()
        .map(|team| {
            let interval = team.heartbeat_interval_seconds.max(1) as u64;
            TeamHeartbeatConfig {
                team_id: team.id,
                interval_seconds: interval,
                stale_after_seconds: interval.saturating_mul(3),
            }
        })
        .collect())
}

pub fn heartbeat_tick(state: &mut TeamHeartbeatState, team_id: &str, now_epoch_s: i64) {
    let team_id = normalize_team_id(team_id);
    if team_id.is_empty() {
        return;
    }
    let entry = state.entries.entry(team_id).or_default();
    entry.last_heartbeat_epoch_s = Some(now_epoch_s.max(0));
    entry.active = true;
}

pub fn evaluate_watchdog(
    state: &mut TeamHeartbeatState,
    configs: &[TeamHeartbeatConfig],
    now_epoch_s: i64,
) -> Vec<TeamHeartbeatSnapshot> {
    let now_epoch_s = now_epoch_s.max(0);
    let mut snapshots = Vec::with_capacity(configs.len());
    for config in configs {
        let team_id = normalize_team_id(&config.team_id);
        if team_id.is_empty() {
            continue;
        }
        let entry = state.entries.entry(team_id.clone()).or_default();
        let age_seconds = entry
            .last_heartbeat_epoch_s
            .and_then(|last| now_epoch_s.checked_sub(last))
            .map(|age| age as u64);
        let stale = age_seconds.is_some_and(|age| age > config.stale_after_seconds);
        if stale && entry.active {
            entry.active = false;
            entry.restart_count = entry.restart_count.saturating_add(1);
        }
        let degraded = stale;
        let status = if entry.last_heartbeat_epoch_s.is_none() {
            "missing"
        } else if degraded {
            "degraded"
        } else {
            "active"
        };
        snapshots.push(TeamHeartbeatSnapshot {
            team_id,
            last_heartbeat_epoch_s: entry.last_heartbeat_epoch_s,
            age_seconds,
            stale,
            degraded,
            restart_count: entry.restart_count,
            status: status.to_string(),
            next_due_epoch_s: entry
                .last_heartbeat_epoch_s
                .map(|last| last.saturating_add(config.interval_seconds as i64)),
        });
    }

    snapshots.sort_by(|a, b| a.team_id.cmp(&b.team_id));
    snapshots
}

pub fn persist_heartbeat_state(path: &Path, state: &TeamHeartbeatState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(state)
        .map_err(|err| format!("encode heartbeat state: {err}"))?;
    fs::write(path, body).map_err(|err| format!("write {}: {err}", path.display()))
}

pub fn restore_heartbeat_state(path: &Path) -> Result<TeamHeartbeatState, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TeamHeartbeatState::default())
        }
        Err(err) => return Err(format!("read {}: {err}", path.display())),
    };
    serde_json::from_str(&raw).map_err(|err| format!("decode {}: {err}", path.display()))
}

#[must_use]
pub fn render_team_heartbeat_rows(
    snapshots: &[TeamHeartbeatSnapshot],
    width: usize,
    max_rows: usize,
) -> Vec<String> {
    if width == 0 || max_rows == 0 {
        return Vec::new();
    }
    let mut rows = vec![fit_width(
        &format!("team-heartbeat rows={}", snapshots.len()),
        width,
    )];
    for snapshot in snapshots {
        if rows.len() >= max_rows {
            break;
        }
        let row = format!(
            "{} status={} age={}s restart={} next_due={}",
            snapshot.team_id,
            snapshot.status,
            snapshot.age_seconds.unwrap_or(0),
            snapshot.restart_count,
            snapshot
                .next_due_epoch_s
                .map_or_else(|| "-".to_string(), |value| value.to_string())
        );
        rows.push(fit_width(&row, width));
    }
    rows.truncate(max_rows);
    rows
}

fn normalize_team_id(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

fn fit_width(input: &str, width: usize) -> String {
    if input.len() <= width {
        input.to_string()
    } else {
        input.chars().take(width).collect()
    }
}

#[cfg(test)]
fn temp_path(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("forge-heartbeat-{tag}-{nanos}.json"))
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_watchdog, heartbeat_tick, load_team_heartbeat_configs_from_db,
        persist_heartbeat_state, render_team_heartbeat_rows, restore_heartbeat_state, temp_path,
        TeamHeartbeatConfig, TeamHeartbeatState,
    };

    #[test]
    fn heartbeat_tick_updates_timestamp() {
        let mut state = TeamHeartbeatState::default();
        heartbeat_tick(&mut state, "team-a", 100);
        let entry = match state.entries.get("team-a") {
            Some(entry) => entry,
            None => panic!("team-a entry should exist"),
        };
        assert_eq!(entry.last_heartbeat_epoch_s, Some(100));
        assert!(entry.active);
    }

    #[test]
    fn watchdog_marks_stale_team_degraded() {
        let mut state = TeamHeartbeatState::default();
        heartbeat_tick(&mut state, "team-a", 100);
        let snapshots = evaluate_watchdog(
            &mut state,
            &[TeamHeartbeatConfig {
                team_id: "team-a".to_string(),
                interval_seconds: 10,
                stale_after_seconds: 20,
            }],
            125,
        );
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].degraded);
        assert_eq!(snapshots[0].status, "degraded");
        assert_eq!(snapshots[0].restart_count, 1);
    }

    #[test]
    fn state_persistence_round_trip_restores_entries() {
        let path = temp_path("persist");
        let mut state = TeamHeartbeatState::default();
        heartbeat_tick(&mut state, "team-b", 77);
        if let Err(err) = persist_heartbeat_state(&path, &state) {
            panic!("persist heartbeat state: {err}");
        }
        let restored = match restore_heartbeat_state(&path) {
            Ok(restored) => restored,
            Err(err) => panic!("restore heartbeat state: {err}"),
        };
        assert_eq!(restored, state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn render_rows_surface_status_for_cli_tui() {
        let mut state = TeamHeartbeatState::default();
        heartbeat_tick(&mut state, "team-a", 200);
        let snapshots = evaluate_watchdog(
            &mut state,
            &[TeamHeartbeatConfig {
                team_id: "team-a".to_string(),
                interval_seconds: 15,
                stale_after_seconds: 45,
            }],
            210,
        );
        let rows = render_team_heartbeat_rows(&snapshots, 120, 5);
        assert!(rows[0].contains("team-heartbeat"));
        assert!(rows[1].contains("status=active"));
    }

    #[test]
    fn config_loads_intervals_from_team_repository() {
        let db_path = std::env::temp_dir().join("forge-heartbeat-config.db");
        let mut db = match forge_db::Db::open(forge_db::Config::new(&db_path)) {
            Ok(db) => db,
            Err(err) => panic!("open db: {err}"),
        };
        if let Err(err) = db.migrate_up() {
            panic!("migrate db: {err}");
        }
        let service = forge_db::team_repository::TeamService::new(&db);
        let team = match service.create_team("ops", "", "", 30) {
            Ok(team) => team,
            Err(err) => panic!("create team: {err}"),
        };

        let configs = match load_team_heartbeat_configs_from_db(&db) {
            Ok(configs) => configs,
            Err(err) => panic!("load team heartbeat configs: {err}"),
        };
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].team_id, team.id);
        assert_eq!(configs[0].interval_seconds, 30);
        assert_eq!(configs[0].stale_after_seconds, 90);

        let _ = std::fs::remove_file(db_path);
    }
}

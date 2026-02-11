use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use forge_tui::app::{App, LoopView};

#[derive(Debug, Clone, Default)]
struct LiveLoopSnapshot {
    loops: Vec<LoopView>,
    total_queue_depth: usize,
    profile_count: usize,
    running: usize,
    sleeping: usize,
    waiting: usize,
    stopped: usize,
    errored: usize,
}

fn main() {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if interactive {
        loop {
            print!("\x1b[2J\x1b[H");
            let _ = std::io::stdout().flush();
            render_snapshot();
            println!();
            println!("refresh: 2s   exit: Ctrl+C");
            thread::sleep(Duration::from_secs(2));
        }
    } else {
        render_snapshot();
    }
}

fn render_snapshot() {
    let db_path = resolve_database_path();
    let snapshot = match load_live_loop_snapshot(&db_path) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            println!("error: load live loop snapshot: {err}");
            return;
        }
    };

    let mut app = App::new("default", 200);
    app.set_loops(snapshot.loops.clone());
    println!("{}", app.render().snapshot());
    println!();
    println!("forge loop snapshot (rust)");
    println!("db: {}", db_path.display());
    println!(
        "loops: {}  queue(pending): {}  profiles: {}",
        snapshot.loops.len(),
        snapshot.total_queue_depth,
        snapshot.profile_count
    );
    println!(
        "states: running={} sleeping={} waiting={} stopped={} error={}",
        snapshot.running, snapshot.sleeping, snapshot.waiting, snapshot.stopped, snapshot.errored
    );
    println!();

    if snapshot.loops.is_empty() {
        println!("No loops found");
        return;
    }

    println!(
        "{:<10} {:<9} {:>5} {:>6} {:<18} NAME",
        "ID", "STATE", "RUNS", "QUEUE", "PROFILE"
    );
    for loop_view in snapshot.loops.iter().take(40) {
        let display_id = if loop_view.short_id.trim().is_empty() {
            trim(&loop_view.id, 10)
        } else {
            trim(&loop_view.short_id, 10)
        };
        let profile = if loop_view.profile_name.trim().is_empty() {
            "-".to_string()
        } else {
            trim(&loop_view.profile_name, 18)
        };
        println!(
            "{:<10} {:<9} {:>5} {:>6} {:<18} {}",
            display_id,
            trim(&loop_view.state, 9),
            loop_view.runs,
            loop_view.queue_depth,
            profile,
            trim(&loop_view.name, 60)
        );
    }
}

fn load_live_loop_snapshot(db_path: &Path) -> Result<LiveLoopSnapshot, String> {
    if !db_path.exists() {
        return Ok(LiveLoopSnapshot::default());
    }

    let db = forge_db::Db::open(forge_db::Config::new(db_path))
        .map_err(|err| format!("open database {}: {err}", db_path.display()))?;

    let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
    let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
    let run_repo = forge_db::loop_run_repository::LoopRunRepository::new(&db);
    let profile_repo = forge_db::profile_repository::ProfileRepository::new(&db);
    let pool_repo = forge_db::pool_repository::PoolRepository::new(&db);

    let loop_rows = match loop_repo.list() {
        Ok(rows) => rows,
        Err(err) if is_missing_table(&err, "loops") => return Ok(LiveLoopSnapshot::default()),
        Err(err) => return Err(err.to_string()),
    };

    let profile_map: HashMap<String, (String, String, String)> = match profile_repo.list() {
        Ok(rows) => rows
            .into_iter()
            .map(|profile| {
                (
                    profile.id,
                    (profile.name, profile.harness, profile.auth_kind),
                )
            })
            .collect(),
        Err(err) if is_missing_table(&err, "profiles") => HashMap::new(),
        Err(err) => return Err(err.to_string()),
    };

    let pool_map: HashMap<String, String> = match pool_repo.list() {
        Ok(rows) => rows.into_iter().map(|pool| (pool.id, pool.name)).collect(),
        Err(err) if is_missing_table(&err, "pools") => HashMap::new(),
        Err(err) => return Err(err.to_string()),
    };

    let mut snapshot = LiveLoopSnapshot {
        profile_count: profile_map.len(),
        ..LiveLoopSnapshot::default()
    };

    for loop_row in loop_rows {
        let queue_depth = match queue_repo.list(&loop_row.id) {
            Ok(items) => items.iter().filter(|item| item.status == "pending").count(),
            Err(err) if is_missing_table(&err, "loop_queue_items") => 0,
            Err(err) => return Err(err.to_string()),
        };
        let runs = match run_repo.count_by_loop(&loop_row.id) {
            Ok(count) => usize::try_from(count).unwrap_or(usize::MAX),
            Err(err) if is_missing_table(&err, "loop_runs") => 0,
            Err(err) => return Err(err.to_string()),
        };

        snapshot.total_queue_depth = snapshot.total_queue_depth.saturating_add(queue_depth);
        match loop_row.state.as_str() {
            "running" => snapshot.running += 1,
            "sleeping" => snapshot.sleeping += 1,
            "waiting" => snapshot.waiting += 1,
            "stopped" => snapshot.stopped += 1,
            "error" => snapshot.errored += 1,
            _ => {}
        }

        let (profile_name, profile_harness, profile_auth) =
            match profile_map.get(&loop_row.profile_id) {
                Some((name, harness, auth)) => (name.clone(), harness.clone(), auth.clone()),
                None => (loop_row.profile_id.clone(), String::new(), String::new()),
            };
        let pool_name = if loop_row.pool_id.is_empty() {
            String::new()
        } else {
            pool_map
                .get(&loop_row.pool_id)
                .cloned()
                .unwrap_or(loop_row.pool_id.clone())
        };

        snapshot.loops.push(LoopView {
            id: loop_row.id.clone(),
            short_id: loop_row.short_id,
            name: loop_row.name,
            state: loop_row.state.as_str().to_string(),
            repo_path: loop_row.repo_path,
            runs,
            queue_depth,
            last_run_at: loop_row.last_run_at,
            interval_seconds: loop_row.interval_seconds,
            max_runtime_seconds: loop_row.max_runtime_seconds,
            max_iterations: loop_row.max_iterations,
            last_error: loop_row.last_error,
            profile_name,
            profile_harness,
            profile_auth,
            profile_id: loop_row.profile_id,
            pool_name,
            pool_id: loop_row.pool_id,
        });
    }

    snapshot.loops.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(snapshot)
}

fn is_missing_table(err: &forge_db::DbError, table: &str) -> bool {
    err.to_string().contains(&format!("no such table: {table}"))
}

fn resolve_database_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FORGE_DATABASE_PATH") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("FORGE_DB_PATH") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".local");
        path.push("share");
        path.push("forge");
        path.push("forge.db");
        return path;
    }
    PathBuf::from("forge.db")
}

fn trim(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    if max <= 1 {
        return value.chars().take(max).collect();
    }
    let mut out: String = value.chars().take(max - 1).collect();
    out.push('~');
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use forge_db::loop_queue_repository::LoopQueueItem;
    use forge_db::loop_repository::{Loop, LoopRepository, LoopState};
    use forge_db::loop_run_repository::{LoopRun, LoopRunRepository};
    use forge_db::pool_repository::{Pool, PoolRepository};
    use forge_db::profile_repository::{Profile, ProfileRepository};

    use super::load_live_loop_snapshot;

    #[test]
    fn live_snapshot_includes_loop_queue_and_profile_fields() {
        let path = temp_db_path("snapshot-shape");
        let mut db = forge_db::Db::open(forge_db::Config::new(&path)).expect("open db");
        db.migrate_up().expect("migrate db");

        let profile_repo = ProfileRepository::new(&db);
        let mut profile = Profile {
            name: "dev".to_string(),
            harness: "codex".to_string(),
            auth_kind: "oauth".to_string(),
            command_template: "codex run".to_string(),
            ..Default::default()
        };
        profile_repo.create(&mut profile).expect("create profile");

        let pool_repo = PoolRepository::new(&db);
        let mut pool = Pool {
            name: "default".to_string(),
            ..Default::default()
        };
        pool_repo.create(&mut pool).expect("create pool");

        let loop_repo = LoopRepository::new(&db);
        let mut loop_entry = Loop {
            name: "alpha-loop".to_string(),
            repo_path: "/tmp/alpha".to_string(),
            profile_id: profile.id.clone(),
            pool_id: pool.id.clone(),
            state: LoopState::Running,
            ..Default::default()
        };
        loop_repo.create(&mut loop_entry).expect("create loop");

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let mut queue_items = vec![LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: "{\"text\":\"ship\"}".to_string(),
            ..Default::default()
        }];
        queue_repo
            .enqueue(&loop_entry.id, &mut queue_items)
            .expect("enqueue queue item");

        let run_repo = LoopRunRepository::new(&db);
        let mut run = LoopRun {
            loop_id: loop_entry.id.clone(),
            profile_id: profile.id.clone(),
            ..Default::default()
        };
        run_repo.create(&mut run).expect("create loop run");

        let snapshot = load_live_loop_snapshot(&path).expect("load live snapshot");
        assert_eq!(snapshot.loops.len(), 1);
        assert_eq!(snapshot.total_queue_depth, 1);
        assert_eq!(snapshot.profile_count, 1);
        assert_eq!(snapshot.running, 1);

        let view = &snapshot.loops[0];
        assert_eq!(view.name, "alpha-loop");
        assert_eq!(view.queue_depth, 1);
        assert_eq!(view.runs, 1);
        assert_eq!(view.profile_name, "dev");
        assert_eq!(view.profile_harness, "codex");
        assert_eq!(view.profile_auth, "oauth");
        assert_eq!(view.pool_name, "default");

        cleanup_temp_dir(&path);
    }

    #[test]
    fn live_snapshot_refreshes_queue_depth_after_enqueue() {
        let path = temp_db_path("refresh");
        let mut db = forge_db::Db::open(forge_db::Config::new(&path)).expect("open db");
        db.migrate_up().expect("migrate db");

        let loop_repo = LoopRepository::new(&db);
        let mut loop_entry = Loop {
            name: "beta-loop".to_string(),
            repo_path: "/tmp/beta".to_string(),
            state: LoopState::Stopped,
            ..Default::default()
        };
        loop_repo.create(&mut loop_entry).expect("create loop");

        let before = load_live_loop_snapshot(&path).expect("load before enqueue");
        assert_eq!(before.loops.len(), 1);
        assert_eq!(before.loops[0].queue_depth, 0);

        let queue_repo = forge_db::loop_queue_repository::LoopQueueRepository::new(&db);
        let mut queue_items = vec![LoopQueueItem {
            item_type: "message_append".to_string(),
            payload: "{\"text\":\"hello\"}".to_string(),
            ..Default::default()
        }];
        queue_repo
            .enqueue(&loop_entry.id, &mut queue_items)
            .expect("enqueue queue item");

        let after = load_live_loop_snapshot(&path).expect("load after enqueue");
        assert_eq!(after.loops[0].queue_depth, 1);

        cleanup_temp_dir(&path);
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!(
            "forge-tui-live-snapshot-{tag}-{pid}-{nanos}-{seq}.sqlite"
        ))
    }

    fn cleanup_temp_dir(path: &Path) {
        let _ = fs::remove_file(path);
    }
}

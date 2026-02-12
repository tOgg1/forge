use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use forge_tui::app::{App, LoopView};
use forge_tui::polling_pipeline::{PollScheduler, PollingConfig, PollingQueue};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderDiffPlan {
    changed_rows: Vec<usize>,
    clear_start_row: Option<usize>,
    clear_end_row: usize,
}

impl RenderDiffPlan {
    fn is_noop(&self) -> bool {
        self.changed_rows.is_empty() && self.clear_start_row.is_none()
    }
}

#[derive(Debug, Default)]
struct IncrementalRenderEngine {
    previous_lines: Vec<String>,
}

impl IncrementalRenderEngine {
    fn repaint<W: Write>(&mut self, mut out: W, next_lines: &[String]) -> std::io::Result<()> {
        let plan = plan_render_diff(&self.previous_lines, next_lines);
        if plan.is_noop() {
            return Ok(());
        }

        for row in plan.changed_rows {
            let line = next_lines.get(row - 1).map_or("", String::as_str);
            write!(out, "\x1b[{row};1H\x1b[2K{line}")?;
        }

        if let Some(start_row) = plan.clear_start_row {
            for row in start_row..=plan.clear_end_row {
                write!(out, "\x1b[{row};1H\x1b[2K")?;
            }
        }

        write!(out, "\x1b[{};1H", next_lines.len().saturating_add(1))?;
        out.flush()?;
        self.previous_lines = next_lines.to_vec();
        Ok(())
    }
}

fn main() {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if interactive {
        run_interactive();
    } else {
        print!("{}", render_snapshot_text());
    }
}

fn run_interactive() {
    let mut renderer = IncrementalRenderEngine::default();
    let db_path = resolve_database_path();
    let mut scheduler = PollScheduler::new(PollingConfig::default(), &polling_pipeline_key());
    let mut queue = PollingQueue::new(scheduler.config().max_pending_snapshots);
    let mut next_poll_deadline = Instant::now();

    print!("\x1b[2J");
    let _ = std::io::stdout().flush();

    loop {
        let mut now = Instant::now();
        while now >= next_poll_deadline {
            let lines = render_snapshot_lines_for_path(&db_path);
            let _ = queue.push(lines);
            next_poll_deadline += scheduler.next_interval(queue.len());
            now = Instant::now();
        }

        if let Some(mut lines) = queue.drain_latest() {
            lines.push(String::new());
            lines.push(render_poll_status_line(&scheduler, &queue));
            let _ = renderer.repaint(std::io::stdout(), &lines);
        }

        let sleep_for = next_poll_deadline.saturating_duration_since(Instant::now());
        if sleep_for.is_zero() {
            thread::yield_now();
        } else {
            thread::sleep(sleep_for.min(Duration::from_millis(250)));
        }
    }
}

fn polling_pipeline_key() -> String {
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "forge-tui".to_owned());
    format!("{host}:{}", std::process::id())
}

fn render_poll_status_line(scheduler: &PollScheduler, queue: &PollingQueue<Vec<String>>) -> String {
    let config = scheduler.config();
    format!(
        "refresh: {}ms + jitter<= {}ms  queue:max={} dropped={}  exit: Ctrl+C",
        config.base_interval_ms,
        config.max_jitter_ms,
        queue.max_depth_seen(),
        queue.dropped_total()
    )
}

fn plan_render_diff(previous: &[String], next: &[String]) -> RenderDiffPlan {
    let shared = previous.len().min(next.len());
    let mut changed_rows = Vec::new();

    for idx in 0..shared {
        if previous[idx] != next[idx] {
            changed_rows.push(idx + 1);
        }
    }
    if next.len() > shared {
        changed_rows.extend((shared + 1)..=next.len());
    }

    let clear_start_row = (next.len() < previous.len()).then_some(next.len() + 1);

    RenderDiffPlan {
        changed_rows,
        clear_start_row,
        clear_end_row: previous.len(),
    }
}

fn render_snapshot_text() -> String {
    let lines = render_snapshot_lines();
    if lines.is_empty() {
        return String::new();
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn render_snapshot_lines() -> Vec<String> {
    let db_path = resolve_database_path();
    render_snapshot_lines_for_path(&db_path)
}

fn render_snapshot_lines_for_path(db_path: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let snapshot = match load_live_loop_snapshot(db_path) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            lines.push(format!("error: load live loop snapshot: {err}"));
            return lines;
        }
    };

    let mut app = App::new("default", 200);
    app.set_loops(snapshot.loops.clone());
    lines.extend(app.render().snapshot().lines().map(str::to_owned));
    lines.push(String::new());
    lines.push("forge loop snapshot (rust)".to_string());
    lines.push(format!("db: {}", db_path.display()));
    lines.push(format!(
        "loops: {}  queue(pending): {}  profiles: {}",
        snapshot.loops.len(),
        snapshot.total_queue_depth,
        snapshot.profile_count
    ));
    lines.push(format!(
        "states: running={} sleeping={} waiting={} stopped={} error={}",
        snapshot.running, snapshot.sleeping, snapshot.waiting, snapshot.stopped, snapshot.errored
    ));
    lines.push(String::new());

    if snapshot.loops.is_empty() {
        lines.push("No loops found".to_string());
        return lines;
    }

    lines.push(format!(
        "{:<10} {:<9} {:>5} {:>6} {:<18} NAME",
        "ID", "STATE", "RUNS", "QUEUE", "PROFILE"
    ));
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
        lines.push(format!(
            "{:<10} {:<9} {:>5} {:>6} {:<18} {}",
            display_id,
            trim(&loop_view.state, 9),
            loop_view.runs,
            loop_view.queue_depth,
            profile,
            trim(&loop_view.name, 60)
        ));
    }
    lines
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

    use super::{load_live_loop_snapshot, plan_render_diff, IncrementalRenderEngine};

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

    #[test]
    fn render_diff_plan_marks_changed_and_appended_rows() {
        let plan = plan_render_diff(
            &lines(["header", "stable", "tail-old"]),
            &lines(["header", "changed", "tail-old", "new-row"]),
        );
        assert_eq!(plan.changed_rows, vec![2, 4]);
        assert_eq!(plan.clear_start_row, None);
        assert_eq!(plan.clear_end_row, 3);
    }

    #[test]
    fn render_diff_plan_marks_clear_range_when_next_is_shorter() {
        let plan = plan_render_diff(
            &lines(["alpha", "beta", "gamma"]),
            &lines(["alpha", "beta"]),
        );
        assert!(plan.changed_rows.is_empty());
        assert_eq!(plan.clear_start_row, Some(3));
        assert_eq!(plan.clear_end_row, 3);
    }

    #[test]
    fn incremental_repaint_noop_for_identical_frames() {
        let mut engine = IncrementalRenderEngine::default();
        let frame = lines(["row-1", "row-2"]);

        let mut first = Vec::new();
        engine.repaint(&mut first, &frame).expect("first repaint");
        assert!(!first.is_empty());

        let mut second = Vec::new();
        engine.repaint(&mut second, &frame).expect("second repaint");
        assert!(second.is_empty());
    }

    #[test]
    fn incremental_repaint_updates_changed_rows_and_clears_removed_tail() {
        let mut engine = IncrementalRenderEngine::default();

        let mut seed = Vec::new();
        engine
            .repaint(&mut seed, &lines(["alpha", "beta", "gamma"]))
            .expect("seed repaint");

        let mut out = Vec::new();
        engine
            .repaint(&mut out, &lines(["alpha", "BETA"]))
            .expect("incremental repaint");

        let ansi = String::from_utf8(out).expect("valid utf8");
        assert!(!ansi.contains("\x1b[1;1H\x1b[2Kalpha"));
        assert!(ansi.contains("\x1b[2;1H\x1b[2KBETA"));
        assert!(ansi.contains("\x1b[3;1H\x1b[2K"));
        assert!(ansi.ends_with("\x1b[3;1H"));
    }

    fn lines<const N: usize>(rows: [&str; N]) -> Vec<String> {
        rows.into_iter().map(str::to_owned).collect()
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

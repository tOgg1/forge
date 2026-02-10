#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_db::loop_queue_repository::LoopQueueRepository;
use forge_db::loop_repository::LoopRepository;
use forge_db::loop_run_repository::LoopRunRepository;
use forge_db::profile_repository::ProfileRepository;
use forge_db::{Config, Db};

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .map_or(manifest.clone(), PathBuf::from)
}

fn temp_fixture_dir(tag: &str) -> PathBuf {
    static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let suffix = UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forge-db-{tag}-{nanos}-{}-{suffix}",
        std::process::id(),
    ))
}

fn read_snapshot(path: &PathBuf) -> serde_json::Value {
    let body = std::fs::read_to_string(path).expect("read snapshot json");
    serde_json::from_str(&body).expect("parse snapshot json")
}

#[test]
fn rust_reads_existing_go_generated_forge_db_without_data_loss() {
    let fixture_dir = temp_fixture_dir("go-db-compat");
    std::fs::create_dir_all(&fixture_dir).expect("create fixture dir");

    let status = Command::new("env")
        .args([
            "-u",
            "GOROOT",
            "-u",
            "GOTOOLDIR",
            "go",
            "test",
            "./internal/parity",
            "-run",
            "^TestExportGoDBCompatFixture$",
            "-count=1",
        ])
        .current_dir(repo_root())
        .env("FORGE_GO_DB_FIXTURE_DIR", &fixture_dir)
        .status()
        .expect("run go parity fixture export");
    assert!(status.success(), "go fixture export failed: {status}");

    let db_path = fixture_dir.join("forge-go-compat.db");
    let snapshot_path = fixture_dir.join("forge-go-compat.snapshot.json");
    assert!(
        db_path.is_file(),
        "missing db fixture: {}",
        db_path.display()
    );
    assert!(
        snapshot_path.is_file(),
        "missing snapshot fixture: {}",
        snapshot_path.display()
    );

    let snapshot = read_snapshot(&snapshot_path);

    let db = Db::open(Config::new(&db_path)).expect("open go-generated db");
    let profile_repo = ProfileRepository::new(&db);
    let loop_repo = LoopRepository::new(&db);
    let run_repo = LoopRunRepository::new(&db);
    let queue_repo = LoopQueueRepository::new(&db);

    let profile_id = snapshot["profile"]["id"]
        .as_str()
        .expect("profile.id string");
    let got_profile = profile_repo.get(profile_id).expect("read profile");
    assert_eq!(
        got_profile.name,
        snapshot["profile"]["name"].as_str().unwrap()
    );
    assert_eq!(
        got_profile.harness,
        snapshot["profile"]["harness"].as_str().unwrap()
    );
    assert_eq!(
        got_profile.prompt_mode,
        snapshot["profile"]["prompt_mode"].as_str().unwrap()
    );
    assert_eq!(
        got_profile.command_template,
        snapshot["profile"]["command_template"].as_str().unwrap()
    );
    assert_eq!(
        got_profile.model,
        snapshot["profile"]["model"].as_str().unwrap()
    );
    assert_eq!(
        got_profile.max_concurrency,
        snapshot["profile"]["max_concurrency"].as_i64().unwrap()
    );
    assert_eq!(
        got_profile.extra_args,
        vec![snapshot["profile"]["extra_args"][0]
            .as_str()
            .unwrap()
            .to_string()]
    );
    assert_eq!(
        got_profile.env.get("TEAM").map(String::as_str),
        snapshot["profile"]["env"]["TEAM"].as_str()
    );

    let loop_id = snapshot["loop"]["id"].as_str().expect("loop.id string");
    let got_loop = loop_repo.get(loop_id).expect("read loop");
    assert_eq!(
        got_loop.short_id,
        snapshot["loop"]["short_id"].as_str().unwrap()
    );
    assert_eq!(got_loop.name, snapshot["loop"]["name"].as_str().unwrap());
    assert_eq!(
        got_loop.repo_path,
        snapshot["loop"]["repo_path"].as_str().unwrap()
    );
    assert_eq!(
        got_loop.interval_seconds,
        snapshot["loop"]["interval_seconds"].as_i64().unwrap()
    );
    assert_eq!(
        got_loop.profile_id,
        snapshot["loop"]["profile_id"].as_str().unwrap()
    );
    assert_eq!(
        got_loop.state.as_str(),
        snapshot["loop"]["state"].as_str().unwrap()
    );

    let run_id = snapshot["loop_run"]["id"]
        .as_str()
        .expect("loop_run.id string");
    let got_run = run_repo.get(run_id).expect("read loop run");
    assert_eq!(
        got_run.loop_id,
        snapshot["loop_run"]["loop_id"].as_str().unwrap()
    );
    assert_eq!(
        got_run.profile_id,
        snapshot["loop_run"]["profile_id"].as_str().unwrap()
    );
    assert_eq!(
        got_run.status.as_str(),
        snapshot["loop_run"]["status"].as_str().unwrap()
    );
    assert_eq!(
        got_run.prompt_source,
        snapshot["loop_run"]["prompt_source"].as_str().unwrap()
    );
    assert_eq!(
        got_run.exit_code.map(i64::from),
        snapshot["loop_run"]["exit_code"].as_i64()
    );
    assert_eq!(
        got_run.output_tail,
        snapshot["loop_run"]["output_tail"].as_str().unwrap()
    );

    let queue_id = snapshot["queue_item"]["id"]
        .as_str()
        .expect("queue_item.id string");
    let loop_id_for_queue = snapshot["queue_item"]["loop_id"]
        .as_str()
        .expect("queue_item.loop_id string");
    let got_queue_items = queue_repo
        .list(loop_id_for_queue)
        .expect("list queue items for loop");
    let got_queue = got_queue_items
        .iter()
        .find(|item| item.id == queue_id)
        .expect("queue item present");
    assert_eq!(
        got_queue.item_type,
        snapshot["queue_item"]["type"].as_str().unwrap()
    );
    assert_eq!(
        got_queue.status,
        snapshot["queue_item"]["status"].as_str().unwrap()
    );
    assert_eq!(
        got_queue.payload,
        snapshot["queue_item"]["payload_json"].as_str().unwrap()
    );
}

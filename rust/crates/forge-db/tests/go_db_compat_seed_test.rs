#![allow(clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use forge_db::loop_repository::{Loop, LoopRepository, LoopState};
use forge_db::{Config, Db};
use serde_json::json;

#[test]
fn seed_compat_db() {
    let Some(out_path) = std::env::var_os("FORGE_RUST_DB_COMPAT_OUT") else {
        // Compatibility probe is opt-in (driven by Go-side test harness).
        return;
    };
    let path = PathBuf::from(out_path);

    let _ = std::fs::remove_file(&path);

    let mut db = Db::open(Config::new(&path)).expect("open db");
    db.migrate_up().expect("migrate_up");

    let repo = LoopRepository::new(&db);

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), json!("rust"));
    metadata.insert("version".to_string(), json!(1));

    let mut loop_row = Loop {
        name: "rust-compat-loop".to_string(),
        repo_path: "/tmp/rust-compat-repo".to_string(),
        base_prompt_path: "/tmp/prompt.md".to_string(),
        base_prompt_msg: "seeded by rust".to_string(),
        interval_seconds: 42,
        max_iterations: 7,
        max_runtime_seconds: 120,
        state: LoopState::Running,
        last_error: "none".to_string(),
        log_path: "/tmp/rust-compat.log".to_string(),
        ledger_path: "/tmp/rust-compat-ledger.md".to_string(),
        tags: vec!["rust".to_string(), "compat".to_string()],
        metadata: Some(metadata),
        ..Default::default()
    };

    repo.create(&mut loop_row).expect("create loop");

    // Mutate after create so Go verifies Rust-written update paths too.
    loop_row.state = LoopState::Waiting;
    loop_row.last_error = "waiting-for-go-read".to_string();
    repo.update(&mut loop_row).expect("update loop");
}

use std::path::PathBuf;

use serde::Deserialize;

use forge_cli::msg::{run_for_test, InMemoryMsgBackend, LoopRecord, LoopState};

#[derive(Debug, Deserialize)]
struct LoopLifecycleOracle {
    steps: Vec<LoopLifecycleStep>,
}

#[derive(Debug, Deserialize)]
struct LoopLifecycleStep {
    name: String,
    #[serde(default)]
    stdout: String,
}

#[test]
fn msg_json_matches_go_loop_lifecycle_oracle() {
    let fixture_path = repo_root().join("internal/cli/testdata/oracle/loop_lifecycle.json");
    let raw = match std::fs::read_to_string(&fixture_path) {
        Ok(data) => data,
        Err(err) => panic!("read fixture {}: {err}", fixture_path.display()),
    };
    let oracle: LoopLifecycleOracle = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => panic!("decode loop_lifecycle oracle: {err}"),
    };
    let want = oracle
        .steps
        .iter()
        .find(|step| step.name == "msg")
        .map(|step| step.stdout.clone())
        .unwrap_or_else(|| panic!("loop_lifecycle oracle missing msg step"));

    let mut backend = InMemoryMsgBackend::with_loops(vec![LoopRecord {
        id: "loop-001".to_string(),
        short_id: "orc01".to_string(),
        name: "oracle-loop".to_string(),
        repo: "/repo/alpha".to_string(),
        pool: "default".to_string(),
        profile: "codex".to_string(),
        state: LoopState::Running,
        tags: vec![],
    }]);

    let out = run_for_test(
        &["msg", "oracle-loop", "hello from oracle", "--json"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 0);
    assert_eq!(out.stdout, want);
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match manifest.ancestors().nth(3) {
        Some(root) => root.to_path_buf(),
        None => manifest,
    }
}

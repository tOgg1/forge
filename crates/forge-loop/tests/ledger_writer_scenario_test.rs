use chrono::{TimeZone, Utc};
use forge_loop::ledger_writer::{
    append_ledger_entry, ensure_ledger_file, LoopLedgerRecord, LoopRunRecord, ProfileRecord,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn ledger_writer_scenario_creates_and_appends_expected_entry() {
    let temp = TempDir::new("forge-loop-ledger-scenario");
    let ledger_path = temp.path().join(".forge").join("ledgers").join("omega.md");

    let loop_record = LoopLedgerRecord {
        id: "loop-omega".to_string(),
        name: "omega".to_string(),
        repo_path: temp.path().display().to_string(),
        ledger_path: ledger_path.display().to_string(),
    };
    let run_record = LoopRunRecord {
        id: "run-omega-1".to_string(),
        status: "completed".to_string(),
        prompt_source: "override".to_string(),
        prompt_path: "override.md".to_string(),
        prompt_override: true,
        started_at: Utc.with_ymd_and_hms(2026, 2, 9, 17, 10, 0).unwrap(),
        finished_at: Some(Utc.with_ymd_and_hms(2026, 2, 9, 17, 10, 30).unwrap()),
        exit_code: Some(0),
    };
    let profile = ProfileRecord {
        name: "p-default".to_string(),
        harness: "codex".to_string(),
        auth_kind: "local".to_string(),
    };

    if let Err(err) = ensure_ledger_file(&loop_record) {
        panic!("ensure ledger file failed: {err}");
    }
    if let Err(err) = append_ledger_entry(
        &loop_record,
        &run_record,
        &profile,
        "first\nsecond\nthird",
        2,
    ) {
        panic!("append ledger entry failed: {err}");
    }

    let text = match fs::read_to_string(&ledger_path) {
        Ok(text) => text,
        Err(err) => panic!("read ledger failed: {err}"),
    };
    assert!(text.contains("# Loop Ledger: omega"));
    assert!(text.contains("- run_id: run-omega-1"));
    assert!(text.contains("- prompt_override: true"));
    assert!(text.contains("```\nsecond\nthird\n```"));
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            monotonic_nanos()
        ));
        if let Err(err) = fs::create_dir_all(&path) {
            panic!("failed creating temp dir {}: {err}", path.display());
        }
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn monotonic_nanos() -> u128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(_) => 0,
    }
}

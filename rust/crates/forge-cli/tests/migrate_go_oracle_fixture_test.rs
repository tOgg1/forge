use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct OracleReport {
    steps: Vec<OracleStep>,
}

#[derive(Debug, Deserialize)]
struct OracleStep {
    name: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    exit_code: i32,
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn migrate_matches_go_oracle_fixture() {
    let _guard = match env_lock().lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };

    let fixture_path = repo_root().join("internal/cli/testdata/oracle/migrate.json");
    let fixture = match std::fs::read_to_string(&fixture_path) {
        Ok(data) => data,
        Err(err) => panic!("read fixture {}: {err}", fixture_path.display()),
    };
    let report: OracleReport = match serde_json::from_str(&fixture) {
        Ok(value) => value,
        Err(err) => panic!("decode fixture json: {err}"),
    };

    let tmp_base = std::env::temp_dir()
        .join("forge-cli-migrate-oracle")
        .join(format!("pid-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_base);
    let db_path = tmp_base.join("forge.db");

    std::env::set_var("FORGE_DATABASE_PATH", &db_path);

    for step in report.steps {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = forge_cli::run_with_args(&step.args, &mut stdout, &mut stderr);

        let out_stdout = match String::from_utf8(stdout) {
            Ok(value) => value,
            Err(err) => panic!("stdout should be utf-8: {err}"),
        };
        let out_stderr = match String::from_utf8(stderr) {
            Ok(value) => value,
            Err(err) => panic!("stderr should be utf-8: {err}"),
        };

        assert_eq!(
            code,
            step.exit_code,
            "exit code mismatch for step {} ({})",
            step.name,
            step.args.join(" ")
        );
        assert_eq!(
            out_stdout,
            step.stdout,
            "stdout mismatch for step {} ({})",
            step.name,
            step.args.join(" ")
        );
        assert_eq!(
            out_stderr.trim(),
            step.stderr,
            "stderr mismatch for step {} ({})",
            step.name,
            step.args.join(" ")
        );
    }
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match manifest.ancestors().nth(3) {
        Some(root) => root.to_path_buf(),
        None => manifest,
    }
}

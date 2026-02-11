use forge_cli::logs::{
    default_log_path, run_for_test, CommandOutput, InMemoryLogsBackend, LogsBackend, LoopRecord,
};

#[test]
fn logs_single_tail_matches_golden() {
    let mut backend = seeded();
    let out = run(&["logs", "alpha", "--lines", "2"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/logs/single_tail.txt"));
}

#[test]
fn logs_all_repo_matches_golden() {
    let mut backend = seeded();
    let out = run(&["logs", "--all", "--lines", "1"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/logs/all_repo.txt"));
}

#[test]
fn logs_since_filter_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["logs", "alpha", "--since", "2026-01-01T00:00:01Z"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/logs/since_filtered.txt"));
}

#[test]
fn logs_invalid_lines_value_returns_error() {
    let mut backend = seeded();
    let out = run(&["logs", "alpha", "--lines", "abc"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: invalid value 'abc' for --lines\n");
}

#[test]
fn logs_missing_loop_arg_without_all_returns_error() {
    let mut backend = seeded();
    let out = run(&["logs"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "loop name required (or use --all)\n");
}

#[test]
fn logs_oracle_flow_matches_fixture() {
    let mut backend = seeded();
    let mut steps = Vec::new();

    let by_name = run(&["logs", "alpha", "--lines", "2"], &mut backend);
    steps.push(OracleStep {
        name: "logs by name".to_string(),
        stdout: by_name.stdout,
        stderr: by_name.stderr,
        exit_code: by_name.exit_code,
    });

    let all_repo = run(&["logs", "--all", "--lines", "1"], &mut backend);
    steps.push(OracleStep {
        name: "logs all".to_string(),
        stdout: all_repo.stdout,
        stderr: all_repo.stderr,
        exit_code: all_repo.exit_code,
    });

    let since = run(
        &["logs", "alpha", "--since", "2026-01-01T00:00:01Z"],
        &mut backend,
    );
    steps.push(OracleStep {
        name: "logs since".to_string(),
        stdout: since.stdout,
        stderr: since.stderr,
        exit_code: since.exit_code,
    });

    let missing = run(&["logs"], &mut backend);
    steps.push(OracleStep {
        name: "logs missing loop".to_string(),
        stdout: missing.stdout,
        stderr: missing.stderr,
        exit_code: missing.exit_code,
    });

    let unknown = run(&["logs", "alpha", "--bogus"], &mut backend);
    steps.push(OracleStep {
        name: "logs unknown flag".to_string(),
        stdout: unknown.stdout,
        stderr: unknown.stderr,
        exit_code: unknown.exit_code,
    });

    let report = OracleReport { steps };
    let got = match serde_json::to_string_pretty(&report) {
        Ok(text) => text + "\n",
        Err(err) => panic!("failed to encode report: {err}"),
    };
    let path = format!("{}/testdata/logs_oracle.json", env!("CARGO_MANIFEST_DIR"));
    let want = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => panic!("failed to read {path}: {err}"),
    };
    assert_eq!(want.replace("\r\n", "\n"), got.replace("\r\n", "\n"));
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct OracleReport {
    steps: Vec<OracleStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct OracleStep {
    name: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

fn seeded() -> InMemoryLogsBackend {
    let alpha_path = default_log_path("/tmp/forge", "alpha", "loop-001");
    let beta_path = default_log_path("/tmp/forge", "beta", "loop-002");
    let gamma_path = default_log_path("/tmp/forge", "gamma", "loop-003");

    InMemoryLogsBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo-main".to_string(),
            log_path: alpha_path.clone(),
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "def002".to_string(),
            name: "beta".to_string(),
            repo: "/repo-main".to_string(),
            log_path: beta_path.clone(),
        },
        LoopRecord {
            id: "loop-003".to_string(),
            short_id: "ghi003".to_string(),
            name: "gamma".to_string(),
            repo: "/repo-other".to_string(),
            log_path: gamma_path.clone(),
        },
    ])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(
        &alpha_path,
        "[2026-01-01T00:00:00Z] alpha-0\n[2026-01-01T00:00:01Z] alpha-1\n[2026-01-01T00:00:02Z] alpha-2\n",
    )
    .with_log(
        &beta_path,
        "[2026-01-01T00:00:00Z] beta-0\n[2026-01-01T00:00:03Z] beta-3\n",
    )
    .with_log(&gamma_path, "[2026-01-01T00:00:00Z] gamma-0\n")
}

fn run(args: &[&str], backend: &mut dyn LogsBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}

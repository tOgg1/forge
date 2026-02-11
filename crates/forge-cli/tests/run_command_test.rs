use forge_cli::run::{
    run_for_test, CommandOutput, InMemoryRunBackend, LoopRecord, LoopState, RunBackend,
};

#[test]
fn run_success_no_stdout() {
    let mut backend = seeded();
    let out = run(&["run", "alpha"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.is_empty(),
        "run should produce no stdout on success"
    );
    assert_eq!(backend.ran, vec!["loop-001"]);
}

#[test]
fn run_resolves_by_short_id() {
    let mut backend = seeded();
    let out = run(&["run", "abc001"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.ran, vec!["loop-001"]);
}

#[test]
fn run_resolves_by_full_id() {
    let mut backend = seeded();
    let out = run(&["run", "loop-001"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.ran, vec!["loop-001"]);
}

#[test]
fn run_missing_loop_returns_error() {
    let mut backend = seeded();
    let out = run(&["run", "does-not-exist"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("not found"));
}

#[test]
fn run_missing_arg_returns_error() {
    let mut backend = InMemoryRunBackend::default();
    let out = run(&["run"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: requires exactly 1 argument: <loop>\n");
}

#[test]
fn run_extra_args_returns_error() {
    let mut backend = seeded();
    let out = run(&["run", "alpha", "beta"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: accepts exactly 1 argument, received multiple\n"
    );
}

#[test]
fn run_unknown_flag_returns_error() {
    let mut backend = seeded();
    let out = run(&["run", "--bogus"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: unknown argument for run: '--bogus'\n");
}

#[test]
fn run_backend_failure_wraps_error() {
    let mut backend = FailingBackend;
    let out = run(&["run", "fail-loop"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "loop run failed: database connection lost\n");
}

#[test]
fn run_ambiguous_prefix() {
    let mut backend = InMemoryRunBackend::with_loops(vec![
        LoopRecord {
            id: "loop-abc001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            state: LoopState::Running,
        },
        LoopRecord {
            id: "loop-abc002".to_string(),
            short_id: "abc002".to_string(),
            name: "beta".to_string(),
            state: LoopState::Running,
        },
    ]);
    let out = run(&["run", "abc"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002) (use a longer prefix or full ID)\n"
    );
}

#[test]
fn run_help_flag_shows_usage() {
    let mut backend = seeded();
    let out = run(&["run", "--help"], &mut backend);
    // --help returns usage text via stderr with exit code 1
    // (matching cobra.ExactArgs(1) behavior where --help is not special)
    assert_eq!(out.exit_code, 1);
    assert!(out.stderr.contains("Usage: forge run <loop>"));
}

#[test]
fn run_oracle_flow_matches_golden() {
    let mut backend = seeded();
    let mut steps = Vec::new();

    // run by name succeeds with no stdout
    let out = run(&["run", "alpha"], &mut backend);
    steps.push(OracleStep {
        name: "run by name".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    // run by short ID succeeds
    let out = run(&["run", "abc001"], &mut backend);
    steps.push(OracleStep {
        name: "run by short id".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    // run unknown loop fails
    let out = run(&["run", "does-not-exist"], &mut backend);
    steps.push(OracleStep {
        name: "run unknown loop".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    // run with no arg fails
    let out = run(&["run"], &mut InMemoryRunBackend::default());
    steps.push(OracleStep {
        name: "run missing arg".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    let report = OracleReport { steps };
    let got = match serde_json::to_string_pretty(&report) {
        Ok(text) => text + "\n",
        Err(err) => panic!("failed to encode report: {err}"),
    };
    let path = format!("{}/testdata/run_oracle.json", env!("CARGO_MANIFEST_DIR"));
    let want = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => panic!("failed to read {path}: {err}"),
    };
    assert_eq!(want.replace("\r\n", "\n"), got.replace("\r\n", "\n"),);
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

fn seeded() -> InMemoryRunBackend {
    InMemoryRunBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
            state: LoopState::Running,
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "def002".to_string(),
            name: "beta".to_string(),
            state: LoopState::Stopped,
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn RunBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(output: &CommandOutput) {
    assert_eq!(output.exit_code, 0);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {}",
        output.stderr
    );
}

struct FailingBackend;

impl RunBackend for FailingBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(vec![LoopRecord {
            id: "loop-fail".to_string(),
            short_id: "fail01".to_string(),
            name: "fail-loop".to_string(),
            state: LoopState::Running,
        }])
    }

    fn run_once(&mut self, _loop_id: &str) -> Result<(), String> {
        Err("database connection lost".to_string())
    }
}

use forge_cli::loop_internal::{
    run_for_test, CommandOutput, InMemoryLoopInternalBackend, LoopInternalBackend, LoopRecord,
};

#[test]
fn loop_run_success_no_stdout() {
    let mut backend = seeded();
    let out = run(&["loop", "run", "alpha"], &mut backend);
    assert_success(&out);
    assert!(
        out.stdout.is_empty(),
        "loop run should produce no stdout on success"
    );
    assert_eq!(backend.ran_loops, vec!["loop-001"]);
}

#[test]
fn loop_run_resolves_by_short_id() {
    let mut backend = seeded();
    let out = run(&["loop", "run", "abc001"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.ran_loops, vec!["loop-001"]);
}

#[test]
fn loop_run_requires_run_subcommand() {
    let mut backend = seeded();
    let out = run(&["loop"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "Usage: forge loop run <loop-id>\n");
}

#[test]
fn loop_run_missing_arg_returns_error() {
    let mut backend = seeded();
    let out = run(&["loop", "run"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: requires exactly 1 argument: <loop-id>\n"
    );
}

#[test]
fn loop_run_extra_args_returns_error() {
    let mut backend = seeded();
    let out = run(&["loop", "run", "alpha", "beta"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: accepts exactly 1 argument, received multiple\n"
    );
}

#[test]
fn loop_run_unknown_flag_returns_error() {
    let mut backend = seeded();
    let out = run(&["loop", "run", "--bogus"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: unknown argument for loop run: '--bogus'\n"
    );
}

#[test]
fn loop_run_backend_failure_wraps_error() {
    let mut backend = FailingBackend;
    let out = run(&["loop", "run", "alpha"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "loop run failed: runner connection lost\n");
}

#[test]
fn loop_run_ambiguous_prefix() {
    let mut backend = InMemoryLoopInternalBackend::with_loops(vec![
        LoopRecord {
            id: "loop-abc001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
        },
        LoopRecord {
            id: "loop-abc002".to_string(),
            short_id: "abc002".to_string(),
            name: "beta".to_string(),
        },
    ]);

    let out = run(&["loop", "run", "abc"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "loop 'abc' is ambiguous; matches: alpha (abc001), beta (abc002) (use a longer prefix or full ID)\n"
    );
}

#[test]
fn loop_run_oracle_flow_matches_golden() {
    let mut backend = seeded();
    let mut steps = Vec::new();

    let out = run(&["loop", "run", "alpha"], &mut backend);
    steps.push(OracleStep {
        name: "loop run by name".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    let out = run(&["loop", "run", "abc001"], &mut backend);
    steps.push(OracleStep {
        name: "loop run by short id".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    let out = run(&["loop", "run", "does-not-exist"], &mut backend);
    steps.push(OracleStep {
        name: "loop run unknown loop".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    let out = run(&["loop", "run"], &mut backend);
    steps.push(OracleStep {
        name: "loop run missing arg".to_string(),
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    });

    let report = OracleReport { steps };
    let got = match serde_json::to_string_pretty(&report) {
        Ok(text) => text + "\n",
        Err(err) => panic!("failed to encode report: {err}"),
    };
    let path = format!(
        "{}/testdata/loop_internal_oracle.json",
        env!("CARGO_MANIFEST_DIR")
    );
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

fn seeded() -> InMemoryLoopInternalBackend {
    InMemoryLoopInternalBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "def002".to_string(),
            name: "beta".to_string(),
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn LoopInternalBackend) -> CommandOutput {
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

impl LoopInternalBackend for FailingBackend {
    fn list_loops(&self) -> Result<Vec<LoopRecord>, String> {
        Ok(vec![LoopRecord {
            id: "loop-001".to_string(),
            short_id: "abc001".to_string(),
            name: "alpha".to_string(),
        }])
    }

    fn run_loop(&mut self, _loop_id: &str) -> Result<(), String> {
        Err("runner connection lost".to_string())
    }
}

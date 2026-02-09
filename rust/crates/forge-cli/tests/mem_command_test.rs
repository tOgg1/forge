use forge_cli::mem::{run_for_test, CommandOutput, InMemoryMemBackend, MemBackend};

#[test]
fn mem_ls_empty_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["mem", "--loop", "oracle-loop", "ls", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/mem/ls_empty.json"));
}

#[test]
fn mem_set_ok_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "set",
            "blocked_on",
            "agent-b",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/mem/set_ok.json"));
}

#[test]
fn mem_get_and_ls_json_match_goldens() {
    let mut backend = seeded();
    let _ = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "set",
            "blocked_on",
            "agent-b",
            "--json",
        ],
        &mut backend,
    );

    let get_out = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "get",
            "blocked_on",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&get_out);
    assert_eq!(get_out.stdout, include_str!("golden/mem/get_entry.json"));

    let ls_out = run(
        &["mem", "--loop", "oracle-loop", "ls", "--json"],
        &mut backend,
    );
    assert_success(&ls_out);
    assert_eq!(ls_out.stdout, include_str!("golden/mem/ls_one.json"));
}

#[test]
fn mem_rm_then_ls_empty_json_match_goldens() {
    let mut backend = seeded();
    let _ = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "set",
            "blocked_on",
            "agent-b",
            "--json",
        ],
        &mut backend,
    );

    let rm_out = run(
        &["mem", "--loop", "oracle-loop", "rm", "blocked_on", "--json"],
        &mut backend,
    );
    assert_success(&rm_out);
    assert_eq!(rm_out.stdout, include_str!("golden/mem/rm_ok.json"));

    let ls_out = run(
        &["mem", "--loop", "oracle-loop", "ls", "--json"],
        &mut backend,
    );
    assert_success(&ls_out);
    assert_eq!(ls_out.stdout, include_str!("golden/mem/ls_empty.json"));
}

#[test]
fn mem_text_outputs_match_behavior() {
    let mut backend = seeded();

    let empty = run(&["mem", "--loop", "oracle-loop", "ls"], &mut backend);
    assert_success(&empty);
    assert_eq!(empty.stdout, "(empty)\n");

    let set = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "set",
            "blocked_on",
            "agent-b",
        ],
        &mut backend,
    );
    assert_success(&set);
    assert_eq!(set.stdout, "ok\n");

    let get = run(
        &["mem", "--loop", "oracle-loop", "get", "blocked_on"],
        &mut backend,
    );
    assert_success(&get);
    assert_eq!(get.stdout, "agent-b\n");

    let ls = run(&["mem", "--loop", "oracle-loop", "ls"], &mut backend);
    assert_success(&ls);
    assert_eq!(ls.stdout, "blocked_on=agent-b\n");

    let rm = run(
        &["mem", "--loop", "oracle-loop", "rm", "blocked_on"],
        &mut backend,
    );
    assert_success(&rm);
    assert_eq!(rm.stdout, "ok\n");
}

#[test]
fn mem_quiet_suppresses_mutating_human_output() {
    let mut backend = seeded();
    let out = run(
        &[
            "mem",
            "--loop",
            "oracle-loop",
            "set",
            "blocked_on",
            "agent-b",
            "--quiet",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert!(out.stdout.is_empty());
}

#[test]
fn mem_missing_key_errors_with_exit_code_1() {
    let mut backend = seeded();
    let out = run(
        &["mem", "--loop", "oracle-loop", "get", "missing-key"],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "loop kv not found\n");
}

fn seeded() -> InMemoryMemBackend {
    let mut backend = InMemoryMemBackend::default();
    backend.seed_loop("loop-123", "oracle-loop");
    backend
}

fn run(args: &[&str], backend: &mut dyn MemBackend) -> CommandOutput {
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

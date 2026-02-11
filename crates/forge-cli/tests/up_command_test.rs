use forge_cli::up::{run_for_test, CommandOutput, InMemoryUpBackend, QueueItem, UpBackend};

#[test]
fn up_single_json_matches_golden() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(&["up", "--name", "oracle-loop", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/up/single_json.json"));
}

#[test]
fn up_single_jsonl_matches_golden() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(&["up", "--name", "oracle-loop", "--jsonl"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/up/single_jsonl.jsonl"));
}

#[test]
fn up_single_human_matches_golden() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(&["up", "--name", "oracle-loop"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/up/single_human.txt"));
}

#[test]
fn up_multi_human_matches_golden() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(
        &["up", "--count", "3", "--name-prefix", "batch"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/up/multi_human.txt"));
}

#[test]
fn up_validation_errors_match_expected() {
    let mut backend = InMemoryUpBackend::default();

    let bad_count = run(&["up", "--count", "0"], &mut backend);
    assert_eq!(bad_count.exit_code, 1);
    assert!(bad_count.stdout.is_empty());
    assert_eq!(bad_count.stderr, "--count must be at least 1\n");

    let bad_name_count = run(&["up", "--name", "my-loop", "--count", "2"], &mut backend);
    assert_eq!(bad_name_count.exit_code, 1);
    assert!(bad_name_count.stdout.is_empty());
    assert_eq!(bad_name_count.stderr, "--name requires --count=1\n");

    let bad_mix = run(
        &["up", "--pool", "default", "--profile", "codex"],
        &mut backend,
    );
    assert_eq!(bad_mix.exit_code, 1);
    assert!(bad_mix.stdout.is_empty());
    assert_eq!(bad_mix.stderr, "use either --pool or --profile, not both\n");
}

#[test]
fn up_integration_scenario_creates_and_starts() {
    let mut backend = InMemoryUpBackend::default();

    let out = run(
        &[
            "up",
            "--count",
            "2",
            "--name-prefix",
            "oracle",
            "--profile",
            "codex",
            "--initial-wait",
            "30s",
            "--interval",
            "1m",
            "--max-iterations",
            "5",
            "--max-runtime",
            "2h",
            "--tags",
            "team-a, team-b",
            "--spawn-owner",
            "daemon",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);

    assert_eq!(backend.created_specs.len(), 2);
    assert_eq!(backend.created_specs[0].name, "oracle-1");
    assert_eq!(backend.created_specs[1].name, "oracle-2");
    assert_eq!(backend.created_specs[0].profile, "codex");
    assert_eq!(backend.created_specs[0].interval_seconds, 60);
    assert_eq!(backend.created_specs[0].max_iterations, 5);
    assert_eq!(backend.created_specs[0].max_runtime_seconds, 7200);
    assert_eq!(
        backend.created_specs[0].tags,
        vec!["team-a".to_string(), "team-b".to_string()]
    );

    assert_eq!(backend.starts.len(), 2);
    assert_eq!(backend.starts[0].1, "daemon");
    assert_eq!(backend.starts[1].1, "daemon");

    assert_eq!(backend.queued.len(), 2);
    for (_, item) in &backend.queued {
        assert_eq!(
            *item,
            QueueItem::Pause {
                duration_seconds: 30,
                reason: "initial wait".to_string(),
            }
        );
    }
}

#[test]
fn up_rejects_duplicate_name_from_existing() {
    let mut backend = InMemoryUpBackend::with_existing_names(vec!["already-here".to_string()]);
    let out = run(&["up", "--name", "already-here"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert_eq!(out.stderr, "loop name \"already-here\" already exists\n");
}

#[test]
fn up_with_quantitative_stop_passes_config() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(
        &[
            "up",
            "--name",
            "q-loop",
            "--quantitative-stop-cmd",
            "echo ok",
            "--quantitative-stop-every",
            "2",
            "--quantitative-stop-exit-codes",
            "0,1",
            "--quiet",
        ],
        &mut backend,
    );
    assert_success(&out);
    let spec = &backend.created_specs[0];
    let quant = match spec.stop_config.quant.as_ref() {
        Some(value) => value,
        None => panic!("expected quant config"),
    };
    assert_eq!(quant.cmd, "echo ok");
    assert_eq!(quant.every_n, 2);
    assert_eq!(quant.exit_codes, vec![0, 1]);
}

#[test]
fn up_with_qualitative_stop_passes_config() {
    let mut backend = InMemoryUpBackend::default();
    let out = run(
        &[
            "up",
            "--name",
            "j-loop",
            "--qualitative-stop-every",
            "3",
            "--qualitative-stop-prompt-msg",
            "judge this",
            "--quiet",
        ],
        &mut backend,
    );
    assert_success(&out);
    let spec = &backend.created_specs[0];
    let qual = match spec.stop_config.qual.as_ref() {
        Some(value) => value,
        None => panic!("expected qual config"),
    };
    assert_eq!(qual.every_n, 3);
    assert_eq!(qual.prompt, "judge this");
    assert!(!qual.is_prompt_path);
}

fn run(args: &[&str], backend: &mut dyn UpBackend) -> CommandOutput {
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

use forge_cli::scale::{
    run_for_test, CommandOutput, InMemoryScaleBackend, LoopRecord, QueueItem, ScaleBackend,
};

#[test]
fn scale_up_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["scale", "--count", "3", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/scale/up_json.json"));
}

#[test]
fn scale_down_jsonl_matches_golden() {
    let mut backend = seeded();
    let out = run(&["scale", "--count", "1", "--jsonl"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/scale/down_jsonl.jsonl"));
}

#[test]
fn scale_human_output_matches_golden() {
    let mut backend = seeded();
    let out = run(&["scale", "--count", "2"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/scale/human.txt"));
}

#[test]
fn scale_validation_errors_match_expected() {
    let mut backend = seeded();

    let bad_count = run(&["scale", "--count", "-1"], &mut backend);
    assert_eq!(bad_count.exit_code, 1);
    assert!(bad_count.stdout.is_empty());
    assert_eq!(bad_count.stderr, "--count must be >= 0\n");

    let bad_mix = run(
        &["scale", "--pool", "default", "--profile", "codex"],
        &mut backend,
    );
    assert_eq!(bad_mix.exit_code, 1);
    assert!(bad_mix.stdout.is_empty());
    assert_eq!(bad_mix.stderr, "use either --pool or --profile, not both\n");
}

#[test]
fn scale_integration_scenario_enqueues_and_starts_expected() {
    let mut backend = seeded();

    let up = run(
        &[
            "scale",
            "--count",
            "4",
            "--name-prefix",
            "scaled",
            "--initial-wait",
            "45s",
            "--spawn-owner",
            "daemon",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&up);
    assert_eq!(up.stdout, "{\n  \"target\": 4,\n  \"current\": 2\n}\n");

    assert_eq!(backend.created_specs.len(), 2);
    assert_eq!(backend.created_specs[0].name, "scaled-1");
    assert_eq!(backend.created_specs[1].name, "scaled-2");
    assert_eq!(backend.starts.len(), 2);
    assert_eq!(backend.starts[0].1, "daemon");
    assert_eq!(backend.starts[1].1, "daemon");

    for (loop_id, _) in &backend.starts {
        let queued = backend
            .queue_by_loop
            .get(loop_id)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            queued,
            vec![QueueItem::Pause {
                duration_seconds: 45,
                reason: "initial wait".to_string(),
            }]
        );
    }

    let down = run(&["scale", "--count", "1", "--kill", "--json"], &mut backend);
    assert_success(&down);
    assert_eq!(down.stdout, "{\n  \"target\": 1,\n  \"current\": 4\n}\n");

    let selected = backend
        .select_loops(&Default::default())
        .unwrap_or_default();
    let extra_ids: Vec<String> = selected
        .iter()
        .skip(1)
        .map(|entry| entry.id.clone())
        .collect();
    for loop_id in extra_ids {
        let queued = backend
            .queue_by_loop
            .get(&loop_id)
            .cloned()
            .unwrap_or_default();
        assert!(
            queued.iter().any(|item| matches!(item, QueueItem::KillNow)),
            "expected kill item for loop {loop_id}"
        );
    }
}

fn seeded() -> InMemoryScaleBackend {
    InMemoryScaleBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            name: "alpha".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            created_seq: 1,
        },
        LoopRecord {
            id: "loop-002".to_string(),
            name: "beta".to_string(),
            repo: "/repo".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            created_seq: 2,
        },
    ])
}

fn run(args: &[&str], backend: &mut dyn ScaleBackend) -> CommandOutput {
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

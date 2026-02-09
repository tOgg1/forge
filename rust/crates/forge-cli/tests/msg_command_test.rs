use forge_cli::msg::{
    run_for_test, CommandOutput, InMemoryMsgBackend, LoopRecord, LoopState, MsgBackend, QueueItem,
};

#[test]
fn msg_single_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["msg", "oracle-loop", "hello from ops", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/msg/single_json.json"));
}

#[test]
fn msg_multi_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["msg", "--all", "broadcast", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/msg/multi_json.json"));
}

#[test]
fn msg_single_text_matches_golden() {
    let mut backend = seeded();
    let out = run(&["msg", "oracle-loop", "hello from ops"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/msg/single_text.txt"));
}

#[test]
fn msg_single_jsonl_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["msg", "oracle-loop", "hello from ops", "--jsonl"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/msg/single_jsonl.json"));
}

#[test]
fn msg_no_selector_returns_error() {
    let mut backend = seeded();
    let out = run(&["msg"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "specify a loop or selector\n");
}

#[test]
fn msg_requires_message_without_template_seq_or_next_prompt() {
    let mut backend = seeded();
    let out = run(&["msg", "oracle-loop"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "message text required\n");
}

#[test]
fn msg_now_enqueues_steer_message() {
    let mut backend = seeded();
    let out = run(
        &["msg", "oracle-loop", "interrupt now", "--now", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(backend.enqueued.len(), 1);
    let (loop_id, items) = &backend.enqueued[0];
    assert_eq!(loop_id, "loop-001");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_type, "steer_message");
    assert_eq!(items[0].payload, "{\"message\":\"interrupt now\"}");
}

#[test]
fn msg_sequence_and_next_prompt_are_enqueued_in_order() {
    let mut backend = seeded();
    let out = run(
        &[
            "msg",
            "oracle-loop",
            "--next-prompt",
            "prompts/next.md",
            "--seq",
            "warmup",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(backend.enqueued.len(), 1);
    let (loop_id, items) = &backend.enqueued[0];
    assert_eq!(loop_id, "loop-001");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].item_type, "next_prompt_override");
    assert_eq!(
        items[0].payload,
        "{\"is_path\":true,\"prompt\":\"/repo/alpha/prompts/next.md\"}"
    );
    assert_eq!(items[1].item_type, "pause");
}

#[test]
fn msg_rejects_template_and_sequence_together() {
    let mut backend = seeded();
    let out = run(
        &[
            "msg",
            "oracle-loop",
            "--template",
            "daily",
            "--seq",
            "warmup",
        ],
        &mut backend,
    );
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "use either --template or --seq, not both\n");
}

#[test]
fn msg_no_match_returns_error() {
    let mut backend = InMemoryMsgBackend::default();
    let out = run(&["msg", "--all", "hello"], &mut backend);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "no loops matched\n");
}

#[test]
fn msg_enqueues_for_matched_loops() {
    let mut backend = seeded();
    let out = run(&["msg", "--all", "broadcast", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(backend.enqueued.len(), 2);
    assert_eq!(backend.enqueued[0].0, "loop-001");
    assert_eq!(backend.enqueued[1].0, "loop-002");
}

#[test]
fn msg_filters_by_pool() {
    let mut backend = seeded();
    let out = run(
        &["msg", "--pool", "burst", "hello burst", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, "{\n  \"loops\": 1,\n  \"queued\": true\n}\n");
    assert_eq!(backend.enqueued.len(), 1);
    assert_eq!(backend.enqueued[0].0, "loop-002");
}

#[test]
fn msg_integration_scenario() {
    let mut backend = seeded();

    // First: send a message to a specific loop
    let one = run(&["msg", "oracle-loop", "step one", "--json"], &mut backend);
    assert_success(&one);
    assert_eq!(one.stdout, "{\n  \"loops\": 1,\n  \"queued\": true\n}\n");
    assert_eq!(backend.enqueued.len(), 1);
    assert_eq!(backend.enqueued[0].0, "loop-001");

    // Second: send to a different pool using jsonl
    let two = run(
        &["msg", "--pool", "burst", "step two", "--jsonl"],
        &mut backend,
    );
    assert_success(&two);
    assert_eq!(two.stdout, "{\"loops\":1,\"queued\":true}\n");
    assert_eq!(backend.enqueued.len(), 2);
    assert_eq!(backend.enqueued[1].0, "loop-002");
}

fn seeded() -> InMemoryMsgBackend {
    InMemoryMsgBackend::with_loops(vec![
        LoopRecord {
            id: "loop-001".to_string(),
            short_id: "orc01".to_string(),
            name: "oracle-loop".to_string(),
            repo: "/repo/alpha".to_string(),
            pool: "default".to_string(),
            profile: "codex".to_string(),
            state: LoopState::Running,
            tags: vec!["team-a".to_string()],
        },
        LoopRecord {
            id: "loop-002".to_string(),
            short_id: "beta02".to_string(),
            name: "beta-loop".to_string(),
            repo: "/repo/beta".to_string(),
            pool: "burst".to_string(),
            profile: "claude".to_string(),
            state: LoopState::Stopped,
            tags: vec!["team-b".to_string()],
        },
    ])
    .with_template("daily", "rendered daily template")
    .with_sequence(
        "warmup",
        vec![QueueItem {
            item_type: "pause".to_string(),
            payload: "{\"duration_seconds\":30}".to_string(),
        }],
    )
    .with_prompt_path(
        "/repo/alpha",
        "prompts/next.md",
        "/repo/alpha/prompts/next.md",
    )
}

fn run(args: &[&str], backend: &mut dyn MsgBackend) -> CommandOutput {
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

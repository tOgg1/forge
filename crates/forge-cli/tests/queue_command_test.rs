use forge_cli::queue::{
    run_for_test, CommandOutput, InMemoryQueueBackend, LoopRecord, QueueBackend, QueueItem,
};

#[test]
fn queue_ls_pending_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["queue", "ls", "oracle-loop", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/queue/ls_pending.json"));
}

#[test]
fn queue_ls_all_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["queue", "ls", "oracle-loop", "--all", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/queue/ls_all.json"));
}

#[test]
fn queue_clear_json_matches_golden() {
    let mut backend = seeded();
    let out = run(&["queue", "clear", "oracle-loop", "--json"], &mut backend);
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/queue/clear.json"));
}

#[test]
fn queue_rm_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &["queue", "rm", "oracle-loop", "q1", "--json"],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/queue/rm.json"));
}

#[test]
fn queue_move_json_matches_golden() {
    let mut backend = seeded();
    let out = run(
        &[
            "queue",
            "move",
            "oracle-loop",
            "q3",
            "--to",
            "front",
            "--json",
        ],
        &mut backend,
    );
    assert_success(&out);
    assert_eq!(out.stdout, include_str!("golden/queue/move.json"));
}

#[test]
fn queue_integration_scenario_reorders_and_clears() {
    let mut backend = seeded();

    let moved = run(
        &[
            "queue",
            "move",
            "oracle-loop",
            "q3",
            "--to",
            "front",
            "--jsonl",
        ],
        &mut backend,
    );
    assert_success(&moved);
    assert_eq!(moved.stdout, "{\"moved\":\"q3\",\"to\":\"front\"}\n");

    let loop_entry = backend.resolve_loop("oracle-loop").unwrap_or(LoopRecord {
        id: String::new(),
        short_id: String::new(),
        name: String::new(),
    });
    assert_eq!(loop_entry.id, "loop-123");
    let listed = backend.list_queue(&loop_entry.id).unwrap_or_default();
    assert_eq!(listed[0].id, "q3");

    let cleared = run(&["queue", "clear", "oracle-loop", "--json"], &mut backend);
    assert_success(&cleared);
    assert_eq!(cleared.stdout, "{\n  \"cleared\": 2\n}\n");
}

fn seeded() -> InMemoryQueueBackend {
    let loop_entry = LoopRecord {
        id: "loop-123".to_string(),
        short_id: "abc123".to_string(),
        name: "oracle-loop".to_string(),
    };
    let mut backend = InMemoryQueueBackend::with_loops(vec![loop_entry.clone()]);
    backend.seed_queue(
        &loop_entry.id,
        vec![
            QueueItem {
                id: "q1".to_string(),
                item_type: "message_append".to_string(),
                status: "pending".to_string(),
                position: 1,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            },
            QueueItem {
                id: "q2".to_string(),
                item_type: "stop_graceful".to_string(),
                status: "completed".to_string(),
                position: 2,
                created_at: "2025-01-01T00:00:01Z".to_string(),
            },
            QueueItem {
                id: "q3".to_string(),
                item_type: "kill_now".to_string(),
                status: "pending".to_string(),
                position: 3,
                created_at: "2025-01-01T00:00:02Z".to_string(),
            },
        ],
    );
    backend
}

fn run(args: &[&str], backend: &mut dyn QueueBackend) -> CommandOutput {
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

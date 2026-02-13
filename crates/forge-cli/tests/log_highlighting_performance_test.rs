use std::time::{Duration, Instant};

use forge_cli::logs::{
    default_log_path, run_for_test, CommandOutput, InMemoryLogsBackend, LogsBackend, LoopRecord,
};

const REPLAY_TARGET_BYTES: usize = 100 * 1024 * 1024;
const FOLLOW_TARGET_BYTES: usize = 20 * 1024 * 1024;
const MIN_LINES_PER_SECOND: f64 = 10_000.0;
const MAX_REPLAY_LATENCY_SECS: f64 = 120.0;
const MAX_FOLLOW_LATENCY_SECS: f64 = 60.0;
const MAX_OUTPUT_AMPLIFICATION: f64 = 4.0;

#[test]
fn logs_replay_meets_performance_budget() {
    let path = default_log_path("/tmp/forge", "alpha", "loop-perf-replay");
    let (content, line_count) = generate_log_payload(REPLAY_TARGET_BYTES);

    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-perf-replay".to_string(),
        short_id: "perf001".to_string(),
        name: "alpha".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_log(&path, &content);

    let started = Instant::now();
    let out = run(
        &["logs", "alpha", "--lines", "5000000", "--no-color"],
        &mut backend,
    );
    let elapsed = started.elapsed();

    assert_success(&out);
    assert_perf_budget(
        "replay",
        line_count,
        content.len(),
        out.stdout.len(),
        elapsed,
        MIN_LINES_PER_SECOND,
        MAX_REPLAY_LATENCY_SECS,
        MAX_OUTPUT_AMPLIFICATION,
    );
}

#[test]
fn logs_follow_meets_throughput_budget() {
    let path = default_log_path("/tmp/forge", "alpha", "loop-perf-follow");
    let (content, line_count) = generate_log_payload(FOLLOW_TARGET_BYTES);

    let mut backend = InMemoryLogsBackend::with_loops(vec![LoopRecord {
        id: "loop-perf-follow".to_string(),
        short_id: "perf002".to_string(),
        name: "alpha".to_string(),
        repo: "/repo-main".to_string(),
        log_path: path.clone(),
    }])
    .with_data_dir("/tmp/forge")
    .with_repo_path("/repo-main")
    .with_follow_output(&path, &content);

    let started = Instant::now();
    let out = run(
        &[
            "logs",
            "alpha",
            "--follow",
            "--lines",
            "5000000",
            "--no-color",
        ],
        &mut backend,
    );
    let elapsed = started.elapsed();

    assert_success(&out);
    assert_perf_budget(
        "follow",
        line_count,
        content.len(),
        out.stdout.len(),
        elapsed,
        MIN_LINES_PER_SECOND,
        MAX_FOLLOW_LATENCY_SECS,
        MAX_OUTPUT_AMPLIFICATION,
    );
}

fn generate_log_payload(target_bytes: usize) -> (String, usize) {
    const PATTERNS: &[&str] = &[
        "[2026-02-13T00:00:00Z] status: running loop=alpha",
        "exec",
        "$ cargo test --workspace",
        "running 128 tests",
        "test forge_cli::logs::perf ... ok",
        "diff --git a/src/lib.rs b/src/lib.rs",
        "@@ -1,2 +1,2 @@",
        "-let answer = 41;",
        "+let answer = 42;",
        "Traceback (most recent call last):",
        "  File \"runner.py\", line 42, in <module>",
        "ValueError: bad value",
        "```rust",
        "fn main() { println!(\"ok\"); }",
        "```",
        "tokens used",
        "15892",
        "tool: read file `src/lib.rs`",
    ];

    let mut content = String::with_capacity(target_bytes + 4096);
    let mut lines = 0usize;
    let mut index = 0usize;

    while content.len() < target_bytes {
        content.push_str(PATTERNS[index % PATTERNS.len()]);
        content.push('\n');
        lines += 1;
        index += 1;
    }

    (content, lines)
}

fn run(args: &[&str], backend: &mut dyn LogsBackend) -> CommandOutput {
    run_for_test(args, backend)
}

fn assert_success(out: &CommandOutput) {
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty(), "unexpected stderr: {}", out.stderr);
}

fn assert_perf_budget(
    mode: &str,
    line_count: usize,
    input_bytes: usize,
    output_bytes: usize,
    elapsed: Duration,
    min_lines_per_second: f64,
    max_latency_secs: f64,
    max_output_amplification: f64,
) {
    let elapsed_secs = elapsed.as_secs_f64().max(0.000_001);
    let lines_per_second = line_count as f64 / elapsed_secs;
    let output_amplification = output_bytes as f64 / input_bytes as f64;

    assert!(
        elapsed_secs <= max_latency_secs,
        "{mode} latency budget exceeded: {:.3}s > {:.3}s",
        elapsed_secs,
        max_latency_secs
    );

    assert!(
        lines_per_second >= min_lines_per_second,
        "{mode} throughput budget missed: {:.1} lines/s < {:.1} lines/s",
        lines_per_second,
        min_lines_per_second
    );

    assert!(
        output_amplification <= max_output_amplification,
        "{mode} output amplification budget exceeded: {:.3} > {:.3}",
        output_amplification,
        max_output_amplification
    );
}

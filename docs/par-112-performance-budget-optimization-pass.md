# PAR-112 Performance budget and optimization pass

Task: `forge-h6e`

## Scope delivered
- Added workspace CI regression guard tests for logs highlighter performance:
  - `logs_replay_meets_performance_budget`
  - `logs_follow_meets_throughput_budget`
- Benchmarks run via normal `cargo test --workspace` target (no special runner required).

## Budget policy
- Replay benchmark payload: `>= 100MB` synthetic mixed transcript.
- Follow benchmark payload: `>= 20MB` synthetic mixed transcript.
- Throughput floor (both modes): `>= 10,000 lines/sec`.
- Replay latency cap: `<= 120s`.
- Follow latency cap: `<= 60s`.
- Output amplification cap: `<= 4.0x` (`rendered_bytes / input_bytes`).

## Files
- `crates/forge-cli/tests/log_highlighting_performance_test.rs`

## Validation
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

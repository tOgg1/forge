# forge-y1k: forge-cli loop/run unwrap-used test slice (2026-02-13)

## Scope
Remove `unwrap/unwrap_err` usage in parse-node tests within:

- `crates/forge-cli/src/loop_internal.rs`
- `crates/forge-cli/src/run.rs`

## Changes
Replaced `unwrap`/`unwrap_err` in test assertions with explicit `match` branches and `panic!` messages. Runtime behavior unchanged.

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli parse_accepts_node_flag
cargo test -p forge-cli parse_rejects_node_flag_without_value
```

Results:

- clippy slice passed
- parse-node test filters passed for both `loop_internal` and `run`

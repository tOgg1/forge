# forge-stx: forge-cli clippy needless_borrow slice (2026-02-13)

## Scope
Fix low-risk `clippy::needless_borrow` findings in command-dispatch call sites for:

- `crates/forge-cli/src/task.rs`
- `crates/forge-cli/src/team.rs`

## Changes
Removed redundant `&` borrows when forwarding already-referenced match bindings into executor functions.

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --test team_task_command_test
```

Results:

- clippy slice passed (`CLIPPY_EXIT:0`)
- integration test passed (`2 passed, 0 failed`)

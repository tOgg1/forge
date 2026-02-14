# forge-stx: task/team needless_borrow slice validation (2026-02-13)

## Summary
- Task scope (`clippy::needless_borrow` in `task.rs` / `team.rs` dispatch callsites) is clean in current tree.
- No code changes required.

## Validation
- Ran:
  - `cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap-used -A clippy::expect-used`
- Result:
  - `Finished 'dev' profile ...`
  - No warnings/errors from `crates/forge-cli/src/task.rs` or `crates/forge-cli/src/team.rs`.

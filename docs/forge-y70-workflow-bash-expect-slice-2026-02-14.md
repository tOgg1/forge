# forge-y70 - workflow_bash_executor expect/unwrap slice

Date: 2026-02-14
Task: `forge-y70`
Scope:
- `crates/forge-cli/src/workflow_bash_executor.rs`

## Change

- Removed test-module blanket lint allow:
  - `#[allow(clippy::expect_used, clippy::unwrap_used)]`
- Added explicit helpers in tests:
  - `ok_or_panic(result, context)`
  - `err_or_panic(result, context)`
- Replaced all `expect`/`expect_err` callsites in `workflow_bash_executor` tests with explicit handling.

## Validation

```bash
rg -n "\\b(expect|expect_err|unwrap|unwrap_err)\\(" crates/forge-cli/src/workflow_bash_executor.rs
cargo clippy -p forge-cli --lib -- -D clippy::expect_used -D clippy::unwrap_used
cargo test -p forge-cli --lib workflow::bash_executor::tests
```

Result:
- Pattern scan is clean for targeted callsites.
- Focused clippy run passed for `expect_used`/`unwrap_used`.
- Targeted `workflow::bash_executor::tests` passed (4/4).

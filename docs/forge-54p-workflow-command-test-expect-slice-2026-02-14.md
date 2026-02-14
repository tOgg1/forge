# forge-54p - forge-cli workflow_command_test expect-used slice

Date: 2026-02-14
Task: `forge-54p`
Scope: `crates/forge-cli/tests/workflow_command_test.rs`

## Change

- Removed test-file blanket lint allow:
  - `#![allow(clippy::expect_used, clippy::unwrap_used)]`
- Added helper:
  - `ok_or_panic<T, E: Debug>(result, context)`
- Replaced both parser fixture `expect(...)` callsites with explicit `ok_or_panic(...)`.

## Validation

```bash
rg -n "expect\\(|expect_err\\(|unwrap\\(" crates/forge-cli/tests/workflow_command_test.rs
cargo clippy -p forge-cli --test workflow_command_test -- -D clippy::expect_used -D clippy::unwrap_used
cargo test -p forge-cli --test workflow_command_test
```

Result:
- No `expect(` / `expect_err(` / `unwrap(` callsites remain in the target file.
- Focused clippy run passed.
- `workflow_command_test` passed (4/4).

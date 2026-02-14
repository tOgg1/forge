# forge-shq - forge-tui raw_pty_attach unwrap/expect slice

Date: 2026-02-13
Task: `forge-shq`
Scope: `crates/forge-tui/src/raw_pty_attach.rs`

## Change

- Replaced test `unwrap`/`expect` patterns with explicit handling:
  - `plan.detach_command.unwrap()` now uses `match` on `as_deref()`
  - four `begin_raw_pty_session(...).expect("attach")` callsites now use explicit error-aware handling

## Validation

```bash
cargo test -p forge-tui --lib raw_pty_attach::tests
rg -n "expect\\(|unwrap\\(" crates/forge-tui/src/raw_pty_attach.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used -W clippy::unwrap_used 2>&1 | rg 'raw_pty_attach.rs' || true
```

Result:
- Raw PTY attach tests passed (`6 passed`).
- No `expect(` / `unwrap(` remains in this file.
- No `clippy::expect_used` or `clippy::unwrap_used` diagnostics emitted for this file.


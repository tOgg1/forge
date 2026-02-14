# forge-4v4 - forge-tui stacktrace_focus_test expect-used slice

Date: 2026-02-13
Task: `forge-4v4`
Scope: `crates/forge-tui/tests/stacktrace_focus_test.rs`

## Change

- Replaced `expect("failure focus should build")` with explicit `match` + panic context in `jump_to_probable_root_frame_prefers_application_frame`.

## Validation

```bash
cargo test -p forge-tui --test stacktrace_focus_test
rg -n "expect\\(" crates/forge-tui/tests/stacktrace_focus_test.rs
cargo clippy -p forge-tui --tests -- -A warnings -W clippy::expect_used 2>&1 | rg 'stacktrace_focus_test.rs' || true
```

Result:
- Stacktrace focus test target passed (`2 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this test file.


# forge-p5n - forge-tui app failure-explain unwrap-used slice

Date: 2026-02-14
Task: `forge-p5n`
Scope: `crates/forge-tui/src/app.rs`

## Change

- Replaced four `unwrap()` callsites with explicit `match` handling in:
  - `failure_explain_strip_prioritizes_root_cause_then_frame_then_command`
  - stripped text retrieval
  - root-cause index lookup
  - frame index lookup
  - command index lookup

## Validation

```bash
cargo test -p forge-tui --lib app::tests::failure_explain_strip_prioritizes_root_cause_then_frame_then_command
rg -n "failure_explain_strip_text\\(\\)\\.unwrap\\(|find\\(\\\"root cause=\\\"\\)\\.unwrap\\(|find\\(\\\"frame=\\\"\\)\\.unwrap\\(|find\\(\\\"command=\\\"\\)\\.unwrap\\(" crates/forge-tui/src/app.rs || true
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'app.rs' || true
```

Result:
- Targeted app test passed.
- No legacy unwrap patterns remain for this test.
- No `clippy::unwrap_used` diagnostics emitted for `app.rs`.


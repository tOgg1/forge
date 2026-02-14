# forge-xm6 - forge-tui session_recording expect-used slice

Date: 2026-02-14
Task: `forge-xm6`
Scope: `crates/forge-tui/src/session_recording.rs`

## Change

- Replaced all test `expect`/`expect_err` callsites with explicit handling in:
  - `append_rejects_non_monotonic_timestamps`
  - `compact_recording_removes_duplicate_consecutive_frames`
  - `replay_snapshot_selects_latest_frame_before_time`
  - `replay_render_includes_header_frame_and_actions`

## Validation

```bash
cargo test -p forge-tui --lib session_recording::tests
rg -n "expect\\(|expect_err\\(" crates/forge-tui/src/session_recording.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'session_recording.rs' || true
```

Result:
- Session recording tests passed (`4 passed`).
- No `expect(` / `expect_err(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.


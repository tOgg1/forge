# Review: forge-4kf (fmail TUI parity)

## Findings (highest first)

1. **HIGH**: Operator compose submit parity broken for `Enter` / `Ctrl+Enter`.
- Rust consumes `Key::Enter` but performs no submit action in single-line mode: `rust/crates/fmail-tui/src/operator.rs:464`.
- Go baseline submits on `enter` and `ctrl+enter`: `internal/fmailtui/operator_view.go:304`, `internal/fmailtui/operator_view.go:306`.
- Risk: operator cannot send with documented shortcuts; user-visible behavior mismatch.
- Fix hint: return explicit submit command/event from input handler (or do not consume submit keys) and add regression tests for both `Enter` and `Ctrl+Enter`.

## Small fixes landed in this review pass

1. Graph parity: `Shift+Tab` now cycles selected node backwards (plus regression test).
- Code: `rust/crates/fmail-tui/src/graph.rs:776`
- Test: `rust/crates/fmail-tui/src/graph.rs:1273`

2. Operator cap parity: enforce `OPERATOR_MESSAGE_LIMIT` (250) in `set_messages` (plus regression test).
- Code: `rust/crates/fmail-tui/src/operator.rs:188`
- Test: `rust/crates/fmail-tui/src/operator.rs:1062`

## Validation

- `env -u GOROOT -u GOTOOLDIR go test ./...` ✅
- `cd rust && cargo test -p fmail-tui` ✅ (after fixes; 261 passed, 1 ignored)

## Summary

- Status: **issues** (1 high open).
- Parity improved by two low-risk fixes + tests; submit-path mismatch still open.

## Communication log

- `fmail send task`:
  - `review: forge-4kf high operator compose Enter/Ctrl+Enter consumed but no submit path; parity break vs Go submitCompose rust/crates/fmail-tui/src/operator.rs:464 fix hint: return submit command/event (or do not consume Enter) + add Enter/Ctrl+Enter regression tests`
  - `review: forge-4kf info small fixes landed in review pass: Shift+Tab reverse node cycle parity + Operator message cap (250) with regression tests rust/crates/fmail-tui/src/graph.rs:776,rust/crates/fmail-tui/src/operator.rs:188 fix hint: keep`
- `fmail send @forge-orchestrator`:
  - `review summary: forge-4kf issues (1 high open, 2 low-risk fixes landed with tests)`

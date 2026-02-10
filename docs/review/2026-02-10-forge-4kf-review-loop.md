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

## Iteration 2 (agents.rs parity pass)

### Findings (highest first)

1. **HIGH**: History open-thread target could mismatch selected row due inconsistent ordering.
- Root cause: render sorted history by `message_id` desc, but navigation/open-thread read unsorted cache.
- File: `rust/crates/fmail-tui/src/agents.rs:292`, `rust/crates/fmail-tui/src/agents.rs:876`.
- Fix landed: shared sorted-history path for render/navigation/open-thread.

2. **HIGH**: History pane could show/open wrong agent when `detail_agent` was stale.
- Root cause: history data lookup keyed by `detail_agent` instead of current selected roster agent.
- File: `rust/crates/fmail-tui/src/agents.rs:501`, `rust/crates/fmail-tui/src/agents.rs:876`.
- Fix landed: bind history lookups to `selected_agent` and reset `detail_agent` on Enter.

3. **MEDIUM**: Presence indicator parity drift for future `last_seen` timestamps.
- Root cause: Rust used absolute diff; Go uses signed `now-last_seen` thresholds.
- File: `rust/crates/fmail-tui/src/agents.rs:934`.
- Fix landed: signed threshold logic to match Go behavior.

### Regression tests added

- `rust/crates/fmail-tui/src/agents.rs:1112` (`presence_indicator_future_timestamp_matches_go_behavior`)
- `rust/crates/fmail-tui/src/agents.rs:1410` (`history_enter_uses_sorted_order`)
- `rust/crates/fmail-tui/src/agents.rs:1452` (`history_uses_selected_agent_not_stale_detail_agent`)
- `rust/crates/fmail-tui/src/agents.rs:1504` (`empty_cached_detail_still_requires_refresh`)

### Validation

- `cd rust && cargo check -p fmail-tui --lib` ✅
- `cd rust && cargo test -p fmail-tui --lib` ✅ (300 passed, 1 ignored)
- `GOTOOLCHAIN=go1.25.7 go test ./internal/fmailtui/...` ✅
- `GOTOOLCHAIN=go1.25.7 go test ./...` ⚠️ one unrelated failure: `internal/parity TestProtoWireGateCriticalRPCFixtures` fixture drift.

### Residual risk

- Rust unit coverage strong for agents view-model/render.
- Remaining gap: no end-to-end provider/event-loop integration assertion for agents view in Rust app shell.

### Communication log (iteration 2)

- `fmail send task`:
  - `review: forge-4kf high fixed history/open-thread mismatch from unsorted cache order rust/crates/fmail-tui/src/agents.rs:292 compute sorted history for render/nav/open paths`
  - `review: forge-4kf high fixed stale detail_agent causing wrong history data for selected row rust/crates/fmail-tui/src/agents.rs:501 bind history lookup to selected_agent (+ set on Enter)`
  - `review: forge-4kf medium fixed Go parity for future last_seen timestamps rust/crates/fmail-tui/src/agents.rs:934 use signed now-last_seen threshold logic`
  - `review: forge-4kf residual-risk no integration-level provider/event-loop test in Rust yet; covered by unit regressions + lib test suite`
- `fmail send @forge-orchestrator`:
  - `review summary: forge-4kf pass (3 issues found+patched, tests green with noted external go parity fixture drift)`

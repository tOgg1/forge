# Rust Post-Release Verification Checklist

Goal: fast confidence after Rust cutover; catch regressions early; keep rollback ready.

When:
- T+15m (immediate smoke)
- T+2h (deeper workflows)
- T+24h (stability follow-up)

Evidence:
- Capture command output in `build/post-release/<YYYY-MM-DD>/...`.
- Write a short run note in `docs/review/rust-post-release-verification-<YYYY-MM-DD>.md`.

## Immediate smoke (T+15m)

1. Versions/exec
   - `forge --version`
   - `fmail --version`
   - `fmail-tui --version`

2. Basic CLI sanity
   - `forge doctor` (expect no failures)
   - `forge status` (expect sane JSON/text shape)

3. Daemon/runner liveness (if deployed)
   - `forge daemon status` (or equivalent)
   - Start/stop one trivial loop; confirm it advances state and logs stream.

4. Mail sanity
   - `fmail who`
   - `fmail send topic:test "post-release smoke"`
   - Open `fmail-tui`; confirm message visible; navigation responsive.

## Workflow verification (T+2h)

1. Loop workflows
   - create loop via CLI + via TUI wizard
   - stop/resume/delete
   - multi-logs paging and PgUp/PgDn scrolling

2. fmail-tui workflows
   - dashboard/topics/thread navigation
   - operator view: compose, slash commands, approvals (if enabled)
   - search/timeline/stats/graph views render and handle input

3. Storage invariants
   - new messages written with expected permissions
   - DB migrations stable (no repeated-migrate failures)

## Stability follow-up (T+24h)

1. CI/monitoring
   - confirm nightly parity green
   - scan error logs for recurring failures

2. Rollback readiness
   - confirm rollback plan + artifacts still runnable
   - rehearse rollback commands in staging if any incident flags appear


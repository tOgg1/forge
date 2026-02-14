# TUI Failure Explain Strip (2026-02-13)

Task: `forge-mat`  
Scope: deterministic top-cause strip for selected failures.

## What shipped

- Added `App::failure_explain_strip_text()` in `crates/forge-tui/src/app.rs`.
- Strip derives from `failure_focus::build_failure_focus(...)` over selected logs/runs.
- Deterministic priority order for top causes:
  - `root-cause`
  - `root-frame`
  - `command`
  - fallback labels after top priority
- Strip renders in status row when no explicit status message is active.

Example format:

`Failure explain: root cause=<...> | frame=<...> | command=<...>`

## Validation

- `cargo test -p forge-tui --lib failure_explain_strip_ -- --nocapture`
- `cargo test -p forge-tui --lib failure_focus -- --nocapture`
- `cargo test -p forge-cli --lib workflow_ledger_entry_contains_run_id_step_summaries_and_durations -- --nocapture`
- `cargo build -p forge-tui`

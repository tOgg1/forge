# TUI-913 runs pane timeline + exit badges + duration chips

Task: `forge-wze`

## Shipped
- Replaced Runs-tab placeholder with a real command-center pane in `crates/forge-tui/src/app.rs` and `crates/forge-tui/src/runs_tab.rs`.
- Added timeline-style run rows with:
  - status badges (`[RUN ]`, `[OK  ]`, `[ERR ]`, `[STOP]`)
  - exit-state badges (`[exit:N]` / `[live]`)
  - duration chips (`[1m5s]`, `[running]`, ...)
- Added selected-run context panel + output lane windowing in Runs pane (scroll-aware).
- Added quick jump from Runs to Logs (`Enter`) with run-selection source context preserved.
- Wired runtime run-output tails into app state for selected-run rendering in Runs pane.
- Added duration derivation from run timestamps in `crates/forge-tui/src/interactive_runtime.rs`.

## Regression Coverage
- `crates/forge-tui/src/app.rs`:
  - `runs_tab_renders_timeline_not_placeholder`
  - `enter_in_runs_jumps_to_logs_with_run_source`
- `crates/forge-tui/src/interactive_runtime.rs`:
  - `run_duration_running_is_running`
  - `run_duration_finished_is_compact`
  - `run_duration_invalid_started_at_falls_back`
- Updated runs-pane render assertions in `crates/forge-tui/src/runs_tab.rs`.
- Refreshed deterministic layout goldens in `crates/forge-tui/tests/golden/layout/runs_*.txt`.

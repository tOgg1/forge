# Workflow fan-out status display (sv-bsc)

Date: 2026-02-13
Task: sv-bsc (M3.4c)

## Delivered

- Added per-step fan-out counters to workflow logs payload/output:
  - `fan_out_running`
  - `fan_out_queued`
- Added fan-out display to human logs output per step:
  - `fan_out: running=<n> queued=<n>`
- Added fan-out display to `workflow show` step details:
  - static fan-out summary using dependency graph (`running=0`, `queued=<direct_dependents>`).
- Kept logs resilient when workflow source cannot be loaded:
  - fallback to backend workflow-by-name lookup
  - if unavailable, fan-out counts default to `0`.

## Counting semantics

- Fan-out is based on **direct dependents** (`depends_on` edges) per step.
- Runtime logs mode:
  - `running`: number of direct dependents currently in `running`
  - `queued`: number of direct dependents currently in `pending`
- Show mode:
  - static graph view (`running=0`, `queued=direct dependent count`).

## Tests

- `cargo test -p forge-cli --lib workflow::tests::show_workflow_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::logs_ -- --nocapture`
- `cargo test -p forge-cli --lib bash_step_logs_persist_and_are_viewable -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::run_ -- --nocapture`
- `cargo check -p forge-cli`

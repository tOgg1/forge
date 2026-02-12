# TUI-505 quant and qual stop-condition monitor

Task: `forge-f1z`
Status: delivered

## What shipped

- Added stop-condition monitor module: `crates/forge-tui/src/swarm_stop_monitor.rs`.
- Added quant threshold model with direction support:
  - `AtMost`
  - `AtLeast`
- Added qual signal model for expected-vs-observed checks.
- Added per-loop/swarm stop-signal report generation with surfaced fields:
  - threshold snapshots
  - time-to-trigger (seconds)
  - mismatch reasons
- Added deterministic health states:
  - `Healthy`
  - `Warning`
  - `Mismatch`
  - `Triggered`
- Added deterministic sorting for stable operator rendering (`swarm_id`, then `loop_id`).
- Added regression tests for:
  - threshold breach -> triggered
  - near-threshold/time-to-trigger warning
  - qual mismatch reason surfacing
  - row ordering determinism
- Exported module from crate root: `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

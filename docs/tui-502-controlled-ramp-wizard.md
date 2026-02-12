# TUI-502 controlled ramp-up wizard with health gates

Task: `forge-rky`
Status: delivered

## What shipped

- Added controlled ramp-up wizard model in `crates/forge-tui/src/swarm_templates.rs`.
- New staged progression per template:
  - `proof`
  - `ramp`
  - `full`
- Added preflight + health gate evaluation to control stage advancement.
- Expansion now blocks when signals are unhealthy, including:
  - incomplete preflight checks
  - missing health signals
  - claim conflicts
  - stale in-progress tasks
  - insufficient proof pass count / healthy loop count
- Added progression decision API:
  - `Blocked`
  - `Advance`
  - `Complete`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

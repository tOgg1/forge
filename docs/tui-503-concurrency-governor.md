# TUI-503 concurrency governor and pool starvation prevention

Task: `forge-k1s`
Status: delivered

## What shipped

- New governor module: `crates/forge-tui/src/swarm_governor.rs`.
- Added harness/pool usage model + policy config.
- Added starvation detector and throttle recommendation engine.
- Added safety rules to prevent unsafe throttling:
  - reserve floor per pool
  - dedupe recommendations per pool/profile
- Added exhaustion diagnostics when starvation has no safe donor.
- Exported module from crate root: `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

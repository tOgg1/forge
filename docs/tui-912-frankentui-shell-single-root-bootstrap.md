# TUI-912: FrankenTUI Shell Single-Root Bootstrap

## Summary

Implemented `forge-qbx` by enforcing a single runtime root for `forge-tui`:

- Interactive TTY path now always mounts the FrankenTUI runtime (`run_frankentui_bootstrap` in `crates/forge-tui/src/bin/forge-tui.rs`).
- Removed interactive snapshot fallback renderer path.
- Non-interactive snapshot text mode is now CI-only (`CI` truthy).
- Non-interactive local usage now fails fast with a clear terminal preflight error.

## Why

This removes split interactive render roots and prevents silent fallback to a degraded renderer in operator sessions.

## Code

- `crates/forge-tui/src/bin/forge-tui.rs`
  - Removed fallback renderer loop and incremental snapshot fallback machinery.
  - Added `ci_non_tty_snapshot_mode_enabled()` gate.
  - Added `env_truthy()` helper for CI detection.
  - Interactive runtime failure now exits with explicit error.

## Regression Coverage

Added tests in `crates/forge-tui/src/bin/forge-tui.rs`:

- `ci_non_tty_snapshot_mode_is_disabled_without_ci`
- `ci_non_tty_snapshot_mode_accepts_truthy_ci_values`
- `ci_non_tty_snapshot_mode_rejects_falsey_ci_values`

## Validation

- `cargo fmt --check`
- `cargo clippy -p forge-tui --all-targets -- -D warnings`
- `cargo test -p forge-tui`

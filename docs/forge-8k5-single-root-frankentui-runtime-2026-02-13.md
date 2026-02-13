# forge-8k5: Single-root FrankenTUI runtime enforcement

## Scope delivered
- Removed interactive fallback to snapshot renderer from `forge-tui` binary runtime path.
- Interactive mode now always attempts FrankenTUI bootstrap and exits with a hard error on failure.
- Non-interactive snapshot mode is now CI-gated via `CI` truthy env only.
- Removed obsolete fallback env parser (`FORGE_TUI_DEV_SNAPSHOT_FALLBACK`) and related incremental repaint fallback code paths.

## Behavior
- Interactive TTY:
  - `FORGE_TUI_RUNTIME=legacy|old` still hard-fails with explicit migration message.
  - FrankenTUI bootstrap failure exits with code 1 and explicit error.
- Non-interactive:
  - `CI` truthy => snapshot text output allowed.
  - otherwise => hard-fail with preflight message.

## Regression coverage
- `ci_non_tty_snapshot_mode_is_disabled_without_ci`
- `ci_non_tty_snapshot_mode_accepts_truthy_ci_values`
- `ci_non_tty_snapshot_mode_rejects_falsey_ci_values`
- Existing runtime-legacy and snapshot data tests retained.

## Validation
- `cargo test -p forge-tui --bin forge-tui -- --nocapture`
- `cargo check -p forge-tui`

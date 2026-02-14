# TUI legacy renderer removal (forge-brp)

Date: 2026-02-13
Task: `forge-brp`

## Change

- Removed legacy interactive renderer source file:
  - `crates/forge-tui/src/interactive_runtime.rs`
- Interactive TTY startup now rejects `FORGE_TUI_RUNTIME=legacy` and exits with a clear error.
- Interactive runtime path is now single-root on FrankenTUI bootstrap.

## Why

- `forge-dhs` flipped upstream runtime default.
- `forge-sng` demoted legacy path.
- `forge-brp` removes obsolete legacy rendering code after gate bake.

## Validation attempted

```bash
cargo check -p forge-tui
cargo test -p forge-tui runtime_legacy_requested_ -- --nocapture
scripts/rust-frankentui-bootstrap-smoke.sh
```

Current workspace blocker (unrelated churn in `forge-cli`):

- `error[E0425]: cannot find function append_workflow_ledger_entry in module run_persistence`
- Location: `crates/forge-cli/src/workflow.rs`

## Follow-up

- Re-run the three validation commands after `forge-cli` compile issue is resolved.

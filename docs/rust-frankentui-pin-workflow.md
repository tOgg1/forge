# FrankenTUI Pin Maintenance Workflow

Task: `forge-tq3`

## Script

- `scripts/rust-frankentui-pin-maintenance.sh`

## Safe pin bump flow

```bash
scripts/rust-frankentui-pin-maintenance.sh --rev <new-frankentui-commit>
```

What it runs:

1. Updates FrankenTUI `rev` in `rust/crates/forge-ftui-adapter/Cargo.toml`.
2. Refreshes lockfile entries: `cd rust && cargo update -p ftui`.
3. Validates workspace compile: `cd rust && cargo check --workspace`.
4. Verifies pinned source/lock invariants: `scripts/rust-frankentui-pin-check.sh`.
5. Runs parity smoke checks:
   - `scripts/rust-loop-tui-smoke.sh`
   - `scripts/rust-fmail-tui-smoke.sh`

## Validation-only rerun

```bash
scripts/rust-frankentui-pin-maintenance.sh --check-only
```

## Fast path (skip smoke checks)

Only for local iteration; do not use for final merge validation.

```bash
scripts/rust-frankentui-pin-maintenance.sh --check-only --skip-smoke
```

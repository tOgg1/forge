# FrankenTUI Pin And Lock Policy

Task: `forge-mjm`

## Pin

- Adapter crate: `rust/crates/forge-ftui-adapter/Cargo.toml`
- Dependency: `ftui`
- Source: `https://github.com/Dicklesworthstone/frankentui`
- Pinned commit (`rev`): `23429fac0e739635c7b8e0b995bde09401ff6ea0`
- Feature gate: `frankentui-upstream` (adapter keeps upstream optional while bootstrap is in progress)

## Lock policy

- `rust/Cargo.lock` must be committed for Rust workspace changes.
- Any change to FrankenTUI `rev` must be accompanied by a lockfile refresh when
  the `frankentui-upstream` feature is enabled in the workspace.
- CI check: `scripts/rust-frankentui-pin-check.sh`.

## Update procedure

Canonical workflow: `scripts/rust-frankentui-pin-maintenance.sh`

1. Pin update + validation:
   - `scripts/rust-frankentui-pin-maintenance.sh --rev <new-sha>`
2. Validation-only rerun:
   - `scripts/rust-frankentui-pin-maintenance.sh --check-only`
3. Commit `Cargo.toml` + `Cargo.lock` + policy/workflow doc updates together.

Detailed runbook: `docs/rust-frankentui-pin-workflow.md`.

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

1. Change `rev` in `rust/crates/forge-ftui-adapter/Cargo.toml`.
2. Run `cd rust && cargo update -p ftui`.
3. Run `cd rust && cargo check --workspace`.
4. Run `scripts/rust-frankentui-pin-check.sh`.
5. Commit `Cargo.toml` + `Cargo.lock` + policy doc updates together.

# Rust Workspace Lint/Format Policy

Task: `forge-tem`  
Updated: 2026-02-09

## Shared policy

- One workspace-wide format profile via `rust/rustfmt.toml`.
- One workspace-wide clippy profile via `rust/clippy.toml`.
- Deny warnings in CI/local quality checks (`-D warnings`).
- Quality gate command is `scripts/rust-quality-check.sh`.

## Required quality gate

Run from repo root:

```bash
scripts/rust-quality-check.sh
```

The script enforces:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

## Conventions

- Crates in `rust/crates/*` must use shared workspace package keys (`edition.workspace`, `license.workspace`, `publish.workspace`) to keep policy centralized.
- No crate-specific rustfmt/clippy config files.

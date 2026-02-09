# Rust Crate Boundary Policy

Task: `forge-zaa`  
Frozen: 2026-02-09

## Goal

Prevent dependency-direction drift across Rust crates during the rewrite.

## Layer model

Lower layer number = more foundational.
Workspace crates may only depend on crates in the same or lower layer.
Upward edges (higher layer dependency) are forbidden.

Canonical layer map: `docs/rust-crate-boundaries.json`.

## Planned crate layers

- Layer 0: `forge-core`, `forge-ftui-adapter`
- Layer 1: `forge-db`, `fmail-core`
- Layer 2: `forge-loop`
- Layer 3: `forge-daemon`, `forge-runner`
- Layer 4: `forge-cli`, `fmail-cli`
- Layer 5: `forge-tui`, `fmail-tui`
- Layer 9: `forge-parity-stub` (temporary bootstrap/test crate)

## Enforcement

- Checker script: `scripts/rust-boundary-check.sh`.
- CI gate: `.github/workflows/ci.yml` job `rust-quality`.
- Any new workspace crate must be added to `docs/rust-crate-boundaries.json` in the same change.

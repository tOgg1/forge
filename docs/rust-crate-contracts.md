# Rust Crate Contracts

Task: `forge-6ye`  
Scope: Rust workspace bootstrap + architecture boundaries

## Workspace crate map

- `forge-core`: shared domain types, validation, queue/event models.
- `forge-db`: database/migration access and persistence adapters.
- `forge-loop`: loop runtime semantics and queue execution logic.
- `forge-daemon`: daemon process and RPC service surface.
- `forge-runner`: agent-runner process behavior.
- `forge-cli`: `forge` command-line surface.
- `forge-tui`: loop TUI surface.
- `fmail-core`: mail domain/storage logic.
- `fmail-cli`: `fmail` CLI surface.
- `fmail-tui`: mail TUI surface.
- `forge-ftui-adapter`: integration boundary around FrankenTUI.
- `forge-parity-stub`: temporary parity/bootstrap utility crate.

## Dependency direction

- Policy source-of-truth: `docs/rust-crate-boundaries.json`.
- Human policy: `docs/rust-crate-boundary-policy.md`.
- Enforcer: `scripts/rust-boundary-check.sh` (CI `rust-boundary` gate).
- Rule: no upward dependency edges by layer.

## Contribution checklist

1. Add/update crate in `rust/Cargo.toml` workspace members.
2. Add/update crate layer in `docs/rust-crate-boundaries.json`.
3. Keep public API minimal and crate-local tests green.
4. Run:
   - `scripts/rust-boundary-check.sh`
   - `cd rust && cargo fmt --check`
   - `cd rust && cargo clippy --workspace --all-targets -- -D warnings`
   - `cd rust && cargo test --workspace`

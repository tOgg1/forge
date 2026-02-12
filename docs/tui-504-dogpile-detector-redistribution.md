# TUI-504 dogpile detector and redistribution assistant

Task: `forge-0q3`
Status: delivered

## What shipped

- Added dogpile detector module: `crates/forge-tui/src/swarm_dogpile.rs`.
- Added dogpile input models for claim samples and loop load samples.
- Added duplicate-claim detection with keeper selection (earliest claimant wins).
- Added redistribution planner with actionable command hints:
  - handoff hint when idle non-claimant loop exists
  - release-and-pick-next hint when no safe target loop exists
- Added deterministic sorting and guardrails:
  - ignores blank task ids
  - configurable minimum duplicate threshold
  - fallback normalization for unknown loop/agent identities
- Added regression tests for duplicate detection, redistribution targeting, fallback behavior, and threshold handling.
- Exported module from crate root: `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

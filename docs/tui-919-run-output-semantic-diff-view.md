# TUI-919 run output semantic diff view

Task: `forge-rsr`  
Status: delivered

## Scope

- Side-by-side run output diff that suppresses noisy tokens.
- Timestamp/duration/address/ID normalization for semantic matching.
- Deterministic diff summary for UI counters.

## Implementation

- New module: `crates/forge-tui/src/run_output_diff.rs`
- Exported from: `crates/forge-tui/src/lib.rs`

Core API:

- `build_run_output_semantic_diff(left, right, options)`
- `normalize_semantic_line(line, options)`

Core model:

- `RunOutputDiffOptions`
- `SemanticDiffRow`
- `RunOutputDiffReport`

Behavior:

- Reuses `DiffHint` semantics (`=`, `!`, `<`, `>` via `log_compare::diff_hint`).
- Optional suppression for:
  - timestamps
  - durations
  - hex addresses
  - UUID/long hex IDs
  - ANSI escape sequences
- Produces changed ratio (`different / (equal + different)`).

## Regression tests

Added in `crates/forge-tui/src/run_output_diff.rs`:

- normalization noise suppression parity
- equal diff after timestamp/duration/address suppression
- left/right-only row detection
- timestamp suppression toggle behavior

## Validation

- `cargo test -p forge-tui run_output_diff -- --nocapture`
- `cargo build -p forge-tui`

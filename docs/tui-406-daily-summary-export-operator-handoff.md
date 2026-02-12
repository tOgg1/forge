# TUI-406 daily summary export for operator handoff

Task: `forge-ff8`  
Status: delivered

## Scope

- Generate concise daily summary artifact for operator handoff.
- Include sections for:
  - completed work
  - blockers
  - incidents
  - next actions

## Implementation

- New module: `crates/forge-tui/src/daily_summary.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Artifact model

- `build_daily_summary_artifact(...)` returns a structured artifact with:
  - headline
  - section metadata (counts + overflow)
  - markdown output
  - plain-text output
- Section handling:
  - dedupe by id
  - deterministic ordering
  - per-section truncation limits
  - explicit overflow annotation (`+N more`)
  - empty-state placeholder (`- none`)

## Incident handling

- Incidents sorted by severity rank before render (SEV0/SEV1 first).
- Output includes severity, status, summary, owner.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

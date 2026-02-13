# TUI-916 postmortem auto-draft export

Task: `forge-js5`  
Status: delivered

## Scope

- Generate incident postmortem draft from:
  - timeline events
  - replay hotspots
  - key artifacts
  - follow-up actions
- Export bundle for handoff/review.

## Implementation

- New module: `crates/forge-tui/src/postmortem_draft.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Draft builder

- `build_postmortem_draft(...)` emits:
  - markdown body
  - plain-text summary
  - metadata JSON
- Deterministic normalization:
  - timeline sort by `timestamp_ms,event_id`
  - duplicate timeline events removed
  - duplicate artifact refs removed
  - duplicate follow-up actions removed
  - section caps with `... +N more` overflow marker
  - explicit `- none` placeholder when empty

### Export writer

- `export_postmortem_draft(...)` writes:
  - `<basename>.md`
  - `<basename>.txt`
  - `<basename>.json`

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui postmortem_draft::`
- `cargo build -p forge-tui`

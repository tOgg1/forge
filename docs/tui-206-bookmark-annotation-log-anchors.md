# TUI-206 bookmark and annotation system for log anchors

Task: `forge-8v2`  
Status: delivered

## Scope

- Local bookmarks for log anchor regions.
- Lightweight annotations on saved anchors.
- Export/import anchors for operator handoff.

## Implementation

- New module: `crates/forge-tui/src/log_anchors.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Anchor model

- Data types:
  - `LogAnchor`
  - `LogAnchorDraft`
  - `LogAnchorFilter`
  - `LogAnchorStore`
- Core operations:
  - `add_log_anchor(...)`
  - `annotate_log_anchor(...)`
  - `remove_log_anchor(...)`
  - `list_log_anchors(...)`

## Handoff portability

- JSON bundle export:
  - `export_anchor_bundle_json(...)`
  - schema versioned via `LOG_ANCHOR_SCHEMA_VERSION`
- JSON bundle import:
  - `import_anchor_bundle_json(...)`
  - returns `ImportAnchorsOutcome` with import counts + warnings
  - skips duplicates safely
- Human handoff export:
  - `export_anchor_handoff_markdown(...)`

## UI helper

- `render_anchor_rows(...)` for compact TUI row rendering of anchors.

## Validation

- Unit tests in `crates/forge-tui/src/log_anchors.rs` cover:
  - add/annotate/remove flow
  - duplicate id suffixing
  - filter semantics
  - export/import round-trip
  - duplicate import skip
  - invalid import handling
  - handoff markdown content
  - compact row rendering

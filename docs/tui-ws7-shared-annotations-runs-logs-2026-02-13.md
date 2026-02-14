# TUI-WS7 shared annotations on runs/logs

## Scope
- Add shared annotation primitives for run and log-line targets.

## Changes
- Added `crates/forge-tui/src/shared_annotations.rs`:
  - target model (`AnnotationTarget::Run`, `AnnotationTarget::LogLine`)
  - annotation model with author/body/tags/timestamps
  - store operations: add, update, remove, list-by-target, text search
  - deterministic sorting and tag normalization
- Exported module via `crates/forge-tui/src/lib.rs`.

## Validation
- `cargo test -p forge-tui shared_annotations::tests:: -- --nocapture`
- `cargo build -p forge-tui`

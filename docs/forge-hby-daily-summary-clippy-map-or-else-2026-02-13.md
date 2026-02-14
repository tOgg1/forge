# forge-hby: daily_summary clippy slice (2026-02-13)

## Scope
Task was opened as expect-used slice; current blocker was `clippy::unnecessary_option_map_or_else` in tests.

## Changes
In `crates/forge-tui/src/daily_summary.rs` tests:

- replaced two `Option::map_or_else(..., |v| v)` usages with `unwrap_or_else(...)`:
  - incidents section lookup
  - completed section lookup

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib incidents_are_ranked_by_severity
cargo test -p forge-tui --lib duplicate_ids_are_deduped_and_overflow_is_annotated
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- both focused daily-summary tests passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)

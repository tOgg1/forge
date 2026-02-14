# forge-4nm: forge-tui app.rs question_mark slice (2026-02-13)

## Scope
Apply clippy `question_mark` simplification in markdown list parsing helper.

## Changes
In `crates/forge-tui/src/app.rs`:

- replaced `let Some(rest) = ... else { return None; };` with `let rest = ...?;`

Behavior unchanged; function remains `Option`-returning.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib parse_markdown_list_item
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::incompatible_msrv -A clippy::derivable_impls -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- filtered test run completed (no matching lib test name in filter)
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)

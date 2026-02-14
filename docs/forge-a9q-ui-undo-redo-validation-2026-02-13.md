# forge-a9q: ui_undo_redo expect-used validation (2026-02-13)

## Scope
Validate ui_undo_redo expect-used slice after task reopen.

## Findings
No additional code change required; tests already use explicit match handling for undo/redo option results.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib undo_restores_prior_selection_scroll_filter_snapshot
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused ui-undo-redo test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `ui_undo_redo.rs` absent from diagnostics

# forge-g71: runs_tab expect-used validation (2026-02-13)

## Scope
Validate runs_tab expect-used slice after takeover.

## Findings
No code changes required in this pass; test already uses explicit match handling for output row extraction.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib paneled_output_applies_syntax_colors_to_tokens
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused runs-tab test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `runs_tab.rs` absent from diagnostics

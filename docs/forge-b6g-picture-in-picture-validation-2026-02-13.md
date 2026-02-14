# forge-b6g: picture_in_picture expect-used validation (2026-02-13)

## Scope
Validate picture_in_picture expect-used slice after takeover.

## Findings
No code changes required; `crates/forge-tui/src/picture_in_picture/tests.rs` already uses explicit match handling for focus cycling and explicit panic context for lookup.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib collapsed_window_renders_compact_lines_and_focus_cycle
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused picture-in-picture test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `picture_in_picture/tests.rs` absent from diagnostics

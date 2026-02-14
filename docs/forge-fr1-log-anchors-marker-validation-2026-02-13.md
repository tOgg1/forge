# forge-fr1: log_anchors_marker_test expect-used validation (2026-02-13)

## Scope
Validate `log_anchors_marker_test` expect-used slice after takeover.

## Findings
No code changes required; test already uses explicit result handling (match / if let) for anchor inserts.

## Validation
Commands run:

```bash
cargo test -p forge-tui --test log_anchors_marker_test
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::cloned_ref_to_slice_refs
```

Results:

- integration test passed (`2 passed`)
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `log_anchors_marker_test.rs` absent from diagnostics

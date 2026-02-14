# forge-p49: watchpoints restore expect-used validation (2026-02-13)

## Scope
Validate watchpoints restore test expect-used slice after takeover.

## Findings
No code changes required in this pass; `persist_and_restore_round_trip` already uses explicit match handling on `restore_watchpoints`.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib persist_and_restore_round_trip
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused watchpoints test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `watchpoints.rs` absent from diagnostics

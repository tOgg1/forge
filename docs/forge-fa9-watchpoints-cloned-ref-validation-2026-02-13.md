# forge-fa9: watchpoints cloned_ref_to_slice_refs validation (2026-02-13)

## Scope
Validate `watchpoints` cloned-ref slice after opening the task.

## Findings
No code changes required; `crates/forge-tui/src/watchpoints.rs` already uses `std::slice::from_ref(&definition)` in the targeted test.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib cooldown_prevents_retrigger_until_window_passes
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused watchpoints test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)

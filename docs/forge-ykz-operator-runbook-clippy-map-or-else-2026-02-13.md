# forge-ykz: operator_runbook_engine clippy slice (2026-02-13)

## Scope
Address remaining clippy diagnostics in `crates/forge-tui/src/operator_runbook_engine.rs` test code.

## Changes
Replaced two `Option::map_or_else(..., |v| v)` callsites with `unwrap_or_else(...)`:

- incident-response runbook lookup
- shift-handoff runbook lookup

This resolves `clippy::unnecessary_option_map_or_else` while keeping explicit failure context.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib runbook_progression_moves_to_next_pending_step
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused operator-runbook test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)

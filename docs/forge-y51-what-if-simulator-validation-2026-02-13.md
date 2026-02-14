# forge-y51: what_if_simulator unwrap-used validation (2026-02-13)

## Scope
Validate what_if_simulator unwrap-used slice after takeover.

## Findings
No code changes required; tests already use explicit match handling for projection row lookup and ETA options.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib scaling_up_improves_eta
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused what-if test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `what_if_simulator.rs` absent from diagnostics

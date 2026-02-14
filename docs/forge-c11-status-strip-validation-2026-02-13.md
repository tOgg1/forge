# forge-c11: status_strip expect-used validation (2026-02-13)

## Scope
Validate status_strip expect-used slice after takeover.

## Findings
No code changes required; `crates/forge-tui/src/status_strip.rs` has no remaining `expect/unwrap` callsites.

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib render_status_strip_line_prefers_runtime_values_and_truncates
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused status-strip test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `status_strip.rs` absent from diagnostics

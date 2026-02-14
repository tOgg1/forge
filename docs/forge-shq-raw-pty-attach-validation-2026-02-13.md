# forge-shq: raw_pty_attach unwrap/expect validation (2026-02-13)

## Scope
Validate raw_pty_attach unwrap/expect-used slice after takeover.

## Findings
No code changes required; tests already use explicit handling (`unwrap_or_else` with panic context).

## Validation
Commands run:

```bash
cargo test -p forge-tui --lib session_ingest_requires_monotonic_sequence
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::needless_return -A clippy::cloned_ref_to_slice_refs
```

Results:

- focused raw-pty test passed
- clippy run with unrelated lint classes allowed passed (`CLIPPY_EXIT:0`)
- `raw_pty_attach.rs` absent from diagnostics

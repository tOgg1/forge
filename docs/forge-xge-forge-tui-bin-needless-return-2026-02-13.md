# forge-xge: forge-tui binary needless_return slice (2026-02-13)

## Scope
Fix clippy `needless_return` in `crates/forge-tui/src/bin/forge-tui.rs`.

## Changes
In `run_frankentui_bootstrap` (`#[cfg(feature = "frankentui-bootstrap")]` arm):

- removed explicit `return` and returned expression directly

Formatted touched file.

## Validation
Commands run:

```bash
cargo fmt --all -- crates/forge-tui/src/bin/forge-tui.rs
cargo clippy -p forge-tui --all-targets -- -D warnings \
  -A clippy::expect_used -A clippy::unwrap_used -A clippy::too_many_arguments \
  -A clippy::question_mark -A clippy::incompatible_msrv -A clippy::redundant_closure \
  -A clippy::field_reassign_with_default -A clippy::iter_nth_zero \
  -A clippy::cloned_ref_to_slice_refs
```

Result: clippy run passed (`CLIPPY_EXIT:0`).

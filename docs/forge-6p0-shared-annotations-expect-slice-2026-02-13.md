# forge-6p0 - forge-tui shared_annotations expect-used slice

Date: 2026-02-13
Task: `forge-6p0`
Scope: `crates/forge-tui/src/shared_annotations.rs`

## Change

- Replaced all test `expect(...)` callsites with explicit handling across:
  - `add_and_list_annotations_for_target`
  - `update_annotation_refreshes_body_tags_and_timestamp`
  - `search_text_matches_body_author_and_tags`
  - `remove_annotation_deletes_entry`

## Validation

```bash
cargo test -p forge-tui --lib shared_annotations::tests
rg -n "expect\\(" crates/forge-tui/src/shared_annotations.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'shared_annotations.rs' || true
```

Result:
- Shared annotations tests passed (`4 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.


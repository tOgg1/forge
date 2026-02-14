# forge-k9q - forge-tui postmortem_draft unwrap-used slice

Date: 2026-02-14
Task: `forge-k9q`
Scope: `crates/forge-tui/src/postmortem_draft.rs`

## Change

- Replaced three test `unwrap()` callsites with explicit handling in `export_writes_markdown_text_and_metadata_json`:
  - export result
  - markdown file read
  - metadata json file read

## Validation

```bash
cargo test -p forge-tui --lib postmortem_draft::tests::export_writes_markdown_text_and_metadata_json
rg -n "unwrap\\(" crates/forge-tui/src/postmortem_draft.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'postmortem_draft.rs' || true
```

Result:
- Targeted postmortem draft test passed.
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.


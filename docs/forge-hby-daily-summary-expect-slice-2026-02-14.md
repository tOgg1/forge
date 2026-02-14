# forge-hby - forge-tui daily_summary expect-used slice

Date: 2026-02-14
Task: `forge-hby`
Scope: `crates/forge-tui/src/daily_summary.rs`

## Change

- Replaced test `expect(...)` callsites with explicit `map_or_else` handling in:
  - `incidents_are_ranked_by_severity`
  - `duplicate_ids_are_deduped_and_overflow_is_annotated`

## Validation

```bash
cargo test -p forge-tui --lib daily_summary::tests
rg -n "expect\\(" crates/forge-tui/src/daily_summary.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'daily_summary.rs' || true
```

Result:
- Daily summary tests passed (`4 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.


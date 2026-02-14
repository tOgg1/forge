# forge-ykz - forge-tui operator_runbook_engine expect-used slice

Date: 2026-02-13
Task: `forge-ykz`
Scope: `crates/forge-tui/src/operator_runbook_engine.rs`

## Change

- Replaced all test `expect(...)` callsites with explicit handling in:
  - `parse_toml_manifest`
  - `parse_yaml_manifest`
  - `runbook_progression_moves_to_next_pending_step`
  - `render_runbook_lines_contains_progress_and_step_details`

## Validation

```bash
cargo test -p forge-tui --lib operator_runbook_engine::tests
rg -n "expect\\(" crates/forge-tui/src/operator_runbook_engine.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::expect_used 2>&1 | rg 'operator_runbook_engine.rs' || true
```

Result:
- Operator runbook tests passed (`5 passed`).
- No `expect(` remains in this file.
- No `clippy::expect_used` diagnostics emitted for this file.


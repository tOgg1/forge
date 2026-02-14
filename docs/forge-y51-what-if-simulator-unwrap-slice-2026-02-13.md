# forge-y51 - forge-tui what_if_simulator unwrap-used slice

Date: 2026-02-13
Task: `forge-y51`
Scope: `crates/forge-tui/src/what_if_simulator.rs`

## Change

- Replaced all test `unwrap()` callsites with explicit handling in:
  - `stop_loop_projects_blocked_eta`
  - `scaling_up_improves_eta`
  - `scaling_down_clamps_at_zero`

## Validation

```bash
cargo test -p forge-tui --lib what_if_simulator::tests
rg -n "unwrap\\(" crates/forge-tui/src/what_if_simulator.rs
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'what_if_simulator.rs' || true
```

Result:
- What-if simulator tests passed (`4 passed`).
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.


# forge-8ys - forge-tui motion_grammar unwrap-used slice

Date: 2026-02-14
Task: `forge-8ys`
Scope: `crates/forge-tui/src/motion_grammar.rs`

## Change

- Replaced all test `unwrap()` callsites with explicit `match` handling in:
  - `enter_transition_progress_ramps_up`
  - `focus_pulse_fades_out`

## Validation

```bash
rg -n "unwrap\\(" crates/forge-tui/src/motion_grammar.rs || true
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::unwrap_used 2>&1 | rg 'motion_grammar.rs' || true
cargo test -p forge-tui --lib -- --list | rg motion_grammar || true
```

Result:
- No `unwrap(` remains in this file.
- No `clippy::unwrap_used` diagnostics emitted for this file.
- No `motion_grammar`-named lib tests are currently listed in this tree.


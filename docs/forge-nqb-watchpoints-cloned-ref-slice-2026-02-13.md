# forge-nqb - forge-tui watchpoints cloned-ref-to-slice-refs slice

Date: 2026-02-13
Task: `forge-nqb`
Scope: `crates/forge-tui/src/watchpoints.rs`

## Change

- Replaced test callsites using `&[definition.clone()]` with `std::slice::from_ref(&definition)` in `cooldown_prevents_retrigger_until_window_passes`.

## Validation

```bash
cargo clippy -p forge-tui --lib -- -A warnings -W clippy::cloned_ref_to_slice_refs
cargo test -p forge-tui --lib watchpoints::tests::cooldown_prevents_retrigger_until_window_passes
```

Result: both commands passed.


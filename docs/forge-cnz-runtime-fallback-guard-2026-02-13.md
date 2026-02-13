# forge-cnz: runtime fallback reintroduction guard

## Scope delivered
- Added source-level regression test:
  - `crates/forge-tui/tests/runtime_source_guard_test.rs`
- Guard enforces single-root runtime invariants in `crates/forge-tui/src/bin/forge-tui.rs`:
  - interactive path invokes FrankenTUI bootstrap
  - CI-gated non-interactive snapshot path present
  - deprecated fallback hooks are absent

## Validation
- `cargo test -p forge-tui --test runtime_source_guard_test -- --nocapture`
- `cargo check -p forge-tui`

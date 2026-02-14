# forge-76p - forge-cli tui test expect-used slice

Date: 2026-02-14
Task: `forge-76p`
Scope: `crates/forge-cli/src/tui.rs`

## Change

- Removed test-module blanket lint allow:
  - `#[allow(clippy::expect_used, clippy::unwrap_used)]`
- Added explicit helpers in test module:
  - `parse_json_or_panic(raw, context)`
  - `str_or_panic(option, context)`
- Replaced all JSON parse and option `expect(...)` callsites in `tui` tests with explicit helper handling.

## Validation

```bash
rg -n "expect\\(|expect_err\\(|unwrap\\(" crates/forge-cli/src/tui.rs
cargo clippy -p forge-cli --lib -- -D clippy::expect_used -D clippy::unwrap_used
cargo test -p forge-cli tui::tests
```

Result:
- No `expect(` / `expect_err(` / `unwrap(` callsites remain in `crates/forge-cli/src/tui.rs`.
- Focused clippy run passed.
- `tui::tests` passed; run completed green.

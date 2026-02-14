# forge-01b - forge-cli skills test expect/unwrap slice

Date: 2026-02-14
Task: `forge-01b`
Scope: `crates/forge-cli/src/skills.rs`

## Change

- Removed test-module blanket lint allow:
  - `#[allow(clippy::unwrap_used, clippy::expect_used)]`
- Added explicit helpers:
  - `ok_or_panic(result, context)`
  - `parse_json_or_panic(raw, context)`
  - `array_or_panic(value, context)`
- Replaced all `expect/unwrap` callsites in `skills` tests with explicit helper handling.

## Validation

```bash
rg -n "expect\\(|expect_err\\(|unwrap\\(" crates/forge-cli/src/skills.rs
cargo clippy -p forge-cli --lib -- -D clippy::expect_used -D clippy::unwrap_used
cargo test -p forge-cli skills::tests
```

Result:
- No `expect(` / `expect_err(` / `unwrap(` callsites remain in `skills.rs`.
- Focused clippy run passed.
- `skills::tests` passed; full command completed green.

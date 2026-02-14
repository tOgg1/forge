# forge-ybf - forge-cli node test expect/unwrap slice

Date: 2026-02-14
Task: `forge-ybf`
Scope:
- `crates/forge-cli/src/node.rs`

## Change

- No further code edits required in this slice.
- Verified `node.rs` test module currently contains no `expect/unwrap/expect_err/unwrap_err` callsites.

## Validation

```bash
rg -n "expect\\(|unwrap\\(|expect_err\\(|unwrap_err\\(" crates/forge-cli/src/node.rs
cargo test -p forge-cli node::tests
cargo clippy -p forge-cli --all-targets -- -D warnings
```

Result:
- Pattern scan found no remaining callsites in `node.rs`.
- `node::tests` filter run passed (14 tests).
- `forge-cli` clippy sweep passed with `-D warnings`.

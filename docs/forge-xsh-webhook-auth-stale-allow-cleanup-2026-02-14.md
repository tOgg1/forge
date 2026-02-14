# forge-xsh - webhook_auth stale unwrap allow cleanup

Date: 2026-02-14
Task: `forge-xsh`
Scope:
- `crates/forge-cli/src/webhook_auth.rs`

## Change

- Removed stale `#[allow(clippy::unwrap_used)]` from `webhook_auth` tests.
- Confirmed there are no `unwrap/expect` callsites in that module, so the allow was unnecessary.

## Validation

```bash
cargo test -p forge-cli --lib webhook_auth::tests
cargo clippy -p forge-cli --all-targets -- -D warnings
```

Result:
- Focused `webhook_auth` unit tests passed.
- `forge-cli` clippy sweep passed with `-D warnings`.

# forge-ch7: root help/docs CLI parity regression test

## Scope delivered
- Added integration regression test:
  - `crates/forge-cli/tests/docs_cli_parity_test.rs`
- Test compares command IDs in `forge --help` against documented command headings in `docs/cli.md`.
- Includes explicit exclusion list for non-reference infra command families.

## Validation
- `cargo test -p forge-cli --test docs_cli_parity_test -- --nocapture`
- `cargo check -p forge-cli`

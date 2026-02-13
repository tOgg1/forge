# forge-86a: docs parity exclusions tightening

## Scope delivered
- Tightened `crates/forge-cli/tests/docs_cli_parity_test.rs` to remove all root-help docs parity exclusions.
- Parity guard now enforces full root command-family coverage in `docs/cli.md`.

## Validation
- `cargo test -p forge-cli --test docs_cli_parity_test -- --nocapture`
- `cargo check -p forge-cli`

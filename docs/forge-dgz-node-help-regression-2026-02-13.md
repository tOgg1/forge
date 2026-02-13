# forge-dgz: node help regression guard

## Scope delivered
- Added node module regression test in `crates/forge-cli/src/node.rs`:
  - `help_includes_registry_subcommands`
- Test asserts `forge node help` includes registry usage lines for:
  - `registry ls`
  - `registry show`
  - `registry update`

## Validation
- `cargo test -p forge-cli --lib node::tests::help_includes_registry_subcommands -- --nocapture`
- `cargo test -p forge-cli --lib node::tests:: -- --nocapture`

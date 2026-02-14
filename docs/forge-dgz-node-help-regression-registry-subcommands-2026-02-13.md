# forge-dgz - Node help regression for registry subcommands (2026-02-13)

## Scope shipped
- Added node help regression test in `crates/forge-cli/src/node.rs`:
  - `node::tests::help_includes_registry_subcommands`
- Test verifies `forge node help` contains usage lines for:
  - `forge node registry ls <node-id> [agents|prompts]`
  - `forge node registry show <node-id> <agent|prompt> <name>`
  - `forge node registry update <node-id> <agent|prompt> <name> [flags]`

## Validation
```bash
cargo fmt -p forge-cli
cargo test -p forge-cli --lib node::tests::help_includes_registry_subcommands -- --nocapture
cargo check -p forge-cli
```

# forge-zy7: root command regression for legacy `node` exposure (2026-02-13)

## Summary
- Fixed root command parity regression where legacy `node` was:
  - listed in root help
  - accepted as a top-level command (exit `0`)
- Restored behavior expected by root parity tests:
  - `node` omitted from root help
  - `forge node` treated as unknown command (exit `1`)

## Changes
- `crates/forge-cli/src/lib.rs`
  - Removed top-level dispatch arm for `Some("node")`.
  - Removed `node` entry from root help command list.
  - Updated root-help unit assertion list to no longer expect `node`.

## Validation
- `cargo test -p forge-cli --test root_command_test` (21 passed)
- `cargo test -p forge-cli root_help_includes_extended_command_families` (passed)
- `cargo test -p forge-cli docs_cli_covers_root_help_command_families` (passed)

# forge-q4n - CLI docs parity for completion/context/use (2026-02-13)

## Scope shipped
- Added new sections to `docs/cli.md` for:
  - `forge completion`
  - `forge context`
  - `forge use`
- Included usage examples aligned with current command help.
- Tightened docs parity test exclusions in `crates/forge-cli/src/lib.rs`:
  - removed `completion`, `context`, and `use` from exclusion list
  - these are now enforced by `tests::docs_cli_covers_root_help_command_families`

## Validation
```bash
cargo fmt -p forge-cli
cargo test -p forge-cli --lib tests::docs_cli_covers_root_help_command_families -- --nocapture
cargo check -p forge-cli
```

# forge-86a - CLI docs parity for remaining command families (2026-02-13)

## Scope shipped
- Added missing command sections to `docs/cli.md` for:
  - `forge hook`
  - `forge inject`
  - `forge lock`
  - `forge mail`
  - `forge migrate`
  - `forge send`
  - `forge skills`
- Added help-aligned usage examples for each section.
- Updated docs parity test in `crates/forge-cli/src/lib.rs`:
  - removed all remaining command exclusions (`DOC_EXCLUSIONS` is now empty)
  - docs coverage is now fully enforced for root command families reported by `forge --help`

## Validation
```bash
cargo fmt -p forge-cli
cargo test -p forge-cli --lib tests::docs_cli_covers_root_help_command_families -- --nocapture
cargo check -p forge-cli
```

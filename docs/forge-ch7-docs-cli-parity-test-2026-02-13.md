# forge-ch7 - Docs parity regression test for root CLI commands (2026-02-13)

## Scope shipped
- Added regression test in `crates/forge-cli/src/lib.rs`:
  - `tests::docs_cli_covers_root_help_command_families`
- Test behavior:
  - parses command IDs from root `forge --help` output
  - parses command IDs from `docs/cli.md` headings
  - supports multiple inline code spans in one heading (e.g., stop/kill alias heading)
  - supports loop alias sections by mapping `forge loop <subcommand>` headings to top-level alias command IDs
  - fails when docs coverage drifts for non-excluded command families
- Added missing docs section in `docs/cli.md` for:
  - `forge status`

## Validation
```bash
cargo fmt -p forge-cli
cargo test -p forge-cli --lib tests::docs_cli_covers_root_help_command_families -- --nocapture
cargo check -p forge-cli
```

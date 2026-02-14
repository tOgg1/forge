# forge-3rp - CLI help parity regression guard (2026-02-13)

## Scope shipped
- Added root help regression test in:
  - `crates/forge-cli/src/lib.rs`
- Test validates root `forge --help` includes critical command families:
  - `delegation`, `job`, `trigger`, `mesh`, `node`, `registry`, `team`, `task`, `workflow`

## Why
- Prevent silent omission of newly added command families from root help during router/help refactors.

## Validation
```bash
cargo fmt -p forge-cli
cargo test -p forge-cli --lib tests::root_help_includes_extended_command_families -- --nocapture
cargo check -p forge-cli
```

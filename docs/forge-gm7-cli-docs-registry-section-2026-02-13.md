# forge-gm7 - CLI docs parity for registry command family (2026-02-13)

## Scope shipped
- Updated `docs/cli.md` canonical command-family list to include:
  - `forge registry ...`
- Added dedicated `forge registry` section with usage examples for:
  - `status`
  - `export`
  - `import --prefer`
  - `ls` / `ls agents`
  - `show agent|prompt`
  - `update agent ...`
  - `update prompt ...`

## Validation
```bash
cargo run -q -p forge-cli --bin forge-cli -- registry help
cargo check -p forge-cli
```

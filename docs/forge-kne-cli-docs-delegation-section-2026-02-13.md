# forge-kne - CLI docs parity for delegation command family (2026-02-13)

## Scope shipped
- Updated canonical command-family list in `docs/cli.md` to include:
  - `forge delegation ...`
  - `forge team ...`
  - `forge task ...`
- Added dedicated `forge delegation` section with examples for:
  - `route --payload ... --team ...`
  - `explain --payload ... --team ...`
  - `route --payload ... --rules ...`
- Added behavior notes:
  - `--payload` required
  - exactly one of `--rules` or `--team`
  - `route` vs `explain` output intent

## Validation
```bash
cargo run -q -p forge-cli --bin forge-cli -- delegation --help
cargo check -p forge-cli
```

# forge-zv7 - workflow normalization/toml unwrap slice

Date: 2026-02-14
Task: `forge-zv7`
Scope: `crates/forge-cli/src/workflow.rs`

## Change

- Added explicit test helpers:
  - `ok_or_panic(result, context)`
  - `err_or_panic(result, context)`
- Replaced `unwrap`/`unwrap_err` in targeted tests:
  - `normalize_name_simple`
  - `normalize_name_strips_toml_suffix`
  - `parse_toml_basic`
  - `parse_toml_max_parallel`
  - `parse_toml_invalid`

## Validation

```bash
cargo test -p forge-cli --lib normalize_name_
cargo test -p forge-cli --lib parse_toml_
```

Result:
- Targeted workflow normalization and TOML parse tests passed.
- Scoped unwrap sites replaced with explicit handling.

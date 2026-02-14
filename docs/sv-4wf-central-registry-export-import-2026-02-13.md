# sv-4wf: Central registry store + export/import

Date: 2026-02-13
Task: `sv-4wf`

## Scope delivered

- Added central registry store with JSON document model:
  - local: `$FORGE_DATA_DIR/registry/registry.json` (or default data dir)
  - repo export: `.forge/registry/registry.json`
- Added CLI:
  - `forge registry status [--repo <path>]`
  - `forge registry export [--repo <path>]`
  - `forge registry import [--repo <path>] [--prefer local|repo]`
- Merge behavior:
  - default `--prefer local`: keep local entries on conflict
  - `--prefer repo`: repo entries overwrite local on conflict
- Export writes pretty JSON (commit-friendly) and includes prompt discovery from `.forge/prompts/*.md`.

## Files

- `crates/forge-cli/src/registry.rs`
- `crates/forge-cli/src/lib.rs`

## Validation

```bash
cargo test -p forge-cli --lib registry::tests:: -- --nocapture
cargo build -p forge-cli
```

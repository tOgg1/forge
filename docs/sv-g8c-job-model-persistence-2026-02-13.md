# sv-g8c: Job model + persistence

Date: 2026-02-13
Task: `sv-g8c`

## Scope delivered

- Added file-backed job model and run history store:
  - job definitions under `.../jobs/definitions/<name>.json`
  - run history under `.../jobs/runs/<name>.jsonl`
- Added `forge job` command family:
  - `forge job create <name> --workflow <workflow>`
  - `forge job ls`
  - `forge job run <name> [--trigger <source>] [--input key=value]`
  - `forge job runs <name> [--limit <n>]`
- Added JSON/JSONL support for list/create/run/runs outputs.

## Files

- `crates/forge-cli/src/job.rs`
- `crates/forge-cli/src/lib.rs`

## Validation

```bash
cargo test -p forge-cli job::tests:: -- --nocapture
cargo build -p forge-cli
```

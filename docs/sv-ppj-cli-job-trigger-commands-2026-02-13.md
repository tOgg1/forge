# sv-ppj: CLI job/trigger commands

Date: 2026-02-13

## Delivered

- Expanded `forge job` command surface:
  - `forge job show <name>`
  - `forge job logs <name> [--limit <n>]`
  - `forge job cancel <run-id>`
- Added `forge trigger` command family:
  - `forge trigger ls`
  - `forge trigger add <spec> --job <name>`
  - `forge trigger rm <trigger-id>`
- Trigger specs accepted by CLI:
  - `cron:<expr>`
  - `webhook:</path>`
- Root CLI wiring updated so `forge trigger ...` dispatches from `lib.rs`.

## Notes

- Trigger storage reuses job trigger persistence (`jobs/triggers/*.json`).
- Stored trigger records now include `trigger_type` (`cron` or `webhook`).
- Cron scheduler tick only evaluates `trigger_type == "cron"` records.

## Validation

- `cargo test -p forge-cli --lib job::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib trigger::tests:: -- --nocapture`
- `cargo test -p forge-cli --lib tests::trigger_module_is_accessible -- --nocapture`
- `cargo check -p forge-cli`

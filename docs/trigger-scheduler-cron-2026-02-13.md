# Trigger scheduler (cron) (sv-jzm)

Date: 2026-02-13
Task: sv-jzm (M5.2)

## Delivered

- Added cron trigger model in `crates/forge-cli/src/job.rs`:
  - `CronTriggerRecord` persisted under `jobs/triggers/*.json`
  - fields include `trigger_id`, `job_name`, `cron`, `next_fire_at`, `enabled`, timestamps.
- Added cron parser + schedule matcher:
  - supports 5-field cron (`minute hour day month weekday`)
  - field syntax: `*` or exact numeric value
  - range validation for each field
  - rejects malformed or unsupported expressions.
- Added next-fire calculation:
  - minute-granularity scheduler search
  - computes deterministic `next_fire_at` in UTC.
- Added scheduler tick primitive:
  - `JobStore::tick_cron_triggers(now)`
  - fires due cron triggers
  - records job runs (`trigger=cron:<expr>`)
  - advances trigger `next_fire_at`.

## Acceptance mapping

- Cron triggers start jobs: implemented via `tick_cron_triggers` + run record append.
- Misconfigured cron rejected: implemented in parser validation and covered by unit tests.

## Tests

- `cargo test -p forge-cli --lib job::tests:: -- --nocapture`
- `cargo check -p forge-cli`

# forge-46f: agent test cleanup/lock poison stabilization (2026-02-13)

## Summary
- Fixed `forge-cli` agent test helper behavior that caused:
  - panic on temp dir cleanup (`Directory not empty`)
  - poisoned env mutex lock
  - cascade of unrelated test failures

## Root cause
- `with_observability_db_path` panicked on `remove_dir_all` failure.
- panic poisoned `ENV_LOCK`, and later helpers panic on lock acquisition.
- env restoration and cleanup were not panic-safe if callback panicked.

## Changes
- `crates/forge-cli/src/agent.rs`
  - `ENV_LOCK` acquisition now recovers from poison via `err.into_inner()`.
  - `with_observability_db_path` now:
    - wraps callback in `catch_unwind`
    - always restores env vars after callback
    - performs best-effort temp dir cleanup (warns instead of panics)
    - re-throws callback panic payload after cleanup/env restore
  - `with_temp_env` now:
    - wraps callback in `catch_unwind`
    - always restores env vars
    - re-throws callback panic payload after restoration

## Validation
- `cargo test -p forge-cli --tests`
  - prior agent lock/cleanup failure set no longer fails.
  - run now progresses to separate completion golden mismatches (handled as follow-up task).


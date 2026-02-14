# forge-qna: `up` sqlite metadata test cwd race (2026-02-13)

## Summary
- Fixed flaky/failing `up` test caused by process-wide cwd races across parallel tests.
- Failure manifested as `created.repo_path` mismatch in:
  - `up::tests::up_sqlite_backend_creates_loop_and_sets_running_metadata`

## Root Cause
- `with_current_dir(...)` in `crates/forge-cli/src/up.rs` mutated process cwd without a shared lock.
- Another test changing cwd concurrently could leak into this metadata assertion test.

## Changes
- `crates/forge-cli/src/up.rs`
  - Added test-only global cwd mutex guard (`OnceLock<Mutex<()>>` + poison-safe recovery).
  - Guarded `with_current_dir(...)` with that mutex.
  - Guarded `up_sqlite_backend_creates_loop_and_sets_running_metadata` with the same mutex.

## Validation
- `cargo test -p forge-cli --lib up::tests::up_sqlite_backend_creates_loop_and_sets_running_metadata`
- `cargo test -p forge-cli --lib up::tests::up_sqlite_backend_resolves_registered_prompt_name_before_path_fallback`
- `cargo test -p forge-cli --lib up::tests::up_sqlite_backend_enqueues_initial_wait_pause_item`
- `cargo test -p forge-cli --tests` now passes `up` integration/unit suites and continues until a later unrelated `workflow_help_matches_golden` drift.

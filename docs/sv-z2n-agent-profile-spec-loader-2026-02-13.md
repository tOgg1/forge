# sv-z2n - Agent profile spec + loader (2026-02-13)

## Scope mapping
- Task asks for an agent profile schema plus loader/registry lookup.
- Existing implementation already provides this through profile model + persistence layers:
  - `crates/forge-cli/src/profile.rs`
    - profile schema fields (id/name/harness/auth/prompt mode/command template/model/env/max concurrency)
    - profile validation and normalization
    - backend trait + in-memory + sqlite-backed profile loading and lookup (name/id)
  - `crates/forge-db/src/profile_repository.rs`
    - persistent profile model + validation
    - CRUD + `get` / `get_by_name` lookup APIs
    - typed validation errors for invalid profile data

## Validation run
- Passing validation:
```bash
cargo test -p forge-db --test profile_repository_test -- --nocapture
```
- Result: `32 passed; 0 failed`.

## Current workspace blocker (separate from profile repository)
- Attempted CLI-side profile test run:
```bash
cargo test -p forge-cli profile::tests:: -- --nocapture
```
- Blocked by unrelated `workflow`/`workflow_bash_executor` compile drift (signature mismatch around `load_workflow_logs_result` and `WorkflowBackend` bounds), not by profile schema/loader code.

Given the delivered schema + loader + passing repository validation suite, `sv-z2n` is treated as delivered baseline.

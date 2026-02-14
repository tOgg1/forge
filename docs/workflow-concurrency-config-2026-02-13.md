# Workflow concurrency config (sv-xw8)

Date: 2026-02-13
Task: sv-xw8 (M3.4b)

## Delivered

- Added workflow-level concurrency field: `max_parallel`.
- Added environment override: `FORGE_WORKFLOW_MAX_PARALLEL`.
- Added global config override from `scheduler.workflow_max_parallel` in Forge config.
- Wired resolved concurrency into workflow run execution (`execute_parallel_workflow`).
- Surfaced effective value and source in `forge workflow show` output.

## Precedence

Resolution order for effective workflow parallelism:

1. `workflow.max_parallel` (workflow TOML)
2. `FORGE_WORKFLOW_MAX_PARALLEL` (env)
3. `scheduler.workflow_max_parallel` (global config)
4. built-in default (`4`)

## Validation semantics

- `workflow.max_parallel` must be `>= 0` in schema validation.
  - `0` means "unset" (fall through to env/global/default resolution).
- Resolved runtime concurrency source values must be greater than `0`.
  - Invalid env/global values return explicit errors.

## Tests run

- `cargo test -p forge-cli --lib workflow::tests::resolve_workflow_max_parallel_ -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::show_workflow -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::validate_negative_max_parallel -- --nocapture`
- `cargo test -p forge-cli --lib workflow::tests::parse_toml_max_parallel -- --nocapture`
- `cargo check -p forge-cli`

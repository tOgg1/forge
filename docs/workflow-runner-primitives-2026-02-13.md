# Workflow Runner Primitives (2026-02-13)

Implemented in `crates/forge-cli/src/workflow_run_persistence.rs`:

- `execute_agent_step`: harness-plan execution for agent steps.
  - Captures `stdout` + `stderr` + `exit_code`.
  - Emits step logs + timestamps + duration.
- `execute_loop_step`: loop iteration driver over agent-step execution.
  - Tracks `iterations`.
  - Tracks stop status: `stop_condition_met|max_iterations_reached|iteration_failed`.
- `workflow_step_order` + `execute_sequential_workflow`: deterministic DAG engine core.
  - Deterministic topological order.
  - Sequential step state progression.
  - Failure propagation: failed step + remaining steps skipped.

Implemented in `crates/forge-cli/src/workflow.rs`:

- `forge workflow run <name>` command path.
  - Creates persisted workflow run record.
  - Executes sequential DAG via `execute_sequential_workflow`.
  - Persists step status transitions + per-step logs.
  - Appends workflow ledger entry in `.forge/ledgers/workflow-<workflow-name>.md`.
  - Returns run id + final status.
  - Current execution support: `bash` step type (explicit error for unsupported types).

Key finding:

- Use `bash -c`, not `bash -lc`, for step execution in tests/runtime.
  - `-lc` can source user profile files and pollute `stderr` with unrelated shell errors.

Verification commands:

- `cargo test -p forge-cli --lib workflow::run_persistence::`
- `cargo check -p forge-cli`

## M3.4a Parallel Scheduler Core (2026-02-13)

Implemented in `crates/forge-cli/src/workflow_run_persistence.rs`:

- `execute_parallel_workflow(...)`
  - Launches dependency-ready steps concurrently.
  - Enforces `max_parallel` cap (`0` coerces to `1`).
  - Marks dependents `Skipped` when an upstream step `Failed` or `Skipped`.
  - Keeps independent branches running after failures.
  - Preserves deterministic result ordering using topological step order.

Added engine tests:

- `parallel_workflow_runs_independent_steps_concurrently`
- `parallel_workflow_respects_concurrency_limit`
- `parallel_workflow_failure_stops_only_dependents`
- `parallel_workflow_zero_limit_defaults_to_one`

## M3.4b Concurrency Config (2026-02-13)

Implemented in `crates/forge-cli/src/workflow.rs`:

- Workflow-level config:
  - `max_parallel = <n>` in workflow TOML.
- Global default config:
  - `scheduler.workflow_max_parallel` from Forge global config (`FORGE_CONFIG_PATH` or `~/.config/forge/config.yaml`).
  - Environment override: `FORGE_WORKFLOW_MAX_PARALLEL`.
- Resolution precedence:
  - workflow `max_parallel` -> env override -> global config -> default (`1`).
- `forge workflow run` now uses parallel engine execution with resolved max parallel.
- `forge workflow show` now prints resolved max parallel and source.

Verification commands:

- `cargo test -p forge-cli --lib resolve_workflow_max_parallel_ -- --nocapture`
- `cargo test -p forge-cli --lib parse_toml_max_parallel -- --nocapture`
- `cargo test -p forge-cli --lib validate_negative_max_parallel -- --nocapture`
- `cargo test -p forge-cli --lib show_workflow -- --nocapture`
- `cargo test -p forge-cli --lib parallel_workflow_ -- --nocapture`
- `cargo build -p forge-cli`

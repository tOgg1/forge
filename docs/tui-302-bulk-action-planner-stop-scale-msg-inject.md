# TUI-302 bulk action planner for stop/scale/msg/inject

Task: `forge-s1r`  
Status: delivered

## Scope

- Add bulk action planning primitives for:
  - `stop`
  - `scale`
  - `msg`
  - `inject`
- Include dry-run summary before execution.
- Detect planning conflicts and surface clear reasons.
- Attach rollback hints for each queued action.
- Expose queue transparency lines for TUI rendering.

## Implementation

- New module: `crates/forge-tui/src/bulk_action_planner.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Planner model

- `BulkPlannerAction` models supported bulk intents:
  - `Stop`
  - `Scale { target_count }`
  - `Message { body }`
  - `Inject { body }`
- `plan_bulk_action(...)` returns `BulkActionPlan` with:
  - dry-run summary text
  - target counts (`total/ready/blocked`)
  - structured conflicts (`warning/error`)
  - transparent queued command list

## Conflict checks

- Empty selection.
- Empty message/inject payload.
- Missing loop id.
- Duplicate loop ids.
- Stop no-op on terminal loop states (`stopped/error`).
- Inject blocked on terminal loop states.
- Scale rows missing pool.
- Multi-pool scale warning with explicit per-pool queueing.

## Queue transparency + rollback

- `queue_transparency_lines(...)` renders human-readable queue rows:
  - queue index
  - ready/blocked status
  - target
  - command preview
  - block reason (if any)
  - rollback hint

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

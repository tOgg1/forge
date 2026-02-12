# TUI-804 keyboard macro recorder and runner

Task: `forge-nkh`  
Status: delivered

## Scope

- Record and replay common key sequences.
- Keep macro definitions reviewable.
- Apply safety checks before execution.

## Implementation

- New module: `crates/forge-tui/src/keyboard_macro.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Macro model

- Recording lifecycle:
  - `start_macro_recording(...)`
  - `append_macro_step(...)`
  - `finalize_macro_recording(...)`
- Execution lifecycle:
  - `review_macro_definition(...)`
  - `plan_macro_run(...)`
- Review rendering:
  - `render_macro_definition(...)`

## Safety controls

- Policy limits:
  - max macro steps
  - max repeat count
  - blocked step keys
  - destructive token detection
- Blocking checks prevent run planning.
- Warning checks remain reviewable in macro inspection output.

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

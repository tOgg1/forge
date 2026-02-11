# Rust Claude stream-json logs update (2026-02-11)

## Problem
- Claude loops emitted buffered text logs with `claude -p` text mode.
- Live logs lacked incremental visibility compared to Codex.

## Decision
- Standardize Claude loop profiles on stream JSON output:
  - `--verbose --output-format stream-json --include-partial-messages`
- Keep prompt injection via env mode:
  - `-p "$FORGE_PROMPT_CONTENT"`

## Runtime profile updates applied
- `cc1`, `cc2`, `cc3` command templates updated in live DB.
- `prompt_mode` set to `env` for all three profiles.

## Code updates
- `forge logs` now parses Claude stream-json lines and renders readable output:
  - init summary (`[claude:init] ...`)
  - streamed text deltas
  - result summary (`[claude:result] ...`)
- Raw JSON still available via `forge logs --raw`.
- Colorized rendering enabled by default; disable with `--no-color`.

## Validation
- Unit tests added for formatted and raw Claude log rendering.
- Profile default test confirms new Claude template defaults to stream-json config.
- Real-loop smoke validated readable parsed output from stream-json logs.

# Swarm CLI Output Style Guide

This guide defines how CLI output should look in human, JSON, and JSONL modes.
It should align with `docs/ux/terminology.md`.

## Output modes

- **Human (default)**: tables and labeled blocks.
- **JSON (`--json`)**: pretty-printed JSON object/array.
- **JSONL (`--jsonl`)**: one JSON object per line (streaming-friendly).

Human output must be stable and scannable. JSON output must be machine-safe.

## Color and icon semantics

Human mode may include color and status markers. JSON/JSONL must not include
color or icons.

Recommended ASCII markers (default):

- `OK` (Idle, Online)
- `BUSY` (Working)
- `WAIT` (AwaitingApproval, Paused)
- `WARN` (RateLimited, Cooldown)
- `ERR` (Error, Offline)

Optional Unicode icons may be layered on top of these in terminals that
support them. Always provide a no-color mode (`--no-color` or `NO_COLOR=1`).

## Column order and table layouts

### `swarm node list`

Columns (in order):
1. NAME
2. ID (short)
3. STATUS
4. LOCAL
5. SSH
6. AGENTS

### `swarm ws list`

Columns:
1. NAME
2. ID (short)
3. NODE
4. PATH (truncate from left if long)
5. STATUS
6. AGENTS
7. SESSION

### `swarm agent list`

Columns:
1. ID (short)
2. TYPE
3. STATE
4. WORKSPACE
5. PANE
6. QUEUE

### `swarm agent status`

Human layout should be a labeled block:
- Agent ID, Type, State, Confidence, Reason
- Workspace, Pane, Last Activity, Queue Length
- Optional: Evidence list

### Queue lists

Columns:
1. ID (short)
2. TYPE
3. STATUS
4. POSITION
5. CREATED

## Truncation rules

- IDs: show first 8 chars in human mode.
- Paths: truncate from the left, preserve the repo name and suffix.
- Long reasons: show one line and allow a "details" view in TUI.

JSON output must never truncate.

## Error output (JSON/JSONL)

When `--json` or `--jsonl` is set, errors should use this envelope:

```json
{
  "error": {
    "code": "ERR_NOT_FOUND",
    "message": "agent not found",
    "hint": "Run swarm agent list to see valid IDs",
    "details": {
      "resource": "agent",
      "id": "abc123"
    }
  }
}
```

Rules:
- `code` is stable and machine-readable.
- `message` is short and human-friendly.
- `hint` is optional but recommended for user-facing errors.
- `details` is optional and may be empty.

## Exit codes

- `0`: success
- `1`: user error (invalid input, not found)
- `2`: operational error (runtime or dependency failure)

## Watch/streaming output

`--watch` should emit JSONL objects with a top-level `type` field to allow
clients to filter messages by kind.

# Contextual Footer Hints (2026-02-13)

Task: `forge-m3d`

## Behavior

Footer hints are contextual (mode + tab), ranked by utility and recent usage, and capped at 6-8 items.

## Cap Rules

- Deep focus mode: max `6`
- Compact density: max `7`
- Standard mode: max `8`

## Rendering Strategy

- Key-first hint format (`<key> <label>`) for quick scan.
- Width truncation at render time via `trim_to_width(...)`.
- Per-tab hints appended for Logs/Runs/MultiLogs/Inbox.

## Regression Coverage

- `footer_hints_never_exceed_eight_items`
- `footer_hints_promote_recent_follow_action`
- `footer_hints_follow_latest_recency_signal`

# Harness Usage Capture

Background usage capture for local harness profiles.

## Script

`scripts/harness-usage-capture.sh`

## Quick start

```bash
# one snapshot
scripts/harness-usage-capture.sh run-once

# start background loop (5m interval)
scripts/harness-usage-capture.sh start --interval 300

# check status / logs
scripts/harness-usage-capture.sh status
scripts/harness-usage-capture.sh tail

# stop
scripts/harness-usage-capture.sh stop
```

## Output

- `.forge/usage-capture/latest.json`
- `.forge/usage-capture/snapshots.jsonl`

## Coverage

- `oc*`: direct `opencode stats` capture.
- `cc*`: local cache capture from `~/.claude-*/stats-cache.json`.
- `codex*`: auth status + latest token usage line from `~/.codex-*/log/codex-tui.log`.
- `pi*`: marked unsupported (no stable native usage/quota command yet).

## Limitation

Provider “remaining quota” panels shown by interactive slash UIs (`/status`, `/usage`) are not currently exposed as a stable machine-readable CLI surface. This script captures best-effort local metrics available without driving TUI rendering.

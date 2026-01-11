---
id: f-cd1f
status: closed
deps: [f-0fd1, f-c44a]
links: []
created: 2026-01-10T20:06:51Z
type: task
priority: 0
assignee: Tormod Haugland
parent: f-c2d0
---
# Implement fmail send (standalone)

Implement `fmail send` for standalone mode (directly writing to .fmail/).

Required behavior (per docs/forge-mail/SPEC.md):
- Usage:
  - fmail send <topic|@agent> <message>
  - fmail send <topic|@agent> -f <file>
  - echo "msg" | fmail send <topic|@agent>
- Options:
  - -f/--file
  - -r/--reply-to
  - -p/--priority low|normal|high
  - --json (print sent message)

Implementation notes:
- Auto-create .fmail/ + required subdirs if missing
- Body auto-detects JSON (if valid JSON literal/object/array), otherwise string
- For @agent targets: write under dm/<recipient>/
- For topic targets: write under topics/<topic>/
- Update agent registry (agents/<from>.json) with first_seen/last_seen and host (if available)

## Acceptance Criteria

- Sending to a topic creates .fmail/topics/<topic>/<id>.json with required fields
- Sending to a direct message creates .fmail/dm/<recipient>/<id>.json
- --reply-to populates reply_to; --priority populates priority
- --json outputs the created message object on stdout
- Message IDs follow YYYYMMDD-HHMMSS-NNNN and are monotonically sortable
- Errors are clear for invalid topic names, invalid @agent names, missing message body, and oversized payloads


## Notes

**2026-01-11T07:01:20Z**

Implemented standalone fmail send with JSON body detection and agent registry update.

**2026-01-11T07:02:16Z**

go test ./... failed: permission denied writing to /root/.cache/go-build (sandbox).

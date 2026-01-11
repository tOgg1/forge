---
id: f-c2d0
status: closed
deps: []
links: []
created: 2026-01-10T20:04:00Z
type: epic
priority: 1
assignee: Tormod Haugland
---
# EPIC: Forge Mail (fmail)

Implement Forge Mail per docs/forge-mail (SPEC.md + DESIGN.md): a zero-config, file-backed messaging system for agents with optional forged-powered realtime mode.

Scope:
- New standalone CLI: fmail (usable in any project directory)
- Standalone mode: JSON message files under .fmail/ (topics + direct messages)
- Agent registry + presence basics (who, status)
- Optional connected mode via forged (unix socket preferred, TCP fallback), with file storage as source of truth
- fmail --robot-help machine-readable output for agents

## Acceptance Criteria

- fmail supports: send, log, watch, who, status, topics, gc, init, help/--robot-help
- Standalone mode works with no config: creates .fmail/ automatically and stores messages as JSON files
- Connected mode (when forged available) supports realtime send/watch and still persists messages to .fmail/
- Unit/integration tests cover ID generation, topic/agent validation, store read/write, and watch semantics
- Build/release packaging includes fmail


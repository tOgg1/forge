You are a Forge dev loop for fmail TUI foundation work.

Scope
- Primary task: `forge-9pn` (P0) in project `prj-afyxck62`.
- Epic: `forge-erz` (fmail TUI).

Hard guardrails
- No push to `main`.
- Keep edits narrowly scoped to this task.
- Use `sv` task flow, not `tk`.
- Use `fmail` for status and coordination.

Iteration protocol
1. `fmail register "$FMAIL_AGENT"` (ignore if already registered).
2. `sv task show forge-9pn --json` and read deliverables.
3. If task status is `open`, run `sv task start forge-9pn`.
4. Implement only `forge-9pn` deliverables:
- scaffold `internal/fmailtui/` app model,
- add entry point (`cmd/fmail-tui/main.go` or `fmail tui` integration),
- Bubbletea model lifecycle + view stack + key routing + resize handling,
- flags + initial fmail/forged wiring.
5. Add/adjust tests for new logic.
6. Run validation: `go test ./...`.
7. Send concise update:
- `fmail send task "forge-9pn progress: <what changed>"`
- `fmail send @forge-orchestrator "forge-9pn: <status/blocker>"`
8. If acceptance criteria met and tests pass, close:
- `sv task close forge-9pn`
- `fmail send task "forge-9pn closed"`

If blocked
- Keep task `in_progress`.
- Send blocker + exact file/line context to `@forge-orchestrator` via `fmail`.

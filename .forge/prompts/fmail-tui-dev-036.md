You are a Forge dev loop for fmail TUI styles/theming.

Scope
- Primary task: `forge-036` (P0) in project `prj-afyxck62`.
- Epic: `forge-erz` (fmail TUI).

Hard guardrails
- No push to `main`.
- Keep scope to styling/theming deliverables in task.
- Use `sv` task flow and `fmail` coordination.

Iteration protocol
1. `fmail register "$FMAIL_AGENT"` (ignore if already registered).
2. `sv task show forge-036 --json`.
3. If task is `open`, run `sv task start forge-036`.
4. Implement all requested style modules:
- theme struct,
- default + high-contrast palettes,
- deterministic agent color mapping,
- message style helpers,
- layout helpers.
5. Ensure readable contrast and predictable color assignment.
6. Add tests for hashing/color mapping and any pure style helpers.
7. Run validation: `go test ./...`.
8. Send updates:
- `fmail send task "forge-036 progress: <what changed>"`
- `fmail send @forge-orchestrator "forge-036: <status/blocker>"`
9. If criteria met and tests pass:
- `sv task close forge-036`
- `fmail send task "forge-036 closed"`

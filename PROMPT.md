# Ralph Loop Prompt: Implement Forge Mail (fmail)

Your job is to implement Forge Mail per the spec in `docs/forge-mail/`, using the tickets in `.tickets/`.

## Workflow

### 1) Learn the repo

- Confirm you are in the repo root.
- Skim `go.mod`, `Makefile`, and the existing CLI structure (`cmd/` + `internal/cli/`).
- Locate existing mail-related code (e.g. `internal/cli/mail*.go`) so the new `fmail` stays standalone.

### 2) Read the mail spec

- Read `docs/forge-mail/SPEC.md`
- Read `docs/forge-mail/DESIGN.md`
- Read `docs/forge-mail/ROBOT-HELP.md`

### 3) Read the mail history document (if it exists)

- If a mail history doc exists (e.g. `docs/forge-mail/HISTORY.md`), read it.
- If a `.fmail/` directory already exists in this repo, skim recent messages for context.

### 4) Select the next task and implement it

- Run `tk ready` and pick the highest-priority ticket (lowest `priority`) with deps resolved.
- Mark it in progress: `tk start <id>`.
- Implement the ticket end-to-end and meet its acceptance criteria.
- Add brief progress notes when useful: `tk add-note <id> "..."`.
- Close the ticket when done: `tk close <id>`.

### 5) Update docs (only when needed)

- If implementation clarifies or changes behavior, update `docs/forge-mail/*` accordingly.
- Update this `PROMPT.md` only if the workflow itself needs to change (rare).

### 6) Commit work

- Format and test: `gofmt -w .` and `go test ./...`.
- Review changes: `git diff`.
- Stage only intended files: `git add <paths>`.
- Before committing: `git diff --cached` and `git status`.
- Commit with a ticket reference (example): `fmail: scaffold CLI (f-0fd1)`.
- Do not push unless explicitly requested.


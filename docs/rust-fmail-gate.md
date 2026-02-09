# Rust fmail/fmail-tui Parity Gate

Task: `forge-3ca`  
Status: in-progress

## Goal

Define strict, testable criteria for `fmail` CLI and `fmail-tui` parity before Rust cutover.

## Scope

- Go source workflows:
  - `cmd/fmail`
  - `internal/fmail`
  - `cmd/fmail-tui`
  - `internal/fmailtui`
- Rust target crates:
  - `fmail-core`
  - `fmail-cli`
  - `fmail-tui`

## Required gate criteria

1. Command surface parity (`fmail`)
- Help snapshot and command list remain locked:
  - `docs/forge-mail/help/fmail-help.txt`
- Port manifest remains locked:
  - `docs/rust-fmail-command-manifest.md`
- Required command families stay compatible:
  - `send`, `watch`, `register`, `topics`, `messages`, `log`, `status`, `who`, `gc`, `init`.

2. TUI CLI parity (`fmail-tui`)
- Flag/help snapshot remains locked:
  - `docs/forge-mail/help/fmail-tui-help.txt`
- Required startup/CLI flags stay compatible:
  - `--project`, `--root`, `--theme`, `--poll-interval`, `--operator`, `--agent`.

3. Workflow parity requirements
- Core operator workflows preserve behavior:
  - inbox refresh,
  - message open/read/ack,
  - compose/send (topic + DM),
  - topic browsing and watch-style live updates.
- Any divergence requires explicit parity exception and owner sign-off.

4. Cutover rule
- fmail cutover is blocked until command and TUI gate checks are green.

## CI gate wiring

- `parity` workflow job runs:
  - `go test ./internal/parity -run '^TestFmailGateCommandAndTUIBaseline$' -count=1`
- `internal/doccheck` pins this gate doc, matrix linkage, and CI invocation.

## Evidence for final cutover

- Green CI run for fmail gate baseline test.
- Parity matrix fmail workflow row references this gate doc.
- Release checklist includes fmail/fmail-tui workflow sign-off.

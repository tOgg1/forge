# sv-dj9 - CLI/TUI for teams + tasks (2026-02-13)

## Scope shipped
- Added new CLI command family `forge team`:
  - `ls|new|rm|show`
  - `member add|rm|ls`
  - id-or-name team reference support
  - text + `--json`/`--jsonl` output modes
- Added new CLI command family `forge task`:
  - `send|ls|show|assign|retry`
  - payload builder (`type`, `title`, optional body/repo/tags/external_id)
  - team reference resolution by id or name
  - `assign` auto-promotes to `reassign` when already assigned
  - `retry` clones a terminal task (`done|failed|canceled`) into a new queued task
- Wired root CLI dispatch/help for `team` and updated `task` help text.

## TUI read-only v1
- Extended `forge-tui` snapshot renderer with read-only sections:
  - `teams snapshot (read-only)`
  - `team task inbox (read-only)`
- Added per-team queue counters:
  - `queued|assigned|running|blocked|open`
- Added inbox rows with task id/team/status/priority/assignee/title.

## Files touched
- `crates/forge-cli/src/team.rs`
- `crates/forge-cli/src/task.rs`
- `crates/forge-cli/src/lib.rs`
- `crates/forge-tui/src/bin/forge-tui.rs`
- `docs/cli.md`

## Tests added
- `crates/forge-cli/src/team.rs`
  - create/member/show/remove flow
  - invalid role validation
  - json list shape
- `crates/forge-cli/src/task.rs`
  - send/list/show/assign/retry flow
  - retry non-terminal validation
  - send usage validation
- `crates/forge-tui/src/bin/forge-tui.rs`
  - snapshot renders teams summary + task inbox sections

## Validation
```bash
cargo fmt -p forge-cli -p forge-tui
cargo test -p forge-cli --lib team::tests:: -- --nocapture
cargo test -p forge-cli --lib task::tests:: -- --nocapture
cargo test -p forge-tui --bin forge-tui snapshot_renders_team_summary_and_task_inbox_sections -- --nocapture
cargo build -p forge-cli
cargo build -p forge-tui
```

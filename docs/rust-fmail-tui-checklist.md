# Rust fmail TUI Baseline Checklist

Task: `forge-gn2`  
Status: in-progress

## Purpose

Baseline checklist for fmail TUI workflow/keymap parity before Rust cutover.
Use together with `scripts/rust-fmail-tui-smoke.sh`.

## Scripted smoke probes

- Run:
  - `scripts/rust-fmail-tui-smoke.sh`
- Expected:
  - `TestFmailGateCommandAndTUIBaseline` passes (`internal/parity`).
  - targeted Go `internal/fmailtui` workflow probes pass.
  - targeted Rust `fmail-tui` topic/operator/timeline/thread probes pass.

## Manual workflow checklist

1. Core views and navigation
- [ ] Open dashboard, topics, timeline, and thread views.
- [ ] Move focus between panes/views with configured keymap.
- [ ] Verify no panic/crash during rapid view switching.

2. Inbox/topic behavior
- [ ] Refresh topics and confirm unread counters update.
- [ ] Open topic preview and verify lazy-load/caching behavior.
- [ ] Toggle star/filter/sort states and confirm persistence.

3. Compose and send flows
- [ ] Compose DM and topic messages from TUI.
- [ ] Validate slash-command parsing (priority/tags/DM target).
- [ ] Confirm send success updates read markers and timeline.
- [ ] Validate operator reply flow from selected conversation.

4. Timeline/thread parity
- [ ] Open thread from timeline and verify depth/overflow rendering.
- [ ] Confirm bookmark toggle behavior and persistence.
- [ ] Verify chronological ordering and gap markers are stable.

5. Evidence capture
- [ ] Record smoke script output in release notes/checklist.
- [ ] Link run to `docs/rust-release-gate-checklist.md` fmail sign-off row.

## Suggested evidence command

```bash
scripts/rust-fmail-tui-smoke.sh | tee build/rust-fmail-tui-smoke.txt
```

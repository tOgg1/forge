# Rust Loop TUI Baseline Checklist

Task: `forge-wa7`  
Status: parity suite in-progress

## Purpose

Baseline checklist for loop TUI behavior parity before Rust cutover.
Use together with `scripts/rust-loop-tui-smoke.sh`.

## Scripted smoke probes

- Run:
  - `scripts/rust-loop-tui-smoke.sh`
- Expected:
  - targeted Go `internal/looptui` workflow + failure-state probes pass.
  - targeted Rust `forge-tui` workflow + failure-state probes pass.

## Manual workflow checklist

1. Tabs and mode transitions
- [ ] Move between overview/runs/logs/multi logs tabs.
- [ ] Enter/exit help mode and return to main mode.
- [ ] Verify no panic/crash when switching modes rapidly.

2. Selection and queue views
- [ ] Select a loop, switch to logs tab, cycle log source.
- [ ] Verify selection fallback when a loop disappears.
- [ ] Confirm queue-related status text remains readable.

3. Paging and keymap behavior
- [ ] Validate `[` / `]` tab movement behavior.
- [ ] Validate paging keys in multi logs view.
- [ ] Validate `PgUp` log scrolling behavior in logs tab.

4. Filter and wizard interactions
- [ ] Enter filter mode and confirm realtime status text updates.
- [ ] Validate wizard step validation and create-loops wizard happy path.

5. Failure-state handling
- [ ] Force-delete prompt appears for a running loop.
- [ ] Error banner/state rendering stays readable and non-crashing.
- [ ] Stop/kill confirm prompts match expected operator wording.

6. Evidence capture
- [ ] Record smoke script output in release notes/checklist.
- [ ] Link run to `docs/rust-release-gate-checklist.md` loop TUI sign-off row.

## Suggested evidence command

```bash
scripts/rust-loop-tui-smoke.sh | tee build/rust-loop-tui-smoke.txt
```

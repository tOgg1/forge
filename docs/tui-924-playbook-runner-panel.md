# TUI-924 playbook runner panel

Task: `forge-qth`  
Status: delivered

## Scope

- Guided incident workflow execution with step tracking.
- Dependency-aware step transitions.
- Progress + next-step output suitable for a panel surface.

## Implementation

- Added module: `crates/forge-tui/src/playbook_runner_panel.rs`
- Added playbook template/run model:
  - `PlaybookTemplate`, `PlaybookTemplateStep`
  - `PlaybookRunState`, `PlaybookRunStep`
  - `PlaybookStepStatus`
- Added run lifecycle:
  - `start_playbook_run` initializes run + auto-promotes first ready step.
  - `update_playbook_step` enforces dependency gate for `InProgress`/`Done`.
  - Auto-promotes next ready pending step when no active step is in progress.
- Added progress + panel helpers:
  - `compute_playbook_progress`
  - `render_playbook_panel_lines`
- Exported module in `crates/forge-tui/src/lib.rs`.

## Validation

- `cargo test -p forge-tui playbook_runner_panel::tests::`
- `cargo build -p forge-tui`

# Modal Action Rail Integration (2026-02-13)

Task: `forge-c6m`

## Goal

Integrate a keyboard-navigable modal action rail for destructive confirms with safe defaults.

## Confirm Rail Behavior

- Default rail selection: `Cancel` (non-destructive default)
- `Tab` / `Right`: next rail action
- `Shift+Tab` / `Left`: previous rail action
- `Enter`: execute selected rail action
- `y`: immediate confirm shortcut
- `n` / `Esc` / `q`: cancel

## Safety Contract

- Enter on default selection cancels action.
- Explicit rail movement or `y` required to submit destructive action.

## Regression Coverage

- `confirm_enter_uses_safe_cancel_by_default`
- `confirm_action_rail_tab_then_enter_submits`
- Existing confirm-mode tests still pass.

# TUI-103 Keyboard Mapping Engine (forge-3yh)

Date: 2026-02-12
Scope: centralized keybinding registry, per-view scopes, collision detection, diagnostics panel, resolution snapshots.

## Engine model

Core module: `crates/forge-tui/src/keymap.rs`

Key types:

- `KeyChord`: normalized key + modifiers
- `KeyScope`: `Global`, `Mode(<mode>)`, `View(<tab>)`
- `KeyCommand`: typed command ids (quit, tabs, palette ops, log ops, etc)
- `KeyBinding`: scope + chord -> command
- `Keymap`: resolve + conflict detection + diagnostics renderer

## Scope precedence

Resolution order is explicit and deterministic:

1. `View(<tab>)`
2. `Mode(<current-mode>)`
3. `Global`

This allows per-view overrides without losing global fallbacks.

## Collision detection

`Keymap::conflicts()` groups bindings by `(scope, chord)` and reports collisions where multiple commands share same key in same scope.

Diagnostics panel:

- `Keymap::conflict_diagnostics_lines(width, max_rows)`
- Integrated into TUI help screen so operators can audit active keymap state.

## Runtime integration

`crates/forge-tui/src/app.rs` now:

- owns `Keymap` in app state
- resolves global/mode/view scoped commands via `resolve_key_command`
- uses mapped commands for:
  - global quit (`Ctrl+C`)
  - command palette open (`Ctrl+P`)
  - palette navigation/execute/close actions
- renders conflict diagnostics in help content

## Tests

- keymap resolution precedence snapshot
- collision detector behavior with injected duplicate
- diagnostics panel snapshot (`no conflicts detected`)
- app-level help rendering includes diagnostics panel

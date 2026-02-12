# TUI-902 extension API for custom actions and palette commands

Task: `forge-j52`  
Status: delivered

## Scope

- Added extension action registry for custom commands exposed in command palette.
- Added command validation, permission validation, and audit metadata contracts.
- Added deterministic export of extension actions as `PaletteActionId::Custom(...)`.

## Contracts

- `ExtensionActionSpec`
  - extension/action identifiers
  - title + command
  - keywords + selection requirement
  - permissions
  - audit metadata
- `ExtensionActionAudit`
  - registered by
  - ticket
  - rationale
  - registration timestamp
- `ExtensionActionRegistry`
  - register
  - unregister
  - lookup by palette id
  - export palette actions
  - export deterministic audit rows

## Validation rules

- Reject invalid extension/action ids.
- Reject malformed commands (shell metacharacters and invalid chars).
- Enforce permission gates for sensitive command classes:
  - loop control (`loop stop/kill/delete/resume/new`) requires `ControlLoops`
  - `exec ...` requires `ExecuteShell`
  - URL/network commands require `NetworkAccess`

## Audit + permissions

- Every registered action stores audit context.
- Audit view emits stable rows sorted by normalized action key.
- Exported palette actions include permission slugs in keywords for operator search and filtering.

## Implementation

- New module: `crates/forge-tui/src/extension_actions.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

# Rust Rewrite: Legacy Drop List

Scope lock task: `forge-c0d`
Frozen: 2026-02-09

Source-of-truth signal:
- `internal/cli/legacy.go` keeps `legacyCommandsEnabled = false`.
- Commands registered through `addLegacyCommand(...)` are excluded from active non-legacy surface.

## Dropped legacy command groups

| Command group | Registered from | Drop status | Caveat |
|---|---|---|---|
| `accounts` | `internal/cli/accounts.go` | drop | Keep only data-model pieces needed by non-legacy commands. |
| `agent` | `internal/cli/agent.go` | drop | `forge send/inject/wait/status` remain non-legacy and must be ported separately. |
| `attach` | `internal/cli/attach.go` | drop | Legacy workspace attach path. |
| `node` | `internal/cli/node.go` | drop | Legacy node lifecycle path; do not auto-drop shared protocol types. |
| `recipe` | `internal/cli/recipe.go` | drop | Legacy recipe runner surface. |
| `vault` | `internal/cli/vault.go` | drop | Legacy vault management surface. |
| `workspace` (`ws`) | `internal/cli/workspace.go` | drop | Legacy workspace manager; modern loop workflows stay in scope. |

## Caveats and boundaries

- Drop list applies to legacy command entrypoints, not to shared internals still required by non-legacy behavior.
- Any change to `addLegacyCommand(...)` registrations must update this file in the same change.
- If a dropped group needs restoration, reopen as explicit scope decision before implementation.

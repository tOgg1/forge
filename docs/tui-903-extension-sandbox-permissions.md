# TUI-903 sandbox and permission model for extensions

Task: `forge-4jp`  
Status: delivered

## Scope

- Added extension sandbox policy engine with explicit-grant enforcement.
- Enforces capability restrictions for extension intents touching:
  - filesystem access
  - process spawn / exec
- Preserves typed decision + audit records for every evaluated request.

## Contracts

- `SandboxCapability`
  - `FilesystemRead`
  - `FilesystemWrite`
  - `ProcessSpawn`
- `SandboxGrant`
  - extension id + capability + scope
  - grant actor/reason
  - optional expiry
- `SandboxPolicy`
  - explicit-grant requirements
  - read/write root allowlists
  - blocked path prefixes
  - allowed process prefixes
- `SandboxIntent`
  - palette command
  - file read
  - file write
  - process spawn

## Enforcement behavior

- Filesystem read/write:
  - deny protected prefixes
  - enforce root allowlist
  - enforce explicit grant when policy requires
  - writes also require `WriteState` extension permission
- Process spawn:
  - requires `ExecuteShell` extension permission
  - must match allowed program prefixes
  - requires explicit grant when policy requires
- Palette `exec ...` commands are routed through process sandbox checks.
- Loop-control palette commands still enforce `ControlLoops` permission.

## Audit output

- Every decision returns `SandboxDecision` and embedded `SandboxAuditRecord`:
  - extension id
  - normalized intent
  - allow/deny result
  - capability target
  - reason
  - matched grant scope (if any)

## Implementation

- New module: `crates/forge-tui/src/extension_sandbox.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

## Validation

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`
